use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct ThinkTool;

#[derive(Debug, Deserialize)]
pub struct ThinkArgs {
    pub thought: String,
}

#[derive(Debug, thiserror::Error)]
#[error("think error")]
pub struct ThinkError;

impl Tool for ThinkTool {
    const NAME: &'static str = "think";
    type Error = ThinkError;
    type Args = ThinkArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "think".to_string(),
            description: "Use this tool to reason about ambiguous report layouts, unclear marker names, or extraction strategy before committing to an extraction.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "thought": {
                        "type": "string",
                        "description": "Your reasoning about the report structure or extraction strategy"
                    }
                },
                "required": ["thought"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        tracing::debug!("Agent thinking: {}", args.thought);
        Ok("Thought recorded. Continue with extraction.".to_string())
    }
}
