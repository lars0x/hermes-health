use sqlx::SqlitePool;

use crate::db::models::{Biomarker, NewBiomarker};
use crate::db::queries;
use crate::error::{HermesError, Result};
use crate::services::loinc::LoincCatalog;

/// Resolve a biomarker identifier (LOINC code, name, or alias) to a Biomarker record.
/// First checks the database (tracked biomarkers), then falls back to LOINC catalog.
pub async fn resolve_biomarker(
    pool: &SqlitePool,
    identifier: &str,
    catalog: &LoincCatalog,
) -> Result<Biomarker> {
    // 1. Try by LOINC code
    if let Some(bm) = queries::get_biomarker_by_loinc(pool, identifier).await? {
        return Ok(bm);
    }

    // 2. Try by name or alias in tracked biomarkers
    let all = queries::list_biomarkers(pool, None).await?;
    let identifier_lower = identifier.to_lowercase();

    for bm in &all {
        if bm.name.to_lowercase() == identifier_lower {
            return Ok(bm.clone());
        }
        for alias in bm.aliases_vec() {
            if alias.to_lowercase() == identifier_lower {
                return Ok(bm.clone());
            }
        }
    }

    // 3. Search LOINC catalog and auto-create if high confidence match
    let candidates = catalog.search(identifier, 1);
    if let Some(candidate) = candidates.first() {
        if candidate.confidence >= 0.85 {
            // Check if this LOINC code is already tracked
            if let Some(bm) = queries::get_biomarker_by_loinc(pool, &candidate.loinc_code).await? {
                return Ok(bm);
            }

            // Auto-create from LOINC catalog
            if let Some(entry) = catalog.get_by_code(&candidate.loinc_code) {
                let new_bm = NewBiomarker {
                    loinc_code: entry.loinc_num.clone(),
                    name: entry.component.clone(),
                    aliases: vec![identifier.to_string()],
                    unit: if entry.example_ucum_units.is_empty() {
                        "".to_string()
                    } else {
                        entry.example_ucum_units.clone()
                    },
                    category: entry.class.clone(),
                    reference_low: None,
                    reference_high: None,
                    optimal_low: None,
                    optimal_high: None,
                    source: "measured".to_string(),
                };

                let id = queries::insert_biomarker(pool, &new_bm).await?;
                return queries::get_biomarker_by_id(pool, id).await;
            }
        }
    }

    Err(HermesError::NotFound(format!(
        "Could not resolve biomarker: '{}'. Use a LOINC code, exact name, or known alias.",
        identifier
    )))
}

pub async fn list_biomarkers(
    pool: &SqlitePool,
    category: Option<&str>,
) -> Result<Vec<Biomarker>> {
    queries::list_biomarkers(pool, category).await
}

pub async fn get_biomarker(pool: &SqlitePool, id: i64) -> Result<Biomarker> {
    queries::get_biomarker_by_id(pool, id).await
}

/// Get all biomarkers whose latest observation is outside reference or optimal range
pub async fn get_out_of_range(pool: &SqlitePool) -> Result<Vec<(Biomarker, f64, String)>> {
    let biomarkers = queries::list_biomarkers(pool, None).await?;
    let latest_obs = queries::get_latest_observation_per_biomarker(pool).await?;
    let mut out_of_range = Vec::new();

    for bm in &biomarkers {
        if let Some(obs) = latest_obs.iter().find(|o| o.biomarker_id == bm.id) {
            let status = range_status(obs.value, bm);
            if status != "in_range" && status != "no_range" {
                out_of_range.push((bm.clone(), obs.value, status));
            }
        }
    }

    Ok(out_of_range)
}

/// Determine the range status of a value for a biomarker
pub fn range_status(value: f64, bm: &Biomarker) -> String {
    // If no ranges are defined at all, we can't assess
    let has_any_range = bm.reference_low.is_some()
        || bm.reference_high.is_some()
        || bm.optimal_low.is_some()
        || bm.optimal_high.is_some();
    if !has_any_range {
        return "no_range".to_string();
    }

    // Check reference range first
    if let (Some(low), Some(high)) = (bm.reference_low, bm.reference_high) {
        if value < low || value > high {
            return "out_of_reference".to_string();
        }
    } else if let Some(high) = bm.reference_high {
        if value > high {
            return "out_of_reference".to_string();
        }
    } else if let Some(low) = bm.reference_low {
        if value < low {
            return "out_of_reference".to_string();
        }
    }

    // Check optimal range
    if let (Some(low), Some(high)) = (bm.optimal_low, bm.optimal_high) {
        if value < low || value > high {
            return "suboptimal".to_string();
        }
    } else if let Some(high) = bm.optimal_high {
        if value > high {
            return "suboptimal".to_string();
        }
    } else if let Some(low) = bm.optimal_low {
        if value < low {
            return "suboptimal".to_string();
        }
    }

    "in_range".to_string()
}

/// Dashboard summary
pub async fn dashboard_summary(pool: &SqlitePool) -> Result<DashboardSummary> {
    let biomarkers = queries::list_biomarkers(pool, None).await?;
    let latest_obs = queries::get_latest_observation_per_biomarker(pool).await?;

    let total = biomarkers.len();
    let mut in_optimal = 0;
    let mut out_of_range = 0;
    let mut suboptimal = 0;

    for bm in &biomarkers {
        if let Some(obs) = latest_obs.iter().find(|o| o.biomarker_id == bm.id) {
            match range_status(obs.value, bm).as_str() {
                "in_range" => in_optimal += 1,
                "out_of_reference" => out_of_range += 1,
                "suboptimal" => suboptimal += 1,
                _ => {}
            }
        }
    }

    // Days since last lab
    let days_since_last = if let Some(latest) = latest_obs.iter().max_by_key(|o| &o.observed_at) {
        if let Some(date) = latest.observed_date() {
            let today = chrono::Local::now().date_naive();
            (today - date).num_days()
        } else {
            -1
        }
    } else {
        -1
    };

    Ok(DashboardSummary {
        total_tracked: total,
        in_optimal,
        out_of_range,
        suboptimal,
        days_since_last_lab: days_since_last,
    })
}

#[derive(Debug, serde::Serialize)]
pub struct DashboardSummary {
    pub total_tracked: usize,
    pub in_optimal: usize,
    pub out_of_range: usize,
    pub suboptimal: usize,
    pub days_since_last_lab: i64,
}
