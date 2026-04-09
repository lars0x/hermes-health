use std::sync::Arc;

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::agent::{ExtractedObservation, ExtractionResult, UnresolvedMarker};
use crate::config::HermesConfig;
use crate::error::{HermesError, Result};
use crate::ingest::normalize;
use crate::services::loinc::LoincCatalog;

#[derive(Debug, Deserialize, Serialize)]
pub struct LabResultRow {
    pub marker_name: String,
    pub value: serde_json::Value, // number, string ("Negative"), or null
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub specimen: Option<String>, // "serum", "urine", "blood", "plasma", etc.
}

/// Run direct extraction via raw Ollama API call.
/// More reliable than Rig's Extractor for structured JSON output.
pub async fn run_direct_extraction(
    _pool: SqlitePool,
    catalog: Arc<LoincCatalog>,
    config: Arc<HermesConfig>,
    raw_text: &str,
) -> Result<(ExtractionResult, Vec<crate::agent::LlmLogEntry>)> {
    tracing::info!("Running direct extraction with model {}", config.ollama.model);

    // Truncate to avoid context length issues (find a valid UTF-8 boundary)
    let max_len = 48_000;
    let text = if raw_text.len() > max_len {
        let mut end = max_len;
        while end > 0 && !raw_text.is_char_boundary(end) {
            end -= 1;
        }
        &raw_text[..end]
    } else {
        raw_text
    };

    let prompt = format!(
        "/nothink\nExtract ALL biomarker results from this lab report. The report may be in any language - extract the marker names in English where possible, but preserve the original name if unsure.\nFor each result, identify the specimen type from section headers or context (e.g. \"Urine Chemistry\", \"Haematology\", \"Serum\").\nReturn JSON: {{\"results\": [{{\"marker_name\": str, \"value\": number, \"unit\": str, \"specimen\": \"serum\" or \"urine\" or \"blood\" or \"plasma\" or null}}]}}\n\nLab report:\n{}",
        text
    );

    tracing::info!("Extraction prompt: {} chars, text starts with: {:?}", prompt.len(), &text[..text.len().min(100)]);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(config.ollama.timeout_seconds))
        .build()
        .map_err(|e| HermesError::Agent(format!("HTTP client error: {e}")))?;

    let response = client
        .post(format!("{}/api/chat", config.ollama.url))
        .json(&serde_json::json!({
            "model": config.ollama.model,
            "messages": [
                {"role": "system", "content": "/nothink"},
                {"role": "user", "content": prompt}
            ],
            "stream": false,
            "format": "json",
            "think": false,
            "options": {
                "temperature": config.ollama.temperature,
                "num_predict": config.ollama.num_predict,
                "num_ctx": config.ollama.num_ctx
            }
        }))
        .send()
        .await
        .map_err(|e| HermesError::Agent(format!("Ollama request failed: {e}")))?;

    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| HermesError::Agent(format!("Failed to parse Ollama response: {e}")))?;

    let response_text = body
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| HermesError::Agent(format!("No message content in Ollama response: {}", body)))?;

    tracing::info!("Ollama returned {} chars", response_text.len());

    let mut llm_log = vec![crate::agent::LlmLogEntry {
        step: "extraction".to_string(),
        prompt: prompt.clone(),
        response: response_text.to_string(),
        messages: None,
        tool_calls_count: None,
        turns: None,
    }];

    // Parse the JSON response - handle both array and object-with-array formats
    let mut rows = parse_extraction_response(response_text)?;

    // Some LLMs put specimen at the top level instead of per-result.
    // If most rows lack specimen, check for a top-level specimen field and propagate it.
    if let Ok(obj) = serde_json::from_str::<serde_json::Value>(response_text) {
        if let Some(top_specimen) = obj.get("specimen").and_then(|v| v.as_str()) {
            let top_specimen = top_specimen.to_string();
            for row in &mut rows {
                if row.specimen.is_none() {
                    row.specimen = Some(top_specimen.clone());
                }
            }
        }
    }

    tracing::info!("Parsed {} lab result rows", rows.len());

    // Pre-process rows: extract values, build UnresolvedMarker list for LLM
    struct ParsedRow {
        marker_name: String,
        value: f64,
        original_value: String,
        unit: String,
        specimen: Option<String>,
    }

    let mut parsed_rows: Vec<ParsedRow> = Vec::new();
    for row in rows {
        let (value, original_value_str) = match &row.value {
            serde_json::Value::Number(n) => {
                let v = n.as_f64().unwrap_or(0.0);
                (v, v.to_string())
            }
            serde_json::Value::String(s) => (0.0, s.clone()),
            serde_json::Value::Null => continue,
            other => (0.0, other.to_string()),
        };
        parsed_rows.push(ParsedRow {
            marker_name: row.marker_name,
            value,
            original_value: original_value_str,
            unit: row.unit.unwrap_or_default(),
            specimen: row.specimen,
        });
    }

    // Tier 1: catalog search (Jaro-Winkler) on all markers
    let mut tier1: std::collections::HashMap<String, (String, f64)> = std::collections::HashMap::new();
    for row in &parsed_rows {
        let candidates = catalog.search_lab(&row.marker_name, 1, row.specimen.as_deref());
        if let Some(best) = candidates.first() {
            if best.confidence >= 0.85 {
                tracing::debug!(
                    "Tier 1 match: '{}' -> {} '{}' at {:.0}%",
                    row.marker_name, best.loinc_code, best.canonical_name, best.confidence * 100.0
                );
                tier1.insert(row.marker_name.clone(), (best.loinc_code.clone(), best.confidence));
            }
        }
    }
    tracing::info!("Tier 1 (catalog): {}/{} matched", tier1.len(), parsed_rows.len());

    // Tier 2: LLM with tool calling on ALL markers (parallel with date extraction)
    let all_as_unresolved: Vec<UnresolvedMarker> = parsed_rows.iter()
        .map(|r| UnresolvedMarker {
            marker_name: r.marker_name.clone(),
            value: r.original_value.clone(),
            unit: r.unit.clone(),
            reason: String::new(),
            specimen: r.specimen.clone(),
        })
        .collect();

    let resolve_future = llm_resolve_markers(&client, &config, &catalog, all_as_unresolved);
    let date_future = llm_extract_test_date(&client, &config, raw_text);

    let ((llm_resolved, _llm_unresolved, resolve_log), (test_date, date_log)) =
        tokio::join!(resolve_future, date_future);

    let resolve_turns = resolve_log.as_ref()
        .and_then(|l| l.turns)
        .unwrap_or(1);

    if let Some(entry) = resolve_log {
        llm_log.push(entry);
    }
    if let Some(entry) = date_log {
        llm_log.push(entry);
    }

    // Build tier 2 lookup: marker_name -> (loinc_code, confidence)
    let mut tier2: std::collections::HashMap<String, (String, f64)> = std::collections::HashMap::new();
    for obs in &llm_resolved {
        tier2.insert(obs.marker_name.clone(), (obs.loinc_code.clone(), obs.confidence));
    }
    tracing::info!("Tier 2 (LLM): {}/{} matched", tier2.len(), parsed_rows.len());

    // Merge: tier 2 wins on conflict, "both" when they agree
    let mut observations = Vec::new();
    let mut unresolved = Vec::new();

    for row in parsed_rows {
        let t1 = tier1.get(&row.marker_name);
        let t2 = tier2.get(&row.marker_name);

        let (loinc_code, confidence, match_source) = match (t1, t2) {
            (Some((c1, conf1)), Some((c2, conf2))) => {
                if c1 == c2 {
                    // Both agree - strongest signal
                    (c1.clone(), conf1.max(*conf2), "both")
                } else {
                    // Disagree - prefer LLM (it can reason about context)
                    tracing::info!(
                        "Tier conflict for '{}': catalog={} vs LLM={}, using LLM",
                        row.marker_name, c1, c2
                    );
                    (c2.clone(), *conf2, "llm")
                }
            }
            (None, Some((c2, conf2))) => (c2.clone(), *conf2, "llm"),
            (Some((c1, conf1)), None) => (c1.clone(), *conf1, "catalog"),
            (None, None) => {
                unresolved.push(UnresolvedMarker {
                    marker_name: row.marker_name,
                    value: row.original_value,
                    unit: row.unit,
                    reason: "No match from catalog or LLM".to_string(),
                    specimen: row.specimen,
                });
                continue;
            }
        };

        observations.push(ExtractedObservation {
            marker_name: row.marker_name,
            loinc_code,
            value: row.value,
            original_value: row.original_value.clone(),
            unit: row.unit.clone(),
            canonical_unit: row.unit,
            canonical_value: row.value,
            confidence,
            detection_limit: None,
            specimen: row.specimen,
            match_source: Some(match_source.to_string()),
        });
    }

    // Auto-dedup unit variants (same LOINC, same marker name, different units)
    let observations = dedup_unit_variants(&catalog, observations);

    Ok((ExtractionResult {
        observations,
        unresolved,
        model_used: config.ollama.model.clone(),
        agent_turns: resolve_turns,
        test_date,
    }, llm_log))
}

/// Auto-deduplicate unit-variant observations.
///
/// Singapore lab reports often list the same measurement in both SI and conventional units.
/// When we detect this pattern (same LOINC code, same marker name, different units), we pick
/// the best observation using a tiered strategy:
///
/// 1. Original unit matches LOINC canonical (zero conversion error)
/// 2. Already converted to canonical by normalization pipeline (canonical_unit matches)
/// 3. Simple scale conversion possible (same mass prefix, different volume: ng/mL -> ng/dL)
///
/// Among observations at the same tier, prefer more significant figures (higher precision).
///
/// Safety checks before attempting dedup:
/// - All entries in the group have the same marker_name (case-insensitive)
/// - All entries have different units (after normalization)
fn dedup_unit_variants(
    catalog: &LoincCatalog,
    observations: Vec<ExtractedObservation>,
) -> Vec<ExtractedObservation> {
    use crate::ingest::units;
    use std::collections::{HashMap, HashSet};

    let mut remove: HashSet<usize> = HashSet::new();

    let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, obs) in observations.iter().enumerate() {
        groups.entry(obs.loinc_code.clone()).or_default().push(i);
    }

    for (loinc_code, indices) in &groups {
        if indices.len() < 2 {
            continue;
        }

        // Safety check 1: all marker names must match (case-insensitive)
        let first_name = observations[indices[0]].marker_name.to_lowercase();
        if !indices.iter().all(|&i| observations[i].marker_name.to_lowercase() == first_name) {
            continue;
        }

        // Safety check 2: all units must be different (after normalization)
        let normalized_units: Vec<String> = indices.iter()
            .map(|&i| units::normalize_unit(&observations[i].unit))
            .collect();
        let unique_units: HashSet<&str> = normalized_units.iter().map(|s| s.as_str()).collect();
        if unique_units.len() < indices.len() {
            continue; // Some share a unit - genuine duplicate, leave for human review
        }

        let loinc_entry = match catalog.get_by_code(loinc_code) {
            Some(entry) => entry,
            None => continue,
        };
        let canonical_unit = strip_ucum_annotation(&units::normalize_unit(&loinc_entry.example_ucum_units));

        // Score each observation: (tier, sig_figs) - lower tier is better, higher sig_figs is better
        // Tier 1: original unit already matches LOINC canonical (no conversion needed at commit)
        // Tier 2: scale-convertible to canonical (same mass prefix, different volume)
        let mut candidates: Vec<(usize, u8, usize)> = Vec::new();

        for (pos, &idx) in indices.iter().enumerate() {
            let obs = &observations[idx];
            let sig_figs = normalize::significant_figures(&obs.original_value);
            let obs_unit = strip_ucum_annotation(&normalized_units[pos]);

            if obs_unit == canonical_unit {
                candidates.push((idx, 1, sig_figs));
            } else if try_scale_convert(
                obs.value, &obs.original_value, &obs_unit, &canonical_unit,
            ).is_some() {
                candidates.push((idx, 2, sig_figs));
            }
        }

        if candidates.is_empty() {
            continue; // No observation can reach canonical - leave for human review
        }

        // Pick best: lowest tier, then highest sig_figs
        candidates.sort_by(|a, b| a.1.cmp(&b.1).then(b.2.cmp(&a.2)));
        let keep_idx = candidates[0].0;
        let keep_tier = candidates[0].1;

        let tier_label = match keep_tier {
            1 => "exact",
            2 => "scale-convertible",
            _ => "unknown",
        };

        for &idx in indices {
            if idx != keep_idx {
                tracing::info!(
                    "Auto-dedup: {} '{}' - keeping {} {} ({tier_label}), dropping {} {}",
                    loinc_code,
                    observations[keep_idx].marker_name,
                    observations[keep_idx].value, observations[keep_idx].unit,
                    observations[idx].value, observations[idx].unit,
                );
                remove.insert(idx);
            }
        }
    }

    if !remove.is_empty() {
        tracing::info!("Auto-dedup removed {} unit-variant duplicates", remove.len());
    }

    observations.into_iter()
        .enumerate()
        .filter(|(i, _)| !remove.contains(i))
        .map(|(_, obs)| obs)
        .collect()
}

/// Strip UCUM annotations like `{creat}` from unit strings.
/// e.g., `mg/mmol{creat}` -> `mg/mmol`
fn strip_ucum_annotation(unit: &str) -> String {
    if let Some(pos) = unit.find('{') {
        unit[..pos].to_string()
    } else {
        unit.to_string()
    }
}

/// Try a simple scale conversion between units that share the same mass prefix
/// but differ in volume denominator (e.g., ng/mL -> ng/dL, g/L -> g/dL).
/// Returns (converted_value, precision) preserving significant figures.
fn try_scale_convert(
    value: f64,
    original_value_str: &str,
    from_unit: &str,
    to_unit: &str,
) -> Option<(f64, i32)> {
    let (from_mass, from_vol) = from_unit.split_once('/')?;
    let (to_mass, to_vol) = to_unit.split_once('/')?;

    // Mass prefix must match
    if from_mass.to_lowercase() != to_mass.to_lowercase() {
        return None;
    }

    let from_liters = volume_in_liters(from_vol)?;
    let to_liters = volume_in_liters(to_vol)?;

    let factor = to_liters / from_liters;
    let converted = value * factor;

    let sig_figs = normalize::significant_figures(original_value_str);
    let rounded = normalize::round_to_sig_figs(converted, sig_figs);

    Some((rounded, normalize::derive_precision(&format!("{rounded}"))))
}

/// Convert a volume unit string to its value in liters.
fn volume_in_liters(vol: &str) -> Option<f64> {
    match vol {
        "L" => Some(1.0),
        "dL" => Some(0.1),
        "mL" => Some(0.001),
        _ => None,
    }
}

/// Execute a search_loinc tool call against the LOINC catalog.
/// Uses word-overlap text search (not Jaro-Winkler) for better multi-word matching.
fn execute_search_loinc(
    catalog: &LoincCatalog,
    args: &serde_json::Value,
) -> serde_json::Value {
    let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
    let specimen = args.get("specimen").and_then(|v| v.as_str());
    let max_results = args.get("max_results").and_then(|v| v.as_u64()).unwrap_or(10).min(25) as usize;

    let candidates = catalog.text_search_lab(query, max_results, specimen);
    let results: Vec<serde_json::Value> = candidates.iter().map(|c| {
        let entry = catalog.get_by_code(&c.loinc_code);
        serde_json::json!({
            "loinc_code": c.loinc_code,
            "name": c.canonical_name,
            "specimen_system": entry.map(|e| e.system.as_str()).unwrap_or(""),
            "units": entry.map(|e| e.example_ucum_units.as_str()).unwrap_or(""),
            "confidence": c.confidence,
            "match_type": c.match_type.to_string(),
        })
    }).collect();
    serde_json::json!({ "candidates": results })
}

/// The search_loinc tool definition sent to Ollama.
fn search_loinc_tool_def() -> serde_json::Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": "search_loinc",
            "description": "Search the LOINC catalog for lab test codes matching a biomarker name. Returns candidates ranked by confidence. Use this to find the correct LOINC code for each unresolved marker.",
            "parameters": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Biomarker name or search term (e.g., 'HDL Cholesterol', 'Sodium', 'HbA1c')"
                    },
                    "specimen": {
                        "type": "string",
                        "description": "Specimen type to filter results. Omit if unknown.",
                        "enum": ["serum", "plasma", "blood", "urine"]
                    },
                    "max_results": {
                        "type": "number",
                        "description": "Maximum number of candidates to return (default: 10, max: 25)"
                    }
                },
                "required": ["query"]
            }
        }
    })
}

/// Use the LLM with tool calling to map unresolved marker names to LOINC codes.
/// The LLM can search the LOINC catalog interactively before answering.
async fn llm_resolve_markers(
    client: &reqwest::Client,
    config: &HermesConfig,
    catalog: &LoincCatalog,
    unresolved: Vec<UnresolvedMarker>,
) -> (Vec<ExtractedObservation>, Vec<UnresolvedMarker>, Option<crate::agent::LlmLogEntry>) {
    use crate::agent::{ConversationMessage, ToolCallRecord};

    let unresolved_list: Vec<String> = unresolved.iter()
        .enumerate()
        .map(|(i, u)| format!("{}. {} (value: {}, unit: {}{})", i + 1, u.marker_name, u.value, u.unit,
            u.specimen.as_ref().map(|s| format!(", specimen: {}", s)).unwrap_or_default()))
        .collect();

    let system_msg = r#"You are a clinical laboratory informatics specialist. Your task is to map unresolved biomarker names from lab reports to LOINC codes using the search_loinc tool.

## Tool: search_loinc

Searches a LOINC catalog of ~59,000 lab tests. Returns candidates ranked by relevance.

Parameters:
- query (required): search term - use full biomarker names, not abbreviations
- specimen (optional): "serum", "plasma", "blood", or "urine" - filters results to that specimen type
- max_results (optional): number of results to return (default 10, max 25)

The tool uses word matching, so search for the words that describe the test. Abbreviations will NOT match - always expand them to full names.

## How to search effectively

1. ALWAYS expand abbreviations to full clinical names before searching:
   - MCV -> search "Mean corpuscular volume"
   - MCH -> search "Mean corpuscular hemoglobin"
   - MCHC -> search "Mean corpuscular hemoglobin concentration"
   - RBC -> search "Erythrocytes"
   - WBC -> search "Leukocytes"
   - RDW -> search "RDW" or "Erythrocyte distribution width" (NOT reticulocyte)
   - MPV -> search "Mean platelet volume"
   - ESR -> search "Erythrocyte sedimentation rate"
   - ALT/SGPT -> search "Alanine aminotransferase"
   - AST/SGOT -> search "Aspartate aminotransferase"
   - GGT -> search "Gamma glutamyl transferase"
   - ALP -> search "Alkaline phosphatase"
   - BUN -> search "Urea nitrogen"
   - LDL -> search "Cholesterol in LDL"
   - HDL -> search "Cholesterol in HDL"
   - T.Chol -> search "Cholesterol"
   - T.Chol/HDL Ratio -> search "Cholesterol.total/Cholesterol.in HDL"
   - TG -> search "Triglyceride"
   - TG/HDL -> search "Triglyceride/Cholesterol.in HDL"
   - HbA1c -> search "Hemoglobin A1c"
   - eAG -> search "Estimated average glucose"
   - TSH -> search "Thyrotropin"
   - PSA -> search "Prostate specific antigen"
   - CRP -> search "C reactive protein"
   - eGFR -> search "Glomerular filtration rate"
   - A/G -> search "Albumin/Globulin"
   - TIBC -> search "Iron binding capacity"
   - UIBC -> search "Unsaturated iron binding capacity"
   - T3 -> search "Triiodothyronine"
   - T4 -> search "Thyroxine"
   - FT3 -> search "Triiodothyronine Free"
   - FT4 -> search "Thyroxine Free"
   - PT -> search "Prothrombin time"
   - INR -> search "INR"
   - aPTT -> search "Activated partial thromboplastin time"
   - Total Protein -> search "Protein" (LOINC uses "Protein" without "Total")
   - Iron Saturation -> search "Iron saturation"
   - Haematocrit/Hematocrit -> search "Hematocrit"
   - CEA -> search "Carcinoembryonic antigen"
   - AFP -> search "Alpha-1-Fetoprotein"
   - VD/VDRL/Syphilis -> search "Treponema pallidum"
   - HBsAg -> search "Hepatitis B virus surface antigen"
   - HBsAb -> search "Hepatitis B virus surface antibody"
   - HBeAg -> search "Hepatitis B virus e antigen"
   - HBeAb -> search "Hepatitis B virus e antibody"
   - HBcAb/Anti-HBc -> search "Hepatitis B virus core antibody"
   - Anti-HAV -> search "Hepatitis A virus antibody"
   - Anti-HCV -> search "Hepatitis C virus antibody"
   - Anti-HIV -> search "HIV 1+2 antibody"
   - Specific Gravity -> search "Specific gravity"

2. Use the specimen parameter when you know the specimen type. This is critical - the same test measured in serum vs urine has a different LOINC code.

3. NEVER repeat a query that returned no results. Try different words instead.

4. The search returns multiple candidates. Pick the best one by checking:
   - Does the name match what the lab report is measuring?
   - Does the specimen system match (e.g. "Ser" for serum, "Bld" for blood)?
   - Prefer simpler/standard entries over specialized ones (e.g. prefer "Sodium [Moles/volume] in Serum or Plasma" over "Sodium [Moles/volume] (Maximum value during study)")

5. IMPORTANT - Urinalysis dipstick vs microscopy: Lab reports often list BOTH for the same analyte.
   They are DIFFERENT tests with DIFFERENT LOINC codes. Distinguish them by the value:
   - Dipstick (qualitative): values like "Negative", "Trace", "1+", "2+", "3+", "Positive", "Small", "Moderate", "Large"
     -> Use LOINC codes with "by Test strip" in the name
   - Microscopy (quantitative): numeric values with units like "cells/uL", "/HPF", "cells/HPF"
     -> Use standard LOINC codes WITHOUT "by Test strip"
   Each marker MUST get its own unique LOINC code. Two markers must never share the same code.

## Examples

Example 1 - abbreviation:
  Marker: "MCV" (specimen: blood)
  -> search_loinc(query="Mean corpuscular volume", specimen="blood")
  -> Pick "787-2 Mean corpuscular volume [Entitic volume] in Red Blood Cells by Automated count"

Example 2 - reordered name:
  Marker: "LDL Cholesterol" (specimen: serum)
  -> search_loinc(query="Cholesterol in LDL", specimen="serum")
  -> Pick "2089-1 Cholesterol in LDL [Mass/volume] in Serum or Plasma"

Example 3 - calculated value:
  Marker: "eAG" (specimen: blood)
  -> search_loinc(query="Estimated average glucose", specimen="blood")
  -> Pick "27353-2 Glucose mean value [Mass/volume] in Blood Estimated from glycated hemoglobin"

Example 4 - ratio marker:
  Marker: "T.Chol/HDL Ratio" (specimen: serum)
  -> search_loinc(query="Cholesterol.total/Cholesterol.in HDL", specimen="serum")
  -> Pick "32309-7 Cholesterol.total/Cholesterol in HDL [Molar ratio] in Serum or Plasma"

Example 5 - drop the word "Total" for simple tests:
  Marker: "Total Protein" (specimen: serum)
  -> search_loinc(query="Protein", specimen="serum")
  -> Pick "2885-2 Protein [Mass/volume] in Serum or Plasma"

Example 6 - dipstick vs microscopy (different LOINC codes for the same analyte):
  Marker: "Urine Leukocytes" (value: Trace) -> dipstick result (qualitative value)
  -> search_loinc(query="Leukocytes", specimen="urine")
  -> Pick the "by Test strip" entry: "20408-1 Leukocytes [#/volume] in Urine by Test strip"
  Marker: "Urine White Blood Cells" (value: 3, unit: cells/uL) -> microscopy count (numeric value)
  -> search_loinc(query="Leukocytes", specimen="urine")
  -> Pick the plain entry: "30405-5 Leukocytes [#/volume] in Urine"
  Similarly for erythrocytes: dipstick -> 20409-9 (by Test strip), microscopy -> 30391-7 (plain)

Example 7 - no match after trying alternatives:
  Marker: "Specimen Adequacy"
  -> search_loinc(query="Specimen adequacy") -> no results
  -> This is not a quantitative lab test. Set to_loinc to null.

Example 8 - serology test (ordinal/qualitative):
  Marker: "Hepatitis Bs Antigen" (value: Non-reactive, specimen: serum)
  -> search_loinc(query="Hepatitis B virus surface antigen", specimen="serum")
  -> Pick "5195-3 Hepatitis B virus surface Ag [Presence] in Serum"
  NOTE: Serology tests are qualitative (Reactive/Non-reactive). This is correct.
  Hepatitis B surface antigen and Hepatitis B surface antibody are DIFFERENT tests
  with DIFFERENT LOINC codes - never map one to the other.

## Strict rules

- NEVER assign an unrelated LOINC code as a proxy or fallback. If no matching code exists, use null for to_loinc. It is better to leave a marker unresolved than to assign a wrong code.
- Each marker MUST map to a code that measures EXACTLY what the original marker measures. A Hepatitis B test must NOT be mapped to a Syphilis code, and vice versa.
- Every marker in the list must get its own unique LOINC code. Two different markers must never share the same code.

## Output

When done searching, return your final answer as JSON (no markdown fences):
{"mappings": [{"from": "original marker name", "to_loinc": "LOINC code or null", "confidence": 0.0-1.0, "reasoning": "brief explanation"}]}

Confidence guide:
- 0.95: search returned a clear match
- 0.85: good match, but name or specimen not exact
- 0.70: plausible but uncertain
- Use null for to_loinc if no reasonable match exists"#;

    let user_msg = format!(
        "Resolve these unresolved biomarkers from a lab report:\n\n{}\n\nSearch the LOINC catalog for each one using search_loinc, then return your final mappings as JSON.",
        unresolved_list.join("\n")
    );

    // Build conversation
    let mut messages = vec![
        serde_json::json!({"role": "system", "content": system_msg}),
        serde_json::json!({"role": "user", "content": user_msg}),
    ];
    let mut conversation_log: Vec<ConversationMessage> = vec![
        ConversationMessage { role: "system".into(), content: system_msg.to_string(), tool_calls: None, thinking: None },
        ConversationMessage { role: "user".into(), content: user_msg.clone(), tool_calls: None, thinking: None },
    ];

    let tools = vec![search_loinc_tool_def()];
    let max_turns = config.extraction.resolve_max_turns;
    let mut turn = 0u32;
    let mut total_tool_calls = 0u32;
    let mut final_content = String::new();
    let mut query_cache: std::collections::HashMap<String, serde_json::Value> = std::collections::HashMap::new();

    loop {
        if turn >= max_turns {
            tracing::warn!("Resolve loop hit max turns ({}), forcing final answer", max_turns);
            let force_msg = "You have reached the maximum number of tool calls. Please provide your final answer now as the JSON mappings.";
            messages.push(serde_json::json!({"role": "user", "content": force_msg}));
            conversation_log.push(ConversationMessage {
                role: "user".into(), content: force_msg.to_string(), tool_calls: None, thinking: None,
            });
        }

        let request_body = serde_json::json!({
            "model": config.ollama.model,
            "messages": messages,
            "tools": if turn >= max_turns { serde_json::json!([]) } else { serde_json::json!(tools) },
            "stream": false,
            "options": {
                "temperature": config.ollama.temperature,
                "num_predict": config.ollama.num_predict,
                "num_ctx": config.ollama.num_ctx
            }
        });

        let response = match client
            .post(format!("{}/api/chat", config.ollama.url))
            .json(&request_body)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("LLM resolve request failed on turn {}: {e}", turn);
                break;
            }
        };

        let body: serde_json::Value = match response.json().await {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!("Failed to parse LLM resolve response on turn {}: {e}", turn);
                break;
            }
        };

        let msg = match body.get("message") {
            Some(m) => m,
            None => {
                tracing::warn!("No message in LLM resolve response on turn {}", turn);
                break;
            }
        };

        let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
        let thinking = msg.get("thinking").and_then(|v| v.as_str()).map(String::from);
        let tool_calls = msg.get("tool_calls").and_then(|v| v.as_array());

        // Add assistant message to conversation
        messages.push(msg.clone());

        if let Some(calls) = tool_calls {
            if !calls.is_empty() {
                let call_records: Vec<ToolCallRecord> = calls.iter().filter_map(|tc| {
                    let func = tc.get("function")?;
                    Some(ToolCallRecord {
                        name: func.get("name")?.as_str()?.to_string(),
                        arguments: func.get("arguments").cloned().unwrap_or(serde_json::Value::Null),
                    })
                }).collect();

                conversation_log.push(ConversationMessage {
                    role: "assistant".into(),
                    content: content.to_string(),
                    tool_calls: Some(call_records.clone()),
                    thinking,
                });

                // Execute each tool call (with dedup cache)
                for tc in &call_records {
                    total_tool_calls += 1;
                    let result = if tc.name == "search_loinc" {
                        let cache_key = tc.arguments.to_string();
                        if let Some(cached) = query_cache.get(&cache_key) {
                            let mut cached = cached.clone();
                            cached.as_object_mut().map(|o| o.insert(
                                "note".to_string(),
                                serde_json::json!("This is a cached result - you already searched for this exact query. Try a different search term instead (e.g. expand abbreviations to full names).")
                            ));
                            cached
                        } else {
                            let result = execute_search_loinc(catalog, &tc.arguments);
                            query_cache.insert(cache_key, result.clone());
                            result
                        }
                    } else {
                        serde_json::json!({"error": format!("Unknown tool: {}", tc.name)})
                    };

                    let result_str = serde_json::to_string(&result).unwrap_or_default();
                    tracing::debug!("Tool call: {}({}) -> {}", tc.name, tc.arguments, &result_str[..result_str.len().min(200)]);

                    messages.push(serde_json::json!({"role": "tool", "content": result_str}));
                    conversation_log.push(ConversationMessage {
                        role: "tool".into(),
                        content: result_str,
                        tool_calls: None,
                        thinking: None,
                    });
                }

                turn += 1;
                continue;
            }
        }

        // No tool calls - this is the final answer
        conversation_log.push(ConversationMessage {
            role: "assistant".into(),
            content: content.to_string(),
            tool_calls: None,
            thinking,
        });
        final_content = content.to_string();
        turn += 1;
        break;
    }

    // If the model stopped without producing parseable JSON, try a compact summary request.
    // This handles the case where the full conversation exceeded context limits and the
    // API call failed after tool-calling turns. We build a fresh short conversation with
    // just the search results summarized, asking for the final JSON.
    if parse_resolve_mappings(&final_content).is_empty() && total_tool_calls > 0 {
        tracing::info!("No parseable mappings after {} turns - sending compact summary request", turn);

        // Build a summary of all search results from the conversation log.
        // Each assistant turn may have multiple parallel tool calls, so we queue
        // queries and pair them positionally with the subsequent tool results.
        let mut search_summary = String::new();
        let mut pending_queries: std::collections::VecDeque<String> = std::collections::VecDeque::new();
        // Deduplicate: keep only the first result per query string, in insertion order
        let mut seen_queries: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut results_ordered: Vec<(String, String, String)> = Vec::new();
        for msg in &conversation_log {
            if let Some(ref calls) = msg.tool_calls {
                for tc in calls {
                    if tc.name == "search_loinc" {
                        pending_queries.push_back(
                            tc.arguments.get("query")
                                .and_then(|v| v.as_str())
                                .unwrap_or("").to_string()
                        );
                    }
                }
            }
            if msg.role == "tool" {
                let query = pending_queries.pop_front().unwrap_or_default();
                if query.is_empty() || seen_queries.contains(&query) { continue; }
                seen_queries.insert(query.clone());
                if let Ok(result) = serde_json::from_str::<serde_json::Value>(&msg.content) {
                    if let Some(candidates) = result.get("candidates").and_then(|v| v.as_array()) {
                        if let Some(top) = candidates.first() {
                            let code = top.get("loinc_code").and_then(|v| v.as_str()).unwrap_or("?").to_string();
                            let name = top.get("name").and_then(|v| v.as_str()).unwrap_or("?").to_string();
                            results_ordered.push((query, code, name));
                        } else {
                            results_ordered.push((query, "none".into(), "no results".into()));
                        }
                    }
                }
            }
        }
        for (query, code, name) in &results_ordered {
            if code == "none" {
                search_summary.push_str(&format!("- Query \"{}\": no results\n", query));
            } else {
                search_summary.push_str(&format!("- Query \"{}\": best match {} ({})\n", query, code, name));
            }
        }

        let compact_prompt = format!(
            "You searched the LOINC catalog for these markers:\n\n{}\n\nHere is a summary of your search results:\n{}\n\nNow provide the final JSON mappings for ALL markers above.\n\
            Return JSON: {{\"mappings\": [{{\"from\": \"original marker name\", \"to_loinc\": \"LOINC code or null\", \"confidence\": 0.0-1.0, \"reasoning\": \"brief\"}}]}}",
            unresolved_list.join("\n"),
            search_summary
        );

        let compact_messages = vec![
            serde_json::json!({"role": "user", "content": compact_prompt}),
        ];

        conversation_log.push(ConversationMessage {
            role: "user".into(), content: compact_prompt.clone(), tool_calls: None, thinking: None,
        });

        // Use a larger predict budget for the compact summary: the model must output
        // one JSON entry per marker and may spend tokens on thinking.
        let compact_predict = config.ollama.num_predict.max(16384);
        tracing::debug!("Compact summary ({} search results, {} chars): {}", results_ordered.len(), search_summary.len(), &search_summary[..search_summary.len().min(500)]);
        let request_body = serde_json::json!({
            "model": config.ollama.model,
            "messages": compact_messages,
            "stream": false,
            "format": "json",
            "options": {
                "temperature": config.ollama.temperature,
                "num_predict": compact_predict,
                "num_ctx": config.ollama.num_ctx
            }
        });

        if let Ok(response) = client.post(format!("{}/api/chat", config.ollama.url))
            .json(&request_body).send().await
        {
            if let Ok(body) = response.json::<serde_json::Value>().await {
                if let Some(content) = body.get("message")
                    .and_then(|m| m.get("content"))
                    .and_then(|v| v.as_str())
                {
                    conversation_log.push(ConversationMessage {
                        role: "assistant".into(),
                        content: content.to_string(),
                        tool_calls: None,
                        thinking: body.get("message").and_then(|m| m.get("thinking")).and_then(|v| v.as_str()).map(String::from),
                    });
                    final_content = content.to_string();
                    turn += 1;
                    tracing::info!("Compact summary request produced {} chars", content.len());
                }
            }
        }
    }

    tracing::info!("Resolve conversation: {} turns, {} tool calls", turn, total_tool_calls);

    let log_entry = build_resolve_log(conversation_log, total_tool_calls, turn);

    // Parse the final content for mappings
    let mappings = parse_resolve_mappings(&final_content);

    tracing::info!("LLM resolved {} marker mappings", mappings.len());

    let mut resolved = Vec::new();
    let mut still_unresolved = Vec::new();

    for u in unresolved {
        let mapping = mappings.iter().find(|m| {
            m.from.to_lowercase() == u.marker_name.to_lowercase()
        });

        let (loinc, conf) = mapping
            .map(|m| (m.to_loinc.as_deref().filter(|s| !s.is_empty() && *s != "null").map(String::from), m.confidence))
            .unwrap_or((None, 0.0));

        if let Some(loinc_code) = loinc {
            if catalog.get_by_code(&loinc_code).is_some() {
                let value: f64 = u.value.parse().unwrap_or(0.0);

                resolved.push(ExtractedObservation {
                    marker_name: u.marker_name,
                    loinc_code: loinc_code.to_string(),
                    value,
                    original_value: u.value,
                    unit: u.unit.clone(),
                    canonical_unit: u.unit,
                    canonical_value: value,
                    confidence: conf,
                    detection_limit: None,
                    specimen: u.specimen,
                    match_source: Some("llm".to_string()),
                });
                continue;
            }
        }

        still_unresolved.push(u);
    }

    tracing::info!("LLM resolution: {} resolved, {} still unresolved", resolved.len(), still_unresolved.len());
    (resolved, still_unresolved, Some(log_entry))
}

fn build_resolve_log(
    messages: Vec<crate::agent::ConversationMessage>,
    tool_calls_count: u32,
    turns: u32,
) -> crate::agent::LlmLogEntry {
    crate::agent::LlmLogEntry {
        step: "resolve_markers".to_string(),
        prompt: format!("(agentic resolve: {} turns, {} tool calls)", turns, tool_calls_count),
        response: format!("(see conversation below)"),
        messages: Some(messages),
        tool_calls_count: Some(tool_calls_count),
        turns: Some(turns),
    }
}

#[derive(serde::Deserialize)]
struct MappingResponse {
    mappings: Vec<Mapping>,
}
#[derive(serde::Deserialize)]
struct Mapping {
    from: String,
    to_loinc: Option<String>,
    #[serde(default = "default_llm_confidence")]
    confidence: f64,
}
fn default_llm_confidence() -> f64 { 0.85 }

/// Parse the LLM's final response into marker mappings.
fn parse_resolve_mappings(text: &str) -> Vec<Mapping> {
    // Strip markdown fences
    let cleaned = if text.trim().starts_with("```") {
        let first_nl = text.find('\n').unwrap_or(3);
        let inner = &text[first_nl..];
        inner.rfind("```").map(|p| &inner[..p]).unwrap_or(inner).trim()
    } else {
        text.trim()
    };

    // Try direct parse
    if let Ok(r) = serde_json::from_str::<MappingResponse>(cleaned) {
        return r.mappings;
    }

    // Try parsing as Value and extracting the mappings array
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(cleaned) {
        if let Some(arr) = v.get("mappings").and_then(|m| m.as_array()) {
            return arr.iter()
                .filter_map(|item| {
                    Some(Mapping {
                        from: item.get("from")?.as_str()?.to_string(),
                        to_loinc: item.get("to_loinc").and_then(|v| v.as_str().map(String::from)),
                        confidence: item.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.85),
                    })
                })
                .collect();
        }
    }

    // Try to find JSON object in text
    if let Some(start) = cleaned.find('{') {
        if let Some(end) = cleaned.rfind('}') {
            let json_str = &cleaned[start..=end];
            if let Ok(r) = serde_json::from_str::<MappingResponse>(json_str) {
                return r.mappings;
            }
        }
    }

    tracing::warn!("Could not parse LLM resolve response as mappings");
    vec![]
}

/// Extract the test/specimen collection date from the lab report via a dedicated LLM call.
/// Looks for collection date specifically, not report print date.
async fn llm_extract_test_date(
    client: &reqwest::Client,
    config: &HermesConfig,
    raw_text: &str,
) -> (Option<String>, Option<crate::agent::LlmLogEntry>) {
    // Only send the first 2000 chars - the date is usually near the top
    let text = if raw_text.len() > 2000 { &raw_text[..2000] } else { raw_text };

    let prompt = format!(
        "/nothink\nWhat date was the blood test or specimen collected? Look at all dates on this lab report and determine which one represents when the sample was taken from the patient.\nPriority: Date Collected > Specimen Date > Date Received (acceptable proxy - specimen is typically collected the same day it is received) > any other date that is NOT a report/print date.\nYou MUST return a date if any reasonable candidate exists. Only return null if there are truly no dates on the report at all.\nReturn JSON: {{\"test_date\": \"YYYY-MM-DD\", \"source_field\": \"the field name you found it in\", \"reasoning\": \"brief explanation of why you chose this date\"}}.\n\n{}",
        text
    );

    let response = match client
        .post(format!("{}/api/chat", config.ollama.url))
        .json(&serde_json::json!({
            "model": config.ollama.model,
            "messages": [
                {"role": "system", "content": "/nothink"},
                {"role": "user", "content": prompt}
            ],
            "stream": false,
            "format": "json",
            "think": false,
            "options": {
                "temperature": config.ollama.temperature,
                "num_predict": 256,
                "num_ctx": config.ollama.num_ctx
            }
        }))
        .send()
        .await {
            Ok(r) => r,
            Err(_) => return (None, None),
        };

    let body: serde_json::Value = match response.json().await {
        Ok(b) => b,
        Err(_) => return (None, None),
    };

    let content = match body.get("message").and_then(|m| m.get("content")).and_then(|v| v.as_str()) {
        Some(c) => c,
        None => return (None, None),
    };

    let log_entry = crate::agent::LlmLogEntry {
        step: "extract_date".to_string(),
        prompt: prompt.clone(),
        response: content.to_string(),
        messages: None,
        tool_calls_count: None,
        turns: None,
    };

    tracing::info!("Date extraction LLM response: {}", &content[..content.len().min(200)]);

    // Strip markdown fences
    let cleaned = if content.trim().starts_with("```") {
        let first_nl = content.find('\n').unwrap_or(3);
        let inner = &content[first_nl..];
        inner.rfind("```").map(|p| &inner[..p]).unwrap_or(inner).trim()
    } else {
        content.trim()
    };

    let parsed: serde_json::Value = match serde_json::from_str(cleaned) {
        Ok(v) => v,
        Err(_) => return (None, Some(log_entry)),
    };
    let date = match parsed.get("test_date").and_then(|v| v.as_str()) {
        Some(d) => d,
        None => return (None, Some(log_entry)),
    };
    let source = parsed.get("source_field").and_then(|v| v.as_str()).unwrap_or("unknown");
    let reasoning = parsed.get("reasoning").and_then(|v| v.as_str()).unwrap_or("");

    if date.is_empty() || date == "null" {
        tracing::info!("Test date not found in report. Reasoning: {}", reasoning);
        (None, Some(log_entry))
    } else {
        tracing::info!("Extracted test date: {} (from: {}, reasoning: {})", date, source, reasoning);
        (Some(date.to_string()), Some(log_entry))
    }
}

/// Parse the LLM's JSON response into lab result rows.
/// Handles various formats: direct array, object wrapping array, single object.
fn parse_extraction_response(text: &str) -> Result<Vec<LabResultRow>> {
    // Strip markdown code fences if present
    let trimmed = text.trim();
    let trimmed = if trimmed.starts_with("```") {
        let first_newline = trimmed.find('\n').unwrap_or(3);
        let inner = &trimmed[first_newline..];
        let inner = if let Some(pos) = inner.rfind("```") {
            &inner[..pos]
        } else {
            inner
        };
        inner.trim()
    } else {
        trimmed
    };

    // Try direct parse as array (no test_date in this format)
    if let Ok(rows) = serde_json::from_str::<Vec<LabResultRow>>(trimmed) {
        return Ok(rows);
    }

    // Try as object with a nested array
    if let Ok(obj) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if let Some(obj_map) = obj.as_object() {
            for (_key, value) in obj_map {
                if let Ok(rows) = serde_json::from_value::<Vec<LabResultRow>>(value.clone()) {
                    if !rows.is_empty() {
                        return Ok(rows);
                    }
                }
            }
        }
        // Try as single object
        if let Ok(row) = serde_json::from_value::<LabResultRow>(obj) {
            return Ok(vec![row]);
        }
    }

    // Try to find JSON array in the text
    if let Some(start) = trimmed.find('[') {
        if let Some(end) = trimmed.rfind(']') {
            let json_str = &trimmed[start..=end];
            if let Ok(rows) = serde_json::from_str::<Vec<LabResultRow>>(json_str) {
                return Ok(rows);
            }
        }
    }

    Err(HermesError::Agent(format!(
        "Could not parse extraction response as lab results. Response: {}",
        &trimmed[..trimmed.len().min(500)]
    )))
}
