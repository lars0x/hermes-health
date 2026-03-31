use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::Html;
use serde::Deserialize;

use crate::error::HermesError;
use crate::services::{biomarker, observation, trend};
use crate::web::htmx;
use crate::web::AppState;

#[derive(Deserialize)]
pub struct ChartQuery {
    pub range: Option<String>,
}

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: Option<String>,
}

pub async fn biomarker_detail(
    headers: HeaderMap,
    Path(id): Path<i64>,
    State(state): State<AppState>,
) -> Result<Html<String>, HermesError> {
    let is_htmx = htmx::is_htmx_request(&headers);
    let bm = biomarker::get_biomarker(&state.pool, id).await?;
    let observations = observation::list_for_biomarker(&state.pool, bm.id, None, None).await?;

    let window = state.config.display.default_trend_window_days;
    let trend_result = trend::compute_trend(
        &state.pool,
        bm.id,
        window,
        state.config.trends.min_data_points,
        state.config.trends.rapid_change_threshold_pct,
        state.config.trends.projection_horizon_days,
    )
    .await?;

    let obs_timestamps: Vec<i64> = observations
        .iter()
        .filter_map(|o| {
            chrono::NaiveDate::parse_from_str(&o.observed_at, "%Y-%m-%d")
                .ok()
                .map(|d| d.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp())
        })
        .collect();
    let obs_values: Vec<f64> = observations.iter().map(|o| o.value).collect();

    let chart_data = serde_json::json!({
        "timestamps": obs_timestamps,
        "values": obs_values,
        "reference_low": bm.reference_low,
        "reference_high": bm.reference_high,
        "optimal_low": bm.optimal_low,
        "optimal_high": bm.optimal_high,
    });

    let regression_data = trend_result.trend.as_ref().map(|t| {
        serde_json::json!({
            "slope": t.slope,
            "latest_value": t.latest_value,
        })
    });

    // Get interventions targeting this biomarker
    let all_interventions =
        crate::db::queries::list_interventions(&state.pool, false).await.unwrap_or_default();
    let mut intervention_markers = Vec::new();
    for intervention in &all_interventions {
        let targets =
            crate::db::queries::get_intervention_targets(&state.pool, intervention.id)
                .await
                .unwrap_or_default();
        if targets.iter().any(|t| t.biomarker_id == bm.id) {
            if let Ok(d) = chrono::NaiveDate::parse_from_str(&intervention.started_at, "%Y-%m-%d") {
                intervention_markers.push(serde_json::json!({
                    "timestamp": d.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp(),
                    "name": intervention.name,
                }));
            }
        }
    }

    // Get latest value and status
    let latest_obs = observations.last();
    let latest_value = latest_obs.map(|o| o.value);
    let latest_status = latest_obs.map(|o| {
        crate::services::biomarker::range_status(o.value, &bm)
    });

    let ctx = minijinja::context! {
        is_fragment => is_htmx,
        current_path => format!("/biomarkers/{}", id),
        biomarker => bm,
        observations => observations,
        trend => trend_result.trend,
        chart_data_json => chart_data.to_string(),
        regression_json => regression_data.map(|r| r.to_string()).unwrap_or("null".to_string()),
        intervention_json => serde_json::to_string(&intervention_markers).unwrap_or("[]".to_string()),
        latest_value => latest_value,
        latest_status => latest_status,
        window => window,
    };

    let html = state.templates.render("pages/biomarker_detail.html", ctx)?;
    Ok(Html(html))
}

pub async fn biomarker_chart(
    Path(id): Path<i64>,
    Query(query): Query<ChartQuery>,
    State(state): State<AppState>,
) -> Result<Html<String>, HermesError> {
    let range = query.range.unwrap_or("12m".to_string());
    let window_days: u32 = match range.as_str() {
        "6m" => 180,
        "12m" => 365,
        "all" => 3650,
        _ => 365,
    };

    let bm = biomarker::get_biomarker(&state.pool, id).await?;
    let cutoff = chrono::Local::now()
        .date_naive()
        .checked_sub_days(chrono::Days::new(window_days as u64))
        .unwrap_or(chrono::NaiveDate::from_ymd_opt(2000, 1, 1).unwrap());
    let from_str = cutoff.format("%Y-%m-%d").to_string();

    let observations =
        observation::list_for_biomarker(&state.pool, bm.id, Some(&from_str), None).await?;

    let trend_result = trend::compute_trend(
        &state.pool,
        bm.id,
        window_days,
        state.config.trends.min_data_points,
        state.config.trends.rapid_change_threshold_pct,
        state.config.trends.projection_horizon_days,
    )
    .await?;

    let obs_timestamps: Vec<i64> = observations
        .iter()
        .filter_map(|o| {
            chrono::NaiveDate::parse_from_str(&o.observed_at, "%Y-%m-%d")
                .ok()
                .map(|d| d.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp())
        })
        .collect();
    let obs_values: Vec<f64> = observations.iter().map(|o| o.value).collect();

    let chart_data = serde_json::json!({
        "timestamps": obs_timestamps,
        "values": obs_values,
        "reference_low": bm.reference_low,
        "reference_high": bm.reference_high,
        "optimal_low": bm.optimal_low,
        "optimal_high": bm.optimal_high,
    });

    let regression_data = trend_result.trend.as_ref().map(|t| {
        serde_json::json!({ "slope": t.slope, "latest_value": t.latest_value })
    });

    let ctx = minijinja::context! {
        biomarker => bm,
        chart_data_json => chart_data.to_string(),
        regression_json => regression_data.map(|r| r.to_string()).unwrap_or("null".to_string()),
        intervention_json => "[]",
        trend => trend_result.trend,
        range => range,
        window => window_days,
    };

    let html = state.templates.render("components/chart.html", ctx)?;
    Ok(Html(html))
}

pub async fn biomarker_search(
    Query(query): Query<SearchQuery>,
    State(state): State<AppState>,
) -> Result<Html<String>, HermesError> {
    let q = query.q.unwrap_or_default();
    if q.len() < 2 {
        return Ok(Html(String::new()));
    }

    let all = biomarker::list_biomarkers(&state.pool, None).await?;
    let q_lower = q.to_lowercase();
    let mut results: Vec<minijinja::Value> = Vec::new();

    for bm in &all {
        let name_match = bm.name.to_lowercase().contains(&q_lower);
        let alias_match = bm.aliases_vec().iter().any(|a| a.to_lowercase().contains(&q_lower));
        let code_match = bm.loinc_code.to_lowercase().starts_with(&q_lower);

        if name_match || alias_match || code_match {
            results.push(minijinja::context! {
                id => bm.id,
                name => bm.name,
                loinc_code => bm.loinc_code,
                category => bm.category,
                unit => bm.unit,
                tracked => true,
            });
        }
    }

    if results.len() < 5 {
        let catalog_results = state.catalog.search(&q, 5 - results.len());
        for cr in catalog_results {
            let already_listed = results.iter().any(|r| {
                r.get_attr("loinc_code")
                    .ok()
                    .and_then(|v| v.as_str().map(|s| s == cr.loinc_code))
                    .unwrap_or(false)
            });
            if !already_listed {
                if let Some(entry) = state.catalog.get_by_code(&cr.loinc_code) {
                    results.push(minijinja::context! {
                        id => 0,
                        name => entry.component,
                        loinc_code => entry.loinc_num,
                        category => entry.class,
                        unit => entry.example_ucum_units,
                        tracked => false,
                    });
                }
            }
        }
    }

    let ctx = minijinja::context! { results => results };
    let html = state.templates.render("components/biomarker_autocomplete.html", ctx)?;
    Ok(Html(html))
}
