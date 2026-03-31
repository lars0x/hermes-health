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
    pub value: f64,
    #[serde(default)]
    pub unit: Option<String>,
    pub reference_low: Option<f64>,
    pub reference_high: Option<f64>,
    pub flag: Option<String>,
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

    // Truncate to avoid context length issues
    let text = if raw_text.len() > 8000 { &raw_text[..8000] } else { raw_text };

    let prompt = format!(
        "/nothink\nExtract ALL biomarker results from this lab report.\nAlso extract the date the test/specimen was COLLECTED (not the date the report was printed or produced). Look for fields like \"Date Collected\", \"Specimen Date\", \"Collection Date\", or \"Date of Test\".\nReturn JSON: {{\"test_date\": \"YYYY-MM-DD\" or null, \"results\": [{{\"marker_name\": str, \"value\": number, \"unit\": str, \"reference_low\": number or null, \"reference_high\": number or null, \"flag\": \"H\" or \"L\" or null}}]}}\n\nLab report:\n{}",
        text
    );

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
                "temperature": 0.0,
                "num_predict": 8192,
                "num_ctx": 16384
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
    // Write raw response to a debug file for inspection
    let _ = std::fs::write("/tmp/hermes-raw-response.json", response_text);

    // Parse the JSON response - handle both array and object-with-array formats
    let (rows, test_date) = parse_extraction_response(response_text)?;

    tracing::info!("Parsed {} lab result rows, test_date={:?}", rows.len(), test_date);

    // Post-process: LOINC matching + unit conversion
    // First load all tracked biomarkers for alias matching
    let tracked = crate::db::queries::list_biomarkers(&pool, None).await.unwrap_or_default();

    let mut observations = Vec::new();
    let mut unresolved = Vec::new();

    for row in rows {
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
            // 2. Fall back to LOINC catalog search
            let candidates = catalog.search(&row.marker_name, 1);
            if let Some(best) = candidates.first() {
                if best.confidence >= 0.80 {
                    let bm = crate::db::queries::get_biomarker_by_loinc(&pool, &best.loinc_code)
                        .await
                        .ok()
                        .flatten();
                    (best.loinc_code.clone(), best.confidence, bm)
                } else {
                    // Unresolved
                    unresolved.push(UnresolvedMarker {
                        marker_name: row.marker_name,
                        value: row.value.to_string(),
                        unit: row.unit.clone().unwrap_or_default(),
                        reason: format!("Best LOINC match confidence {:.0}% is below threshold", best.confidence * 100.0),
                    });
                    continue;
                }
            } else {
                unresolved.push(UnresolvedMarker {
                    marker_name: row.marker_name,
                    value: row.value.to_string(),
                    unit: row.unit.clone().unwrap_or_default(),
                    reason: "No LOINC match found".to_string(),
                });
                continue;
            }
        };

        let original_str = row.value.to_string();
        let unit_str = row.unit.clone().unwrap_or_default();

        let (canonical_value, canonical_unit) = if let Some(ref b) = bm {
            match normalize::normalize_observation(
                &pool, b.id, &b.unit, &original_str, &unit_str,
            ).await {
                Ok(norm) => (norm.value, norm.canonical_unit),
                Err(_) => (row.value, unit_str.clone()),
            }
        } else {
            (row.value, unit_str.clone())
        };

        observations.push(ExtractedObservation {
            marker_name: row.marker_name,
            loinc_code,
            value: row.value,
            original_value: original_str,
            unit: unit_str,
            canonical_unit,
            canonical_value,
            reference_low: row.reference_low,
            reference_high: row.reference_high,
            flag: row.flag,
            confidence,
            detection_limit: None,
        });
    }

    Ok(ExtractionResult {
        observations,
        unresolved,
        model_used: config.ollama.model.clone(),
        agent_turns: 1,
        test_date,
    })
}

/// Parse the LLM's JSON response, returning (rows, test_date).
/// Handles various formats: direct array, object wrapping array, single object.
fn parse_extraction_response(text: &str) -> Result<(Vec<LabResultRow>, Option<String>)> {
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
        return Ok((rows, None));
    }

    // Try as object with a nested array + optional test_date
    if let Ok(obj) = serde_json::from_str::<serde_json::Value>(trimmed) {
        let test_date = obj
            .get("test_date")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        if let Some(obj_map) = obj.as_object() {
            for (_key, value) in obj_map {
                if let Ok(rows) = serde_json::from_value::<Vec<LabResultRow>>(value.clone()) {
                    if !rows.is_empty() {
                        return Ok((rows, test_date));
                    }
                }
            }
        }
        // Try as single object
        if let Ok(row) = serde_json::from_value::<LabResultRow>(obj) {
            return Ok((vec![row], test_date));
        }
    }

    // Try to find JSON array in the text
    if let Some(start) = trimmed.find('[') {
        if let Some(end) = trimmed.rfind(']') {
            let json_str = &trimmed[start..=end];
            if let Ok(rows) = serde_json::from_str::<Vec<LabResultRow>>(json_str) {
                return Ok((rows, None));
            }
        }
    }

    Err(HermesError::Agent(format!(
        "Could not parse extraction response as lab results. Response: {}",
        &trimmed[..trimmed.len().min(500)]
    )))
}
