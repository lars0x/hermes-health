use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};

pub struct ValidateRowTool;

#[derive(Debug, Deserialize)]
pub struct ValidateArgs {
    #[allow(dead_code)]
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
            description: "Sanity-check an extracted observation value for basic plausibility.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "loinc_code": { "type": "string", "description": "LOINC code" },
                    "value": { "type": "number", "description": "Numeric value" },
                },
                "required": ["loinc_code", "value"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let mut warnings = Vec::new();

        if args.value < 0.0 {
            warnings.push("Negative value - verify this is correct".to_string());
        }

        Ok(ValidationResult {
            valid: warnings.is_empty(),
            warnings,
        })
    }
}
