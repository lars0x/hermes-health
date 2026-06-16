-- Migration 009: seed the default tracked biomarkers and their unit conversions.
--
-- This is the single source of truth for the seeded catalog (previously lived in
-- src/services/seed.rs). It runs on every startup via the migration runner; every
-- statement is idempotent (INSERT OR IGNORE keyed on the UNIQUE columns), so it
-- inserts missing rows on a fresh database and is a no-op once seeded.
--
-- `aliases` is a JSON array of alternate names. `source` is 'measured' or
-- 'calculated'. NULL range bounds mean "open-ended on that side".

INSERT OR IGNORE INTO biomarkers
    (loinc_code, name, aliases, unit, category, source)
VALUES
    -- Lipid Panel
    ('2093-3', 'Total Cholesterol', '["TC","Chol","Cholesterol, Total","CHOL","T.Chol","Total Chol"]', 'mg/dL', 'Lipid Panel', 'measured'),
    ('13457-7', 'LDL Cholesterol', '["LDL-C","LDL","Low Density Lipoprotein","LDL Chol"]', 'mg/dL', 'Lipid Panel', 'measured'),
    ('2085-9', 'HDL Cholesterol', '["HDL-C","HDL","High Density Lipoprotein","HDL Chol"]', 'mg/dL', 'Lipid Panel', 'measured'),
    ('2571-8', 'Triglycerides', '["TG","Trig","Triglyceride"]', 'mg/dL', 'Lipid Panel', 'measured'),
    ('1884-6', 'Apolipoprotein B', '["ApoB","Apo B","Apolipoprotein-B"]', 'mg/dL', 'Lipid Panel', 'measured'),
    ('10835-7', 'Lipoprotein(a)', '["Lp(a)","Lpa","Lipoprotein a"]', 'nmol/L', 'Lipid Panel', 'measured'),

    -- Metabolic
    ('2345-7', 'Glucose', '["Fasting Glucose","Blood Sugar","FBG","Glu"]', 'mg/dL', 'Metabolic', 'measured'),
    ('4548-4', 'Hemoglobin A1c', '["HbA1c","A1c","Glycated Hemoglobin","Glycosylated Hemoglobin"]', '%', 'Metabolic', 'measured'),
    ('2484-4', 'Insulin', '["Fasting Insulin","Ins"]', 'uIU/mL', 'Metabolic', 'measured'),

    -- Liver
    ('1742-6', 'ALT', '["Alanine Aminotransferase","SGPT","GPT","ALT/SGPT"]', 'U/L', 'Liver', 'measured'),
    ('1920-8', 'AST', '["Aspartate Aminotransferase","SGOT","GOT","AST/SGOT"]', 'U/L', 'Liver', 'measured'),
    ('2324-2', 'GGT', '["Gamma-Glutamyl Transferase","Gamma GT","GGTP","Gamma-GT"]', 'U/L', 'Liver', 'measured'),
    ('6768-6', 'ALP', '["Alkaline Phosphatase","Alk Phos"]', 'U/L', 'Liver', 'measured'),
    ('1975-2', 'Bilirubin Total', '["Total Bilirubin","Bili","T. Bili"]', 'mg/dL', 'Liver', 'measured'),

    -- Kidney
    ('2160-0', 'Creatinine', '["Creat","Serum Creatinine","SCr"]', 'mg/dL', 'Kidney', 'measured'),
    ('3094-0', 'BUN', '["Blood Urea Nitrogen","Urea Nitrogen"]', 'mg/dL', 'Kidney', 'measured'),
    ('33863-2', 'Cystatin C', '["CysC","Cystatin-C"]', 'mg/L', 'Kidney', 'measured'),

    -- Thyroid
    ('3016-3', 'TSH', '["Thyroid Stimulating Hormone","Thyrotropin"]', 'mIU/L', 'Thyroid', 'measured'),
    ('3024-7', 'Free T4', '["FT4","Free Thyroxine","Thyroxine Free"]', 'ng/dL', 'Thyroid', 'measured'),
    ('3051-0', 'Free T3', '["FT3","Free Triiodothyronine","Triiodothyronine Free"]', 'pg/mL', 'Thyroid', 'measured'),

    -- Inflammatory
    ('30522-7', 'hsCRP', '["High-sensitivity CRP","hs-CRP","C-Reactive Protein","CRP"]', 'mg/L', 'Inflammatory', 'measured'),
    ('4537-7', 'ESR', '["Erythrocyte Sedimentation Rate","Sed Rate"]', 'mm/h', 'Inflammatory', 'measured'),
    ('2276-4', 'Ferritin', '["Serum Ferritin"]', 'ng/mL', 'Inflammatory', 'measured'),
    ('13965-9', 'Homocysteine', '["Hcy","Homocys"]', 'umol/L', 'Inflammatory', 'measured'),

    -- Hormonal
    ('2986-8', 'Testosterone', '["Total Testosterone","Testo","T"]', 'ng/dL', 'Hormonal', 'measured'),
    ('2191-5', 'DHEA-S', '["DHEA Sulfate","Dehydroepiandrosterone Sulfate","DHEAS"]', 'ug/dL', 'Hormonal', 'measured'),
    ('2143-6', 'Cortisol', '["Serum Cortisol","Morning Cortisol"]', 'ug/dL', 'Hormonal', 'measured'),

    -- Vitamins & Minerals
    ('1989-3', 'Vitamin D', '["25-OH Vitamin D","25-Hydroxyvitamin D","Vit D","25(OH)D"]', 'ng/mL', 'Vitamins', 'measured'),
    ('2132-9', 'Vitamin B12', '["B12","Cobalamin"]', 'pg/mL', 'Vitamins', 'measured'),
    ('2284-8', 'Folate', '["Folic Acid","Serum Folate"]', 'ng/mL', 'Vitamins', 'measured'),
    ('19123-9', 'Magnesium', '["Mg","Serum Magnesium"]', 'mg/dL', 'Vitamins', 'measured'),
    ('2601-3', 'Zinc', '["Serum Zinc","Zn"]', 'ug/dL', 'Vitamins', 'measured'),

    -- Hematology (CBC)
    ('718-7', 'Hemoglobin', '["Hgb","Hb","HGB","Haemoglobin"]', 'g/dL', 'Hematology', 'measured'),
    ('4544-3', 'Hematocrit', '["Hct","HCT","Packed Cell Volume","PCV","Haematocrit","Haematocrit (PCV)"]', '%', 'Hematology', 'measured'),
    ('6690-2', 'WBC', '["White Blood Cell Count","Leukocytes","White Blood Cells","Total White Cell Count"]', '10*3/uL', 'Hematology', 'measured'),
    ('26515-7', 'Platelets', '["Platelet Count","PLT","Thrombocytes"]', '10*3/uL', 'Hematology', 'measured'),
    ('789-8', 'RBC', '["Red Blood Cell Count","Erythrocytes","Red Blood Cells","Red Cell Count"]', '10*6/uL', 'Hematology', 'measured'),
    ('787-2', 'MCV', '["Mean Corpuscular Volume","Mean Cell Volume"]', 'fL', 'Hematology', 'measured'),
    ('785-6', 'MCH', '["Mean Corpuscular Hemoglobin","Mean Cell Hemoglobin"]', 'pg', 'Hematology', 'measured'),
    ('786-4', 'MCHC', '["Mean Corpuscular Hemoglobin Concentration"]', 'g/dL', 'Hematology', 'measured'),
    ('788-0', 'RDW', '["Red Cell Distribution Width","RDW-CV"]', '%', 'Hematology', 'measured'),

    -- Protein
    ('2885-2', 'Total Protein', '["TP","Serum Protein","Protein Total"]', 'g/L', 'Metabolic', 'measured'),
    ('1751-7', 'Albumin', '["Alb","Serum Albumin"]', 'g/L', 'Metabolic', 'measured'),
    ('10834-0', 'Globulin', '["Glob","Serum Globulin"]', 'g/L', 'Metabolic', 'measured'),

    -- Calculated markers
    ('32309-7', 'T.Chol/HDL Ratio', '["Total Cholesterol/HDL Ratio","T.Chol/HDL Ratio","TC/HDL","Chol/HDL Ratio","T.Chol/HDL"]', '', 'Lipid Panel', 'calculated'),
    ('1759-0', 'A/G Ratio', '["Albumin/Globulin Ratio","AG Ratio","Albumin Globulin Ratio","A/G"]', '', 'Metabolic', 'calculated'),
    ('98979-8', 'eGFR', '["eGFR (CKD-EPI 2009)","eGFR (CKD-EPI)","eGFR (CKD-EPI 2021)","Estimated GFR","GFR"]', 'mL/min/1.73m2', 'Kidney', 'calculated'),
    ('44733-4', 'TG/HDL Ratio', '["Triglyceride HDL Ratio","TG/HDL"]', '', 'Lipid Panel', 'calculated'),
    ('HOMA-IR', 'HOMA-IR', '["Homeostatic Model Assessment"]', '', 'Metabolic', 'calculated');

-- Unit conversions (Singapore lab SI -> conventional). to_unit is always the
-- biomarker's canonical unit, resolved from the biomarkers row by loinc_code.
INSERT OR IGNORE INTO unit_conversions (biomarker_id, from_unit, to_unit, factor, offset)
    SELECT id, 'mmol/L', unit, 38.67, 0.0 FROM biomarkers WHERE loinc_code = '2093-3';
INSERT OR IGNORE INTO unit_conversions (biomarker_id, from_unit, to_unit, factor, offset)
    SELECT id, 'mmol/L', unit, 38.67, 0.0 FROM biomarkers WHERE loinc_code = '13457-7';
INSERT OR IGNORE INTO unit_conversions (biomarker_id, from_unit, to_unit, factor, offset)
    SELECT id, 'mmol/L', unit, 38.67, 0.0 FROM biomarkers WHERE loinc_code = '2085-9';
INSERT OR IGNORE INTO unit_conversions (biomarker_id, from_unit, to_unit, factor, offset)
    SELECT id, 'mmol/L', unit, 88.57, 0.0 FROM biomarkers WHERE loinc_code = '2571-8';
INSERT OR IGNORE INTO unit_conversions (biomarker_id, from_unit, to_unit, factor, offset)
    SELECT id, 'mmol/L', unit, 18.018, 0.0 FROM biomarkers WHERE loinc_code = '2345-7';
INSERT OR IGNORE INTO unit_conversions (biomarker_id, from_unit, to_unit, factor, offset)
    SELECT id, 'umol/L', unit, 0.01131, 0.0 FROM biomarkers WHERE loinc_code = '2160-0';
INSERT OR IGNORE INTO unit_conversions (biomarker_id, from_unit, to_unit, factor, offset)
    SELECT id, 'nmol/L', unit, 28.84, 0.0 FROM biomarkers WHERE loinc_code = '2986-8';
INSERT OR IGNORE INTO unit_conversions (biomarker_id, from_unit, to_unit, factor, offset)
    SELECT id, 'nmol/L', unit, 0.4006, 0.0 FROM biomarkers WHERE loinc_code = '1989-3';
INSERT OR IGNORE INTO unit_conversions (biomarker_id, from_unit, to_unit, factor, offset)
    SELECT id, 'pmol/L', unit, 1.355, 0.0 FROM biomarkers WHERE loinc_code = '2132-9';
INSERT OR IGNORE INTO unit_conversions (biomarker_id, from_unit, to_unit, factor, offset)
    SELECT id, 'nmol/L', unit, 0.105, 0.0 FROM biomarkers WHERE loinc_code = '30522-7';
INSERT OR IGNORE INTO unit_conversions (biomarker_id, from_unit, to_unit, factor, offset)
    SELECT id, 'x 10^9/L', unit, 1.0, 0.0 FROM biomarkers WHERE loinc_code = '6690-2';
INSERT OR IGNORE INTO unit_conversions (biomarker_id, from_unit, to_unit, factor, offset)
    SELECT id, 'x 10^9/L', unit, 1.0, 0.0 FROM biomarkers WHERE loinc_code = '26515-7';
INSERT OR IGNORE INTO unit_conversions (biomarker_id, from_unit, to_unit, factor, offset)
    SELECT id, 'x 10^12/L', unit, 1.0, 0.0 FROM biomarkers WHERE loinc_code = '789-8';
INSERT OR IGNORE INTO unit_conversions (biomarker_id, from_unit, to_unit, factor, offset)
    SELECT id, 'mmol/L', unit, 2.8013, 0.0 FROM biomarkers WHERE loinc_code = '3094-0';
INSERT OR IGNORE INTO unit_conversions (biomarker_id, from_unit, to_unit, factor, offset)
    SELECT id, 'umol/L', unit, 0.058466, 0.0 FROM biomarkers WHERE loinc_code = '1975-2';
INSERT OR IGNORE INTO unit_conversions (biomarker_id, from_unit, to_unit, factor, offset)
    SELECT id, 'nmol/L', unit, 0.44140, 0.0 FROM biomarkers WHERE loinc_code = '2284-8';
INSERT OR IGNORE INTO unit_conversions (biomarker_id, from_unit, to_unit, factor, offset)
    SELECT id, 'mmol/mol', unit, 0.0915, 2.15 FROM biomarkers WHERE loinc_code = '4548-4';
