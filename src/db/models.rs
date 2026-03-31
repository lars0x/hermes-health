use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Biomarker {
    pub id: i64,
    pub loinc_code: String,
    pub name: String,
    pub aliases: String, // JSON array stored as TEXT
    pub unit: String,
    pub category: String,
    pub reference_low: Option<f64>,
    pub reference_high: Option<f64>,
    pub optimal_low: Option<f64>,
    pub optimal_high: Option<f64>,
    pub source: String,
}

impl Biomarker {
    pub fn aliases_vec(&self) -> Vec<String> {
        serde_json::from_str(&self.aliases).unwrap_or_default()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Observation {
    pub id: i64,
    pub biomarker_id: i64,
    pub value: f64,
    pub original_value: String,
    pub original_unit: String,
    pub precision: i32,
    pub observed_at: String,
    pub lab_name: Option<String>,
    pub report_id: Option<i64>,
    pub fasting: Option<bool>,
    pub notes: Option<String>,
    pub detection_limit: Option<String>,
    pub created_at: String,
}

impl Observation {
    pub fn observed_date(&self) -> Option<NaiveDate> {
        NaiveDate::parse_from_str(&self.observed_at, "%Y-%m-%d").ok()
    }

    pub fn formatted_value(&self) -> String {
        let prec = self.precision as usize;
        format!("{:.prec$}", self.value)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Report {
    pub id: i64,
    pub filename: String,
    pub file_hash: String,
    pub file_path: String,
    pub format: String,
    pub imported_at: String,
    pub extraction_status: String,
    pub raw_extraction: Option<String>,
    pub model_used: Option<String>,
    pub agent_turns: Option<i64>,
    pub extracted_count: Option<i64>,
    pub unresolved_count: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Intervention {
    pub id: i64,
    pub name: String,
    pub category: String,
    pub dosage: Option<String>,
    pub frequency: Option<String>,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct InterventionBiomarkerTarget {
    pub intervention_id: i64,
    pub biomarker_id: i64,
    pub expected_effect: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UnitConversion {
    pub id: i64,
    pub biomarker_id: i64,
    pub from_unit: String,
    pub to_unit: String,
    pub factor: f64,
    pub offset: f64,
}

// Input types for creating records (no id/timestamps)

#[derive(Debug, Clone, Deserialize)]
pub struct NewBiomarker {
    pub loinc_code: String,
    pub name: String,
    pub aliases: Vec<String>,
    pub unit: String,
    pub category: String,
    pub reference_low: Option<f64>,
    pub reference_high: Option<f64>,
    pub optimal_low: Option<f64>,
    pub optimal_high: Option<f64>,
    pub source: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewObservation {
    pub biomarker: String, // LOINC code, name, or alias
    pub value: f64,
    pub unit: String,
    pub observed_at: String,
    pub lab_name: Option<String>,
    pub fasting: Option<bool>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewIntervention {
    pub name: String,
    pub category: String,
    pub dosage: Option<String>,
    pub frequency: Option<String>,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub notes: Option<String>,
    pub target_biomarkers: Vec<InterventionTarget>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InterventionTarget {
    pub biomarker_id: i64,
    pub expected_effect: String,
}
