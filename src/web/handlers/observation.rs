use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::Html;
use axum::Form;
use serde::Deserialize;

use crate::db::models::NewObservation;
use crate::error::HermesError;
use crate::services::observation;
use crate::web::htmx;
use crate::web::AppState;

#[derive(Deserialize)]
pub struct ObservationForm {
    pub biomarker: String,
    pub value: f64,
    pub unit: String,
    pub date: String,
    pub lab: Option<String>,
    pub fasting: Option<String>,
    pub notes: Option<String>,
}

pub async fn data_entry_page(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Html<String>, HermesError> {
    let is_htmx = htmx::is_htmx_request(&headers);
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();

    // Load imports for the import tab
    let imports_list = crate::db::queries::list_imports(&state.pool).await.unwrap_or_default();
    let mut imports = Vec::new();
    for imp in &imports_list {
        let report = crate::db::queries::get_report_by_id(&state.pool, imp.report_id).await.ok();
        imports.push(minijinja::context! {
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
        current_path => "/entry",
        today => today,
        imports => imports,
    };
    let html = state.templates.render("pages/data_entry.html", ctx)?;
    Ok(Html(html))
}

pub async fn create_observation(
    State(state): State<AppState>,
    Form(form): Form<ObservationForm>,
) -> Result<Html<String>, HermesError> {
    let fasting = match form.fasting.as_deref() {
        Some("yes") => Some(true),
        Some("no") => Some(false),
        _ => None,
    };

    let obs = NewObservation {
        biomarker: form.biomarker,
        value: form.value,
        unit: form.unit,
        observed_at: form.date,
        lab_name: form.lab.filter(|s| !s.is_empty()),
        fasting,
        notes: form.notes.filter(|s| !s.is_empty()),
        report_id: None,
        import_id: None,
        original_value: None,
    };

    match observation::add_observation(&state.pool, &state.catalog, &obs).await {
        Ok(result) => {
            let msg = if result.converted {
                format!(
                    "Added: {} = {} {} (converted from original unit)",
                    result.biomarker_name, result.value, result.unit
                )
            } else {
                format!(
                    "Added: {} = {} {}",
                    result.biomarker_name, result.value, result.unit
                )
            };
            Ok(Html(format!(
                r#"<div class="alert alert-success">{}</div>"#,
                msg
            )))
        }
        Err(e) => Ok(Html(format!(
            r#"<div class="alert alert-error">{}</div>"#,
            e
        ))),
    }
}

/// Flat table of every committed observation (the human datapoints loaded from
/// reports, plus any added manually), joined with its biomarker and source report.
pub async fn observations_table(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Html<String>, HermesError> {
    let is_htmx = htmx::is_htmx_request(&headers);

    let observations = crate::db::queries::list_all_observations(&state.pool, None, None).await?;
    let biomarkers = crate::services::biomarker::list_biomarkers(&state.pool, None).await?;
    let reports = crate::db::queries::list_reports(&state.pool).await?;

    // list_all_observations returns ascending by date; show newest first.
    let mut rows: Vec<minijinja::Value> = Vec::new();
    for o in observations.iter().rev() {
        let bm = biomarkers.iter().find(|b| b.id == o.biomarker_id);

        // Qualitative results (text_value) carry no numeric range to assess.
        let status = match bm {
            _ if o.text_value.is_some() => "qualitative".to_string(),
            Some(b) => crate::services::biomarker::range_status(o.value, b),
            None => "no_data".to_string(),
        };

        let unit = bm
            .map(|b| b.unit.clone())
            .unwrap_or_else(|| o.original_unit.clone());

        let result = if let Some(tv) = &o.text_value {
            tv.clone()
        } else {
            let p = o.precision.max(0) as usize;
            let prefix = o.detection_limit.as_deref().unwrap_or("");
            format!("{}{:.*} {}", prefix, p, o.value, unit)
        };

        let source = o
            .report_id
            .and_then(|rid| reports.iter().find(|r| r.id == rid))
            .map(|r| r.filename.clone());

        let fasting = match o.fasting {
            Some(true) => "Yes",
            Some(false) => "No",
            None => "—",
        };

        rows.push(minijinja::context! {
            date => o.observed_at.clone(),
            biomarker_id => o.biomarker_id,
            biomarker => bm.map(|b| b.name.clone()).unwrap_or_else(|| "(unknown)".to_string()),
            category => bm.map(|b| b.category.clone()).unwrap_or_default(),
            result => result,
            reference_low => bm.and_then(|b| b.reference_low),
            reference_high => bm.and_then(|b| b.reference_high),
            optimal_low => bm.and_then(|b| b.optimal_low),
            optimal_high => bm.and_then(|b| b.optimal_high),
            status => status,
            fasting => fasting,
            source => source,
            import_id => o.import_id,
        });
    }

    let ctx = minijinja::context! {
        is_fragment => is_htmx,
        current_path => "/observations",
        rows => rows,
        count => observations.len(),
    };
    state.templates.render("pages/observations.html", ctx).map(Html)
}
