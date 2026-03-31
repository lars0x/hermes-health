use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::Html;
use serde::Deserialize;

use crate::db::models::NewIntervention;
use crate::db::queries;
use crate::error::HermesError;
use crate::web::htmx;
use crate::web::AppState;

pub async fn interventions_page(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Html<String>, HermesError> {
    let is_htmx = htmx::is_htmx_request(&headers);
    let interventions = queries::list_interventions(&state.pool, false).await?;

    let mut rows = Vec::new();
    for i in &interventions {
        let targets = queries::get_intervention_targets(&state.pool, i.id).await?;
        let target_names: Vec<String> = {
            let mut names = Vec::new();
            for t in &targets {
                if let Ok(bm) = queries::get_biomarker_by_id(&state.pool, t.biomarker_id).await {
                    names.push(format!("{} ({})", bm.name, t.expected_effect));
                }
            }
            names
        };
        rows.push(minijinja::context! {
            id => i.id,
            name => i.name,
            category => i.category,
            dosage => i.dosage,
            frequency => i.frequency,
            started_at => i.started_at,
            ended_at => i.ended_at,
            notes => i.notes,
            active => i.ended_at.is_none(),
            targets => target_names,
        });
    }

    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let ctx = minijinja::context! {
        is_fragment => is_htmx,
        current_path => "/interventions",
        interventions => rows,
        today => today,
    };
    state.templates.render("pages/interventions.html", ctx).map(Html)
}

pub async fn intervention_detail(
    headers: HeaderMap,
    Path(id): Path<i64>,
    State(state): State<AppState>,
) -> Result<Html<String>, HermesError> {
    let is_htmx = htmx::is_htmx_request(&headers);
    let intervention = queries::get_intervention_by_id(&state.pool, id).await?;
    let targets = queries::get_intervention_targets(&state.pool, id).await?;

    let mut target_rows = Vec::new();
    for t in &targets {
        if let Ok(bm) = queries::get_biomarker_by_id(&state.pool, t.biomarker_id).await {
            target_rows.push(minijinja::context! {
                biomarker_id => bm.id,
                name => bm.name,
                loinc_code => bm.loinc_code,
                expected_effect => t.expected_effect,
            });
        }
    }

    let ctx = minijinja::context! {
        is_fragment => is_htmx,
        current_path => format!("/interventions/{}", id),
        intervention => intervention,
        targets => target_rows,
    };
    state.templates.render("pages/intervention_detail.html", ctx).map(Html)
}

#[derive(Deserialize)]
pub struct InterventionForm {
    pub name: String,
    pub category: String,
    pub dosage: Option<String>,
    pub frequency: Option<String>,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub notes: Option<String>,
    #[serde(default)]
    pub target_biomarker_ids: String,   // comma-separated IDs
    #[serde(default)]
    pub target_effects: String,         // comma-separated effects
}

pub async fn create_intervention(
    State(state): State<AppState>,
    axum::Form(form): axum::Form<InterventionForm>,
) -> Result<Html<String>, HermesError> {
    let targets = parse_targets(&form.target_biomarker_ids, &form.target_effects);

    let new = NewIntervention {
        name: form.name,
        category: form.category,
        dosage: form.dosage.filter(|s| !s.is_empty()),
        frequency: form.frequency.filter(|s| !s.is_empty()),
        started_at: form.started_at,
        ended_at: form.ended_at.filter(|s| !s.is_empty()),
        notes: form.notes.filter(|s| !s.is_empty()),
        target_biomarkers: targets,
    };

    let id = queries::insert_intervention(&state.pool, &new).await?;

    Ok(Html(format!(
        r##"<div class="alert alert-success">Created: {}. <a href="/interventions/{}">View details</a></div>"##,
        new.name, id
    )))
}

pub async fn end_intervention(
    Path(id): Path<i64>,
    State(state): State<AppState>,
) -> Result<Html<String>, HermesError> {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    queries::end_intervention(&state.pool, id, &today).await?;
    Ok(Html(format!(
        r##"<div class="alert alert-success">Intervention ended on {}. <a href="/interventions">Back to list</a></div>"##,
        today
    )))
}

fn parse_targets(ids_str: &str, effects_str: &str) -> Vec<crate::db::models::InterventionTarget> {
    let ids: Vec<i64> = ids_str.split(',').filter_map(|s| s.trim().parse().ok()).collect();
    let effects: Vec<&str> = effects_str.split(',').map(|s| s.trim()).collect();

    ids.into_iter()
        .enumerate()
        .map(|(i, biomarker_id)| crate::db::models::InterventionTarget {
            biomarker_id,
            expected_effect: effects.get(i).unwrap_or(&"stabilize").to_string(),
        })
        .collect()
}
