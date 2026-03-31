use sqlx::SqlitePool;

use crate::db::models::NewBiomarker;
use crate::db::queries;
use crate::error::Result;

/// Seed the database with commonly tracked biomarkers and their unit conversions.
/// Only inserts biomarkers that don't already exist (by loinc_code).
pub async fn seed_biomarkers(pool: &SqlitePool) -> Result<usize> {
    let biomarkers = get_seed_biomarkers();
    let mut inserted = 0;

    for b in &biomarkers {
        let existing = queries::get_biomarker_by_loinc(pool, &b.loinc_code).await?;
        if existing.is_none() {
            let id = queries::insert_biomarker(pool, b).await?;

            // Seed unit conversions for this biomarker
            if let Some(conversions) = get_conversions_for(&b.loinc_code) {
                for (from_unit, factor, offset) in conversions {
                    queries::insert_unit_conversion(pool, id, from_unit, &b.unit, factor, offset)
                        .await?;
                }
            }

            inserted += 1;
        }
    }

    tracing::info!("Seeded {inserted} biomarkers");
    Ok(inserted)
}

fn get_seed_biomarkers() -> Vec<NewBiomarker> {
    vec![
        // --- Lipid Panel ---
        bm("2093-3", "Total Cholesterol", &["TC", "Chol", "Cholesterol, Total", "CHOL", "T.Chol", "Total Chol"], "mg/dL", "Lipid Panel",
           Some(125.0), Some(200.0), Some(150.0), Some(180.0)),
        bm("13457-7", "LDL Cholesterol", &["LDL-C", "LDL", "Low Density Lipoprotein", "LDL Chol"], "mg/dL", "Lipid Panel",
           None, Some(130.0), None, Some(70.0)),
        bm("2085-9", "HDL Cholesterol", &["HDL-C", "HDL", "High Density Lipoprotein", "HDL Chol"], "mg/dL", "Lipid Panel",
           Some(40.0), None, Some(60.0), None),
        bm("2571-8", "Triglycerides", &["TG", "Trig", "Triglyceride"], "mg/dL", "Lipid Panel",
           None, Some(150.0), None, Some(100.0)),
        bm("1884-6", "Apolipoprotein B", &["ApoB", "Apo B", "Apolipoprotein-B"], "mg/dL", "Lipid Panel",
           None, Some(130.0), None, Some(80.0)),
        bm("10835-7", "Lipoprotein(a)", &["Lp(a)", "Lpa", "Lipoprotein a"], "nmol/L", "Lipid Panel",
           None, Some(75.0), None, Some(30.0)),

        // --- Metabolic ---
        bm("2345-7", "Glucose", &["Fasting Glucose", "Blood Sugar", "FBG", "Glu"], "mg/dL", "Metabolic",
           Some(65.0), Some(100.0), Some(70.0), Some(90.0)),
        bm("4548-4", "Hemoglobin A1c", &["HbA1c", "A1c", "Glycated Hemoglobin", "Glycosylated Hemoglobin"], "%", "Metabolic",
           None, Some(5.7), None, Some(5.0)),
        bm("2484-4", "Insulin", &["Fasting Insulin", "Ins"], "uIU/mL", "Metabolic",
           Some(2.0), Some(25.0), Some(2.0), Some(8.0)),

        // --- Liver ---
        bm("1742-6", "ALT", &["Alanine Aminotransferase", "SGPT", "GPT", "ALT/SGPT"], "U/L", "Liver",
           Some(7.0), Some(56.0), Some(7.0), Some(30.0)),
        bm("1920-8", "AST", &["Aspartate Aminotransferase", "SGOT", "GOT", "AST/SGOT"], "U/L", "Liver",
           Some(10.0), Some(40.0), Some(10.0), Some(30.0)),
        bm("2324-2", "GGT", &["Gamma-Glutamyl Transferase", "Gamma GT", "GGTP", "Gamma-GT"], "U/L", "Liver",
           Some(8.0), Some(61.0), Some(8.0), Some(30.0)),
        bm("6768-6", "ALP", &["Alkaline Phosphatase", "Alk Phos"], "U/L", "Liver",
           Some(44.0), Some(147.0), Some(44.0), Some(100.0)),
        bm("1975-2", "Bilirubin Total", &["Total Bilirubin", "Bili", "T. Bili"], "mg/dL", "Liver",
           Some(0.1), Some(1.2), Some(0.1), Some(1.0)),

        // --- Kidney ---
        bm("2160-0", "Creatinine", &["Creat", "Serum Creatinine", "SCr"], "mg/dL", "Kidney",
           Some(0.7), Some(1.3), Some(0.7), Some(1.1)),
        bm("3094-0", "BUN", &["Blood Urea Nitrogen", "Urea Nitrogen"], "mg/dL", "Kidney",
           Some(6.0), Some(20.0), Some(7.0), Some(18.0)),
        bm("33863-2", "Cystatin C", &["CysC", "Cystatin-C"], "mg/L", "Kidney",
           Some(0.5), Some(1.0), Some(0.5), Some(0.9)),

        // --- Thyroid ---
        bm("3016-3", "TSH", &["Thyroid Stimulating Hormone", "Thyrotropin"], "mIU/L", "Thyroid",
           Some(0.4), Some(4.0), Some(1.0), Some(2.5)),
        bm("3024-7", "Free T4", &["FT4", "Free Thyroxine", "Thyroxine Free"], "ng/dL", "Thyroid",
           Some(0.8), Some(1.8), Some(1.0), Some(1.5)),
        bm("3051-0", "Free T3", &["FT3", "Free Triiodothyronine", "Triiodothyronine Free"], "pg/mL", "Thyroid",
           Some(2.0), Some(4.4), Some(2.5), Some(4.0)),

        // --- Inflammatory ---
        bm("30522-7", "hsCRP", &["High-sensitivity CRP", "hs-CRP", "C-Reactive Protein", "CRP"], "mg/L", "Inflammatory",
           None, Some(3.0), None, Some(1.0)),
        bm("4537-7", "ESR", &["Erythrocyte Sedimentation Rate", "Sed Rate"], "mm/h", "Inflammatory",
           None, Some(20.0), None, Some(10.0)),
        bm("2276-4", "Ferritin", &["Serum Ferritin"], "ng/mL", "Inflammatory",
           Some(12.0), Some(300.0), Some(40.0), Some(200.0)),
        bm("13965-9", "Homocysteine", &["Hcy", "Homocys"], "umol/L", "Inflammatory",
           None, Some(15.0), None, Some(8.0)),

        // --- Hormonal ---
        bm("2986-8", "Testosterone", &["Total Testosterone", "Testo", "T"], "ng/dL", "Hormonal",
           Some(264.0), Some(916.0), Some(500.0), Some(900.0)),
        bm("2191-5", "DHEA-S", &["DHEA Sulfate", "Dehydroepiandrosterone Sulfate", "DHEAS"], "ug/dL", "Hormonal",
           Some(80.0), Some(560.0), Some(200.0), Some(500.0)),
        bm("2143-6", "Cortisol", &["Serum Cortisol", "Morning Cortisol"], "ug/dL", "Hormonal",
           Some(6.0), Some(23.0), Some(8.0), Some(15.0)),

        // --- Vitamins & Minerals ---
        bm("1989-3", "Vitamin D", &["25-OH Vitamin D", "25-Hydroxyvitamin D", "Vit D", "25(OH)D"], "ng/mL", "Vitamins",
           Some(20.0), Some(100.0), Some(40.0), Some(80.0)),
        bm("2132-9", "Vitamin B12", &["B12", "Cobalamin"], "pg/mL", "Vitamins",
           Some(200.0), Some(900.0), Some(500.0), Some(800.0)),
        bm("2284-8", "Folate", &["Folic Acid", "Serum Folate"], "ng/mL", "Vitamins",
           Some(3.0), Some(20.0), Some(10.0), Some(20.0)),
        bm("19123-9", "Magnesium", &["Mg", "Serum Magnesium"], "mg/dL", "Vitamins",
           Some(1.7), Some(2.2), Some(2.0), Some(2.2)),
        bm("2601-3", "Zinc", &["Serum Zinc", "Zn"], "ug/dL", "Vitamins",
           Some(60.0), Some(120.0), Some(80.0), Some(110.0)),

        // --- Hematology (CBC) ---
        bm("718-7", "Hemoglobin", &["Hgb", "Hb", "HGB", "Haemoglobin"], "g/dL", "Hematology",
           Some(13.0), Some(17.0), Some(14.0), Some(16.0)),
        bm("4544-3", "Hematocrit", &["Hct", "HCT", "Packed Cell Volume", "PCV", "Haematocrit", "Haematocrit (PCV)"], "%", "Hematology",
           Some(38.0), Some(50.0), Some(40.0), Some(48.0)),
        bm("6690-2", "WBC", &["White Blood Cell Count", "Leukocytes", "White Blood Cells", "Total White Cell Count"], "10*3/uL", "Hematology",
           Some(4.0), Some(11.0), Some(4.5), Some(8.0)),
        bm("26515-7", "Platelets", &["Platelet Count", "PLT", "Thrombocytes"], "10*3/uL", "Hematology",
           Some(150.0), Some(400.0), Some(200.0), Some(350.0)),
        bm("789-8", "RBC", &["Red Blood Cell Count", "Erythrocytes", "Red Blood Cells", "Red Cell Count"], "10*6/uL", "Hematology",
           Some(4.5), Some(5.5), Some(4.5), Some(5.5)),
        bm("787-2", "MCV", &["Mean Corpuscular Volume", "Mean Cell Volume"], "fL", "Hematology",
           Some(80.0), Some(100.0), Some(82.0), Some(95.0)),
        bm("785-6", "MCH", &["Mean Corpuscular Hemoglobin", "Mean Cell Hemoglobin"], "pg", "Hematology",
           Some(27.0), Some(32.0), Some(27.0), Some(32.0)),
        bm("786-4", "MCHC", &["Mean Corpuscular Hemoglobin Concentration"], "g/dL", "Hematology",
           Some(31.0), Some(35.0), Some(32.0), Some(35.0)),
        bm("788-0", "RDW", &["Red Cell Distribution Width", "RDW-CV"], "%", "Hematology",
           Some(11.5), Some(14.5), Some(11.5), Some(14.0)),

        // --- Protein ---
        bm("2885-2", "Total Protein", &["TP", "Serum Protein", "Protein Total"], "g/L", "Metabolic",
           Some(63.0), Some(83.0), Some(65.0), Some(80.0)),
        bm("1751-7", "Albumin", &["Alb", "Serum Albumin"], "g/L", "Metabolic",
           Some(35.0), Some(52.0), Some(38.0), Some(50.0)),
        bm("10834-0", "Globulin", &["Glob", "Serum Globulin"], "g/L", "Metabolic",
           Some(20.0), Some(39.0), Some(22.0), Some(35.0)),

        // --- Calculated markers ---
        calc("T.Chol/HDL", "T.Chol/HDL Ratio", &["Total Cholesterol/HDL Ratio", "T.Chol/HDL Ratio", "TC/HDL", "Chol/HDL Ratio"], "", "Lipid Panel",
             None, Some(5.0), None, Some(4.0)),
        calc("A/G", "A/G Ratio", &["Albumin/Globulin Ratio", "AG Ratio", "Albumin Globulin Ratio"], "", "Metabolic",
             Some(1.2), Some(2.2), Some(1.2), Some(2.2)),
        calc("eGFR", "eGFR", &["eGFR (CKD-EPI 2009)", "eGFR (CKD-EPI)", "Estimated GFR", "GFR"], "mL/min/1.73m2", "Kidney",
             Some(60.0), None, Some(90.0), None),
        calc("TG/HDL", "TG/HDL Ratio", &["Triglyceride HDL Ratio"], "", "Lipid Panel",
             None, Some(3.5), None, Some(2.0)),
        calc("HOMA-IR", "HOMA-IR", &["Homeostatic Model Assessment"], "", "Metabolic",
             None, Some(2.5), None, Some(1.5)),
    ]
}

fn bm(
    loinc: &str, name: &str, aliases: &[&str], unit: &str, category: &str,
    ref_low: Option<f64>, ref_high: Option<f64>,
    opt_low: Option<f64>, opt_high: Option<f64>,
) -> NewBiomarker {
    NewBiomarker {
        loinc_code: loinc.to_string(),
        name: name.to_string(),
        aliases: aliases.iter().map(|s| s.to_string()).collect(),
        unit: unit.to_string(),
        category: category.to_string(),
        reference_low: ref_low,
        reference_high: ref_high,
        optimal_low: opt_low,
        optimal_high: opt_high,
        source: "measured".to_string(),
    }
}

fn calc(
    code: &str, name: &str, aliases: &[&str], unit: &str, category: &str,
    ref_low: Option<f64>, ref_high: Option<f64>,
    opt_low: Option<f64>, opt_high: Option<f64>,
) -> NewBiomarker {
    NewBiomarker {
        loinc_code: code.to_string(),
        name: name.to_string(),
        aliases: aliases.iter().map(|s| s.to_string()).collect(),
        unit: unit.to_string(),
        category: category.to_string(),
        reference_low: ref_low,
        reference_high: ref_high,
        optimal_low: opt_low,
        optimal_high: opt_high,
        source: "calculated".to_string(),
    }
}

/// Unit conversions for common Singapore lab report formats (SI -> conventional)
/// Returns (from_unit, factor, offset) tuples
fn get_conversions_for(loinc_code: &str) -> Option<Vec<(&'static str, f64, f64)>> {
    match loinc_code {
        // Cholesterol: mmol/L -> mg/dL (factor = 38.67)
        "2093-3" | "13457-7" | "2085-9" => Some(vec![("mmol/L", 38.67, 0.0)]),
        // Triglycerides: mmol/L -> mg/dL (factor = 88.57)
        "2571-8" => Some(vec![("mmol/L", 88.57, 0.0)]),
        // Glucose: mmol/L -> mg/dL (factor = 18.018)
        "2345-7" => Some(vec![("mmol/L", 18.018, 0.0)]),
        // Creatinine: umol/L -> mg/dL (factor = 0.01131)
        "2160-0" => Some(vec![("umol/L", 0.01131, 0.0)]),
        // Testosterone: nmol/L -> ng/dL (factor = 28.84)
        "2986-8" => Some(vec![("nmol/L", 28.84, 0.0)]),
        // Vitamin D: nmol/L -> ng/mL (factor = 0.4006)
        "1989-3" => Some(vec![("nmol/L", 0.4006, 0.0)]),
        // Vitamin B12: pmol/L -> pg/mL (factor = 1.355)
        "2132-9" => Some(vec![("pmol/L", 1.355, 0.0)]),
        // Homocysteine: SI is canonical (umol/L), no conversion needed
        // hsCRP: nmol/L -> mg/L (factor = 0.105)
        "30522-7" => Some(vec![("nmol/L", 0.105, 0.0)]),
        // HbA1c: mmol/mol -> % (IFCC -> NGSP)
        "4548-4" => Some(vec![("mmol/mol", 0.0915, 2.15)]),
        _ => None,
    }
}
