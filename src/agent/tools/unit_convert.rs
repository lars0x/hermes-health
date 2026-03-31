use std::sync::Arc;

use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::db::queries;
use crate::ingest::{normalize, units};
use crate::services::loinc::LoincCatalog;

pub struct UnitConvertTool {
    pool: SqlitePool,
    catalog: Arc<LoincCatalog>,
}

impl UnitConvertTool {
    pub fn new(pool: SqlitePool, catalog: Arc<LoincCatalog>) -> Self {
        Self { pool, catalog }
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
            description: "Check and convert a unit for a given biomarker. Returns the canonical value and unit.".to_string(),
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
        let bm = queries::get_biomarker_by_loinc(&self.pool, &args.loinc_code)
            .await
            .map_err(|e| ConvertError(format!("DB error: {e}")))?;

        let bm = match bm {
            Some(b) => b,
            None => {
                if let Some(entry) = self.catalog.get_by_code(&args.loinc_code) {
                    let canon = if entry.example_ucum_units.is_empty() {
                        units::normalize_unit(&args.from_unit)
                    } else {
                        entry.example_ucum_units.clone()
                    };
                    let precision = normalize::derive_precision(&args.value.to_string());
                    return Ok(ConversionResult {
                        canonical_unit: canon,
                        canonical_value: args.value,
                        conversion_applied: false,
                        precision,
                    });
                }
                return Err(ConvertError(format!("Unknown LOINC code: {}", args.loinc_code)));
            }
        };

        let original_str = args.value.to_string();
        match normalize::normalize_observation(&self.pool, bm.id, &bm.unit, &original_str, &args.from_unit).await {
            Ok(norm) => Ok(ConversionResult {
                canonical_unit: norm.canonical_unit,
                canonical_value: norm.value,
                conversion_applied: norm.original_unit != bm.unit,
                precision: norm.precision,
            }),
            Err(e) => Err(ConvertError(format!("{e}"))),
        }
    }
}
