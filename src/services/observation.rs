use sqlx::SqlitePool;

use crate::db::models::{NewObservation, Observation};
use crate::db::queries;
use crate::error::{HermesError, Result};
use crate::ingest::normalize;
use crate::services::biomarker;
use crate::services::loinc::LoincCatalog;

/// Result of adding a single observation
#[derive(Debug)]
pub struct ObservationResult {
    pub id: i64,
    pub biomarker_name: String,
    pub value: f64,
    pub unit: String,
    pub converted: bool,
}

/// Result of a batch observation insert
#[derive(Debug)]
pub struct BatchResult {
    pub successes: Vec<ObservationResult>,
    pub failures: Vec<BatchFailure>,
}

#[derive(Debug)]
pub struct BatchFailure {
    pub index: usize,
    pub biomarker: String,
    pub error: String,
}

/// Add a single observation with full normalization pipeline.
pub async fn add_observation(
    pool: &SqlitePool,
    catalog: &LoincCatalog,
    obs: &NewObservation,
) -> Result<ObservationResult> {
    // 1. Resolve biomarker
    let bm = biomarker::resolve_biomarker(pool, &obs.biomarker, catalog).await?;

    // 2. Normalize value + unit
    let original_value_str = format!("{}", obs.value);
    let normalized = normalize::normalize_observation(
        pool,
        bm.id,
        &bm.unit,
        &original_value_str,
        &obs.unit,
    )
    .await?;

    // 3. Combine notes
    let notes = match (&obs.notes, &normalized.notes_append) {
        (Some(n), Some(a)) => Some(format!("{n}; {a}")),
        (Some(n), None) => Some(n.clone()),
        (None, Some(a)) => Some(a.clone()),
        (None, None) => None,
    };

    // 4. Insert
    let id = queries::insert_observation(
        pool,
        bm.id,
        normalized.value,
        &normalized.original_value,
        &normalized.original_unit,
        normalized.precision,
        &obs.observed_at,
        obs.lab_name.as_deref(),
        obs.report_id,
        obs.import_id,
        obs.fasting,
        notes.as_deref(),
        normalized.detection_limit.as_deref(),
    )
    .await?;

    let converted = normalized.original_unit != normalized.canonical_unit;

    Ok(ObservationResult {
        id,
        biomarker_name: bm.name.clone(),
        value: normalized.value,
        unit: bm.unit.clone(),
        converted,
    })
}

/// Add a batch of observations. Each is processed independently; failures don't block successes.
pub async fn add_batch(
    pool: &SqlitePool,
    catalog: &LoincCatalog,
    observations: &[NewObservation],
) -> Result<BatchResult> {
    let mut successes = Vec::new();
    let mut failures = Vec::new();

    for (i, obs) in observations.iter().enumerate() {
        match add_observation(pool, catalog, obs).await {
            Ok(result) => successes.push(result),
            Err(e) => failures.push(BatchFailure {
                index: i,
                biomarker: obs.biomarker.clone(),
                error: e.to_string(),
            }),
        }
    }

    Ok(BatchResult {
        successes,
        failures,
    })
}

/// List observations for a specific biomarker
pub async fn list_for_biomarker(
    pool: &SqlitePool,
    biomarker_id: i64,
    from_date: Option<&str>,
    to_date: Option<&str>,
) -> Result<Vec<Observation>> {
    queries::list_observations_for_biomarker(pool, biomarker_id, from_date, to_date).await
}

/// List all observations
pub async fn list_all(
    pool: &SqlitePool,
    from_date: Option<&str>,
    to_date: Option<&str>,
) -> Result<Vec<Observation>> {
    queries::list_all_observations(pool, from_date, to_date).await
}
