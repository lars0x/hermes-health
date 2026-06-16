use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};
use axum::Form;
use serde::Deserialize;

use crate::db::queries;
use crate::error::HermesError;
use crate::web::htmx;
use crate::web::AppState;

/// Render the settings page.
pub async fn settings_page(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Html<String>, HermesError> {
    let is_htmx = htmx::is_htmx_request(&headers);
    let sex = queries::get_setting(&state.pool, "profile_sex").await?;
    // Stored canonically as YYYY-MM-DD. The visible field shows DD/MM/YYYY; the
    // hidden native date input (calendar popup) needs the ISO value.
    let dob_iso = queries::get_setting(&state.pool, "profile_dob")
        .await?
        .filter(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").is_ok())
        .unwrap_or_default();
    let dob_display = chrono::NaiveDate::parse_from_str(&dob_iso, "%Y-%m-%d")
        .map(|d| d.format("%d/%m/%Y").to_string())
        .unwrap_or_default();
    let ctx = minijinja::context! {
        is_fragment => is_htmx,
        current_path => "/settings",
        profile_sex => sex.unwrap_or_default(),
        profile_dob => dob_display,
        profile_dob_iso => dob_iso,
    };
    state.templates.render("pages/settings.html", ctx).map(Html)
}

#[derive(Deserialize)]
pub struct ProfileForm {
    pub sex: Option<String>,
    pub dob: Option<String>,
}

/// Save the profile (sex + date of birth) that drives sex/age range resolution.
/// Empty values clear the setting (back to the sex-agnostic / open-age fallback).
pub async fn update_profile(
    State(state): State<AppState>,
    Form(form): Form<ProfileForm>,
) -> Result<Html<String>, HermesError> {
    // Sex: only 'male'/'female' are stored; anything else clears it.
    match form.sex.as_deref() {
        Some("male") => queries::set_setting(&state.pool, "profile_sex", "male").await?,
        Some("female") => queries::set_setting(&state.pool, "profile_sex", "female").await?,
        _ => {
            queries::set_setting(&state.pool, "profile_sex", "").await?;
        }
    }

    // DOB comes in as DD/MM/YYYY (also accept YYYY-MM-DD); store canonically as
    // YYYY-MM-DD. Anything unparseable clears the setting.
    let dob = form
        .dob
        .as_deref()
        .map(str::trim)
        .and_then(|d| {
            chrono::NaiveDate::parse_from_str(d, "%d/%m/%Y")
                .or_else(|_| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d"))
                .ok()
        })
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_default();
    queries::set_setting(&state.pool, "profile_dob", &dob).await?;

    Ok(Html(
        r#"<div class="alert alert-success">Profile saved. Ranges now resolve to your sex and age.</div>"#.to_string(),
    ))
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
