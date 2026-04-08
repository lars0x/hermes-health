use std::sync::Arc;
use tokio::sync::Mutex;

use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::Deserialize;

use crate::agent::{ExtractedObservation, ExtractionResult, UnresolvedMarker};

pub struct SubmitResultsTool {
    result_slot: Arc<Mutex<Option<ExtractionResult>>>,
}

impl SubmitResultsTool {
    pub fn new(result_slot: Arc<Mutex<Option<ExtractionResult>>>) -> Self {
        Self { result_slot }
    }
}

#[derive(Debug, Deserialize)]
pub struct SubmitResultsArgs {
    pub observations: Vec<SubmitObservation>,
    #[serde(default)]
    pub unresolved: Vec<SubmitUnresolved>,
}

#[derive(Debug, Deserialize)]
pub struct SubmitObservation {
    pub marker_name: String,
    pub loinc_code: String,
    pub value: f64,
    #[serde(default)]
    pub original_value: String,
    pub unit: String,
    #[serde(default)]
    pub canonical_unit: String,
    #[serde(default)]
    pub canonical_value: f64,
    #[serde(default = "default_confidence")]
    pub confidence: f64,
    pub detection_limit: Option<String>,
}

fn default_confidence() -> f64 {
    0.9
}

#[derive(Debug, Deserialize)]
pub struct SubmitUnresolved {
    pub marker_name: String,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub unit: String,
    #[serde(default)]
    pub reason: String,
}

#[derive(Debug, thiserror::Error)]
#[error("submit error: {0}")]
pub struct SubmitError(String);

impl Tool for SubmitResultsTool {
    const NAME: &'static str = "submit_results";
    type Error = SubmitError;
    type Args = SubmitResultsArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "submit_results".to_string(),
            description: "Submit the complete extraction results for human review. Call this ONCE after processing ALL markers. Include both resolved observations and any unresolved markers.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "observations": {
                        "type": "array",
                        "description": "Extracted observations ready for review",
                        "items": {
                            "type": "object",
                            "properties": {
                                "marker_name": { "type": "string", "description": "Marker name as printed" },
                                "loinc_code": { "type": "string", "description": "Resolved LOINC code" },
                                "value": { "type": "number", "description": "Numeric value as reported" },
                                "original_value": { "type": "string", "description": "Value as printed (preserving precision)" },
                                "unit": { "type": "string", "description": "Unit as reported" },
                                "canonical_unit": { "type": "string", "description": "Canonical unit after conversion" },
                                "canonical_value": { "type": "number", "description": "Value in canonical units" },
                                "confidence": { "type": "number", "description": "LOINC match confidence 0-1" },
                                "detection_limit": { "type": "string", "description": "< or > if applicable" }
                            },
                            "required": ["marker_name", "loinc_code", "value", "unit"]
                        }
                    },
                    "unresolved": {
                        "type": "array",
                        "description": "Markers that could not be resolved",
                        "items": {
                            "type": "object",
                            "properties": {
                                "marker_name": { "type": "string" },
                                "value": { "type": "string" },
                                "unit": { "type": "string" },
                                "reason": { "type": "string" }
                            },
                            "required": ["marker_name"]
                        }
                    }
                },
                "required": ["observations"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let observations: Vec<ExtractedObservation> = args
            .observations
            .into_iter()
            .map(|o| {
                let canonical_value = if o.canonical_value != 0.0 { o.canonical_value } else { o.value };
                let canonical_unit = if o.canonical_unit.is_empty() { o.unit.clone() } else { o.canonical_unit };
                let original_value = if o.original_value.is_empty() { o.value.to_string() } else { o.original_value };
                ExtractedObservation {
                    marker_name: o.marker_name,
                    loinc_code: o.loinc_code,
                    value: o.value,
                    original_value,
                    unit: o.unit,
                    canonical_unit,
                    canonical_value,
                    confidence: o.confidence,
                    detection_limit: o.detection_limit,
                    specimen: None,
                }
            })
            .collect();

        let unresolved: Vec<UnresolvedMarker> = args
            .unresolved
            .into_iter()
            .map(|u| UnresolvedMarker {
                marker_name: u.marker_name,
                value: u.value,
                unit: u.unit,
                reason: u.reason,
                specimen: None,
            })
            .collect();

        let count = observations.len();
        let result = ExtractionResult {
            observations,
            unresolved,
            model_used: String::new(),
            agent_turns: 0,
            test_date: None,
        };

        let mut slot = self.result_slot.lock().await;
        *slot = Some(result);

        tracing::info!("Agent submitted {} extraction results", count);
        Ok(format!("Submitted {} observations for review.", count))
    }
}
