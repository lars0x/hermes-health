use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::Html;
use axum::Json;
use axum_extra::extract::Multipart;
use sha2::{Digest, Sha256};

use crate::agent::{self, ExtractionResult};
use crate::db::queries;
use crate::db::models::NewObservation;
use crate::error::HermesError;
use crate::services::observation;
use crate::web::htmx;
use crate::web::AppState;

/// GET /import - render the import page
pub async fn import_page(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Html<String>, HermesError> {
    let is_htmx = htmx::is_htmx_request(&headers);
    let reports = queries::list_reports(&state.pool).await.unwrap_or_default();
    let ctx = minijinja::context! {
        is_fragment => is_htmx,
        current_path => "/import",
        reports => reports,
    };
    state.templates.render("pages/import.html", ctx).map(Html)
}

/// POST /api/v1/reports/upload - multipart file upload
pub async fn upload(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, HermesError> {
    let mut file_bytes = Vec::new();
    let mut filename = String::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| HermesError::Validation(format!("Multipart error: {e}")))?
    {
        if field.name() == Some("file") {
            filename = field
                .file_name()
                .unwrap_or("unknown")
                .to_string();
            file_bytes = field
                .bytes()
                .await
                .map_err(|e| HermesError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?
                .to_vec();
        }
    }

    if file_bytes.is_empty() {
        return Err(HermesError::Validation("No file uploaded".to_string()));
    }

    // Compute SHA-256
    let mut hasher = Sha256::new();
    hasher.update(&file_bytes);
    let hash = format!("{:x}", hasher.finalize());

    // Check for duplicates
    if let Some(_existing) = queries::get_report_by_hash(&state.pool, &hash).await? {
        return Err(HermesError::Duplicate(format!(
            "File '{}' has already been uploaded",
            filename
        )));
    }

    // Determine format
    let format = if filename.to_lowercase().ends_with(".pdf") {
        "pdf"
    } else if filename.to_lowercase().ends_with(".csv") {
        "csv"
    } else {
        return Err(HermesError::Validation(
            "Only PDF and CSV files are supported".to_string(),
        ));
    };

    // Store file
    let reports_dir = std::path::Path::new("data/reports");
    std::fs::create_dir_all(reports_dir)?;
    let file_path = reports_dir.join(format!("{}.{}", hash, format));
    std::fs::write(&file_path, &file_bytes)?;

    // Create report record
    let report_id = queries::insert_report(
        &state.pool,
        &filename,
        &hash,
        &file_path.to_string_lossy(),
        format,
    )
    .await?;

    Ok(Json(serde_json::json!({
        "report_id": report_id,
        "filename": filename,
        "format": format,
        "status": "pending"
    })))
}

/// POST /api/v1/reports/{id}/extract - trigger background extraction
pub async fn trigger_extraction(
    Path(id): Path<i64>,
    State(state): State<AppState>,
) -> Result<Html<String>, HermesError> {
    // Get the report
    let report = queries::get_report_by_id(&state.pool, id).await?;

    if report.extraction_status != "pending" && report.extraction_status != "failed" {
        return Ok(Html(format!(
            r#"<div class="alert alert-error">Report is already {}</div>"#,
            report.extraction_status
        )));
    }

    // Update status to extracting
    queries::update_report_status(&state.pool, id, "extracting", None).await?;

    // Extract text
    let raw_text = if report.format == "pdf" {
        extract_pdf_text(&report.file_path)?
    } else {
        std::fs::read_to_string(&report.file_path)?
    };

    // Spawn background extraction task
    let pool = state.pool.clone();
    let catalog = state.catalog.clone();
    let config = state.config.clone();

    tokio::spawn(async move {
        tracing::info!("Starting extraction for report {}", id);
        match agent::run_extraction(pool.clone(), catalog, config, &raw_text).await {
            Ok(result) => {
                let json = serde_json::to_string(&result).unwrap_or_default();
                let _ = queries::update_report_extraction(
                    &pool,
                    id,
                    "extracted",
                    Some(&json),
                    Some(&result.model_used),
                    result.agent_turns as i64,
                    result.observations.len() as i64,
                    result.unresolved.len() as i64,
                )
                .await;
                tracing::info!(
                    "Extraction complete for report {}: {} observations, {} unresolved",
                    id,
                    result.observations.len(),
                    result.unresolved.len()
                );
            }
            Err(e) => {
                let error_json = serde_json::json!({"error": e.to_string()}).to_string();
                let _ = queries::update_report_status(&pool, id, "failed", Some(&error_json)).await;
                tracing::error!("Extraction failed for report {}: {}", id, e);
            }
        }
    });

    Ok(Html(
        r#"<div id="extraction-status" hx-get="/api/v1/reports/STATUS_ID/status" hx-trigger="every 2s" hx-swap="innerHTML">
        <div class="alert" style="background: var(--info-bg); color: var(--info-text);">Extraction in progress...</div>
        </div>"#
            .replace("STATUS_ID", &id.to_string()),
    ))
}

/// GET /api/v1/reports/{id}/status - poll extraction status
pub async fn extraction_status(
    Path(id): Path<i64>,
    State(state): State<AppState>,
) -> Result<Html<String>, HermesError> {
    let report = queries::get_report_by_id(&state.pool, id).await?;

    match report.extraction_status.as_str() {
        "extracting" => Ok(Html(format!(
            r#"<div id="extraction-status" hx-get="/api/v1/reports/{}/status" hx-trigger="every 2s" hx-swap="innerHTML">
            <div class="alert" style="background: var(--info-bg); color: var(--info-text);">Extraction in progress...</div>
            </div>"#,
            id
        ))),
        "extracted" => {
            // Load the extraction result and render the review table
            let extraction = get_extraction_result(&report)?;
            let ctx = minijinja::context! {
                report => report,
                extraction => extraction,
                report_id => id,
            };
            state.templates.render("components/review_table.html", ctx).map(Html)
        }
        "failed" => {
            let error_msg = report
                .raw_extraction
                .as_deref()
                .and_then(|j| serde_json::from_str::<serde_json::Value>(j).ok())
                .and_then(|v| v.get("error").and_then(|e| e.as_str().map(String::from)))
                .unwrap_or("Unknown error".to_string());
            Ok(Html(format!(
                r##"<div class="alert alert-error">Extraction failed: {}</div>
                <button class="btn-primary" style="max-width:200px;margin-top:8px;"
                        hx-post="/api/v1/reports/{}/extract"
                        hx-target="#extraction-status"
                        hx-swap="innerHTML">Retry</button>"##,
                error_msg, id
            )))
        }
        _ => Ok(Html(format!(
            r#"<div class="alert">Status: {}</div>"#,
            report.extraction_status
        ))),
    }
}

/// GET /api/v1/reports/{id}/extraction - get extraction results as JSON
pub async fn get_extraction_json(
    Path(id): Path<i64>,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, HermesError> {
    let report = queries::get_report_by_id(&state.pool, id).await?;
    let extraction = get_extraction_result(&report)?;
    Ok(Json(serde_json::to_value(extraction)?))
}

/// POST /api/v1/reports/{id}/commit - commit selected observations
pub async fn commit(
    Path(id): Path<i64>,
    State(state): State<AppState>,
    axum::Form(form): axum::Form<CommitForm>,
) -> Result<Html<String>, HermesError> {
    let report = queries::get_report_by_id(&state.pool, id).await?;
    let extraction = get_extraction_result(&report)?;

    let selected: Vec<usize> = form
        .selected
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();

    let mut committed = 0;
    let mut errors = Vec::new();

    for idx in &selected {
        if let Some(obs) = extraction.observations.get(*idx) {
            let new_obs = NewObservation {
                biomarker: obs.loinc_code.clone(),
                value: obs.canonical_value,
                unit: obs.canonical_unit.clone(),
                observed_at: chrono::Local::now().format("%Y-%m-%d").to_string(),
                lab_name: None,
                fasting: None,
                notes: None,
            };
            match observation::add_observation(&state.pool, &state.catalog, &new_obs).await {
                Ok(_) => committed += 1,
                Err(e) => errors.push(format!("{}: {}", obs.marker_name, e)),
            }
        }
    }

    // Update report status
    let _ = queries::update_report_status(&state.pool, id, "committed", report.raw_extraction.as_deref())
        .await;

    let mut msg = format!("Committed {} observations.", committed);
    if !errors.is_empty() {
        msg.push_str(&format!(" {} errors: {}", errors.len(), errors.join("; ")));
    }

    Ok(Html(format!(
        r#"<div class="alert alert-success">{}</div>
        <a href="/" style="font-size:13px;">Back to dashboard</a>"#,
        msg
    )))
}

/// POST /api/v1/reports/{id}/map - manual LOINC mapping (alias learning)
pub async fn map_marker(
    Path(id): Path<i64>,
    State(state): State<AppState>,
    axum::Form(form): axum::Form<MapForm>,
) -> Result<Html<String>, HermesError> {
    // Look up the biomarker
    if let Some(bm) = queries::get_biomarker_by_loinc(&state.pool, &form.loinc_code).await? {
        // Add the marker name as a new alias
        let mut aliases = bm.aliases_vec();
        if !aliases.iter().any(|a| a.to_lowercase() == form.marker_name.to_lowercase()) {
            aliases.push(form.marker_name.clone());
            queries::update_biomarker_aliases(&state.pool, bm.id, &aliases).await?;
            tracing::info!("Learned alias '{}' for {} ({})", form.marker_name, bm.name, bm.loinc_code);
        }
        Ok(Html(format!(
            r#"<span class="pill pill-supplement">{} mapped</span>"#,
            form.marker_name
        )))
    } else {
        Ok(Html(format!(
            r#"<span class="text-red">LOINC code {} not found</span>"#,
            form.loinc_code
        )))
    }
}

#[derive(serde::Deserialize)]
pub struct CommitForm {
    pub selected: String,
}

#[derive(serde::Deserialize)]
pub struct MapForm {
    pub marker_name: String,
    pub loinc_code: String,
}

// Helpers

fn extract_pdf_text(path: &str) -> Result<String, HermesError> {
    // Try Rust pdf-extract first
    let bytes = std::fs::read(path)?;
    match pdf_extract::extract_text_from_mem(&bytes) {
        Ok(text) if !text.trim().is_empty() => return Ok(text),
        Ok(_) => tracing::warn!("pdf-extract returned empty text, trying Python fallback"),
        Err(e) => tracing::warn!("pdf-extract failed: {e}, trying Python fallback"),
    }

    // Fallback: use Python pypdf (handles encrypted PDFs common in Singapore lab reports)
    let output = std::process::Command::new("python3")
        .args(["-c", &format!(
            "import pypdf; r = pypdf.PdfReader('{}'); print('\\n'.join(p.extract_text() for p in r.pages))",
            path.replace('\'', "\\'")
        )])
        .output()
        .map_err(|e| HermesError::Pdf(format!("Failed to run Python PDF extractor: {e}")))?;

    if output.status.success() {
        let text = String::from_utf8_lossy(&output.stdout).to_string();
        if text.trim().is_empty() {
            return Err(HermesError::Pdf("PDF text extraction returned empty result".to_string()));
        }
        tracing::info!("Extracted {} chars from PDF via Python fallback", text.len());
        Ok(text)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(HermesError::Pdf(format!("Python PDF extraction failed: {stderr}")))
    }
}

fn get_extraction_result(
    report: &crate::db::models::Report,
) -> Result<ExtractionResult, HermesError> {
    let json = report
        .raw_extraction
        .as_deref()
        .ok_or_else(|| HermesError::NotFound("No extraction data".to_string()))?;
    serde_json::from_str(json).map_err(|e| HermesError::Json(e))
}
