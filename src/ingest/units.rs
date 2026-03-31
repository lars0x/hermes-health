use std::collections::HashMap;

/// Normalize a unit string to its canonical UCUM form.
/// Case-insensitive, handles common variants.
pub fn normalize_unit(unit: &str) -> String {
    let unit_lower = unit.trim().to_lowercase();
    let aliases = get_unit_aliases();

    if let Some(canonical) = aliases.get(unit_lower.as_str()) {
        return canonical.to_string();
    }

    // If no alias match, return the trimmed input as-is
    unit.trim().to_string()
}

fn get_unit_aliases() -> HashMap<&'static str, &'static str> {
    let mut m = HashMap::new();

    // mg/dL variants
    m.insert("mg/dl", "mg/dL");
    m.insert("mg/dl", "mg/dL");
    m.insert("mg/dl", "mg/dL");

    // mmol/L variants
    m.insert("mmol/l", "mmol/L");

    // umol/L variants
    m.insert("umol/l", "umol/L");
    m.insert("umol/l", "umol/L");
    m.insert("µmol/l", "umol/L");
    m.insert("μmol/l", "umol/L");

    // nmol/L variants
    m.insert("nmol/l", "nmol/L");

    // pmol/L variants
    m.insert("pmol/l", "pmol/L");

    // pg/mL variants
    m.insert("pg/ml", "pg/mL");

    // ng/mL variants
    m.insert("ng/ml", "ng/mL");

    // ng/dL variants
    m.insert("ng/dl", "ng/dL");

    // ug/dL variants
    m.insert("ug/dl", "ug/dL");
    m.insert("µg/dl", "ug/dL");
    m.insert("μg/dl", "ug/dL");

    // g/L variants
    m.insert("g/l", "g/L");

    // U/L variants
    m.insert("u/l", "U/L");
    m.insert("iu/l", "U/L");

    // mIU/L variants
    m.insert("miu/l", "mIU/L");
    m.insert("miu/ml", "mIU/mL");
    m.insert("uiu/ml", "uIU/mL");

    // % variants
    m.insert("percent", "%");

    // mg/L variants
    m.insert("mg/l", "mg/L");

    // mm/h variants
    m.insert("mm/hr", "mm/h");

    // mmol/mol variants
    m.insert("mmol/mol", "mmol/mol");

    // g/dL variants
    m.insert("g/dl", "g/dL");
    m.insert("gm/dl", "g/dL");

    // fL
    m.insert("fl", "fL");

    // 10*3/uL
    m.insert("10^3/ul", "10*3/uL");
    m.insert("k/ul", "10*3/uL");
    m.insert("x10e3/ul", "10*3/uL");
    m.insert("10*3/ul", "10*3/uL");
    m.insert("thou/ul", "10*3/uL");

    // 10*6/uL
    m.insert("10^6/ul", "10*6/uL");
    m.insert("m/ul", "10*6/uL");
    m.insert("x10e6/ul", "10*6/uL");
    m.insert("10*6/ul", "10*6/uL");
    m.insert("mil/ul", "10*6/uL");

    m
}

/// Check if two unit strings refer to the same unit (after normalization)
pub fn units_match(a: &str, b: &str) -> bool {
    normalize_unit(a) == normalize_unit(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_common_units() {
        assert_eq!(normalize_unit("mg/dl"), "mg/dL");
        assert_eq!(normalize_unit("mmol/l"), "mmol/L");
        assert_eq!(normalize_unit("pg/ml"), "pg/mL");
        assert_eq!(normalize_unit("u/l"), "U/L");
        assert_eq!(normalize_unit("percent"), "%");
        assert_eq!(normalize_unit("mg/dL"), "mg/dL"); // already canonical
    }

    #[test]
    fn test_units_match() {
        assert!(units_match("mg/dl", "mg/dL"));
        assert!(units_match("mmol/l", "mmol/L"));
        assert!(!units_match("mg/dL", "mmol/L"));
    }
}
