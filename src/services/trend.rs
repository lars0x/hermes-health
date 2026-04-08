use chrono::NaiveDate;
use serde::Serialize;
use sqlx::SqlitePool;

use crate::db::models::{Biomarker, Observation};
use crate::db::queries;
use crate::error::Result;

#[derive(Debug, Clone, Serialize)]
pub struct TrendAnalysis {
    pub biomarker_id: i64,
    pub loinc_code: String,
    pub observations: Vec<ObservationPoint>,
    pub trend: Option<TrendStats>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ObservationPoint {
    pub date: String,
    pub value: f64,
    pub precision: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrendStats {
    pub direction: String,         // increasing, decreasing, stable
    pub slope: f64,                // units per year
    pub slope_unit: String,        // e.g. "mg/dL per year"
    pub r_squared: f64,            // goodness of fit
    pub rate_of_change_pct: f64,   // % change between two most recent
    pub annualized_rate_pct: f64,  // annualized rate from regression
    pub latest_value: f64,
    pub previous_value: Option<f64>,
    pub status: String,            // improving, worsening, stable, insufficient_data
    pub alerts: Vec<TrendAlert>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrendAlert {
    pub alert_type: String, // approaching_limit, rapid_change, reversal
    pub message: String,
}

/// Compute trend analysis for a biomarker within a time window.
pub async fn compute_trend(
    pool: &SqlitePool,
    biomarker_id: i64,
    window_days: u32,
    min_points: u32,
    rapid_change_threshold_pct: f64,
    projection_horizon_days: u32,
) -> Result<TrendAnalysis> {
    let bm = queries::get_biomarker_by_id(pool, biomarker_id).await?;
    let cutoff_date = chrono::Local::now()
        .date_naive()
        .checked_sub_days(chrono::Days::new(window_days as u64))
        .unwrap_or(NaiveDate::from_ymd_opt(2000, 1, 1).unwrap());

    let from_str = cutoff_date.format("%Y-%m-%d").to_string();
    let observations =
        queries::list_observations_for_biomarker(pool, biomarker_id, Some(&from_str), None).await?;

    let points: Vec<ObservationPoint> = observations
        .iter()
        .map(|o| ObservationPoint {
            date: o.observed_at.clone(),
            value: o.value,
            precision: o.precision,
        })
        .collect();

    let trend = if points.len() >= min_points as usize {
        Some(compute_stats(
            &observations,
            &bm,
            rapid_change_threshold_pct,
            projection_horizon_days,
        ))
    } else {
        None
    };

    Ok(TrendAnalysis {
        biomarker_id,
        loinc_code: bm.loinc_code,
        observations: points,
        trend,
    })
}

fn compute_stats(
    observations: &[Observation],
    bm: &Biomarker,
    rapid_change_threshold: f64,
    projection_days: u32,
) -> TrendStats {
    let n = observations.len();

    // Parse dates and compute days from first observation
    let dates: Vec<NaiveDate> = observations
        .iter()
        .filter_map(|o| o.observed_date())
        .collect();

    if dates.is_empty() {
        return insufficient_data_stats(observations);
    }

    let first_date = dates[0];
    let x: Vec<f64> = dates
        .iter()
        .map(|d| (*d - first_date).num_days() as f64)
        .collect();
    let y: Vec<f64> = observations.iter().map(|o| o.value).collect();

    // OLS linear regression: y = slope * x + intercept
    let (slope, _intercept, r_squared) = linear_regression(&x, &y);

    // Slope in units per year (x is in days)
    let slope_per_year = slope * 365.0;

    // Mean value
    let mean = y.iter().sum::<f64>() / n as f64;

    // Direction classification
    // "stable" if slope within +/-1% of mean per year
    let threshold = mean * 0.01; // 1% of mean
    let direction = if slope_per_year.abs() <= threshold {
        "stable"
    } else if slope_per_year > 0.0 {
        "increasing"
    } else {
        "decreasing"
    };

    // Rate of change between two most recent
    let latest = y[n - 1];
    let previous = if n >= 2 { Some(y[n - 2]) } else { None };
    let rate_of_change_pct = if let Some(prev) = previous {
        if prev != 0.0 {
            ((latest - prev) / prev) * 100.0
        } else {
            0.0
        }
    } else {
        0.0
    };

    // Annualized rate from regression
    let annualized_rate_pct = if mean != 0.0 {
        (slope_per_year / mean) * 100.0
    } else {
        0.0
    };

    // Status: contextual interpretation
    let status = determine_status(direction, bm, latest);

    // Alerts
    let mut alerts = Vec::new();

    // Alert: approaching_limit
    if let Some(alert) = check_approaching_limit(slope, &dates, latest, bm, projection_days) {
        alerts.push(alert);
    }

    // Alert: rapid_change
    if annualized_rate_pct.abs() > rapid_change_threshold {
        alerts.push(TrendAlert {
            alert_type: "rapid_change".to_string(),
            message: format!(
                "Rapid change detected: {:.1}% annualized rate exceeds {:.0}% threshold",
                annualized_rate_pct, rapid_change_threshold
            ),
        });
    }

    TrendStats {
        direction: direction.to_string(),
        slope: slope_per_year,
        slope_unit: format!("{} per year", bm.unit),
        r_squared,
        rate_of_change_pct,
        annualized_rate_pct,
        latest_value: latest,
        previous_value: previous,
        status,
        alerts,
    }
}

/// OLS linear regression. Returns (slope, intercept, r_squared).
fn linear_regression(x: &[f64], y: &[f64]) -> (f64, f64, f64) {
    let n = x.len() as f64;
    if n < 2.0 {
        return (0.0, y.first().copied().unwrap_or(0.0), 0.0);
    }

    let sum_x: f64 = x.iter().sum();
    let sum_y: f64 = y.iter().sum();
    let sum_xy: f64 = x.iter().zip(y.iter()).map(|(xi, yi)| xi * yi).sum();
    let sum_x2: f64 = x.iter().map(|xi| xi * xi).sum();

    let denominator = n * sum_x2 - sum_x * sum_x;
    if denominator.abs() < f64::EPSILON {
        return (0.0, sum_y / n, 0.0);
    }

    let slope = (n * sum_xy - sum_x * sum_y) / denominator;
    let intercept = (sum_y - slope * sum_x) / n;

    // R-squared
    let y_mean = sum_y / n;
    let ss_tot: f64 = y.iter().map(|yi| (yi - y_mean).powi(2)).sum();
    let ss_res: f64 = x
        .iter()
        .zip(y.iter())
        .map(|(xi, yi)| {
            let predicted = slope * xi + intercept;
            (yi - predicted).powi(2)
        })
        .sum();

    let r_squared = if ss_tot > f64::EPSILON {
        1.0 - (ss_res / ss_tot)
    } else {
        0.0
    };

    (slope, intercept, r_squared)
}

/// Determine whether a trend is "improving" or "worsening" based on biomarker ranges.
fn determine_status(direction: &str, bm: &Biomarker, latest: f64) -> String {
    if direction == "stable" {
        return "stable".to_string();
    }

    let is_increasing = direction == "increasing";

    // Check against optimal range
    if let Some(opt_high) = bm.optimal_high {
        if latest > opt_high && is_increasing {
            return "worsening".to_string();
        }
        if latest > opt_high && !is_increasing {
            return "improving".to_string();
        }
    }

    if let Some(opt_low) = bm.optimal_low {
        if latest < opt_low && !is_increasing {
            return "worsening".to_string();
        }
        if latest < opt_low && is_increasing {
            return "improving".to_string();
        }
    }

    // If within optimal range, check which direction moves us away
    if let (Some(opt_low), Some(opt_high)) = (bm.optimal_low, bm.optimal_high) {
        let mid = (opt_low + opt_high) / 2.0;
        if latest > mid && is_increasing {
            return "worsening".to_string(); // moving toward upper limit
        }
        if latest < mid && !is_increasing {
            return "worsening".to_string(); // moving toward lower limit
        }
    }

    // Default: if within range and not clearly worsening
    "stable".to_string()
}

/// Check if the current trend projects the value to cross a reference range boundary.
fn check_approaching_limit(
    slope_per_day: f64,
    _dates: &[NaiveDate],
    latest: f64,
    bm: &Biomarker,
    projection_days: u32,
) -> Option<TrendAlert> {
    if slope_per_day.abs() < f64::EPSILON {
        return None;
    }

    let projected = latest + slope_per_day * projection_days as f64;

    // Check reference range boundaries
    if let Some(ref_high) = bm.reference_high {
        if latest <= ref_high && projected > ref_high {
            let days_to_cross = ((ref_high - latest) / slope_per_day) as i64;
            return Some(TrendAlert {
                alert_type: "approaching_limit".to_string(),
                message: format!(
                    "Projected to exceed upper reference limit ({}) in ~{} days",
                    ref_high, days_to_cross
                ),
            });
        }
    }

    if let Some(ref_low) = bm.reference_low {
        if latest >= ref_low && projected < ref_low {
            let days_to_cross = ((ref_low - latest) / slope_per_day).abs() as i64;
            return Some(TrendAlert {
                alert_type: "approaching_limit".to_string(),
                message: format!(
                    "Projected to drop below lower reference limit ({}) in ~{} days",
                    ref_low, days_to_cross
                ),
            });
        }
    }

    None
}

fn insufficient_data_stats(observations: &[Observation]) -> TrendStats {
    let latest = observations.last().map(|o| o.value).unwrap_or(0.0);
    TrendStats {
        direction: "stable".to_string(),
        slope: 0.0,
        slope_unit: "".to_string(),
        r_squared: 0.0,
        rate_of_change_pct: 0.0,
        annualized_rate_pct: 0.0,
        latest_value: latest,
        previous_value: None,
        status: "insufficient_data".to_string(),
        alerts: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linear_regression_perfect_fit() {
        let x = vec![0.0, 1.0, 2.0, 3.0];
        let y = vec![10.0, 12.0, 14.0, 16.0];
        let (slope, intercept, r2) = linear_regression(&x, &y);
        assert!((slope - 2.0).abs() < 1e-10);
        assert!((intercept - 10.0).abs() < 1e-10);
        assert!((r2 - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_linear_regression_flat() {
        let x = vec![0.0, 30.0, 60.0, 90.0];
        let y = vec![100.0, 100.0, 100.0, 100.0];
        let (slope, _intercept, _r2) = linear_regression(&x, &y);
        assert!(slope.abs() < 1e-10);
    }

    #[test]
    fn test_direction_classification() {
        // slope_per_year = 5 for mean=100: 5% > 1% threshold -> "increasing"
        let x = vec![0.0, 365.0];
        let y = vec![97.5, 102.5]; // slope=5/365 per day, 5 per year, mean=100
        let (slope, _, _) = linear_regression(&x, &y);
        let slope_per_year = slope * 365.0;
        let mean = 100.0;
        let threshold = mean * 0.01;
        assert!(slope_per_year > threshold);
    }

    #[test]
    fn test_determine_status_worsening() {
        let bm = Biomarker {
            id: 1,
            loinc_code: "2093-3".to_string(),
            name: "Total Cholesterol".to_string(),
            aliases: "[]".to_string(),
            unit: "mg/dL".to_string(),
            category: "Lipid Panel".to_string(),
            reference_low: Some(125.0),
            reference_high: Some(200.0),
            optimal_low: Some(150.0),
            optimal_high: Some(180.0),
            source: "measured".to_string(),
        };

        // Value above optimal and increasing = worsening
        assert_eq!(determine_status("increasing", &bm, 190.0), "worsening");
        // Value above optimal and decreasing = improving
        assert_eq!(determine_status("decreasing", &bm, 190.0), "improving");
        // Value below optimal and decreasing = worsening
        assert_eq!(determine_status("decreasing", &bm, 140.0), "worsening");
        // Stable = stable
        assert_eq!(determine_status("stable", &bm, 165.0), "stable");
    }
}
