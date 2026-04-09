use std::sync::Arc;

use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};

use crate::ingest::{normalize, units};
use crate::services::loinc::LoincCatalog;

pub struct UnitConvertTool {
    catalog: Arc<LoincCatalog>,
}

impl UnitConvertTool {
    pub fn new(catalog: Arc<LoincCatalog>) -> Self {
        Self { catalog }
    }
}

#[derive(Debug, Deserialize)]
pub struct UnitConvertArgs {
    pub loinc_code: String,
    pub value: f64,
    pub from_unit: String,
}

#[derive(Debug, Serialize)]
pub struct ConversionResult {
    pub canonical_unit: String,
    pub canonical_value: f64,
    pub conversion_applied: bool,
    pub precision: i32,
}

#[derive(Debug, thiserror::Error)]
#[error("unit conversion error: {0}")]
pub struct ConvertError(pub String);

impl Tool for UnitConvertTool {
    const NAME: &'static str = "unit_convert";
    type Error = ConvertError;
    type Args = UnitConvertArgs;
    type Output = ConversionResult;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "unit_convert".to_string(),
            description: "Look up the canonical LOINC unit for a biomarker. Returns the expected unit from the LOINC catalog.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "loinc_code": { "type": "string", "description": "LOINC code of the biomarker" },
                    "value": { "type": "number", "description": "The numeric value as reported" },
                    "from_unit": { "type": "string", "description": "The unit as reported on the lab report" }
                },
                "required": ["loinc_code", "value", "from_unit"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let entry = self.catalog.get_by_code(&args.loinc_code)
            .ok_or_else(|| ConvertError(format!("Unknown LOINC code: {}", args.loinc_code)))?;

        let canonical_unit = if entry.example_ucum_units.is_empty() {
            units::normalize_unit(&args.from_unit)
        } else {
            entry.example_ucum_units.clone()
        };

        let from_normalized = units::normalize_unit(&args.from_unit);
        let conversion_applied = from_normalized != units::normalize_unit(&canonical_unit);
        let precision = normalize::derive_precision(&args.value.to_string());

        Ok(ConversionResult {
            canonical_unit,
            canonical_value: args.value,
            conversion_applied,
            precision,
        })
    }
}
