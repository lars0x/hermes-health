use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse};
use axum::Json;
use axum_extra::extract::Multipart;
use sha2::{Digest, Sha256};

use crate::agent::ExtractionResult;
use crate::db::models::NewObservation;
use crate::db::queries;
use crate::error::HermesError;
use crate::services::observation;
use crate::web::htmx;
use crate::web::AppState;

// --- Import list page ---

pub async fn import_page(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Html<String>, HermesError> {
    let is_htmx = htmx::is_htmx_request(&headers);
    let imports = queries::list_imports(&state.pool).await.unwrap_or_default();

    // Enrich imports with report filenames
    let mut import_rows = Vec::new();
    for imp in &imports {
        let report = queries::get_report_by_id(&state.pool, imp.report_id).await.ok();
        import_rows.push(minijinja::context! {
            id => imp.id,
            filename => report.as_ref().map(|r| r.filename.clone()).unwrap_or_default(),
            format => report.as_ref().map(|r| r.format.clone()).unwrap_or_default(),
            model_used => imp.model_used,
            status => imp.status,
            extracted_count => imp.extracted_count,
            unresolved_count => imp.unresolved_count,
            created_at => imp.created_at,
        });
    }

    let ctx = minijinja::context! {
        is_fragment => is_htmx,
        current_path => "/import",
        imports => import_rows,
    };
    state.templates.render("pages/import.html", ctx).map(Html)
}

pub async fn imports_list(
    State(state): State<AppState>,
) -> Result<Html<String>, HermesError> {
    let imports = queries::list_imports(&state.pool).await.unwrap_or_default();
    let mut import_rows = Vec::new();
    for imp in &imports {
        let report = queries::get_report_by_id(&state.pool, imp.report_id).await.ok();
        import_rows.push(minijinja::context! {
            id => imp.id,
            filename => report.as_ref().map(|r| r.filename.clone()).unwrap_or_default(),
            format => report.as_ref().map(|r| r.format.clone()).unwrap_or_default(),
            model_used => imp.model_used,
            status => imp.status,
            extracted_count => imp.extracted_count,
            unresolved_count => imp.unresolved_count,
            created_at => imp.created_at,
        });
    }
    let ctx = minijinja::context! { imports => import_rows };
    state.templates.render("components/imports_list.html", ctx).map(Html)
}

// --- Upload ---

pub async fn upload(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<axum::response::Response, HermesError> {
    let mut file_bytes = Vec::new();
    let mut filename = String::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| HermesError::Validation(format!("Multipart error: {e}")))?
    {
        if field.name() == Some("file") {
            filename = field.file_name().unwrap_or("unknown").to_string();
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

    let mut hasher = Sha256::new();
    hasher.update(&file_bytes);
    let hash = format!("{:x}", hasher.finalize());

    // Check for duplicate file
    let existing_report = queries::get_report_by_hash(&state.pool, &hash).await?;

    let format = if filename.to_lowercase().ends_with(".pdf") {
        "pdf"
    } else if filename.to_lowercase().ends_with(".csv") {
        "csv"
    } else {
        return Err(HermesError::Validation("Only PDF and CSV files are supported".to_string()));
    };

    // Create or reuse report
    let report_id = if let Some(existing) = existing_report {
        existing.id
    } else {
        let reports_dir = std::path::Path::new("data/reports");
        std::fs::create_dir_all(reports_dir)?;
        let file_id = uuid::Uuid::new_v4();
        let file_path = reports_dir.join(format!("{}.{}", file_id, format));
        std::fs::write(&file_path, &file_bytes)?;
        queries::insert_report(&state.pool, &filename, &hash, &file_path.to_string_lossy(), format).await?
    };

    let report = queries::get_report_by_id(&state.pool, report_id).await?;

    // Create import as queued
    let import_id = queries::create_import(&state.pool, report_id, &state.config.ollama.model).await?;
    queries::update_import_status(&state.pool, import_id, "queued").await?;

    // Submit to extraction queue (worker processes sequentially)
    let _ = state.extraction_queue.send(crate::web::extraction_queue::ExtractionJob {
        import_id,
        report_id,
        file_path: report.file_path.clone(),
        format: format.to_string(),
    });

    // Return HTML + trigger imports list refresh
    let html = format!(
        r##"<div class="alert alert-success">Uploaded: {} ({}). Extraction queued.</div>
<div id="extraction-status"
     hx-get="/api/v1/imports/{}/status"
     hx-trigger="load, every 3s"
     hx-target="#extraction-status"
     hx-swap="innerHTML">
  <div class="alert" style="background: var(--info-bg); color: var(--info-text);">
    Waiting for extraction to start...
  </div>
</div>"##,
        filename, format, import_id
    );

    Ok((
        [("HX-Trigger", "imports-updated")],
        Html(html),
    ).into_response())
}

// --- Import status polling ---

pub async fn import_status(
    Path(id): Path<i64>,
    State(state): State<AppState>,
) -> Result<Html<String>, HermesError> {
    let import = queries::get_import_by_id(&state.pool, id).await?;

    match import.status.as_str() {
        "queued" => Ok(Html(format!(
            r##"<div id="extraction-status" hx-get="/api/v1/imports/{}/status" hx-trigger="every 3s" hx-swap="innerHTML">
            <div class="alert" style="background: var(--amber-bg); color: var(--amber-dark);">Queued for extraction. Another import is being processed...</div>
            </div>"##,
            id
        ))),
        "extracting" | "pending" => Ok(Html(format!(
            r##"<div id="extraction-status" hx-get="/api/v1/imports/{}/status" hx-trigger="every 3s" hx-swap="innerHTML">
            <div class="alert" style="background: var(--info-bg); color: var(--info-text);">Extracting biomarkers. This may take 1-2 minutes...</div>
            </div>"##,
            id
        ))),
        "extracted" => {
            Ok(Html(format!(
                r##"<div class="alert alert-success">
                    Extraction complete: {} biomarkers found, {} unresolved.
                    <a href="/imports/{}" style="font-weight: 500;">Review results &rarr;</a>
                </div>"##,
                import.extracted_count.unwrap_or(0),
                import.unresolved_count.unwrap_or(0),
                id
            )))
        }
        "failed" => {
            let error_msg = import.raw_extraction.as_deref()
                .and_then(|j| serde_json::from_str::<serde_json::Value>(j).ok())
                .and_then(|v| v.get("error").and_then(|e| e.as_str().map(String::from)))
                .unwrap_or("Unknown error".to_string());
            Ok(Html(format!(
                r##"<div class="alert alert-error">Extraction failed: {}</div>"##,
                error_msg
            )))
        }
        _ => Ok(Html(format!(
            r##"<div class="alert">Status: {}</div>"##,
            import.status
        ))),
    }
}

// --- Import detail page ---

pub async fn import_detail(
    headers: HeaderMap,
    Path(id): Path<i64>,
    State(state): State<AppState>,
) -> Result<Html<String>, HermesError> {
    let is_htmx = htmx::is_htmx_request(&headers);
    let import = queries::get_import_by_id(&state.pool, id).await?;
    let report = queries::get_report_by_id(&state.pool, import.report_id).await?;

    let extraction = if import.status == "extracted" || import.status == "committed" {
        get_extraction_result_from_import(&import).ok()
    } else {
        None
    };

    let error_message = if import.status == "failed" {
        import.raw_extraction.as_deref()
            .and_then(|j| serde_json::from_str::<serde_json::Value>(j).ok())
            .and_then(|v| v.get("error").and_then(|e| e.as_str().map(String::from)))
    } else {
        None
    };

    let ctx = minijinja::context! {
        is_fragment => is_htmx,
        current_path => format!("/imports/{}", id),
        report => report,
        import => import,
        extraction => extraction,
        import_id => id,
        error_message => error_message,
    };
    state.templates.render("pages/import_detail.html", ctx).map(Html)
}

// --- Import extraction JSON ---

pub async fn get_extraction_json(
    Path(id): Path<i64>,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, HermesError> {
    let import = queries::get_import_by_id(&state.pool, id).await?;
    let extraction = get_extraction_result_from_import(&import)?;
    Ok(Json(serde_json::to_value(extraction)?))
}

// --- Commit ---

pub async fn commit(
    Path(id): Path<i64>,
    State(state): State<AppState>,
    axum::Form(form): axum::Form<CommitForm>,
) -> Result<Html<String>, HermesError> {
    let import = queries::get_import_by_id(&state.pool, id).await?;
    let extraction = get_extraction_result_from_import(&import)?;

    let selected: Vec<usize> = form
        .selected
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();

    let observed_at = form.test_date.as_deref()
        .filter(|s| !s.is_empty())
        .or(extraction.test_date.as_deref())
        .unwrap_or(&chrono::Local::now().format("%Y-%m-%d").to_string())
        .to_string();

    let mut committed = 0;
    let mut errors = Vec::new();
    let mut committed_loinc_codes: Vec<String> = Vec::new();

    for idx in &selected {
        if let Some(obs) = extraction.observations.get(*idx) {
            // Skip duplicates: if we already committed this LOINC code from this import, skip
            if committed_loinc_codes.contains(&obs.loinc_code) {
                continue;
            }
            let new_obs = NewObservation {
                biomarker: obs.loinc_code.clone(),
                value: obs.canonical_value,
                unit: obs.canonical_unit.clone(),
                observed_at: observed_at.clone(),
                lab_name: None,
                fasting: None,
                notes: None,
                report_id: Some(import.report_id),
                import_id: Some(id),
            };
            match observation::add_observation(&state.pool, &state.catalog, &new_obs).await {
                Ok(_) => {
                    committed += 1;
                    committed_loinc_codes.push(obs.loinc_code.clone());
                }
                Err(e) => errors.push(format!("{}: {}", obs.marker_name, e)),
            }
        }
    }

    let _ = queries::update_import_status(&state.pool, id, "committed").await;

    let mut msg = format!("Committed {} observations.", committed);
    if !errors.is_empty() {
        msg.push_str(&format!(" {} errors: {}", errors.len(), errors.join("; ")));
    }

    Ok(Html(format!(
        r##"<div class="alert alert-success">{}</div>
        <a href="/" style="font-size:13px;">Back to dashboard</a>"##,
        msg
    )))
}

// --- LOINC mapping ---

pub async fn map_marker(
    Path(id): Path<i64>,
    State(state): State<AppState>,
    axum::Form(form): axum::Form<MapForm>,
) -> Result<Html<String>, HermesError> {
    if let Some(bm) = queries::get_biomarker_by_loinc(&state.pool, &form.loinc_code).await? {
        let mut aliases = bm.aliases_vec();
        if !aliases.iter().any(|a| a.to_lowercase() == form.marker_name.to_lowercase()) {
            aliases.push(form.marker_name.clone());
            queries::update_biomarker_aliases(&state.pool, bm.id, &aliases).await?;
        }
        Ok(Html(format!(
            r##"<span class="pill pill-supplement">{} mapped</span>"##,
            form.marker_name
        )))
    } else {
        Ok(Html(format!(
            r##"<span class="text-red">LOINC code {} not found</span>"##,
            form.loinc_code
        )))
    }
}

#[derive(serde::Deserialize)]
pub struct CommitForm {
    pub selected: String,
    pub test_date: Option<String>,
}

#[derive(serde::Deserialize)]
pub struct MapForm {
    pub marker_name: String,
    pub loinc_code: String,
}

// --- Helpers ---

fn get_extraction_result_from_import(import: &crate::db::models::Import) -> Result<ExtractionResult, HermesError> {
    let json = import.raw_extraction.as_deref()
        .ok_or_else(|| HermesError::NotFound("No extraction data".to_string()))?;
    serde_json::from_str(json).map_err(|e| HermesError::Json(e))
}
