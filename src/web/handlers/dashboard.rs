use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::Html;

use crate::error::HermesError;
use crate::services::{biomarker, trend};
use crate::web::htmx;
use crate::web::AppState;

pub async fn dashboard(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Html<String>, HermesError> {
    let is_htmx = htmx::is_htmx_request(&headers);
    let summary = biomarker::dashboard_summary(&state.pool).await?;
    let out_of_range = biomarker::get_out_of_range(&state.pool).await?;

    let mut attention_items = Vec::new();
    for (bm, value, status) in &out_of_range {
        let trend_result = trend::compute_trend(
            &state.pool,
            bm.id,
            state.config.display.default_trend_window_days,
            state.config.trends.min_data_points,
            state.config.trends.rapid_change_threshold_pct,
            state.config.trends.projection_horizon_days,
        )
        .await
        .ok();

        let (direction, rate, trend_status) = trend_result
            .and_then(|t| t.trend)
            .map(|t| (t.direction.clone(), t.annualized_rate_pct, t.status.clone()))
            .unwrap_or(("stable".to_string(), 0.0, "insufficient_data".to_string()));

        attention_items.push(minijinja::context! {
            id => bm.id,
            name => bm.name,
            loinc_code => bm.loinc_code,
            value => value,
            unit => bm.unit,
            direction => direction,
            rate => rate,
            status => trend_status,
            range_status => status,
        });
    }

    let interventions =
        crate::db::queries::list_interventions(&state.pool, true).await.unwrap_or_default();

    let all_biomarkers = biomarker::list_biomarkers(&state.pool, None).await?;
    let latest_obs =
        crate::db::queries::get_latest_observation_per_biomarker(&state.pool).await?;
    let mut improvements = Vec::new();

    for bm in &all_biomarkers {
        if let Some(obs) = latest_obs.iter().find(|o| o.biomarker_id == bm.id) {
            if let Ok(t) = trend::compute_trend(
                &state.pool,
                bm.id,
                state.config.display.default_trend_window_days,
                state.config.trends.min_data_points,
                state.config.trends.rapid_change_threshold_pct,
                state.config.trends.projection_horizon_days,
            )
            .await
            {
                if let Some(trend_stats) = &t.trend {
                    if trend_stats.status == "improving" {
                        improvements.push(minijinja::context! {
                            name => bm.name,
                            value => obs.value,
                            unit => bm.unit,
                            direction => trend_stats.direction,
                            rate => trend_stats.annualized_rate_pct,
                            precision => obs.precision,
                        });
                    }
                }
            }
        }
    }

    let ctx = minijinja::context! {
        is_fragment => is_htmx,
        current_path => "/",
        summary => minijinja::context! {
            total_tracked => summary.total_tracked,
            in_optimal => summary.in_optimal,
            out_of_range => summary.out_of_range,
            suboptimal => summary.suboptimal,
            days_since_last_lab => summary.days_since_last_lab,
        },
        attention_items => attention_items,
        interventions => interventions,
        improvements => improvements,
    };

    let html = state.templates.render("pages/dashboard.html", ctx)?;
    Ok(Html(html))
}
