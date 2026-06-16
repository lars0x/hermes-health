use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};

use crate::error::HermesError;
use crate::web::htmx;
use crate::web::AppState;

/// Render the settings page.
pub async fn settings_page(
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

/// Directory uploaded report files are written to (see report::upload).
const REPORTS_DIR: &str = "data/reports";

/// Delete all data and return to the first-boot state.
///
/// Rather than enumerating tables (which would need updating for every new
/// feature), this drops every table in the database and re-runs the migration
/// runner, which recreates the schema and re-seeds the default biomarkers. It
/// then replays the boot-time conversion seeding and clears uploaded files, so
/// the result is identical to launching the app on a fresh database.
pub async fn delete_all_data(
    State(state): State<AppState>,
) -> Result<Response, HermesError> {
    // 1. Drop every table. Foreign keys are enforced on the pool, so disable
    //    enforcement on this connection to allow dropping in any order.
    let mut conn = state.pool.acquire().await?;
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&mut *conn)
        .await?;

    let tables: Vec<(String,)> = sqlx::query_as(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
    )
    .fetch_all(&mut *conn)
    .await?;

    for (table,) in &tables {
        sqlx::query(&format!("DROP TABLE IF EXISTS \"{table}\""))
            .execute(&mut *conn)
            .await?;
    }

    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut *conn)
        .await?;
    drop(conn);

    // 2. Recreate schema + re-seed biomarkers (and their unit conversions).
    crate::db::migrate::run_migrations(&state.pool).await?;

    // 3. Replay the boot-time generic conversion seeding (see Serve in main.rs)
    //    so the conversion set matches a fresh launch exactly.
    crate::services::conversions::seed_conversions_from_miracum(&state.pool).await?;

    // 4. Delete uploaded report files (keep the .gitkeep placeholder).
    if let Ok(entries) = std::fs::read_dir(REPORTS_DIR) {
        for entry in entries.flatten() {
            if entry.file_name() == ".gitkeep" {
                continue;
            }
            if entry.path().is_file() {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    tracing::info!("All data deleted; database reset to fresh-install state");

    // Tell HTMX to navigate to the dashboard, which now shows a clean slate.
    Ok(([("HX-Redirect", "/")], Html(String::new())).into_response())
}
