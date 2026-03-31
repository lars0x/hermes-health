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
