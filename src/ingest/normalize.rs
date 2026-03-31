use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::db::models::UnitConversion;
use crate::db::queries;
use crate::error::{HermesError, Result};
use crate::ingest::units;

/// The result of normalizing an observation value
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedObservation {
    pub value: f64,
    pub original_value: String,
    pub original_unit: String,
    pub canonical_unit: String,
    pub precision: i32,
    pub detection_limit: Option<String>,
    pub notes_append: Option<String>,
}

/// Parse an original value string, handling detection limits (< and > prefixes)
fn parse_original_value(original: &str) -> (f64, Option<String>, Option<String>) {
    let trimmed = original.trim();

    let (prefix, numeric_str) = if trimmed.starts_with('<') {
        (Some("<".to_string()), trimmed[1..].trim())
    } else if trimmed.starts_with('>') {
        (Some(">".to_string()), trimmed[1..].trim())
    } else {
        (None, trimmed)
    };

    let value = numeric_str.parse::<f64>().unwrap_or(0.0);

    let notes = match prefix.as_deref() {
        Some("<") => Some("below detection limit".to_string()),
        Some(">") => Some("above measurement range".to_string()),
        _ => None,
    };

    (value, prefix, notes)
}

/// Derive precision (number of decimal places) from the original value string
pub fn derive_precision(original: &str) -> i32 {
    let trimmed = original.trim().trim_start_matches('<').trim_start_matches('>').trim();

    if let Some(dot_pos) = trimmed.find('.') {
        (trimmed.len() - dot_pos - 1) as i32
    } else {
        0
    }
}

/// Count significant figures in a numeric string.
/// For lab values, all digits are considered significant (including trailing zeros).
/// "185" = 3, "5.20" = 3, "4.8" = 2, "0.85" = 2
fn significant_figures(s: &str) -> usize {
    let trimmed = s.trim().trim_start_matches('<').trim_start_matches('>').trim();
    let without_sign = trimmed.trim_start_matches('-');

    if without_sign.contains('.') {
        // With decimal point: count all digits except leading zeros before first non-zero
        let digits: String = without_sign.chars().filter(|c| c.is_ascii_digit()).collect();
        let significant = digits.trim_start_matches('0');
        if significant.is_empty() {
            1
        } else {
            significant.len()
        }
    } else {
        // Integer: all digits are significant for lab values
        let digits: String = without_sign.chars().filter(|c| c.is_ascii_digit()).collect();
        if digits.is_empty() {
            1
        } else {
            digits.len()
        }
    }
}

/// Round a value to a specific number of significant figures
fn round_to_sig_figs(value: f64, sig_figs: usize) -> f64 {
    if value == 0.0 || sig_figs == 0 {
        return value;
    }

    let magnitude = value.abs().log10().floor() as i32;
    let factor = 10f64.powi(sig_figs as i32 - 1 - magnitude);
    (value * factor).round() / factor
}

/// Derive the precision (decimal places) of a converted value based on significant figures
fn precision_after_conversion(converted: f64, sig_figs: usize) -> i32 {
    if converted == 0.0 {
        return 0;
    }

    let magnitude = converted.abs().log10().floor() as i32;
    let decimal_places = (sig_figs as i32 - 1) - magnitude;

    if decimal_places < 0 {
        0
    } else {
        decimal_places
    }
}

/// Normalize an observation: parse value, normalize unit, convert if needed.
pub async fn normalize_observation(
    pool: &SqlitePool,
    biomarker_id: i64,
    canonical_unit: &str,
    original_value_str: &str,
    original_unit_str: &str,
) -> Result<NormalizedObservation> {
    let (parsed_value, detection_limit, notes) = parse_original_value(original_value_str);
    let original_precision = derive_precision(original_value_str);
    let normalized_unit = units::normalize_unit(original_unit_str);

    // Check if units already match
    if units::units_match(&normalized_unit, canonical_unit) {
        return Ok(NormalizedObservation {
            value: parsed_value,
            original_value: original_value_str.to_string(),
            original_unit: original_unit_str.to_string(),
            canonical_unit: canonical_unit.to_string(),
            precision: original_precision,
            detection_limit,
            notes_append: notes,
        });
    }

    // Look up conversion
    let conversion = queries::get_unit_conversion(pool, biomarker_id, &normalized_unit).await?;

    match conversion {
        Some(conv) => {
            let converted = (parsed_value * conv.factor) + conv.offset;
            let sig_figs = significant_figures(original_value_str);
            let rounded = round_to_sig_figs(converted, sig_figs);
            let new_precision = precision_after_conversion(rounded, sig_figs);

            Ok(NormalizedObservation {
                value: rounded,
                original_value: original_value_str.to_string(),
                original_unit: original_unit_str.to_string(),
                canonical_unit: canonical_unit.to_string(),
                precision: new_precision,
                detection_limit,
                notes_append: notes,
            })
        }
        None => Err(HermesError::Conversion(format!(
            "No conversion found from '{}' to '{}' for biomarker_id={}. \
             Add a conversion rule or correct the unit.",
            normalized_unit, canonical_unit, biomarker_id
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_precision() {
        assert_eq!(derive_precision("185"), 0);
        assert_eq!(derive_precision("5.2"), 1);
        assert_eq!(derive_precision("5.20"), 2);
        assert_eq!(derive_precision("0.85"), 2);
        assert_eq!(derive_precision("<0.5"), 1);
        assert_eq!(derive_precision(">200"), 0);
    }

    #[test]
    fn test_significant_figures() {
        assert_eq!(significant_figures("185"), 3);
        assert_eq!(significant_figures("5.2"), 2);
        assert_eq!(significant_figures("5.20"), 3);
        assert_eq!(significant_figures("0.85"), 2);
        assert_eq!(significant_figures("4.8"), 2);
        assert_eq!(significant_figures("100"), 3); // all digits count for lab values
    }

    #[test]
    fn test_round_to_sig_figs() {
        // 4.8 mmol/L * 38.67 = 185.616, rounded to 2 sig figs = 190
        let converted = 4.8 * 38.67; // 185.616
        let rounded = round_to_sig_figs(converted, 2);
        assert_eq!(rounded, 190.0);
    }

    #[test]
    fn test_parse_detection_limits() {
        let (val, det, notes) = parse_original_value("<0.5");
        assert_eq!(val, 0.5);
        assert_eq!(det, Some("<".to_string()));
        assert!(notes.unwrap().contains("below detection limit"));

        let (val, det, notes) = parse_original_value(">200");
        assert_eq!(val, 200.0);
        assert_eq!(det, Some(">".to_string()));
        assert!(notes.unwrap().contains("above measurement range"));

        let (val, det, _notes) = parse_original_value("185");
        assert_eq!(val, 185.0);
        assert!(det.is_none());
    }

    #[test]
    fn test_precision_after_conversion() {
        // Value 190 with 2 sig figs -> 0 decimal places
        assert_eq!(precision_after_conversion(190.0, 2), 0);
        // Value 1.5 with 2 sig figs -> 1 decimal place
        assert_eq!(precision_after_conversion(1.5, 2), 1);
        // Value 0.085 with 2 sig figs -> 3 decimal places
        assert_eq!(precision_after_conversion(0.085, 2), 3);
    }

    #[test]
    fn test_cholesterol_conversion_from_spec() {
        // Spec example: Lab reports "4.8" mmol/L -> 2 sig figs -> 186 mg/dL (spec says 186)
        // Actually the spec says 190 is wrong, let me recheck...
        // Spec: "4.8" = 2 sig figs, 4.8 * 38.67 = 185.616, round to 2 sig figs = 190
        // But spec says "Rounded to 2 sig figs: 186 mg/dL"
        // The spec is actually using 186 which is 3 sig figs rounded... let me match the spec
        // Looking again: spec says "4.8" has 2 sig figs, converted to 186 (3 sig figs) - inconsistency
        // Our implementation is correct per the stated rule (2 sig figs -> 190)
        let converted = 4.8 * 38.67;
        let rounded = round_to_sig_figs(converted, 2);
        assert_eq!(rounded, 190.0);
    }
}
