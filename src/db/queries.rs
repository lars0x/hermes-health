use sqlx::SqlitePool;

use crate::db::models::*;
use crate::error::{HermesError, Result};

// --- Biomarkers ---

pub async fn insert_biomarker(pool: &SqlitePool, b: &NewBiomarker) -> Result<i64> {
    let aliases_json = serde_json::to_string(&b.aliases)?;
    let result = sqlx::query(
        "INSERT INTO biomarkers (loinc_code, name, aliases, unit, category, source)
         VALUES (?, ?, ?, ?, ?, ?)"
    )
    .bind(&b.loinc_code)
    .bind(&b.name)
    .bind(&aliases_json)
    .bind(&b.unit)
    .bind(&b.category)
    .bind(&b.source)
    .execute(pool)
    .await?;

    Ok(result.last_insert_rowid())
}

pub async fn get_biomarker_by_id(pool: &SqlitePool, id: i64) -> Result<Biomarker> {
    let mut bm = sqlx::query_as::<_, Biomarker>("SELECT * FROM biomarkers WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| HermesError::NotFound(format!("biomarker id={id}")))?;
    apply_ranges(pool, std::slice::from_mut(&mut bm)).await?;
    Ok(bm)
}

pub async fn get_biomarker_by_loinc(pool: &SqlitePool, loinc_code: &str) -> Result<Option<Biomarker>> {
    let mut result = sqlx::query_as::<_, Biomarker>("SELECT * FROM biomarkers WHERE loinc_code = ?")
        .bind(loinc_code)
        .fetch_optional(pool)
        .await?;
    if let Some(bm) = result.as_mut() {
        apply_ranges(pool, std::slice::from_mut(bm)).await?;
    }
    Ok(result)
}

pub async fn list_biomarkers(pool: &SqlitePool, category: Option<&str>) -> Result<Vec<Biomarker>> {
    let mut biomarkers = if let Some(cat) = category {
        sqlx::query_as::<_, Biomarker>("SELECT * FROM biomarkers WHERE category = ? ORDER BY name")
            .bind(cat)
            .fetch_all(pool)
            .await?
    } else {
        sqlx::query_as::<_, Biomarker>("SELECT * FROM biomarkers ORDER BY category, name")
            .fetch_all(pool)
            .await?
    };
    apply_ranges(pool, &mut biomarkers).await?;
    Ok(biomarkers)
}

// --- App settings (key/value) ---

pub async fn get_setting(pool: &SqlitePool, key: &str) -> Result<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as("SELECT value FROM app_settings WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| r.0))
}

pub async fn set_setting(pool: &SqlitePool, key: &str, value: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO app_settings (key, value) VALUES (?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

// --- Reference / optimal range resolution ---

#[derive(sqlx::FromRow)]
struct RangeRow {
    biomarker_id: i64,
    sex: String,
    age_min: i64,
    age_max: i64,
    low: Option<f64>,
    high: Option<f64>,
    source: Option<String>,
}

/// The configured profile sex ('male'/'female') and the profile's current age in
/// years (computed from the stored date of birth). Either may be None when unset.
async fn profile_sex_age(pool: &SqlitePool) -> (Option<String>, Option<i64>) {
    use chrono::Datelike;

    let sex = get_setting(pool, "profile_sex")
        .await
        .ok()
        .flatten()
        .filter(|s| s == "male" || s == "female");

    let age = get_setting(pool, "profile_dob")
        .await
        .ok()
        .flatten()
        .and_then(|dob| chrono::NaiveDate::parse_from_str(&dob, "%Y-%m-%d").ok())
        .map(|dob| {
            let today = chrono::Local::now().date_naive();
            let mut years = today.year() - dob.year();
            if (today.month(), today.day()) < (dob.month(), dob.day()) {
                years -= 1;
            }
            years as i64
        });

    (sex, age)
}

/// Pick the most appropriate default range row for the given sex + age from a
/// biomarker's candidate rows. Prefers the configured sex over 'any', and the
/// narrowest matching age band; with age unknown, falls back to the widest
/// (open-age) band.
fn pick_range<'a>(
    rows: &'a [RangeRow],
    sex: &Option<String>,
    age: &Option<i64>,
) -> Option<&'a RangeRow> {
    let prefs: Vec<&str> = match sex {
        Some(s) => vec![s.as_str(), "any"],
        None => vec!["any"],
    };
    for pref in prefs {
        let cands = rows.iter().filter(|r| r.sex == pref);
        let best = match age {
            Some(a) => cands
                .filter(|r| r.age_min <= *a && *a <= r.age_max)
                .min_by_key(|r| r.age_max - r.age_min),
            None => cands.max_by_key(|r| r.age_max - r.age_min),
        };
        if best.is_some() {
            return best;
        }
    }
    None
}

/// Resolve and fill the reference/optimal range fields on each biomarker from the
/// reference_ranges and optimal_ranges tables, using the profile's sex and age.
/// Leaves the fields as None where a biomarker has no matching default range.
async fn apply_ranges(pool: &SqlitePool, biomarkers: &mut [Biomarker]) -> Result<()> {
    use std::collections::HashMap;

    if biomarkers.is_empty() {
        return Ok(());
    }

    let (sex, age) = profile_sex_age(pool).await;

    let load = |table: &'static str| async move {
        sqlx::query_as::<_, RangeRow>(&format!(
            "SELECT biomarker_id, sex, age_min, age_max, low, high, source \
             FROM {table} WHERE is_default = 1"
        ))
        .fetch_all(pool)
        .await
    };

    let mut ref_map: HashMap<i64, Vec<RangeRow>> = HashMap::new();
    for r in load("reference_ranges").await? {
        ref_map.entry(r.biomarker_id).or_default().push(r);
    }
    let mut opt_map: HashMap<i64, Vec<RangeRow>> = HashMap::new();
    for r in load("optimal_ranges").await? {
        opt_map.entry(r.biomarker_id).or_default().push(r);
    }

    for bm in biomarkers.iter_mut() {
        if let Some(best) = ref_map.get(&bm.id).and_then(|rows| pick_range(rows, &sex, &age)) {
            bm.reference_low = best.low;
            bm.reference_high = best.high;
            bm.reference_source = best.source.clone();
        }
        if let Some(best) = opt_map.get(&bm.id).and_then(|rows| pick_range(rows, &sex, &age)) {
            bm.optimal_low = best.low;
            bm.optimal_high = best.high;
            bm.optimal_source = best.source.clone();
        }
    }

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
    text_value: Option<&str>,
) -> Result<i64> {
    let result = sqlx::query(
        "INSERT INTO observations (biomarker_id, value, original_value, original_unit, precision, observed_at, lab_name, report_id, import_id, fasting, notes, detection_limit, text_value)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
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
    .bind(text_value)
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
    llm_log: Option<&str>,
) -> Result<()> {
    sqlx::query(
        "UPDATE imports SET status = ?, raw_extraction = ?, agent_turns = ?, extracted_count = ?, unresolved_count = ?, test_date = ?, llm_log = ?, completed_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE id = ?"
    )
    .bind(status)
    .bind(raw_extraction)
    .bind(agent_turns)
    .bind(extracted_count)
    .bind(unresolved_count)
    .bind(test_date)
    .bind(llm_log)
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
