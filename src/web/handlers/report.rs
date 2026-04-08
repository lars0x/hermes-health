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

/// Format duration between two ISO 8601 timestamps as "XmXs".
fn format_duration(created: &str, completed: &str) -> String {
    let parse = |s: &str| -> Option<chrono::NaiveDateTime> {
        chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%SZ").ok()
    };
    if let (Some(start), Some(end)) = (parse(created), parse(completed)) {
        let secs = (end - start).num_seconds().max(0);
        let mins = secs / 60;
        let rem = secs % 60;
        if mins > 0 {
            format!("{}m{}s", mins, rem)
        } else {
            format!("{}s", rem)
        }
    } else {
        String::new()
    }
}

async fn enrich_imports(pool: &sqlx::SqlitePool, imports: &[crate::db::models::Import]) -> Vec<minijinja::Value> {
    let mut rows = Vec::new();
    for imp in imports {
        let report = queries::get_report_by_id(pool, imp.report_id).await.ok();
        let skipped_count = if imp.status == "committed" {
            let committed = queries::count_observations_for_import(pool, imp.id).await.unwrap_or(0);
            let extracted = imp.extracted_count.unwrap_or(0);
            (extracted - committed).max(0)
        } else {
            0
        };
        let duration = imp.completed_at.as_deref()
            .map(|c| format_duration(&imp.created_at, c))
            .unwrap_or_default();
        rows.push(minijinja::context! {
            id => imp.id,
            filename => report.as_ref().map(|r| r.filename.clone()).unwrap_or_default(),
            format => report.as_ref().map(|r| r.format.clone()).unwrap_or_default(),
            model_used => imp.model_used,
            status => imp.status,
            extracted_count => imp.extracted_count,
            unresolved_count => imp.unresolved_count,
            skipped_count => skipped_count,
            created_at => imp.created_at,
            duration => duration,
        });
    }
    rows
}

pub async fn import_page(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Html<String>, HermesError> {
    let is_htmx = htmx::is_htmx_request(&headers);
    let imports = queries::list_imports(&state.pool).await.unwrap_or_default();
    let import_rows = enrich_imports(&state.pool, &imports).await;

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
    let import_rows = enrich_imports(&state.pool, &imports).await;
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
                .map_err(|e| HermesError::Io(std::io::Error::other(e)))?
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

    // Find duplicate LOINC codes and load human overwrites
    let duplicate_loinc_codes: Vec<String> = if let Some(ref ext) = extraction {
        let mut counts = std::collections::HashMap::new();
        for obs in &ext.observations {
            *counts.entry(obs.loinc_code.clone()).or_insert(0) += 1;
        }
        counts.into_iter().filter(|(_, c)| *c > 1).map(|(k, _)| k).collect()
    } else {
        vec![]
    };

    let overwrites = queries::list_import_overwrites(&state.pool, id).await.unwrap_or_default();
    let overwrite_map: std::collections::HashMap<String, usize> = overwrites.iter()
        .map(|o| (o.loinc_code.clone(), o.chosen_idx as usize))
        .collect();

    // Look up LOINC long common names for all codes in extraction
    let mut loinc_names: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    if let Some(ref ext) = extraction {
        for obs in &ext.observations {
            if !loinc_names.contains_key(&obs.loinc_code) {
                if let Some(entry) = state.catalog.get_by_code(&obs.loinc_code) {
                    loinc_names.insert(obs.loinc_code.clone(), entry.long_common_name.clone());
                }
            }
        }
    }

    let (matched_observations, duplicate_observations, dismissed_observations) = if let Some(ref ext) = extraction {
        build_observation_lists(ext, &duplicate_loinc_codes, &overwrite_map, &loinc_names)
    } else {
        (vec![], vec![], vec![])
    };

    let matched_count = matched_observations.len();
    let duplicate_count = duplicate_observations.len();
    let dismissed_count = dismissed_observations.len();

    let error_message = if import.status == "failed" {
        import.raw_extraction.as_deref()
            .and_then(|j| serde_json::from_str::<serde_json::Value>(j).ok())
            .and_then(|v| v.get("error").and_then(|e| e.as_str().map(String::from)))
    } else {
        None
    };

    // Find skipped observations (extracted but not committed)
    let skipped_observations = if import.status == "committed" {
        if let Some(ref ext) = extraction {
            let committed = queries::list_observations_for_import(&state.pool, id).await.unwrap_or_default();
            let mut committed_loinc_codes = std::collections::HashSet::new();
            for o in &committed {
                if let Ok(bm) = queries::get_biomarker_by_id(&state.pool, o.biomarker_id).await {
                    committed_loinc_codes.insert(bm.loinc_code);
                }
            }
            ext.observations.iter().enumerate()
                .filter(|(_, obs)| !committed_loinc_codes.contains(&obs.loinc_code))
                .map(|(idx, obs)| {
                    let biomarker_name = loinc_names.get(&obs.loinc_code).cloned().unwrap_or_default();
                    minijinja::context! {
                    idx => idx,
                    marker_name => obs.marker_name,
                    value => obs.value,
                    original_value => obs.original_value,
                    unit => obs.unit,
                    canonical_value => obs.canonical_value,
                    canonical_unit => obs.canonical_unit,
                    loinc_code => obs.loinc_code,
                    confidence => obs.confidence,
                    biomarker_name => biomarker_name,
                    specimen => obs.specimen,
                    match_source => obs.match_source,
                }})
                .collect::<Vec<_>>()
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    let skipped_count = skipped_observations.len();

    let llm_log_entries: Vec<serde_json::Value> = import.llm_log.as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();

    let duration = import.completed_at.as_deref()
        .map(|c| format_duration(&import.created_at, c))
        .unwrap_or_default();

    let ctx = minijinja::context! {
        is_fragment => is_htmx,
        current_path => format!("/imports/{}", id),
        report => report,
        import => import,
        duration => duration,
        extraction => extraction,
        import_id => id,
        error_message => error_message,
        loinc_names => loinc_names,
        matched_observations => matched_observations,
        matched_count => matched_count,
        duplicate_observations => duplicate_observations,
        duplicate_count => duplicate_count,
        dismissed_observations => dismissed_observations,
        dismissed_count => dismissed_count,
        skipped_observations => skipped_observations,
        skipped_count => skipped_count,
        llm_log_entries => llm_log_entries,
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
    let mut committed_loinc_codes = Vec::new();

    for idx in &selected {
        if let Some(obs) = extraction.observations.get(*idx) {
            if committed_loinc_codes.contains(&obs.loinc_code) {
                errors.push(format!(
                    "{}: duplicate LOINC {} - uncheck one of the conflicting rows",
                    obs.marker_name, obs.loinc_code
                ));
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

// --- Uncommit ---

pub async fn uncommit(
    Path(id): Path<i64>,
    State(state): State<AppState>,
) -> Result<Html<String>, HermesError> {
    let import = queries::get_import_by_id(&state.pool, id).await?;

    if import.status != "committed" {
        return Err(HermesError::Validation(
            "Only committed imports can be undone".to_string(),
        ));
    }

    let deleted = queries::delete_observations_by_import(&state.pool, id).await?;
    queries::update_import_status(&state.pool, id, "extracted").await?;

    Ok(Html(format!(
        r##"<div class="alert alert-success">Removed {} observations. Import is ready for review again.</div>
        <script>setTimeout(function(){{ window.location.reload(); }}, 1500);</script>"##,
        deleted
    )))
}

// --- Decline ---

pub async fn decline(
    Path(id): Path<i64>,
    State(state): State<AppState>,
) -> Result<Html<String>, HermesError> {
    let import = queries::get_import_by_id(&state.pool, id).await?;

    if import.status != "extracted" {
        return Err(HermesError::Validation(
            "Only imports ready for review can be declined".to_string(),
        ));
    }

    queries::update_import_status(&state.pool, id, "declined").await?;

    Ok(Html(
        r##"<div class="alert" style="background: var(--bg-secondary); color: var(--text-secondary);">Import declined.</div>
        <script>setTimeout(function(){ window.location.reload(); }, 1500);</script>"##.to_string()
    ))
}

// --- Resolve duplicate ---

pub async fn resolve_duplicate(
    Path(id): Path<i64>,
    State(state): State<AppState>,
    axum::Form(form): axum::Form<ResolveDuplicateForm>,
) -> Result<Html<String>, HermesError> {
    let import = queries::get_import_by_id(&state.pool, id).await?;
    if import.status != "extracted" {
        return Err(HermesError::Validation("Import is not in review".to_string()));
    }

    queries::upsert_import_overwrite(&state.pool, id, &form.loinc_code, form.chosen_idx as i64).await?;

    // Re-render the review table with updated state
    render_review_table(id, &state).await
}

async fn render_review_table(
    id: i64,
    state: &AppState,
) -> Result<Html<String>, HermesError> {
    let import = queries::get_import_by_id(&state.pool, id).await?;
    let report = queries::get_report_by_id(&state.pool, import.report_id).await?;
    let extraction = get_extraction_result_from_import(&import).ok();

    let duplicate_loinc_codes: Vec<String> = if let Some(ref ext) = extraction {
        let mut counts = std::collections::HashMap::new();
        for obs in &ext.observations {
            *counts.entry(obs.loinc_code.clone()).or_insert(0) += 1;
        }
        counts.into_iter().filter(|(_, c)| *c > 1).map(|(k, _)| k).collect()
    } else {
        vec![]
    };

    let overwrites = queries::list_import_overwrites(&state.pool, id).await.unwrap_or_default();
    let overwrite_map: std::collections::HashMap<String, usize> = overwrites.iter()
        .map(|o| (o.loinc_code.clone(), o.chosen_idx as usize))
        .collect();

    let mut loinc_names: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    if let Some(ref ext) = extraction {
        for obs in &ext.observations {
            if !loinc_names.contains_key(&obs.loinc_code) {
                if let Some(entry) = state.catalog.get_by_code(&obs.loinc_code) {
                    loinc_names.insert(obs.loinc_code.clone(), entry.long_common_name.clone());
                }
            }
        }
    }

    let (matched_observations, duplicate_observations, dismissed_observations) = if let Some(ref ext) = extraction {
        build_observation_lists(ext, &duplicate_loinc_codes, &overwrite_map, &loinc_names)
    } else {
        (vec![], vec![], vec![])
    };

    let matched_count = matched_observations.len();
    let duplicate_count = duplicate_observations.len();
    let dismissed_count = dismissed_observations.len();

    let ctx = minijinja::context! {
        report => report,
        extraction => extraction,
        import_id => id,
        loinc_names => loinc_names,
        matched_observations => matched_observations,
        matched_count => matched_count,
        duplicate_observations => duplicate_observations,
        duplicate_count => duplicate_count,
        dismissed_observations => dismissed_observations,
        dismissed_count => dismissed_count,
    };
    state.templates.render("components/review_table.html", ctx).map(Html)
}

// --- LOINC mapping ---

pub async fn map_marker(
    Path(_id): Path<i64>,
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
pub struct ResolveDuplicateForm {
    pub loinc_code: String,
    pub chosen_idx: usize,
}

#[derive(serde::Deserialize)]
pub struct MapForm {
    pub marker_name: String,
    pub loinc_code: String,
}

// --- Observation list splitting ---

fn build_observation_lists(
    ext: &ExtractionResult,
    duplicate_loinc_codes: &[String],
    overwrite_map: &std::collections::HashMap<String, usize>,
    loinc_names: &std::collections::HashMap<String, String>,
) -> (Vec<minijinja::Value>, Vec<minijinja::Value>, Vec<minijinja::Value>) {
    let mut matched = Vec::new();
    let mut dupe_indices: Vec<(String, usize)> = Vec::new();
    let mut dismissed = Vec::new();

    for (idx, obs) in ext.observations.iter().enumerate() {
        if !duplicate_loinc_codes.contains(&obs.loinc_code) {
            let biomarker_name = loinc_names.get(&obs.loinc_code).cloned().unwrap_or_default();
            matched.push(minijinja::context! {
                idx => idx,
                marker_name => obs.marker_name,
                value => obs.value,
                original_value => obs.original_value,
                unit => obs.unit,
                canonical_value => obs.canonical_value,
                canonical_unit => obs.canonical_unit,
                loinc_code => obs.loinc_code,
                confidence => obs.confidence,
                biomarker_name => biomarker_name,
                human_resolved => false,
                specimen => obs.specimen,
                    match_source => obs.match_source,
            });
        } else if let Some(&chosen_idx) = overwrite_map.get(&obs.loinc_code) {
            if idx == chosen_idx {
                let biomarker_name = loinc_names.get(&obs.loinc_code).cloned().unwrap_or_default();
                matched.push(minijinja::context! {
                    idx => idx,
                    marker_name => obs.marker_name,
                    value => obs.value,
                    original_value => obs.original_value,
                    unit => obs.unit,
                    canonical_value => obs.canonical_value,
                    canonical_unit => obs.canonical_unit,
                    loinc_code => obs.loinc_code,
                    confidence => obs.confidence,
                    biomarker_name => biomarker_name,
                    human_resolved => true,
                    specimen => obs.specimen,
                    match_source => obs.match_source,
                });
            } else {
                dismissed.push(minijinja::context! {
                    marker_name => obs.marker_name,
                    value => if obs.original_value.is_empty() { obs.value.to_string() } else { obs.original_value.clone() },
                    unit => obs.unit,
                    loinc_code => obs.loinc_code,
                    reason => "Duplicate - another match selected by user",
                });
            }
        } else {
            dupe_indices.push((obs.loinc_code.clone(), idx));
        }
    }

    dupe_indices.sort_by(|a, b| a.0.cmp(&b.0));
    let mut prev_loinc = String::new();
    let dupes: Vec<minijinja::Value> = dupe_indices.into_iter().map(|(loinc, idx)| {
        let obs = &ext.observations[idx];
        let group_first = loinc != prev_loinc;
        prev_loinc = loinc.clone();
        let biomarker_name = loinc_names.get(&loinc).cloned().unwrap_or_default();
        minijinja::context! {
            idx => idx,
            marker_name => obs.marker_name,
            value => obs.value,
            original_value => obs.original_value,
            unit => obs.unit,
            canonical_value => obs.canonical_value,
            canonical_unit => obs.canonical_unit,
            loinc_code => obs.loinc_code,
            confidence => obs.confidence,
            group_first => group_first,
            biomarker_name => biomarker_name,
            specimen => obs.specimen,
                    match_source => obs.match_source,
        }
    }).collect();

    (matched, dupes, dismissed)
}

// --- Helpers ---

fn get_extraction_result_from_import(import: &crate::db::models::Import) -> Result<ExtractionResult, HermesError> {
    let json = import.raw_extraction.as_deref()
        .ok_or_else(|| HermesError::NotFound("No extraction data".to_string()))?;
    serde_json::from_str(json).map_err(HermesError::Json)
}
