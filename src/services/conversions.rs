use rust_embed::Embed;
use sqlx::SqlitePool;
use std::collections::HashMap;

use crate::db::queries;
use crate::error::Result;

#[derive(Embed)]
#[folder = "data/"]
#[include = "loinc_conversions.tsv"]
struct ConversionData;

/// A conversion entry from the miracum LOINC conversion table
struct ConversionEntry {
    from_loinc: String,
    from_unit: String,
    to_loinc: String,
    to_unit: String,
    factor: f64,
}

/// Load the miracum LOINC conversion table and register conversions
/// for all tracked biomarkers that don't already have them.
pub async fn seed_conversions_from_miracum(pool: &SqlitePool) -> Result<usize> {
    let entries = load_conversion_table();
    let biomarkers = queries::list_biomarkers(pool, None).await?;
    let mut added = 0;

    for bm in &biomarkers {
        if bm.unit.is_empty() {
            continue;
        }

        // Find conversions where TO_LOINC matches our biomarker
        // (lab reports typically use SI, we want to convert TO our canonical unit)
        for entry in &entries {
            if entry.to_loinc == bm.loinc_code && entry.to_unit == bm.unit {
                // Check if this conversion already exists
                let existing = queries::get_unit_conversion(pool, bm.id, &entry.from_unit).await?;
                if existing.is_none() {
                    queries::insert_unit_conversion(
                        pool, bm.id, &entry.from_unit, &bm.unit, entry.factor, 0.0,
                    ).await?;
                    added += 1;
                }
            }

            // Also check FROM_LOINC (reverse: our canonical is SI, lab reports use conventional)
            if entry.from_loinc == bm.loinc_code && entry.from_unit == bm.unit {
                // Reverse conversion: factor becomes 1/factor
                let reverse_factor = 1.0 / entry.factor;
                let existing = queries::get_unit_conversion(pool, bm.id, &entry.to_unit).await?;
                if existing.is_none() {
                    queries::insert_unit_conversion(
                        pool, bm.id, &entry.to_unit, &bm.unit, reverse_factor, 0.0,
                    ).await?;
                    added += 1;
                }
            }
        }

        // Also register conversions for biomarkers where the LOINC code doesn't match
        // but the analyte is the same (e.g., different specimen types).
        // Match by looking for any conversion where the units make sense.
        if queries::get_unit_conversion(pool, bm.id, "mmol/L").await?.is_none() && bm.unit == "mg/dL" {
            // Find a conversion from mmol/L to mg/dL for a related analyte
            if let Some(entry) = entries.iter().find(|e| {
                e.from_unit == "mmol/L" && e.to_unit == "mg/dL" && e.to_loinc == bm.loinc_code
            }) {
                queries::insert_unit_conversion(pool, bm.id, "mmol/L", &bm.unit, entry.factor, 0.0).await?;
                added += 1;
            }
        }
    }

    if added > 0 {
        tracing::info!("Registered {} unit conversions from miracum table", added);
    }
    Ok(added)
}

fn load_conversion_table() -> Vec<ConversionEntry> {
    let mut entries = Vec::new();

    if let Some(data) = ConversionData::get("loinc_conversions.tsv") {
        let content = std::str::from_utf8(&data.data).unwrap_or("");
        for line in content.lines().skip(1) {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 5 {
                if let Ok(factor) = parts[4].parse::<f64>() {
                    entries.push(ConversionEntry {
                        from_loinc: parts[0].to_string(),
                        from_unit: parts[1].to_string(),
                        to_loinc: parts[2].to_string(),
                        to_unit: parts[3].to_string(),
                        factor,
                    });
                }
            }
        }
    }

    tracing::info!("Loaded {} conversion entries from miracum table", entries.len());
    entries
}

/// Build a lookup of conversion factors indexed by (from_unit, to_unit, to_loinc)
/// for quick access during extraction
pub fn get_conversion_factor_for_loinc(
    entries: &[ConversionEntry],
    loinc_code: &str,
    from_unit: &str,
    to_unit: &str,
) -> Option<f64> {
    // Direct match: TO_LOINC is our code
    if let Some(e) = entries.iter().find(|e| e.to_loinc == loinc_code && e.from_unit == from_unit && e.to_unit == to_unit) {
        return Some(e.factor);
    }
    // Reverse match: FROM_LOINC is our code
    if let Some(e) = entries.iter().find(|e| e.from_loinc == loinc_code && e.to_unit == from_unit && e.from_unit == to_unit) {
        return Some(1.0 / e.factor);
    }
    None
}
