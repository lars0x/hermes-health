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
}

/// Run direct extraction via raw Ollama API call.
/// More reliable than Rig's Extractor for structured JSON output.
pub async fn run_direct_extraction(
    pool: SqlitePool,
    catalog: Arc<LoincCatalog>,
    config: Arc<HermesConfig>,
    raw_text: &str,
) -> Result<ExtractionResult> {
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
        "/nothink\nExtract ALL biomarker results from this lab report. The report may be in any language - extract the marker names in English where possible, but preserve the original name if unsure.\nReturn JSON: {{\"results\": [{{\"marker_name\": str, \"value\": number, \"unit\": str}}]}}\n\nLab report:\n{}",
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

    tracing::info!("Ollama returned {} chars, first 200: {:?}", response_text.len(), &response_text[..response_text.len().min(200)]);
    let _ = std::fs::write("/tmp/hermes-raw-response.json", response_text);

    // Parse the JSON response - handle both array and object-with-array formats
    let rows = parse_extraction_response(response_text)?;

    tracing::info!("Parsed {} lab result rows", rows.len());

    // Post-process: LOINC matching + unit conversion
    // First load all tracked biomarkers for alias matching
    let tracked = crate::db::queries::list_biomarkers(&pool, None).await.unwrap_or_default();

    let mut observations = Vec::new();
    let mut unresolved = Vec::new();

    for row in rows {
        // Extract numeric value, or store qualitative result (e.g., "Negative") as 0.0 with the text in original_value
        let (value, original_value_str, is_qualitative) = match &row.value {
            serde_json::Value::Number(n) => {
                let v = n.as_f64().unwrap_or(0.0);
                (v, v.to_string(), false)
            }
            serde_json::Value::String(s) => {
                // Qualitative: "Negative", "Positive", "Reactive", etc.
                (0.0, s.clone(), true)
            }
            serde_json::Value::Null => continue, // truly empty - skip
            other => (0.0, other.to_string(), true),
        };
        let marker_lower = row.marker_name.to_lowercase();

        // 1. Check tracked biomarkers by name/alias first (highest priority)
        let tracked_match = tracked.iter().find(|bm| {
            bm.name.to_lowercase() == marker_lower
                || bm.loinc_code.to_lowercase() == marker_lower
                || bm.aliases_vec().iter().any(|a| a.to_lowercase() == marker_lower)
        });

        let (loinc_code, confidence, bm) = if let Some(bm) = tracked_match {
            (bm.loinc_code.clone(), 1.0_f64, Some(bm.clone()))
        } else {
            // 2. Fuzzy match against tracked biomarkers only (not the full 59K LOINC catalog)
            let fuzzy_threshold = 0.85;
            let mut best_score = 0.0_f64;
            let mut best_bm: Option<&crate::db::models::Biomarker> = None;

            for bm in &tracked {
                let sim_name = strsim::jaro_winkler(&marker_lower, &bm.name.to_lowercase());
                let sim_aliases = bm.aliases_vec().iter()
                    .map(|a| strsim::jaro_winkler(&marker_lower, &a.to_lowercase()))
                    .fold(0.0_f64, f64::max);
                let best_sim = sim_name.max(sim_aliases);
                if best_sim > best_score {
                    best_score = best_sim;
                    best_bm = Some(bm);
                }
            }

            if best_score >= fuzzy_threshold {
                let bm = best_bm.unwrap();
                (bm.loinc_code.clone(), best_score, Some(bm.clone()))
            } else {
                unresolved.push(UnresolvedMarker {
                    marker_name: row.marker_name,
                    value: original_value_str.clone(),
                    unit: row.unit.clone().unwrap_or_default(),
                    reason: if best_score > 0.0 {
                        format!("Best tracked biomarker match {:.0}% below threshold", best_score * 100.0)
                    } else {
                        "No tracked biomarker match".to_string()
                    },
                });
                continue;
            }
        };

        let unit_str = row.unit.clone().unwrap_or_default();

        // Skip unit conversion for qualitative results
        let (canonical_value, canonical_unit) = if is_qualitative {
            (value, unit_str.clone())
        } else if let Some(ref b) = bm {
            match normalize::normalize_observation(
                &pool, b.id, &b.unit, &original_value_str, &unit_str,
            ).await {
                Ok(norm) => (norm.value, norm.canonical_unit),
                Err(_) => (value, unit_str.clone()),
            }
        } else {
            (value, unit_str.clone())
        };

        observations.push(ExtractedObservation {
            marker_name: row.marker_name,
            loinc_code,
            value,
            original_value: original_value_str,
            unit: unit_str,
            canonical_unit,
            canonical_value,
            confidence,
            detection_limit: None,
        });
    }

    // Second pass: run LLM resolution and date extraction in parallel
    let resolve_future = async {
        if !unresolved.is_empty() && !tracked.is_empty() {
            llm_resolve_markers(&client, &config, &tracked, &pool, unresolved).await
        } else {
            (vec![], unresolved)
        }
    };

    let date_future = llm_extract_test_date(&client, &config, raw_text);

    let ((resolved, still_unresolved), test_date) =
        tokio::join!(resolve_future, date_future);

    observations.extend(resolved);

    Ok(ExtractionResult {
        observations,
        unresolved: still_unresolved,
        model_used: config.ollama.model.clone(),
        agent_turns: 1,
        test_date,
    })
}

/// Use the LLM to map unresolved marker names to tracked biomarkers.
/// Sends a single batch request: "which of these known biomarkers does each unresolved name correspond to?"
async fn llm_resolve_markers(
    client: &reqwest::Client,
    config: &HermesConfig,
    tracked: &[crate::db::models::Biomarker],
    pool: &SqlitePool,
    unresolved: Vec<UnresolvedMarker>,
) -> (Vec<ExtractedObservation>, Vec<UnresolvedMarker>) {
    // Build the list of known biomarker names for the prompt
    let known_list: Vec<String> = tracked
        .iter()
        .map(|b| format!("{} ({})", b.name, b.loinc_code))
        .collect();

    let unresolved_names: Vec<&str> = unresolved.iter().map(|u| u.marker_name.as_str()).collect();

    let prompt = format!(
        "/nothink\nI have these biomarker names from a lab report that I could not automatically match:\n{}\n\nHere are the known biomarkers in my system:\n{}\n\nFor each unresolved name, tell me which known biomarker it maps to (if any). Rate your confidence from 0.0 to 1.0 (1.0 = certain match like \"Red Cell Count\" -> RBC, 0.5 = plausible but not sure, 0.0 = no match). Return JSON:\n{{\"mappings\": [{{\"from\": \"lab report name\", \"to_loinc\": \"LOINC code or null if no match\", \"confidence\": 0.0-1.0}}]}}",
        unresolved_names.join(", "),
        known_list.join("\n")
    );

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
                "num_predict": 2048,
                "num_ctx": config.ollama.num_ctx
            }
        }))
        .send()
        .await;

    let response = match response {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("LLM marker resolution request failed: {e}");
            return (vec![], unresolved);
        }
    };

    let body: serde_json::Value = match response.json().await {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!("Failed to parse LLM resolution response: {e}");
            return (vec![], unresolved);
        }
    };

    let response_text = body
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // Strip markdown fences
    let cleaned = if response_text.trim().starts_with("```") {
        let first_nl = response_text.find('\n').unwrap_or(3);
        let inner = &response_text[first_nl..];
        inner.rfind("```").map(|p| &inner[..p]).unwrap_or(inner).trim()
    } else {
        response_text.trim()
    };

    // Parse mappings
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

    let mappings: Vec<Mapping> = match serde_json::from_str::<MappingResponse>(cleaned) {
        Ok(r) => r.mappings,
        Err(_) => {
            // Try parsing as Value and extracting
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(cleaned) {
                if let Some(arr) = v.get("mappings").and_then(|m| m.as_array()) {
                    arr.iter()
                        .filter_map(|item| {
                            Some(Mapping {
                                from: item.get("from")?.as_str()?.to_string(),
                                to_loinc: item.get("to_loinc").and_then(|v| v.as_str().map(String::from)),
                                confidence: item.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.85),
                            })
                        })
                        .collect()
                } else {
                    vec![]
                }
            } else {
                tracing::warn!("Could not parse LLM resolution response");
                return (vec![], unresolved);
            }
        }
    };

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
            // Find the tracked biomarker
            if let Some(bm) = tracked.iter().find(|b| b.loinc_code == loinc_code) {
                let value: f64 = u.value.parse().unwrap_or(0.0);
                let original_str = u.value.clone();

                let (canonical_value, canonical_unit) = match normalize::normalize_observation(
                    pool, bm.id, &bm.unit, &original_str, &u.unit,
                ).await {
                    Ok(norm) => (norm.value, norm.canonical_unit),
                    Err(_) => (value, u.unit.clone()),
                };

                resolved.push(ExtractedObservation {
                    marker_name: u.marker_name,
                    loinc_code: loinc_code.to_string(),
                    value,
                    original_value: original_str,
                    unit: u.unit,
                    canonical_unit,
                    canonical_value,
                    confidence: conf,
                    detection_limit: None,
                });
                continue;
            }
        }

        still_unresolved.push(u);
    }

    tracing::info!("LLM resolution: {} resolved, {} still unresolved", resolved.len(), still_unresolved.len());
    (resolved, still_unresolved)
}

/// Extract the test/specimen collection date from the lab report via a dedicated LLM call.
/// Looks for collection date specifically, not report print date.
async fn llm_extract_test_date(
    client: &reqwest::Client,
    config: &HermesConfig,
    raw_text: &str,
) -> Option<String> {
    // Only send the first 2000 chars - the date is usually near the top
    let text = if raw_text.len() > 2000 { &raw_text[..2000] } else { raw_text };

    let prompt = format!(
        "/nothink\nWhat date was the blood test or specimen collected? Look at all dates on this lab report and determine which one represents when the sample was taken from the patient.\nPriority: Date Collected > Specimen Date > Date Received (acceptable proxy - specimen is typically collected the same day it is received) > any other date that is NOT a report/print date.\nYou MUST return a date if any reasonable candidate exists. Only return null if there are truly no dates on the report at all.\nReturn JSON: {{\"test_date\": \"YYYY-MM-DD\", \"source_field\": \"the field name you found it in\", \"reasoning\": \"brief explanation of why you chose this date\"}}.\n\n{}",
        text
    );

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
                "num_predict": 256,
                "num_ctx": config.ollama.num_ctx
            }
        }))
        .send()
        .await
        .ok()?;

    let body: serde_json::Value = response.json().await.ok()?;
    let content = body.get("message")?.get("content")?.as_str()?;

    // Debug: log the raw response
    let _ = std::fs::write("/tmp/hermes-date-response.json", content);
    tracing::info!("Date extraction LLM response: {}", &content[..content.len().min(200)]);

    // Strip markdown fences
    let cleaned = if content.trim().starts_with("```") {
        let first_nl = content.find('\n').unwrap_or(3);
        let inner = &content[first_nl..];
        inner.rfind("```").map(|p| &inner[..p]).unwrap_or(inner).trim()
    } else {
        content.trim()
    };

    let parsed: serde_json::Value = serde_json::from_str(cleaned).ok()?;
    let date = parsed.get("test_date")?.as_str()?;
    let source = parsed.get("source_field").and_then(|v| v.as_str()).unwrap_or("unknown");
    let reasoning = parsed.get("reasoning").and_then(|v| v.as_str()).unwrap_or("");

    if date.is_empty() || date == "null" {
        tracing::info!("Test date not found in report. Reasoning: {}", reasoning);
        None
    } else {
        tracing::info!("Extracted test date: {} (from: {}, reasoning: {})", date, source, reasoning);
        Some(date.to_string())
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
