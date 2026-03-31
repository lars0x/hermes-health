use std::sync::Arc;

use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};

use crate::services::loinc::LoincCatalog;

pub struct LoincLookupTool {
    catalog: Arc<LoincCatalog>,
}

impl LoincLookupTool {
    pub fn new(catalog: Arc<LoincCatalog>) -> Self {
        Self { catalog }
    }
}

#[derive(Debug, Deserialize)]
pub struct LoincLookupArgs {
    pub marker_name: String,
}

#[derive(Debug, Serialize)]
pub struct LoincLookupResult {
    pub candidates: Vec<LoincMatch>,
}

#[derive(Debug, Serialize)]
pub struct LoincMatch {
    pub loinc_code: String,
    pub canonical_name: String,
    pub confidence: f64,
    pub match_type: String,
}

#[derive(Debug, thiserror::Error)]
#[error("LOINC lookup error: {0}")]
pub struct LookupError(String);

impl Tool for LoincLookupTool {
    const NAME: &'static str = "loinc_lookup";
    type Error = LookupError;
    type Args = LoincLookupArgs;
    type Output = LoincLookupResult;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "loinc_lookup".to_string(),
            description: "Resolve a biomarker name to LOINC code candidates. Returns up to 3 matches ranked by confidence.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "marker_name": {
                        "type": "string",
                        "description": "The biomarker name exactly as printed on the lab report"
                    }
                },
                "required": ["marker_name"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let results = self.catalog.search(&args.marker_name, 3);
        let candidates = results
            .into_iter()
            .map(|c| LoincMatch {
                loinc_code: c.loinc_code,
                canonical_name: c.canonical_name,
                confidence: c.confidence,
                match_type: c.match_type.to_string(),
            })
            .collect();
        Ok(LoincLookupResult { candidates })
    }
}
