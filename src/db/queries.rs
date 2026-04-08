use sqlx::SqlitePool;

use crate::db::models::*;
use crate::error::{HermesError, Result};

// --- Biomarkers ---

pub async fn insert_biomarker(pool: &SqlitePool, b: &NewBiomarker) -> Result<i64> {
    let aliases_json = serde_json::to_string(&b.aliases)?;
    let result = sqlx::query(
        "INSERT INTO biomarkers (loinc_code, name, aliases, unit, category, reference_low, reference_high, optimal_low, optimal_high, source)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&b.loinc_code)
    .bind(&b.name)
    .bind(&aliases_json)
    .bind(&b.unit)
    .bind(&b.category)
    .bind(b.reference_low)
    .bind(b.reference_high)
    .bind(b.optimal_low)
    .bind(b.optimal_high)
    .bind(&b.source)
    .execute(pool)
    .await?;

    Ok(result.last_insert_rowid())
}

pub async fn get_biomarker_by_id(pool: &SqlitePool, id: i64) -> Result<Biomarker> {
    sqlx::query_as::<_, Biomarker>("SELECT * FROM biomarkers WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| HermesError::NotFound(format!("biomarker id={id}")))
}

pub async fn get_biomarker_by_loinc(pool: &SqlitePool, loinc_code: &str) -> Result<Option<Biomarker>> {
    let result = sqlx::query_as::<_, Biomarker>("SELECT * FROM biomarkers WHERE loinc_code = ?")
        .bind(loinc_code)
        .fetch_optional(pool)
        .await?;
    Ok(result)
}

pub async fn list_biomarkers(pool: &SqlitePool, category: Option<&str>) -> Result<Vec<Biomarker>> {
    let biomarkers = if let Some(cat) = category {
        sqlx::query_as::<_, Biomarker>("SELECT * FROM biomarkers WHERE category = ? ORDER BY name")
            .bind(cat)
            .fetch_all(pool)
            .await?
    } else {
        sqlx::query_as::<_, Biomarker>("SELECT * FROM biomarkers ORDER BY category, name")
            .fetch_all(pool)
            .await?
    };
    Ok(biomarkers)
}

pub async fn update_biomarker_aliases(pool: &SqlitePool, id: i64, aliases: &[String]) -> Result<()> {
    let aliases_json = serde_json::to_string(aliases)?;
    sqlx::query("UPDATE biomarkers SET aliases = ? WHERE id = ?")
        .bind(&aliases_json)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

#[allow(dead_code)]
pub async fn update_biomarker_ranges(
    pool: &SqlitePool,
    id: i64,
    reference_low: Option<f64>,
    reference_high: Option<f64>,
    optimal_low: Option<f64>,
    optimal_high: Option<f64>,
) -> Result<()> {
    sqlx::query(
        "UPDATE biomarkers SET reference_low = ?, reference_high = ?, optimal_low = ?, optimal_high = ? WHERE id = ?"
    )
    .bind(reference_low)
    .bind(reference_high)
    .bind(optimal_low)
    .bind(optimal_high)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

// --- Observations ---

#[allow(clippy::too_many_arguments)]
pub async fn insert_observation(
    pool: &SqlitePool,
    biomarker_id: i64,
    value: f64,
    original_value: &str,
    original_unit: &str,
    precision: i32,
    observed_at: &str,
    lab_name: Option<&str>,
    report_id: Option<i64>,
    import_id: Option<i64>,
    fasting: Option<bool>,
    notes: Option<&str>,
    detection_limit: Option<&str>,
) -> Result<i64> {
    let result = sqlx::query(
        "INSERT INTO observations (biomarker_id, value, original_value, original_unit, precision, observed_at, lab_name, report_id, import_id, fasting, notes, detection_limit)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(biomarker_id)
    .bind(value)
    .bind(original_value)
    .bind(original_unit)
    .bind(precision)
    .bind(observed_at)
    .bind(lab_name)
    .bind(report_id)
    .bind(import_id)
    .bind(fasting)
    .bind(notes)
    .bind(detection_limit)
    .execute(pool)
    .await?;

    Ok(result.last_insert_rowid())
}

pub async fn list_observations_for_biomarker(
    pool: &SqlitePool,
    biomarker_id: i64,
    from_date: Option<&str>,
    to_date: Option<&str>,
) -> Result<Vec<Observation>> {
    let mut query = String::from(
        "SELECT * FROM observations WHERE biomarker_id = ?"
    );
    if from_date.is_some() {
        query.push_str(" AND observed_at >= ?");
    }
    if to_date.is_some() {
        query.push_str(" AND observed_at <= ?");
    }
    query.push_str(" ORDER BY observed_at ASC");

    let mut q = sqlx::query_as::<_, Observation>(&query).bind(biomarker_id);
    if let Some(from) = from_date {
        q = q.bind(from);
    }
    if let Some(to) = to_date {
        q = q.bind(to);
    }

    let observations = q.fetch_all(pool).await?;
    Ok(observations)
}

pub async fn list_all_observations(
    pool: &SqlitePool,
    from_date: Option<&str>,
    to_date: Option<&str>,
) -> Result<Vec<Observation>> {
    let mut query = String::from("SELECT * FROM observations WHERE 1=1");
    if from_date.is_some() {
        query.push_str(" AND observed_at >= ?");
    }
    if to_date.is_some() {
        query.push_str(" AND observed_at <= ?");
    }
    query.push_str(" ORDER BY observed_at ASC");

    let mut q = sqlx::query_as::<_, Observation>(&query);
    if let Some(from) = from_date {
        q = q.bind(from);
    }
    if let Some(to) = to_date {
        q = q.bind(to);
    }

    let observations = q.fetch_all(pool).await?;
    Ok(observations)
}

#[allow(dead_code)]
pub async fn delete_observation(pool: &SqlitePool, id: i64) -> Result<()> {
    sqlx::query("DELETE FROM observations WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn count_observations_for_import(pool: &SqlitePool, import_id: i64) -> Result<i64> {
    let result: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM observations WHERE import_id = ?")
        .bind(import_id)
        .fetch_one(pool)
        .await?;
    Ok(result.0)
}

pub async fn list_observations_for_import(pool: &SqlitePool, import_id: i64) -> Result<Vec<Observation>> {
    let observations = sqlx::query_as::<_, Observation>(
        "SELECT o.* FROM observations o WHERE o.import_id = ? ORDER BY o.id"
    )
    .bind(import_id)
    .fetch_all(pool)
    .await?;
    Ok(observations)
}

pub async fn delete_observations_by_import(pool: &SqlitePool, import_id: i64) -> Result<u64> {
    let result = sqlx::query("DELETE FROM observations WHERE import_id = ?")
        .bind(import_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

// --- Unit Conversions ---

pub async fn insert_unit_conversion(
    pool: &SqlitePool,
    biomarker_id: i64,
    from_unit: &str,
    to_unit: &str,
    factor: f64,
    offset: f64,
) -> Result<i64> {
    let result = sqlx::query(
        "INSERT OR REPLACE INTO unit_conversions (biomarker_id, from_unit, to_unit, factor, offset)
         VALUES (?, ?, ?, ?, ?)"
    )
    .bind(biomarker_id)
    .bind(from_unit)
    .bind(to_unit)
    .bind(factor)
    .bind(offset)
    .execute(pool)
    .await?;

    Ok(result.last_insert_rowid())
}

pub async fn get_unit_conversion(
    pool: &SqlitePool,
    biomarker_id: i64,
    from_unit: &str,
) -> Result<Option<UnitConversion>> {
    let result = sqlx::query_as::<_, UnitConversion>(
        "SELECT * FROM unit_conversions WHERE biomarker_id = ? AND from_unit = ?"
    )
    .bind(biomarker_id)
    .bind(from_unit)
    .fetch_optional(pool)
    .await?;
    Ok(result)
}

#[allow(dead_code)]
pub async fn list_unit_conversions_for_biomarker(
    pool: &SqlitePool,
    biomarker_id: i64,
) -> Result<Vec<UnitConversion>> {
    let result = sqlx::query_as::<_, UnitConversion>(
        "SELECT * FROM unit_conversions WHERE biomarker_id = ? ORDER BY from_unit"
    )
    .bind(biomarker_id)
    .fetch_all(pool)
    .await?;
    Ok(result)
}

// --- Reports ---

pub async fn insert_report(
    pool: &SqlitePool,
    filename: &str,
    file_hash: &str,
    file_path: &str,
    format: &str,
) -> Result<i64> {
    let result = sqlx::query(
        "INSERT INTO reports (filename, file_hash, file_path, format)
         VALUES (?, ?, ?, ?)"
    )
    .bind(filename)
    .bind(file_hash)
    .bind(file_path)
    .bind(format)
    .execute(pool)
    .await?;
    Ok(result.last_insert_rowid())
}

pub async fn get_report_by_hash(pool: &SqlitePool, file_hash: &str) -> Result<Option<Report>> {
    let result = sqlx::query_as::<_, Report>("SELECT * FROM reports WHERE file_hash = ?")
        .bind(file_hash)
        .fetch_optional(pool)
        .await?;
    Ok(result)
}

#[allow(dead_code)]
pub async fn update_report_status(
    pool: &SqlitePool,
    id: i64,
    status: &str,
    raw_extraction: Option<&str>,
) -> Result<()> {
    sqlx::query("UPDATE reports SET extraction_status = ?, raw_extraction = ? WHERE id = ?")
        .bind(status)
        .bind(raw_extraction)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_report_by_id(pool: &SqlitePool, id: i64) -> Result<Report> {
    sqlx::query_as::<_, Report>("SELECT * FROM reports WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| HermesError::NotFound(format!("report id={id}")))
}

#[allow(dead_code)]
pub async fn list_reports(pool: &SqlitePool) -> Result<Vec<Report>> {
    let reports = sqlx::query_as::<_, Report>("SELECT * FROM reports ORDER BY imported_at DESC")
        .fetch_all(pool)
        .await?;
    Ok(reports)
}

#[allow(clippy::too_many_arguments)]
#[allow(dead_code)]
pub async fn update_report_extraction(
    pool: &SqlitePool,
    id: i64,
    status: &str,
    raw_extraction: Option<&str>,
    model_used: Option<&str>,
    agent_turns: i64,
    extracted_count: i64,
    unresolved_count: i64,
) -> Result<()> {
    sqlx::query(
        "UPDATE reports SET extraction_status = ?, raw_extraction = ?, model_used = ?, agent_turns = ?, extracted_count = ?, unresolved_count = ? WHERE id = ?"
    )
    .bind(status)
    .bind(raw_extraction)
    .bind(model_used)
    .bind(agent_turns)
    .bind(extracted_count)
    .bind(unresolved_count)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

// --- Imports ---

pub async fn create_import(pool: &SqlitePool, report_id: i64, model: &str) -> Result<i64> {
    let result = sqlx::query(
        "INSERT INTO imports (report_id, model_used, status) VALUES (?, ?, 'pending')"
    )
    .bind(report_id)
    .bind(model)
    .execute(pool)
    .await?;
    Ok(result.last_insert_rowid())
}

pub async fn get_import_by_id(pool: &SqlitePool, id: i64) -> Result<Import> {
    sqlx::query_as::<_, Import>("SELECT * FROM imports WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| HermesError::NotFound(format!("import id={id}")))
}

pub async fn list_imports(pool: &SqlitePool) -> Result<Vec<Import>> {
    let imports = sqlx::query_as::<_, Import>("SELECT * FROM imports ORDER BY created_at DESC")
        .fetch_all(pool)
        .await?;
    Ok(imports)
}

#[allow(dead_code)]
pub async fn list_imports_for_report(pool: &SqlitePool, report_id: i64) -> Result<Vec<Import>> {
    let imports = sqlx::query_as::<_, Import>(
        "SELECT * FROM imports WHERE report_id = ? ORDER BY created_at DESC"
    )
    .bind(report_id)
    .fetch_all(pool)
    .await?;
    Ok(imports)
}

pub async fn update_import_status(pool: &SqlitePool, id: i64, status: &str) -> Result<()> {
    sqlx::query("UPDATE imports SET status = ? WHERE id = ?")
        .bind(status)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn update_import_result(
    pool: &SqlitePool,
    id: i64,
    status: &str,
    raw_extraction: Option<&str>,
    agent_turns: i64,
    extracted_count: i64,
    unresolved_count: i64,
    test_date: Option<&str>,
) -> Result<()> {
    sqlx::query(
        "UPDATE imports SET status = ?, raw_extraction = ?, agent_turns = ?, extracted_count = ?, unresolved_count = ?, test_date = ? WHERE id = ?"
    )
    .bind(status)
    .bind(raw_extraction)
    .bind(agent_turns)
    .bind(extracted_count)
    .bind(unresolved_count)
    .bind(test_date)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

// --- Import Overwrites ---

pub async fn upsert_import_overwrite(
    pool: &SqlitePool,
    import_id: i64,
    loinc_code: &str,
    chosen_idx: i64,
) -> Result<()> {
    sqlx::query(
        "INSERT OR REPLACE INTO import_overwrites (import_id, loinc_code, chosen_idx) VALUES (?, ?, ?)"
    )
    .bind(import_id)
    .bind(loinc_code)
    .bind(chosen_idx)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_import_overwrites(pool: &SqlitePool, import_id: i64) -> Result<Vec<ImportOverwrite>> {
    let overwrites = sqlx::query_as::<_, ImportOverwrite>(
        "SELECT * FROM import_overwrites WHERE import_id = ? ORDER BY id"
    )
    .bind(import_id)
    .fetch_all(pool)
    .await?;
    Ok(overwrites)
}

// --- Interventions ---

pub async fn insert_intervention(pool: &SqlitePool, i: &NewIntervention) -> Result<i64> {
    let result = sqlx::query(
        "INSERT INTO interventions (name, category, dosage, frequency, started_at, ended_at, notes)
         VALUES (?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&i.name)
    .bind(&i.category)
    .bind(&i.dosage)
    .bind(&i.frequency)
    .bind(&i.started_at)
    .bind(&i.ended_at)
    .bind(&i.notes)
    .execute(pool)
    .await?;

    let intervention_id = result.last_insert_rowid();

    for target in &i.target_biomarkers {
        sqlx::query(
            "INSERT INTO intervention_biomarker_targets (intervention_id, biomarker_id, expected_effect)
             VALUES (?, ?, ?)"
        )
        .bind(intervention_id)
        .bind(target.biomarker_id)
        .bind(&target.expected_effect)
        .execute(pool)
        .await?;
    }

    Ok(intervention_id)
}

pub async fn list_interventions(pool: &SqlitePool, active_only: bool) -> Result<Vec<Intervention>> {
    let query = if active_only {
        "SELECT * FROM interventions WHERE ended_at IS NULL ORDER BY started_at DESC"
    } else {
        "SELECT * FROM interventions ORDER BY started_at DESC"
    };
    let interventions = sqlx::query_as::<_, Intervention>(query)
        .fetch_all(pool)
        .await?;
    Ok(interventions)
}

pub async fn get_intervention_targets(
    pool: &SqlitePool,
    intervention_id: i64,
) -> Result<Vec<InterventionBiomarkerTarget>> {
    let targets = sqlx::query_as::<_, InterventionBiomarkerTarget>(
        "SELECT * FROM intervention_biomarker_targets WHERE intervention_id = ?"
    )
    .bind(intervention_id)
    .fetch_all(pool)
    .await?;
    Ok(targets)
}

pub async fn get_intervention_by_id(pool: &SqlitePool, id: i64) -> Result<Intervention> {
    sqlx::query_as::<_, Intervention>("SELECT * FROM interventions WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| HermesError::NotFound(format!("intervention id={id}")))
}

pub async fn end_intervention(pool: &SqlitePool, id: i64, ended_at: &str) -> Result<()> {
    sqlx::query("UPDATE interventions SET ended_at = ? WHERE id = ?")
        .bind(ended_at)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

#[allow(dead_code)]
pub async fn delete_intervention_targets(pool: &SqlitePool, intervention_id: i64) -> Result<()> {
    sqlx::query("DELETE FROM intervention_biomarker_targets WHERE intervention_id = ?")
        .bind(intervention_id)
        .execute(pool)
        .await?;
    Ok(())
}

#[allow(dead_code)]
pub async fn insert_intervention_target(
    pool: &SqlitePool,
    intervention_id: i64,
    biomarker_id: i64,
    expected_effect: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT OR REPLACE INTO intervention_biomarker_targets (intervention_id, biomarker_id, expected_effect) VALUES (?, ?, ?)"
    )
    .bind(intervention_id)
    .bind(biomarker_id)
    .bind(expected_effect)
    .execute(pool)
    .await?;
    Ok(())
}

// --- Dashboard helpers ---

pub async fn count_biomarkers(pool: &SqlitePool) -> Result<i64> {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM biomarkers")
        .fetch_one(pool)
        .await?;
    Ok(row.0)
}

pub async fn get_latest_observation_per_biomarker(pool: &SqlitePool) -> Result<Vec<Observation>> {
    let observations = sqlx::query_as::<_, Observation>(
        "SELECT o.* FROM observations o
         INNER JOIN (
             SELECT biomarker_id, MAX(observed_at) as max_date
             FROM observations
             GROUP BY biomarker_id
         ) latest ON o.biomarker_id = latest.biomarker_id AND o.observed_at = latest.max_date
         ORDER BY o.biomarker_id"
    )
    .fetch_all(pool)
    .await?;
    Ok(observations)
}
