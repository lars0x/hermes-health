-- Migration 011: seed default reference & optimal ranges.
--
-- Runs after 009 (biomarker seed); each row resolves biomarker_id from loinc_code
-- exactly like unit_conversions. Idempotent via INSERT OR IGNORE on the
-- UNIQUE(biomarker_id, sex, age_min, age_max, source) key, so re-running on every
-- startup is a no-op once seeded. All seeded rows are is_default = 1.
--
-- Sourcing (see commit discussion / research notes):
--   Reference ("normal") ranges prefer the controlling guideline where one
--   defines the cutoff (ADA 2025, NCEP ATP III, NLA 2024, ACG 2017, CKD-EPI
--   2021/KDIGO 2024, WHO 2024/2020, AHA/CDC 2003, Travison 2017/CDC HoSt),
--   otherwise the major reference labs (Mayo, ARUP, LabCorp, Cleveland Clinic /
--   StatPearls for CBC). "Optimal" ranges are guideline desirable-tiers where
--   they exist, else preventive/functional-medicine convention (flagged
--   low-evidence in notes).
--
-- Every (biomarker, sex) has an open-age (0..200) default row as the fallback;
-- sex-specific markers add 'male'/'female' rows; DHEA-S and ESR add age bands.
-- For strongly sex-dimorphic markers the 'any' row is a widened union so an
-- unknown-sex profile is not falsely flagged.

-- =====================================================================
--  REFERENCE RANGES
-- =====================================================================

-- Lipid Panel
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,125,200,'NCEP ATP III 2001','https://www.nhlbi.nih.gov/files/docs/guidelines/atp3xsum.pdf',NULL,1 FROM biomarkers WHERE loinc_code='2093-3';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,NULL,100,'NCEP ATP III 2001','https://www.nhlbi.nih.gov/files/docs/guidelines/atp3xsum.pdf','Optimal <100; near-optimal 100-129',1 FROM biomarkers WHERE loinc_code='13457-7';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,40,NULL,'NCEP ATP III 2001',NULL,'Sex-agnostic fallback',1 FROM biomarkers WHERE loinc_code='2085-9';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'male',0,200,40,NULL,'NCEP ATP III 2001',NULL,NULL,1 FROM biomarkers WHERE loinc_code='2085-9';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'female',0,200,50,NULL,'NCEP ATP III 2001',NULL,'Low-HDL threshold is higher in women',1 FROM biomarkers WHERE loinc_code='2085-9';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,NULL,150,'NCEP ATP III 2001',NULL,NULL,1 FROM biomarkers WHERE loinc_code='2571-8';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,NULL,90,'NLA Expert Consensus 2024',NULL,'<90 desirable; >130 high',1 FROM biomarkers WHERE loinc_code='1884-6';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,NULL,75,'NLA Lp(a) Statement 2024',NULL,'75-125 intermediate; >=125 nmol/L high',1 FROM biomarkers WHERE loinc_code='10835-7';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,NULL,5,'AHA / Framingham convention',NULL,'Derived ratio; low evidence',1 FROM biomarkers WHERE loinc_code='32309-7';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,NULL,3.5,'Insulin-resistance literature',NULL,'Surrogate for IR; low evidence; mg/dL units',1 FROM biomarkers WHERE loinc_code='44733-4';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'male',0,200,NULL,3.5,'Insulin-resistance literature',NULL,NULL,1 FROM biomarkers WHERE loinc_code='44733-4';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'female',0,200,NULL,2.5,'Insulin-resistance literature',NULL,'Adverse threshold lower in women',1 FROM biomarkers WHERE loinc_code='44733-4';

-- Metabolic & Protein
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,70,99,'ADA Standards of Care 2025',NULL,'Normal FPG <100; prediabetes 100-125',1 FROM biomarkers WHERE loinc_code='2345-7';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,NULL,5.7,'ADA Standards of Care 2025',NULL,'Prediabetes 5.7-6.4; diabetes >=6.5',1 FROM biomarkers WHERE loinc_code='4548-4';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,2,25,'Mayo Clinic Labs 2024',NULL,'Assay-dependent',1 FROM biomarkers WHERE loinc_code='2484-4';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,NULL,2.5,'Insulin-resistance literature',NULL,'No universal cutoff; population-dependent',1 FROM biomarkers WHERE loinc_code='HOMA-IR';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,60,80,'Mayo Clinic Labs 2024',NULL,NULL,1 FROM biomarkers WHERE loinc_code='2885-2';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,35,50,'Mayo Clinic Labs 2024',NULL,NULL,1 FROM biomarkers WHERE loinc_code='1751-7';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,20,35,'Calculated (TP - albumin)',NULL,NULL,1 FROM biomarkers WHERE loinc_code='10834-0';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,1.2,2.2,'Mayo Clinic Labs 2024',NULL,'Calculated ratio',1 FROM biomarkers WHERE loinc_code='1759-0';

-- Liver
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,7,55,'ACG Guideline 2017 (Kwo)','https://journals.lww.com/ajg/fulltext/2017/01000/acg_clinical_guideline__evaluation_of_abnormal.13.aspx','Sex-agnostic fallback',1 FROM biomarkers WHERE loinc_code='1742-6';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'male',0,200,7,55,'ACG Guideline 2017 (Kwo)','https://journals.lww.com/ajg/fulltext/2017/01000/acg_clinical_guideline__evaluation_of_abnormal.13.aspx',NULL,1 FROM biomarkers WHERE loinc_code='1742-6';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'female',0,200,7,45,'ACG Guideline 2017 (Kwo)','https://journals.lww.com/ajg/fulltext/2017/01000/acg_clinical_guideline__evaluation_of_abnormal.13.aspx',NULL,1 FROM biomarkers WHERE loinc_code='1742-6';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,10,40,'ACG Guideline 2017 (Kwo)',NULL,'Sex-agnostic fallback',1 FROM biomarkers WHERE loinc_code='1920-8';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'male',0,200,10,40,'ACG Guideline 2017 (Kwo)',NULL,NULL,1 FROM biomarkers WHERE loinc_code='1920-8';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'female',0,200,10,35,'ACG Guideline 2017 (Kwo)',NULL,NULL,1 FROM biomarkers WHERE loinc_code='1920-8';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,8,61,'ARUP Consult 2024',NULL,'Sex-agnostic fallback',1 FROM biomarkers WHERE loinc_code='2324-2';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'male',0,200,8,61,'ARUP Consult 2024',NULL,NULL,1 FROM biomarkers WHERE loinc_code='2324-2';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'female',0,200,6,42,'ARUP Consult 2024',NULL,'Markedly lower in women',1 FROM biomarkers WHERE loinc_code='2324-2';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,44,147,'Mayo Clinic Labs 2024',NULL,'Assay-dependent; rises in pregnancy/adolescence',1 FROM biomarkers WHERE loinc_code='6768-6';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,0.1,1.2,'Mayo Clinic Labs 2024',NULL,NULL,1 FROM biomarkers WHERE loinc_code='1975-2';

-- Kidney
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,0.59,1.35,'Mayo Clinic Labs 2024',NULL,'Sex-agnostic union; use with CKD-EPI 2021',1 FROM biomarkers WHERE loinc_code='2160-0';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'male',0,200,0.74,1.35,'Mayo Clinic Labs 2024',NULL,NULL,1 FROM biomarkers WHERE loinc_code='2160-0';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'female',0,200,0.59,1.04,'Mayo Clinic Labs 2024',NULL,NULL,1 FROM biomarkers WHERE loinc_code='2160-0';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,6,20,'Mayo Clinic Labs 2024',NULL,NULL,1 FROM biomarkers WHERE loinc_code='3094-0';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,0.5,1.0,'Mayo Clinic Labs 2024',NULL,'Age-partitioned in source',1 FROM biomarkers WHERE loinc_code='33863-2';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,60,NULL,'CKD-EPI 2021 / KDIGO 2024',NULL,'>=60 normal; <60 = CKD G3+',1 FROM biomarkers WHERE loinc_code='98979-8';

-- Thyroid
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,0.4,4.5,'Mayo Clinic Labs 2024',NULL,'Rises with age',1 FROM biomarkers WHERE loinc_code='3016-3';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,0.8,1.8,'Mayo Clinic Labs 2024',NULL,'Assay-dependent',1 FROM biomarkers WHERE loinc_code='3024-7';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,2.0,4.4,'Mayo Clinic Labs 2024',NULL,'Assay-dependent',1 FROM biomarkers WHERE loinc_code='3051-0';

-- Hormonal
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,8,916,'Travison 2017 JCEM / Mayo','https://academic.oup.com/jcem/article/102/4/1161/2884621','Union of sexes; set profile sex for a meaningful range',1 FROM biomarkers WHERE loinc_code='2986-8';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'male',0,200,264,916,'Travison 2017 JCEM (CDC HoSt)','https://academic.oup.com/jcem/article/102/4/1161/2884621','Harmonized adult-male interval',1 FROM biomarkers WHERE loinc_code='2986-8';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'female',0,200,8,60,'Mayo Clinic Labs 2024',NULL,'Adult female interval',1 FROM biomarkers WHERE loinc_code='2986-8';
-- DHEA-S: sex + age banded
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,35,430,'ARUP Consult 2024',NULL,'Sex/age-agnostic fallback; declines several-fold with age',1 FROM biomarkers WHERE loinc_code='2191-5';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'male',18,49,89,450,'ARUP Consult 2024',NULL,NULL,1 FROM biomarkers WHERE loinc_code='2191-5';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'male',50,200,25,330,'ARUP Consult 2024',NULL,NULL,1 FROM biomarkers WHERE loinc_code='2191-5';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'female',18,49,60,395,'ARUP Consult 2024',NULL,NULL,1 FROM biomarkers WHERE loinc_code='2191-5';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'female',50,200,25,200,'ARUP Consult 2024',NULL,NULL,1 FROM biomarkers WHERE loinc_code='2191-5';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,5,25,'Mayo Clinic Labs 2024',NULL,'AM draw; strong diurnal variation',1 FROM biomarkers WHERE loinc_code='2143-6';

-- Inflammatory
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,NULL,3,'AHA/CDC 2003 (Pearson)',NULL,'<1 low, 1-3 average, >3 high CVD risk',1 FROM biomarkers WHERE loinc_code='30522-7';
-- ESR: sex + age banded (approximation of the Westergren age/sex formula)
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,NULL,20,'Westergren formula (Miller 1983)',NULL,'Flat fallback; true cutoff is age/sex adjusted',1 FROM biomarkers WHERE loinc_code='4537-7';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'male',0,49,NULL,15,'Westergren formula (Miller 1983)',NULL,'~age/2',1 FROM biomarkers WHERE loinc_code='4537-7';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'male',50,200,NULL,20,'Westergren formula (Miller 1983)',NULL,'~age/2',1 FROM biomarkers WHERE loinc_code='4537-7';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'female',0,49,NULL,20,'Westergren formula (Miller 1983)',NULL,'~(age+10)/2',1 FROM biomarkers WHERE loinc_code='4537-7';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'female',50,200,NULL,30,'Westergren formula (Miller 1983)',NULL,'~(age+10)/2',1 FROM biomarkers WHERE loinc_code='4537-7';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,11,336,'Mayo Clinic Labs / WHO 2020',NULL,'Sex-agnostic union; WHO deficiency <15 (or <30 with inflammation)',1 FROM biomarkers WHERE loinc_code='2276-4';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'male',0,200,24,336,'Mayo Clinic Labs 2024',NULL,NULL,1 FROM biomarkers WHERE loinc_code='2276-4';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'female',0,200,11,307,'Mayo Clinic Labs 2024',NULL,NULL,1 FROM biomarkers WHERE loinc_code='2276-4';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,NULL,15,'Mayo Clinic Labs 2024',NULL,'Mild elevation 15-30',1 FROM biomarkers WHERE loinc_code='13965-9';

-- Vitamins & Minerals
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,20,100,'Endocrine Society 2024 / IOM 2011',NULL,'IOM sufficiency >20',1 FROM biomarkers WHERE loinc_code='1989-3';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,180,914,'Mayo Clinic Labs 2024',NULL,'<150 deficient; 150-400 borderline',1 FROM biomarkers WHERE loinc_code='2132-9';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,4,NULL,'Mayo Clinic Labs 2024',NULL,'Deficiency <4; no clinical upper bound',1 FROM biomarkers WHERE loinc_code='2284-8';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,1.7,2.4,'LabCorp 2024',NULL,NULL,1 FROM biomarkers WHERE loinc_code='19123-9';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,60,120,'ARUP Consult 2024',NULL,NULL,1 FROM biomarkers WHERE loinc_code='2601-3';

-- Hematology (CBC)
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,12,17,'WHO 2024 / Cleveland Clinic',NULL,'Sex-agnostic union',1 FROM biomarkers WHERE loinc_code='718-7';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'male',0,200,13,17,'WHO 2024 / Cleveland Clinic',NULL,'Anemia <13 (WHO)',1 FROM biomarkers WHERE loinc_code='718-7';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'female',0,200,12,15.5,'WHO 2024 / Cleveland Clinic',NULL,'Anemia <12 (WHO, non-pregnant)',1 FROM biomarkers WHERE loinc_code='718-7';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,36,50,'Cleveland Clinic / StatPearls 2024',NULL,'Sex-agnostic union',1 FROM biomarkers WHERE loinc_code='4544-3';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'male',0,200,40,50,'Cleveland Clinic / StatPearls 2024',NULL,NULL,1 FROM biomarkers WHERE loinc_code='4544-3';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'female',0,200,36,46,'Cleveland Clinic / StatPearls 2024',NULL,NULL,1 FROM biomarkers WHERE loinc_code='4544-3';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,4,11,'StatPearls 2024',NULL,NULL,1 FROM biomarkers WHERE loinc_code='6690-2';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,150,400,'Cleveland Clinic / StatPearls 2024',NULL,NULL,1 FROM biomarkers WHERE loinc_code='26515-7';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,4.0,5.9,'Cleveland Clinic / Mayo 2024',NULL,'Sex-agnostic union',1 FROM biomarkers WHERE loinc_code='789-8';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'male',0,200,4.5,5.9,'Cleveland Clinic / Mayo 2024',NULL,NULL,1 FROM biomarkers WHERE loinc_code='789-8';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'female',0,200,4.0,5.2,'Cleveland Clinic / Mayo 2024',NULL,NULL,1 FROM biomarkers WHERE loinc_code='789-8';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,80,100,'Cleveland Clinic / StatPearls 2024',NULL,NULL,1 FROM biomarkers WHERE loinc_code='787-2';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,27,32,'StatPearls 2024',NULL,NULL,1 FROM biomarkers WHERE loinc_code='785-6';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,32,36,'Cleveland Clinic / StatPearls 2024',NULL,NULL,1 FROM biomarkers WHERE loinc_code='786-4';
INSERT OR IGNORE INTO reference_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,11.5,14.5,'StatPearls 2024',NULL,NULL,1 FROM biomarkers WHERE loinc_code='788-0';


-- =====================================================================
--  OPTIMAL RANGES
-- =====================================================================

-- Lipid Panel
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,150,180,'Preventive-cardiology convention',NULL,'Low evidence; no guideline optimal for TC',1 FROM biomarkers WHERE loinc_code='2093-3';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,NULL,70,'NLA 2024 / risk-based goal',NULL,'<55 for very-high-risk',1 FROM biomarkers WHERE loinc_code='13457-7';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,60,NULL,'NCEP ATP III 2001',NULL,'>=60 is protective (high HDL)',1 FROM biomarkers WHERE loinc_code='2085-9';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,NULL,100,'AHA 2011 scientific statement',NULL,'Optimal <100',1 FROM biomarkers WHERE loinc_code='2571-8';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,NULL,70,'NLA 2024 / risk-based goal',NULL,'<55 for very-high-risk',1 FROM biomarkers WHERE loinc_code='1884-6';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,NULL,30,'NLA / EAS consensus',NULL,'Continuous risk; ~30 nmol/L low percentile',1 FROM biomarkers WHERE loinc_code='10835-7';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,NULL,3.5,'Preventive-cardiology convention',NULL,'Low evidence',1 FROM biomarkers WHERE loinc_code='32309-7';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,NULL,2,'Insulin-resistance literature',NULL,'Low evidence; mg/dL units',1 FROM biomarkers WHERE loinc_code='44733-4';

-- Metabolic & Protein
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,70,90,'Functional-medicine convention',NULL,'Low evidence',1 FROM biomarkers WHERE loinc_code='2345-7';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,NULL,5.0,'Functional-medicine convention',NULL,'Low evidence',1 FROM biomarkers WHERE loinc_code='4548-4';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,2,8,'Insulin-resistance literature',NULL,'Low evidence',1 FROM biomarkers WHERE loinc_code='2484-4';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,NULL,1.5,'Insulin-resistance literature',NULL,'Low evidence',1 FROM biomarkers WHERE loinc_code='HOMA-IR';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,65,80,'Functional-medicine convention',NULL,'Low evidence',1 FROM biomarkers WHERE loinc_code='2885-2';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,38,50,'Weaving 2016 / convention',NULL,'Mild sex/age dependence',1 FROM biomarkers WHERE loinc_code='1751-7';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'male',0,200,40,50,'Weaving 2016 / convention',NULL,NULL,1 FROM biomarkers WHERE loinc_code='1751-7';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'female',0,200,38,48,'Weaving 2016 / convention',NULL,NULL,1 FROM biomarkers WHERE loinc_code='1751-7';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,23,35,'Functional-medicine convention',NULL,'Low evidence',1 FROM biomarkers WHERE loinc_code='10834-0';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,1.2,2.2,'Functional-medicine convention',NULL,'Low evidence',1 FROM biomarkers WHERE loinc_code='1759-0';

-- Liver
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,7,29,'ACG Guideline 2017 (Kwo)',NULL,'True-normal upper limit',1 FROM biomarkers WHERE loinc_code='1742-6';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'male',0,200,7,29,'ACG Guideline 2017 (Kwo)',NULL,NULL,1 FROM biomarkers WHERE loinc_code='1742-6';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'female',0,200,7,25,'ACG Guideline 2017 (Kwo)',NULL,NULL,1 FROM biomarkers WHERE loinc_code='1742-6';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,10,30,'ACG Guideline 2017 (Kwo)',NULL,NULL,1 FROM biomarkers WHERE loinc_code='1920-8';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'male',0,200,10,30,'ACG Guideline 2017 (Kwo)',NULL,NULL,1 FROM biomarkers WHERE loinc_code='1920-8';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'female',0,200,10,26,'ACG Guideline 2017 (Kwo)',NULL,NULL,1 FROM biomarkers WHERE loinc_code='1920-8';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,8,30,'Metabolic-health convention',NULL,'Low evidence',1 FROM biomarkers WHERE loinc_code='2324-2';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'female',0,200,6,30,'Metabolic-health convention',NULL,'Low evidence',1 FROM biomarkers WHERE loinc_code='2324-2';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,44,100,'Functional-medicine convention',NULL,'Low evidence',1 FROM biomarkers WHERE loinc_code='6768-6';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,0.1,1.0,'Functional-medicine convention',NULL,'Low evidence',1 FROM biomarkers WHERE loinc_code='1975-2';

-- Kidney
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,0.59,1.1,'Functional-medicine convention',NULL,'Sex-agnostic; low evidence',1 FROM biomarkers WHERE loinc_code='2160-0';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'male',0,200,0.74,1.1,'Functional-medicine convention',NULL,'Low evidence',1 FROM biomarkers WHERE loinc_code='2160-0';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'female',0,200,0.59,1.0,'Functional-medicine convention',NULL,'Low evidence',1 FROM biomarkers WHERE loinc_code='2160-0';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,7,18,'Functional-medicine convention',NULL,'Low evidence',1 FROM biomarkers WHERE loinc_code='3094-0';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,0.5,0.9,'Functional-medicine convention',NULL,'Low evidence',1 FROM biomarkers WHERE loinc_code='33863-2';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,90,NULL,'CKD-EPI 2021 / KDIGO 2024',NULL,'>=90 = G1',1 FROM biomarkers WHERE loinc_code='98979-8';

-- Thyroid
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,0.5,2.5,'AACE / NACB',NULL,'Narrower euthyroid distribution',1 FROM biomarkers WHERE loinc_code='3016-3';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,1.0,1.5,'Functional-medicine convention',NULL,'Low evidence',1 FROM biomarkers WHERE loinc_code='3024-7';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,2.5,4.0,'Functional-medicine convention',NULL,'Low evidence',1 FROM biomarkers WHERE loinc_code='3051-0';

-- Hormonal
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'male',0,200,500,900,'Functional-medicine convention',NULL,'Low evidence',1 FROM biomarkers WHERE loinc_code='2986-8';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'female',0,200,15,50,'Functional-medicine convention',NULL,'Low evidence',1 FROM biomarkers WHERE loinc_code='2986-8';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'male',0,200,200,400,'Functional-medicine convention',NULL,'Low evidence; declines with age',1 FROM biomarkers WHERE loinc_code='2191-5';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'female',0,200,150,300,'Functional-medicine convention',NULL,'Low evidence; declines with age',1 FROM biomarkers WHERE loinc_code='2191-5';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,8,15,'Functional-medicine convention',NULL,'Low evidence',1 FROM biomarkers WHERE loinc_code='2143-6';

-- Inflammatory
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,NULL,1,'AHA/CDC 2003 (Pearson)',NULL,'<1 = low CVD risk',1 FROM biomarkers WHERE loinc_code='30522-7';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,NULL,10,'Functional-medicine convention',NULL,'Low evidence',1 FROM biomarkers WHERE loinc_code='4537-7';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'female',0,200,NULL,15,'Functional-medicine convention',NULL,'Low evidence',1 FROM biomarkers WHERE loinc_code='4537-7';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,40,200,'Functional-medicine convention',NULL,'Female optimal floor contentious',1 FROM biomarkers WHERE loinc_code='2276-4';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,NULL,8,'Preventive-cardiology convention',NULL,'Low evidence',1 FROM biomarkers WHERE loinc_code='13965-9';

-- Vitamins & Minerals
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,30,50,'Endocrine Society 2024 (commonly cited)',NULL,'2024 ES guideline declined a fixed target',1 FROM biomarkers WHERE loinc_code='1989-3';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,500,800,'Functional-medicine convention',NULL,'Low evidence',1 FROM biomarkers WHERE loinc_code='2132-9';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,10,20,'Functional-medicine convention',NULL,'Low evidence',1 FROM biomarkers WHERE loinc_code='2284-8';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,2.0,2.2,'Functional-medicine convention',NULL,'Low evidence',1 FROM biomarkers WHERE loinc_code='19123-9';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,80,110,'Functional-medicine convention',NULL,'Low evidence',1 FROM biomarkers WHERE loinc_code='2601-3';

-- Hematology (CBC) — optimal indices are mid-reference targets, not guideline-defined
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,13,16,'Mid-reference target',NULL,'Low evidence',1 FROM biomarkers WHERE loinc_code='718-7';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'male',0,200,14,16,'Mid-reference target',NULL,'Low evidence',1 FROM biomarkers WHERE loinc_code='718-7';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'female',0,200,12.5,14.5,'Mid-reference target',NULL,'Low evidence',1 FROM biomarkers WHERE loinc_code='718-7';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,38,48,'Mid-reference target',NULL,'Low evidence',1 FROM biomarkers WHERE loinc_code='4544-3';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'male',0,200,41,48,'Mid-reference target',NULL,'Low evidence',1 FROM biomarkers WHERE loinc_code='4544-3';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'female',0,200,38,44,'Mid-reference target',NULL,'Low evidence',1 FROM biomarkers WHERE loinc_code='4544-3';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,4.5,8,'Mid-reference target',NULL,'Low evidence',1 FROM biomarkers WHERE loinc_code='6690-2';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,200,350,'Mid-reference target',NULL,'Low evidence',1 FROM biomarkers WHERE loinc_code='26515-7';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,4.2,5.5,'Mid-reference target',NULL,'Low evidence',1 FROM biomarkers WHERE loinc_code='789-8';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'male',0,200,4.6,5.7,'Mid-reference target',NULL,'Low evidence',1 FROM biomarkers WHERE loinc_code='789-8';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'female',0,200,4.2,5.2,'Mid-reference target',NULL,'Low evidence',1 FROM biomarkers WHERE loinc_code='789-8';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,82,95,'Mid-reference target',NULL,'Low evidence',1 FROM biomarkers WHERE loinc_code='787-2';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,27,32,'Mid-reference target',NULL,'Low evidence',1 FROM biomarkers WHERE loinc_code='785-6';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,32,35,'Mid-reference target',NULL,'Low evidence',1 FROM biomarkers WHERE loinc_code='786-4';
INSERT OR IGNORE INTO optimal_ranges (biomarker_id, sex, age_min, age_max, low, high, source, source_url, notes, is_default)
  SELECT id,'any',0,200,11.5,14,'Mid-reference target',NULL,'Low evidence',1 FROM biomarkers WHERE loinc_code='788-0';
