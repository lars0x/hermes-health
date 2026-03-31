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
        "/nothink\nExtract ALL biomarker results from this lab report.\nReturn JSON: {{\"results\": [{{\"marker_name\": str, \"value\": number, \"unit\": str, \"reference_low\": number or null, \"reference_high\": number or null, \"flag\": \"H\" or \"L\" or null}}]}}\n\nLab report:\n{}",
        text
    );

    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/api/chat", config.ollama.url))
        .json(&serde_json::json!({
            "model": config.ollama.model,
            "messages": [
                {"role": "user", "content": prompt}
            ],
            "stream": false,
            "format": "json",
            "options": {
                "temperature": 0.0,
                "num_predict": 8192
            },
            "think": false
        }))
        .timeout(std::time::Duration::from_secs(config.ollama.timeout_seconds))
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
    let rows: Vec<LabResultRow> = parse_extraction_response(response_text)?;

    tracing::info!("Parsed {} lab result rows", rows.len());

    // Post-process: LOINC matching + unit conversion
    let mut observations = Vec::new();
    let mut unresolved = Vec::new();

    for row in rows {
        let candidates = catalog.search(&row.marker_name, 1);
        if let Some(best) = candidates.first() {
            if best.confidence >= 0.85 {
                let original_str = row.value.to_string();
                let bm = crate::db::queries::get_biomarker_by_loinc(&pool, &best.loinc_code)
                    .await
                    .ok()
                    .flatten();

                let (canonical_value, canonical_unit) = if let Some(ref bm) = bm {
                    match normalize::normalize_observation(
                        &pool, bm.id, &bm.unit, &original_str, &row.unit.clone().unwrap_or_default(),
                    ).await {
                        Ok(norm) => (norm.value, norm.canonical_unit),
                        Err(_) => (row.value, row.unit.clone().unwrap_or_default()),
                    }
                } else {
                    (row.value, row.unit.clone().unwrap_or_default())
                };

                observations.push(ExtractedObservation {
                    marker_name: row.marker_name,
                    loinc_code: best.loinc_code.clone(),
                    value: row.value,
                    original_value: original_str,
                    unit: row.unit.clone().unwrap_or_default(),
                    canonical_unit,
                    canonical_value,
                    reference_low: row.reference_low,
                    reference_high: row.reference_high,
                    flag: row.flag,
                    confidence: best.confidence,
                    detection_limit: None,
                });
                continue;
            }
        }

        unresolved.push(UnresolvedMarker {
            marker_name: row.marker_name,
            value: row.value.to_string(),
            unit: row.unit.clone().unwrap_or_default(),
            reason: "No high-confidence LOINC match found".to_string(),
        });
    }

    Ok(ExtractionResult {
        observations,
        unresolved,
        model_used: config.ollama.model.clone(),
        agent_turns: 1,
    })
}

/// Parse the LLM's JSON response, handling various formats:
/// - Direct array: [{...}, {...}]
/// - Object wrapping array: {"results": [{...}]} or {"data": [{...}]}
/// - Single object: {...} -> wrapped in array
fn parse_extraction_response(text: &str) -> Result<Vec<LabResultRow>> {
    // Strip markdown code fences if present (LLM often wraps JSON in ```json ... ```)
    let trimmed = text.trim();
    tracing::debug!("Parsing response: {} chars, starts_with_fence={}, first_chars={:?}",
        trimmed.len(), trimmed.starts_with("```"), &trimmed[..trimmed.len().min(30)]);
    let trimmed = if trimmed.starts_with("```") {
        // Find the end of the first line (```json or ```)
        let first_newline = trimmed.find('\n').unwrap_or(3);
        let inner = &trimmed[first_newline..];
        // Strip trailing fence
        let inner = if let Some(pos) = inner.rfind("```") {
            &inner[..pos]
        } else {
            inner
        };
        inner.trim()
    } else {
        trimmed
    };

    // Try direct parse as array
    if let Ok(rows) = serde_json::from_str::<Vec<LabResultRow>>(trimmed) {
        return Ok(rows);
    }

    // Try as object with a nested array
    if let Ok(obj) = serde_json::from_str::<serde_json::Value>(trimmed) {
        // Look for any array field
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

    // Try to find JSON array in the text (LLM might have added text before/after)
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
