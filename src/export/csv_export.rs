use std::io::Write;

use sqlx::SqlitePool;

use crate::db::queries;
use crate::error::Result;

/// Export all observations as a flat CSV.
/// Columns: date, loinc_code, marker_name, value, unit, reference_low, reference_high, flag, lab, fasting, notes
pub async fn export_csv(
    pool: &SqlitePool,
    writer: &mut impl Write,
    from_date: Option<&str>,
    to_date: Option<&str>,
) -> Result<usize> {
    let biomarkers = queries::list_biomarkers(pool, None).await?;
    let observations = queries::list_all_observations(pool, from_date, to_date).await?;

    let mut csv_writer = csv::Writer::from_writer(writer);

    // Write header
    csv_writer.write_record([
        "date",
        "loinc_code",
        "marker_name",
        "value",
        "unit",
        "original_value",
        "original_unit",
        "reference_low",
        "reference_high",
        "flag",
        "lab",
        "fasting",
        "notes",
    ])?;

    let mut count = 0;
    for obs in &observations {
        let bm = biomarkers.iter().find(|b| b.id == obs.biomarker_id);
        let (loinc_code, name, unit, ref_low, ref_high) = match bm {
            Some(b) => (
                b.loinc_code.as_str(),
                b.name.as_str(),
                b.unit.as_str(),
                b.reference_low.map(|v| v.to_string()).unwrap_or_default(),
                b.reference_high.map(|v| v.to_string()).unwrap_or_default(),
            ),
            None => ("", "", "", String::new(), String::new()),
        };

        // Determine flag from detection_limit or reference range
        let flag = if let Some(dl) = &obs.detection_limit {
            dl.clone()
        } else if let Some(bm) = bm {
            let mut f = String::new();
            if let Some(high) = bm.reference_high {
                if obs.value > high {
                    f = "H".to_string();
                }
            }
            if let Some(low) = bm.reference_low {
                if obs.value < low {
                    f = "L".to_string();
                }
            }
            f
        } else {
            String::new()
        };

        let fasting = match obs.fasting {
            Some(true) => "yes",
            Some(false) => "no",
            None => "",
        };

        let prec = obs.precision as usize;
        let formatted_value = format!("{:.prec$}", obs.value);

        csv_writer.write_record([
            &obs.observed_at,
            loinc_code,
            name,
            &formatted_value,
            unit,
            &obs.original_value,
            &obs.original_unit,
            &ref_low,
            &ref_high,
            &flag,
            obs.lab_name.as_deref().unwrap_or(""),
            fasting,
            obs.notes.as_deref().unwrap_or(""),
        ])?;

        count += 1;
    }

    csv_writer.flush()?;
    Ok(count)
}
