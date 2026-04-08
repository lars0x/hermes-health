use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::db::queries;

pub struct ValidateRowTool {
    pool: SqlitePool,
}

impl ValidateRowTool {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, Deserialize)]
pub struct ValidateArgs {
    pub loinc_code: String,
    pub value: f64,
}

#[derive(Debug, Serialize)]
pub struct ValidationResult {
    pub valid: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
#[error("validation error: {0}")]
pub struct ValidateError(String);

impl Tool for ValidateRowTool {
    const NAME: &'static str = "validate_row";
    type Error = ValidateError;
    type Args = ValidateArgs;
    type Output = ValidationResult;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "validate_row".to_string(),
            description: "Sanity-check an extracted observation value against plausible physiological ranges.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "loinc_code": { "type": "string", "description": "LOINC code" },
                    "value": { "type": "number", "description": "Numeric value in canonical units" },
                },
                "required": ["loinc_code", "value"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let mut warnings = Vec::new();

        if let Ok(Some(bm)) = queries::get_biomarker_by_loinc(&self.pool, &args.loinc_code).await {
            if let Some(ref_high) = bm.reference_high {
                if args.value > ref_high * 10.0 {
                    warnings.push(format!("Value {} is implausibly high (>10x ref high {})", args.value, ref_high));
                }
            }
            if let Some(ref_low) = bm.reference_low {
                if ref_low > 0.0 && args.value < ref_low / 10.0 {
                    warnings.push(format!("Value {} is implausibly low (<1/10 ref low {})", args.value, ref_low));
                }
            }
        }

        if args.value < 0.0 {
            warnings.push("Negative value - verify this is correct".to_string());
        }

        Ok(ValidationResult {
            valid: warnings.is_empty(),
            warnings,
        })
    }
}
