use std::sync::Arc;

use sqlx::SqlitePool;
use tokio::sync::mpsc;

use crate::agent;
use crate::config::HermesConfig;
use crate::db::queries;
use crate::services::loinc::LoincCatalog;

/// A request to extract biomarkers from an import
pub struct ExtractionJob {
    pub import_id: i64,
    pub report_id: i64,
    pub file_path: String,
    pub format: String,
}

/// Sender handle for submitting extraction jobs
pub type ExtractionSender = mpsc::UnboundedSender<ExtractionJob>;

/// Start the extraction worker. Returns a sender for submitting jobs.
/// The worker processes one job at a time, sequentially.
pub fn start_worker(
    pool: SqlitePool,
    catalog: Arc<LoincCatalog>,
    config: Arc<HermesConfig>,
) -> ExtractionSender {
    let (tx, mut rx) = mpsc::unbounded_channel::<ExtractionJob>();

    tokio::spawn(async move {
        tracing::info!("Extraction worker started");

        while let Some(job) = rx.recv().await {
            tracing::info!(
                "Processing extraction job: import_id={}, report_id={}, file={}",
                job.import_id, job.report_id, job.file_path
            );

            // Update status to extracting
            let _ = queries::update_import_status(&pool, job.import_id, "extracting").await;

            // Extract text from file
            let raw_text = if job.format == "pdf" {
                match extract_pdf_text(&job.file_path) {
                    Ok(text) => text,
                    Err(e) => {
                        let error_json = serde_json::json!({"error": format!("PDF extraction failed: {}", e)}).to_string();
                        let _ = queries::update_import_result(
                            &pool, job.import_id, "failed", Some(&error_json), 0, 0, 0, None,
                        ).await;
                        tracing::error!("Import {} PDF extraction failed: {}", job.import_id, e);
                        continue;
                    }
                }
            } else {
                match std::fs::read_to_string(&job.file_path) {
                    Ok(text) => text,
                    Err(e) => {
                        let error_json = serde_json::json!({"error": format!("File read failed: {}", e)}).to_string();
                        let _ = queries::update_import_result(
                            &pool, job.import_id, "failed", Some(&error_json), 0, 0, 0, None,
                        ).await;
                        continue;
                    }
                }
            };

            // Run extraction in a sub-task so panics don't kill the worker
            let p = pool.clone();
            let c = catalog.clone();
            let cf = config.clone();
            let import_id = job.import_id;

            let result = tokio::spawn(async move {
                agent::run_extraction(p, c, cf, &raw_text).await
            }).await;

            match result {
                Ok(Ok(extraction)) => {
                    let json = serde_json::to_string(&extraction).unwrap_or_default();
                    let _ = queries::update_import_result(
                        &pool, import_id, "extracted", Some(&json),
                        extraction.agent_turns as i64,
                        extraction.observations.len() as i64,
                        extraction.unresolved.len() as i64,
                        extraction.test_date.as_deref(),
                    ).await;
                    tracing::info!(
                        "Import {} complete: {} observations, {} unresolved",
                        import_id, extraction.observations.len(), extraction.unresolved.len()
                    );
                }
                Ok(Err(e)) => {
                    let error_json = serde_json::json!({"error": e.to_string()}).to_string();
                    let _ = queries::update_import_result(
                        &pool, import_id, "failed", Some(&error_json), 0, 0, 0, None,
                    ).await;
                    tracing::error!("Import {} extraction failed: {}", import_id, e);
                }
                Err(e) => {
                    let error_json = serde_json::json!({"error": format!("Extraction task crashed: {}", e)}).to_string();
                    let _ = queries::update_import_result(
                        &pool, import_id, "failed", Some(&error_json), 0, 0, 0, None,
                    ).await;
                    tracing::error!("Import {} extraction task panicked: {}", import_id, e);
                }
            }
        }

        tracing::info!("Extraction worker stopped");
    });

    tx
}

fn extract_pdf_text(path: &str) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    match pdf_extract::extract_text_from_mem(&bytes) {
        Ok(text) if !text.trim().is_empty() => return Ok(text),
        Ok(_) => tracing::warn!("pdf-extract returned empty, trying Python fallback"),
        Err(e) => tracing::warn!("pdf-extract failed: {e}, trying Python fallback"),
    }

    let output = std::process::Command::new("python3")
        .args(["-c", &format!(
            "import pypdf; r = pypdf.PdfReader('{}'); print('\\n'.join(p.extract_text() for p in r.pages))",
            path.replace('\'', "\\'")
        )])
        .output()
        .map_err(|e| format!("Python fallback failed: {e}"))?;

    if output.status.success() {
        let text = String::from_utf8_lossy(&output.stdout).to_string();
        if text.trim().is_empty() {
            Err("PDF text extraction returned empty result".to_string())
        } else {
            Ok(text)
        }
    } else {
        Err(format!("Python extraction failed: {}", String::from_utf8_lossy(&output.stderr)))
    }
}
