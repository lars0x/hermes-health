use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;

use crate::db::models::NewObservation;
use crate::error::HermesError;
use crate::services::{biomarker, observation, trend};
use crate::web::AppState;

#[derive(Deserialize)]
pub struct BiomarkerListQuery {
    pub category: Option<String>,
}

#[derive(Deserialize)]
pub struct ObservationListQuery {
    pub biomarker: Option<i64>,
    pub from: Option<String>,
    pub to: Option<String>,
}

#[derive(Deserialize)]
pub struct TrendQuery {
    pub window_days: Option<u32>,
}

#[derive(Deserialize)]
pub struct InterventionListQuery {
    pub active: Option<bool>,
}

pub async fn list_biomarkers(
    Query(query): Query<BiomarkerListQuery>,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, HermesError> {
    let biomarkers =
        biomarker::list_biomarkers(&state.pool, query.category.as_deref()).await?;
    Ok(Json(serde_json::to_value(biomarkers)?))
}

pub async fn get_biomarker(
    Path(id): Path<i64>,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, HermesError> {
    let bm = biomarker::get_biomarker(&state.pool, id).await?;
    Ok(Json(serde_json::to_value(bm)?))
}

pub async fn get_trend(
    Path(id): Path<i64>,
    Query(query): Query<TrendQuery>,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, HermesError> {
    let window = query
        .window_days
        .unwrap_or(state.config.display.default_trend_window_days);
    let result = trend::compute_trend(
        &state.pool,
        id,
        window,
        state.config.trends.min_data_points,
        state.config.trends.rapid_change_threshold_pct,
        state.config.trends.projection_horizon_days,
    )
    .await?;
    Ok(Json(serde_json::to_value(result)?))
}

pub async fn dashboard_summary(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, HermesError> {
    let summary = biomarker::dashboard_summary(&state.pool).await?;
    Ok(Json(serde_json::to_value(summary)?))
}

pub async fn out_of_range(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, HermesError> {
    let items = biomarker::get_out_of_range(&state.pool).await?;
    let result: Vec<serde_json::Value> = items
        .into_iter()
        .map(|(bm, value, status)| {
            serde_json::json!({
                "biomarker": bm,
                "latest_value": value,
                "range_status": status,
            })
        })
        .collect();
    Ok(Json(serde_json::to_value(result)?))
}

pub async fn list_observations(
    Query(query): Query<ObservationListQuery>,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, HermesError> {
    let observations = if let Some(bm_id) = query.biomarker {
        observation::list_for_biomarker(
            &state.pool,
            bm_id,
            query.from.as_deref(),
            query.to.as_deref(),
        )
        .await?
    } else {
        observation::list_all(&state.pool, query.from.as_deref(), query.to.as_deref()).await?
    };
    Ok(Json(serde_json::to_value(observations)?))
}

pub async fn create_observation_json(
    State(state): State<AppState>,
    Json(obs): Json<NewObservation>,
) -> Result<Json<serde_json::Value>, HermesError> {
    let result = observation::add_observation(&state.pool, &state.catalog, &obs).await?;
    Ok(Json(serde_json::json!({
        "id": result.id,
        "biomarker_name": result.biomarker_name,
        "value": result.value,
        "unit": result.unit,
        "converted": result.converted,
    })))
}

pub async fn list_interventions(
    Query(query): Query<InterventionListQuery>,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, HermesError> {
    let active_only = query.active.unwrap_or(false);
    let interventions =
        crate::db::queries::list_interventions(&state.pool, active_only).await?;
    Ok(Json(serde_json::to_value(interventions)?))
}
