use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::Html;
use axum::routing::{get, post};
use axum::Router;

use crate::error::HermesError;
use crate::web::handlers;
use crate::web::htmx;
use crate::web::AppState;

async fn settings_page(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Html<String>, HermesError> {
    let is_htmx = htmx::is_htmx_request(&headers);
    let ctx = minijinja::context! {
        is_fragment => is_htmx,
        current_path => "/settings",
    };
    state.templates.render("pages/settings.html", ctx).map(Html)
}

pub fn router() -> Router<AppState> {
    Router::new()
        // HTML pages
        .route("/", get(handlers::dashboard::dashboard))
        .route("/biomarkers", get(handlers::biomarker::biomarkers_list))
        .route("/biomarkers/{id}", get(handlers::biomarker::biomarker_detail))
        .route("/entry", get(handlers::observation::data_entry_page))
        .route("/interventions", get(handlers::intervention::interventions_page))
        .route("/interventions/{id}", get(handlers::intervention::intervention_detail))
        .route("/import", get(handlers::report::import_page))
        .route("/imports/list", get(handlers::report::imports_list))
        .route("/imports/{id}", get(handlers::report::import_detail))
        .route("/settings", get(settings_page))
        // HTMX partials
        .route(
            "/api/v1/biomarkers/search",
            get(handlers::biomarker::biomarker_search),
        )
        .route(
            "/api/v1/biomarkers/dashboard",
            get(handlers::api::dashboard_summary),
        )
        .route(
            "/api/v1/biomarkers/out-of-range",
            get(handlers::api::out_of_range),
        )
        // Parameterized routes after literal ones
        .route(
            "/api/v1/biomarkers/{id}",
            get(handlers::api::get_biomarker),
        )
        .route(
            "/api/v1/biomarkers/{id}/trend",
            get(handlers::api::get_trend),
        )
        .route(
            "/api/v1/biomarkers/{id}/chart",
            get(handlers::biomarker::biomarker_chart),
        )
        // JSON API
        .route(
            "/api/v1/biomarkers",
            get(handlers::api::list_biomarkers),
        )
        .route(
            "/api/v1/observations",
            get(handlers::api::list_observations)
                .post(handlers::api::create_observation_json),
        )
        .route(
            "/api/v1/interventions",
            get(handlers::api::list_interventions),
        )
        // Interventions
        .route(
            "/api/v1/interventions/create",
            post(handlers::intervention::create_intervention),
        )
        .route(
            "/api/v1/interventions/{id}/end",
            post(handlers::intervention::end_intervention),
        )
        // Reports & Imports
        .route(
            "/api/v1/reports/upload",
            post(handlers::report::upload),
        )
        .route(
            "/api/v1/imports/{id}/status",
            get(handlers::report::import_status),
        )
        .route(
            "/api/v1/imports/{id}/extraction",
            get(handlers::report::get_extraction_json),
        )
        .route(
            "/api/v1/imports/{id}/commit",
            post(handlers::report::commit),
        )
        .route(
            "/api/v1/imports/{id}/uncommit",
            post(handlers::report::uncommit),
        )
        .route(
            "/api/v1/imports/{id}/decline",
            post(handlers::report::decline),
        )
        .route(
            "/api/v1/imports/{id}/resolve-duplicate",
            post(handlers::report::resolve_duplicate),
        )
        .route(
            "/api/v1/imports/{id}/map",
            post(handlers::report::map_marker),
        )
        // Form submission (HTMX)
        .route(
            "/observations/create",
            post(handlers::observation::create_observation),
        )
        // Static assets
        .route("/static/{*path}", get(handlers::static_file))
        .layer(tower_http::compression::CompressionLayer::new())
}
