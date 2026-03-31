use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::agent::{ExtractedObservation, ExtractionResult, UnresolvedMarker};
use crate::config::HermesConfig;
use crate::error::{HermesError, Result};
use crate::ingest::normalize;
use crate::services::loinc::LoincCatalog;

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct LabResultRow {
    /// Biomarker name as printed on the report
    pub marker_name: String,
    /// Numeric value
    pub value: f64,
    /// Unit of measurement
    pub unit: String,
    /// Reference range lower bound (if shown)
    pub reference_low: Option<f64>,
    /// Reference range upper bound (if shown)
    pub reference_high: Option<f64>,
    /// Flag: H for high, L for low, or null
    pub flag: Option<String>,
}

pub async fn run_direct_extraction(
    pool: SqlitePool,
    catalog: Arc<LoincCatalog>,
    config: Arc<HermesConfig>,
    raw_text: &str,
) -> Result<ExtractionResult> {
    use rig::client::{CompletionClient, Nothing};
    use rig::providers::ollama;

    let client = ollama::Client::builder()
        .api_key(Nothing)
        .base_url(&config.ollama.url)
        .build()
        .map_err(|e| HermesError::Agent(format!("Failed to create Ollama client: {e}")))?;

    let extractor = client
        .extractor::<Vec<LabResultRow>>(&config.ollama.model)
        .preamble(crate::agent::prompts::EXTRACTOR_PREAMBLE)
        .build();

    tracing::info!("Running direct extraction with model {}", config.ollama.model);

    let rows = extractor
        .extract(raw_text)
        .await
        .map_err(|e| HermesError::Agent(format!("Extractor error: {e}")))?;

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

                let (canonical_value, canonical_unit, precision) = if let Some(ref bm) = bm {
                    match normalize::normalize_observation(
                        &pool, bm.id, &bm.unit, &original_str, &row.unit,
                    ).await {
                        Ok(norm) => (norm.value, norm.canonical_unit, norm.precision),
                        Err(_) => {
                            let prec = normalize::derive_precision(&original_str);
                            (row.value, row.unit.clone(), prec)
                        }
                    }
                } else {
                    let prec = normalize::derive_precision(&original_str);
                    (row.value, row.unit.clone(), prec)
                };

                observations.push(ExtractedObservation {
                    marker_name: row.marker_name,
                    loinc_code: best.loinc_code.clone(),
                    value: row.value,
                    original_value: original_str,
                    unit: row.unit,
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
            unit: row.unit,
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
