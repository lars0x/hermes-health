pub mod extractor;
pub mod prompts;
pub mod tools;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionResult {
    pub observations: Vec<ExtractedObservation>,
    pub unresolved: Vec<UnresolvedMarker>,
    pub model_used: String,
    pub agent_turns: u32,
    /// Date the test/specimen was collected (extracted from report), YYYY-MM-DD
    #[serde(default)]
    pub test_date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedObservation {
    pub marker_name: String,
    pub loinc_code: String,
    pub value: f64,
    pub original_value: String,
    pub unit: String,
    pub canonical_unit: String,
    pub canonical_value: f64,
    pub confidence: f64,
    pub detection_limit: Option<String>,
    #[serde(default)]
    pub specimen: Option<String>,
    /// How this LOINC code was matched: "catalog", "llm", or "both"
    #[serde(default)]
    pub match_source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnresolvedMarker {
    pub marker_name: String,
    pub value: String,
    pub unit: String,
    pub reason: String,
    #[serde(default)]
    pub specimen: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmLogEntry {
    pub step: String,
    pub prompt: String,
    pub response: String,
    /// Full conversation for multi-turn tool-calling steps. None for single-shot calls.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub messages: Option<Vec<ConversationMessage>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turns: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMessage {
    pub role: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCallRecord>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRecord {
    pub name: String,
    pub arguments: serde_json::Value,
}

use std::sync::Arc;
use tokio::sync::Mutex;

use crate::config::HermesConfig;
use crate::error::{HermesError, Result};
use crate::services::loinc::LoincCatalog;
use sqlx::SqlitePool;

/// Run the extraction pipeline on raw text from a lab report.
pub async fn run_extraction(
    pool: SqlitePool,
    catalog: Arc<LoincCatalog>,
    config: Arc<HermesConfig>,
    raw_text: &str,
) -> Result<(ExtractionResult, Vec<LlmLogEntry>)> {
    match config.extraction.mode.as_str() {
        "direct" => extractor::run_direct_extraction(pool, catalog, config, raw_text).await,
        _ => {
            let result = run_agentic_extraction(pool, catalog, config, raw_text).await?;
            Ok((result, vec![]))  // Agentic mode doesn't collect logs yet
        }
    }
}

async fn run_agentic_extraction(
    pool: SqlitePool,
    catalog: Arc<LoincCatalog>,
    config: Arc<HermesConfig>,
    raw_text: &str,
) -> Result<ExtractionResult> {
    use rig::client::{CompletionClient, Nothing};
    use rig::completion::Prompt;
    use rig::providers::ollama;

    let client = ollama::Client::builder()
        .api_key(Nothing)
        .base_url(&config.ollama.url)
        .build()
        .map_err(|e| HermesError::Agent(format!("Failed to create Ollama client: {e}")))?;

    let result_slot: Arc<Mutex<Option<ExtractionResult>>> = Arc::new(Mutex::new(None));

    let loinc_tool = tools::loinc_lookup::LoincLookupTool::new(catalog.clone());
    let unit_tool = tools::unit_convert::UnitConvertTool::new(pool.clone(), catalog.clone());
    let validate_tool = tools::validate_row::ValidateRowTool::new(pool.clone());
    let submit_tool = tools::submit_results::SubmitResultsTool::new(result_slot.clone());
    let think_tool = tools::think::ThinkTool;

    let agent = client
        .agent(&config.ollama.model)
        .preamble(prompts::AGENT_PREAMBLE)
        .temperature(config.ollama.temperature)
        .default_max_turns(config.extraction.max_agent_turns as usize)
        .tool(loinc_tool)
        .tool(unit_tool)
        .tool(validate_tool)
        .tool(submit_tool)
        .tool(think_tool)
        .build();

    let prompt = format!(
        "Extract all biomarker results from this lab report:\n\n{}",
        raw_text
    );

    tracing::info!("Starting agentic extraction with model {}", config.ollama.model);

    let _response = agent
        .prompt(&prompt)
        .await
        .map_err(|e| HermesError::Agent(format!("Agent error: {e}")))?;

    let result = result_slot.lock().await.take();

    match result {
        Some(mut extraction) => {
            extraction.model_used = config.ollama.model.clone();
            Ok(extraction)
        }
        None => Err(HermesError::Agent(
            "Extraction agent completed without submitting results. \
             Try again or switch to direct extraction mode."
                .to_string(),
        )),
    }
}
