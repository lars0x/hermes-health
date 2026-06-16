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
    (loinc_code, name, aliases, unit, category, reference_low, reference_high, optimal_low, optimal_high, source)
VALUES
    -- Lipid Panel
    ('2093-3', 'Total Cholesterol', '["TC","Chol","Cholesterol, Total","CHOL","T.Chol","Total Chol"]', 'mg/dL', 'Lipid Panel', 125.0, 200.0, 150.0, 180.0, 'measured'),
    ('13457-7', 'LDL Cholesterol', '["LDL-C","LDL","Low Density Lipoprotein","LDL Chol"]', 'mg/dL', 'Lipid Panel', NULL, 130.0, NULL, 70.0, 'measured'),
    ('2085-9', 'HDL Cholesterol', '["HDL-C","HDL","High Density Lipoprotein","HDL Chol"]', 'mg/dL', 'Lipid Panel', 40.0, NULL, 60.0, NULL, 'measured'),
    ('2571-8', 'Triglycerides', '["TG","Trig","Triglyceride"]', 'mg/dL', 'Lipid Panel', NULL, 150.0, NULL, 100.0, 'measured'),
    ('1884-6', 'Apolipoprotein B', '["ApoB","Apo B","Apolipoprotein-B"]', 'mg/dL', 'Lipid Panel', NULL, 130.0, NULL, 80.0, 'measured'),
    ('10835-7', 'Lipoprotein(a)', '["Lp(a)","Lpa","Lipoprotein a"]', 'nmol/L', 'Lipid Panel', NULL, 75.0, NULL, 30.0, 'measured'),

    -- Metabolic
    ('2345-7', 'Glucose', '["Fasting Glucose","Blood Sugar","FBG","Glu"]', 'mg/dL', 'Metabolic', 65.0, 100.0, 70.0, 90.0, 'measured'),
    ('4548-4', 'Hemoglobin A1c', '["HbA1c","A1c","Glycated Hemoglobin","Glycosylated Hemoglobin"]', '%', 'Metabolic', NULL, 5.7, NULL, 5.0, 'measured'),
    ('2484-4', 'Insulin', '["Fasting Insulin","Ins"]', 'uIU/mL', 'Metabolic', 2.0, 25.0, 2.0, 8.0, 'measured'),

    -- Liver
    ('1742-6', 'ALT', '["Alanine Aminotransferase","SGPT","GPT","ALT/SGPT"]', 'U/L', 'Liver', 7.0, 56.0, 7.0, 30.0, 'measured'),
    ('1920-8', 'AST', '["Aspartate Aminotransferase","SGOT","GOT","AST/SGOT"]', 'U/L', 'Liver', 10.0, 40.0, 10.0, 30.0, 'measured'),
    ('2324-2', 'GGT', '["Gamma-Glutamyl Transferase","Gamma GT","GGTP","Gamma-GT"]', 'U/L', 'Liver', 8.0, 61.0, 8.0, 30.0, 'measured'),
    ('6768-6', 'ALP', '["Alkaline Phosphatase","Alk Phos"]', 'U/L', 'Liver', 44.0, 147.0, 44.0, 100.0, 'measured'),
    ('1975-2', 'Bilirubin Total', '["Total Bilirubin","Bili","T. Bili"]', 'mg/dL', 'Liver', 0.1, 1.2, 0.1, 1.0, 'measured'),

    -- Kidney
    ('2160-0', 'Creatinine', '["Creat","Serum Creatinine","SCr"]', 'mg/dL', 'Kidney', 0.7, 1.3, 0.7, 1.1, 'measured'),
    ('3094-0', 'BUN', '["Blood Urea Nitrogen","Urea Nitrogen"]', 'mg/dL', 'Kidney', 6.0, 20.0, 7.0, 18.0, 'measured'),
    ('33863-2', 'Cystatin C', '["CysC","Cystatin-C"]', 'mg/L', 'Kidney', 0.5, 1.0, 0.5, 0.9, 'measured'),

    -- Thyroid
    ('3016-3', 'TSH', '["Thyroid Stimulating Hormone","Thyrotropin"]', 'mIU/L', 'Thyroid', 0.4, 4.0, 1.0, 2.5, 'measured'),
    ('3024-7', 'Free T4', '["FT4","Free Thyroxine","Thyroxine Free"]', 'ng/dL', 'Thyroid', 0.8, 1.8, 1.0, 1.5, 'measured'),
    ('3051-0', 'Free T3', '["FT3","Free Triiodothyronine","Triiodothyronine Free"]', 'pg/mL', 'Thyroid', 2.0, 4.4, 2.5, 4.0, 'measured'),

    -- Inflammatory
    ('30522-7', 'hsCRP', '["High-sensitivity CRP","hs-CRP","C-Reactive Protein","CRP"]', 'mg/L', 'Inflammatory', NULL, 3.0, NULL, 1.0, 'measured'),
    ('4537-7', 'ESR', '["Erythrocyte Sedimentation Rate","Sed Rate"]', 'mm/h', 'Inflammatory', NULL, 20.0, NULL, 10.0, 'measured'),
    ('2276-4', 'Ferritin', '["Serum Ferritin"]', 'ng/mL', 'Inflammatory', 12.0, 300.0, 40.0, 200.0, 'measured'),
    ('13965-9', 'Homocysteine', '["Hcy","Homocys"]', 'umol/L', 'Inflammatory', NULL, 15.0, NULL, 8.0, 'measured'),

    -- Hormonal
    ('2986-8', 'Testosterone', '["Total Testosterone","Testo","T"]', 'ng/dL', 'Hormonal', 264.0, 916.0, 500.0, 900.0, 'measured'),
    ('2191-5', 'DHEA-S', '["DHEA Sulfate","Dehydroepiandrosterone Sulfate","DHEAS"]', 'ug/dL', 'Hormonal', 80.0, 560.0, 200.0, 500.0, 'measured'),
    ('2143-6', 'Cortisol', '["Serum Cortisol","Morning Cortisol"]', 'ug/dL', 'Hormonal', 6.0, 23.0, 8.0, 15.0, 'measured'),

    -- Vitamins & Minerals
    ('1989-3', 'Vitamin D', '["25-OH Vitamin D","25-Hydroxyvitamin D","Vit D","25(OH)D"]', 'ng/mL', 'Vitamins', 20.0, 100.0, 40.0, 80.0, 'measured'),
    ('2132-9', 'Vitamin B12', '["B12","Cobalamin"]', 'pg/mL', 'Vitamins', 200.0, 900.0, 500.0, 800.0, 'measured'),
    ('2284-8', 'Folate', '["Folic Acid","Serum Folate"]', 'ng/mL', 'Vitamins', 3.0, 20.0, 10.0, 20.0, 'measured'),
    ('19123-9', 'Magnesium', '["Mg","Serum Magnesium"]', 'mg/dL', 'Vitamins', 1.7, 2.2, 2.0, 2.2, 'measured'),
    ('2601-3', 'Zinc', '["Serum Zinc","Zn"]', 'ug/dL', 'Vitamins', 60.0, 120.0, 80.0, 110.0, 'measured'),

    -- Hematology (CBC)
    ('718-7', 'Hemoglobin', '["Hgb","Hb","HGB","Haemoglobin"]', 'g/dL', 'Hematology', 13.0, 17.0, 14.0, 16.0, 'measured'),
    ('4544-3', 'Hematocrit', '["Hct","HCT","Packed Cell Volume","PCV","Haematocrit","Haematocrit (PCV)"]', '%', 'Hematology', 38.0, 50.0, 40.0, 48.0, 'measured'),
    ('6690-2', 'WBC', '["White Blood Cell Count","Leukocytes","White Blood Cells","Total White Cell Count"]', '10*3/uL', 'Hematology', 4.0, 11.0, 4.5, 8.0, 'measured'),
    ('26515-7', 'Platelets', '["Platelet Count","PLT","Thrombocytes"]', '10*3/uL', 'Hematology', 150.0, 400.0, 200.0, 350.0, 'measured'),
    ('789-8', 'RBC', '["Red Blood Cell Count","Erythrocytes","Red Blood Cells","Red Cell Count"]', '10*6/uL', 'Hematology', 4.5, 5.5, 4.5, 5.5, 'measured'),
    ('787-2', 'MCV', '["Mean Corpuscular Volume","Mean Cell Volume"]', 'fL', 'Hematology', 80.0, 100.0, 82.0, 95.0, 'measured'),
    ('785-6', 'MCH', '["Mean Corpuscular Hemoglobin","Mean Cell Hemoglobin"]', 'pg', 'Hematology', 27.0, 32.0, 27.0, 32.0, 'measured'),
    ('786-4', 'MCHC', '["Mean Corpuscular Hemoglobin Concentration"]', 'g/dL', 'Hematology', 31.0, 35.0, 32.0, 35.0, 'measured'),
    ('788-0', 'RDW', '["Red Cell Distribution Width","RDW-CV"]', '%', 'Hematology', 11.5, 14.5, 11.5, 14.0, 'measured'),

    -- Protein
    ('2885-2', 'Total Protein', '["TP","Serum Protein","Protein Total"]', 'g/L', 'Metabolic', 63.0, 83.0, 65.0, 80.0, 'measured'),
    ('1751-7', 'Albumin', '["Alb","Serum Albumin"]', 'g/L', 'Metabolic', 35.0, 52.0, 38.0, 50.0, 'measured'),
    ('10834-0', 'Globulin', '["Glob","Serum Globulin"]', 'g/L', 'Metabolic', 20.0, 39.0, 22.0, 35.0, 'measured'),

    -- Calculated markers
    ('32309-7', 'T.Chol/HDL Ratio', '["Total Cholesterol/HDL Ratio","T.Chol/HDL Ratio","TC/HDL","Chol/HDL Ratio","T.Chol/HDL"]', '', 'Lipid Panel', NULL, 5.0, NULL, 4.0, 'calculated'),
    ('1759-0', 'A/G Ratio', '["Albumin/Globulin Ratio","AG Ratio","Albumin Globulin Ratio","A/G"]', '', 'Metabolic', 1.2, 2.2, 1.2, 2.2, 'calculated'),
    ('98979-8', 'eGFR', '["eGFR (CKD-EPI 2009)","eGFR (CKD-EPI)","eGFR (CKD-EPI 2021)","Estimated GFR","GFR"]', 'mL/min/1.73m2', 'Kidney', 60.0, NULL, 90.0, NULL, 'calculated'),
    ('44733-4', 'TG/HDL Ratio', '["Triglyceride HDL Ratio","TG/HDL"]', '', 'Lipid Panel', NULL, 3.5, NULL, 2.0, 'calculated'),
    ('HOMA-IR', 'HOMA-IR', '["Homeostatic Model Assessment"]', '', 'Metabolic', NULL, 2.5, NULL, 1.5, 'calculated');

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
