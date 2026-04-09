use hermes_health::db;
use hermes_health::db::models::NewObservation;
use hermes_health::export::csv_export;
use hermes_health::services::{biomarker, loinc, observation, seed, trend};

async fn setup() -> (sqlx::SqlitePool, loinc::LoincCatalog) {
    let pool = db::create_pool(std::path::Path::new(":memory:"))
        .await
        .unwrap();
    db::migrate::run_migrations(&pool).await.unwrap();
    let catalog = loinc::LoincCatalog::load();
    seed::seed_biomarkers(&pool).await.unwrap();
    (pool, catalog)
}

#[tokio::test]
async fn test_seed_biomarkers() {
    let (pool, _catalog) = setup().await;
    let biomarkers = biomarker::list_biomarkers(&pool, None).await.unwrap();
    assert_eq!(biomarkers.len(), 49);

    let tc = biomarkers
        .iter()
        .find(|b| b.loinc_code == "2093-3")
        .unwrap();
    assert_eq!(tc.name, "Total Cholesterol");
    assert_eq!(tc.unit, "mg/dL");
    assert_eq!(tc.category, "Lipid Panel");
}

#[tokio::test]
async fn test_resolve_by_loinc_code() {
    let (pool, catalog) = setup().await;
    let bm = biomarker::resolve_biomarker(&pool, "2093-3", &catalog)
        .await
        .unwrap();
    assert_eq!(bm.name, "Total Cholesterol");
}

#[tokio::test]
async fn test_resolve_by_alias() {
    let (pool, catalog) = setup().await;
    let bm = biomarker::resolve_biomarker(&pool, "HDL-C", &catalog)
        .await
        .unwrap();
    assert_eq!(bm.loinc_code, "2085-9");
}

#[tokio::test]
async fn test_add_observation_same_unit() {
    let (pool, catalog) = setup().await;
    let obs = NewObservation {
        biomarker: "2093-3".to_string(),
        value: 185.0,
        unit: "mg/dL".to_string(),
        observed_at: "2026-03-15".to_string(),
        lab_name: Some("Parkway".to_string()),
        fasting: Some(true),
        notes: None,
        report_id: None,
        import_id: None,
        original_value: None,
    };
    let result = observation::add_observation(&pool, &catalog, &obs)
        .await
        .unwrap();
    assert_eq!(result.value, 185.0);
    assert!(!result.converted);
    assert_eq!(result.biomarker_name, "Total Cholesterol");
}

#[tokio::test]
async fn test_add_observation_with_conversion() {
    let (pool, catalog) = setup().await;
    let obs = NewObservation {
        biomarker: "Total Cholesterol".to_string(),
        value: 4.8,
        unit: "mmol/L".to_string(),
        observed_at: "2026-03-15".to_string(),
        lab_name: None,
        fasting: None,
        notes: None,
        report_id: None,
        import_id: None,
        original_value: None,
    };
    let result = observation::add_observation(&pool, &catalog, &obs)
        .await
        .unwrap();
    assert!(result.converted);
    // 4.8 * 38.67 = 185.616, rounded to 2 sig figs = 190
    assert_eq!(result.value, 190.0);
}

#[tokio::test]
async fn test_hba1c_conversion_with_offset() {
    let (pool, catalog) = setup().await;
    // HbA1c: 42 mmol/mol -> % = 0.0915 * 42 + 2.15 = 5.993
    // 42 has 2 sig figs -> round to 2 sig figs = 6.0
    let obs = NewObservation {
        biomarker: "HbA1c".to_string(),
        value: 42.0,
        unit: "mmol/mol".to_string(),
        observed_at: "2026-03-15".to_string(),
        lab_name: None,
        fasting: None,
        notes: None,
        report_id: None,
        import_id: None,
        original_value: None,
    };
    let result = observation::add_observation(&pool, &catalog, &obs)
        .await
        .unwrap();
    assert!(result.converted);
    assert!((result.value - 6.0).abs() < 0.1);
}

#[tokio::test]
async fn test_trend_analysis() {
    let (pool, catalog) = setup().await;

    // Values above optimal (180) and decreasing toward it = improving
    let dates = ["2025-10-01", "2025-12-01", "2026-01-15", "2026-03-01"];
    let values = [210.0, 205.0, 198.0, 192.0];

    for (date, value) in dates.iter().zip(values.iter()) {
        let obs = NewObservation {
            biomarker: "2093-3".to_string(),
            value: *value,
            unit: "mg/dL".to_string(),
            observed_at: date.to_string(),
            lab_name: None,
            fasting: Some(true),
            notes: None,
            report_id: None,
            import_id: None,
        original_value: None,
        };
        observation::add_observation(&pool, &catalog, &obs)
            .await
            .unwrap();
    }

    let bm = biomarker::resolve_biomarker(&pool, "2093-3", &catalog)
        .await
        .unwrap();
    let result = trend::compute_trend(&pool, bm.id, 365, 3, 20.0, 180)
        .await
        .unwrap();

    assert_eq!(result.observations.len(), 4);
    let t = result.trend.unwrap();
    assert_eq!(t.direction, "decreasing");
    assert_eq!(t.status, "improving"); // above optimal and decreasing = improving
    assert!(t.slope < 0.0);
    assert_eq!(t.latest_value, 192.0);
    assert_eq!(t.previous_value, Some(198.0));
}

#[tokio::test]
async fn test_trend_insufficient_data() {
    let (pool, catalog) = setup().await;

    for (date, val) in [("2026-03-01", 185.0), ("2026-03-15", 180.0)] {
        let obs = NewObservation {
            biomarker: "2093-3".to_string(),
            value: val,
            unit: "mg/dL".to_string(),
            observed_at: date.to_string(),
            lab_name: None,
            fasting: None,
            notes: None,
            report_id: None,
            import_id: None,
        original_value: None,
        };
        observation::add_observation(&pool, &catalog, &obs)
            .await
            .unwrap();
    }

    let bm = biomarker::resolve_biomarker(&pool, "2093-3", &catalog)
        .await
        .unwrap();
    let result = trend::compute_trend(&pool, bm.id, 365, 3, 20.0, 180)
        .await
        .unwrap();

    assert_eq!(result.observations.len(), 2);
    assert!(result.trend.is_none());
}

#[tokio::test]
async fn test_csv_export() {
    let (pool, catalog) = setup().await;

    let obs = NewObservation {
        biomarker: "2093-3".to_string(),
        value: 185.0,
        unit: "mg/dL".to_string(),
        observed_at: "2026-03-15".to_string(),
        lab_name: Some("Parkway".to_string()),
        fasting: Some(true),
        notes: None,
        report_id: None,
        import_id: None,
        original_value: None,
    };
    observation::add_observation(&pool, &catalog, &obs)
        .await
        .unwrap();

    let mut buf = Vec::new();
    let count = csv_export::export_csv(&pool, &mut buf, None, None)
        .await
        .unwrap();
    assert_eq!(count, 1);

    let csv_content = String::from_utf8(buf).unwrap();
    assert!(csv_content.contains("2093-3"));
    assert!(csv_content.contains("Total Cholesterol"));
    assert!(csv_content.contains("185"));
    assert!(csv_content.contains("Parkway"));
}

#[tokio::test]
async fn test_loinc_catalog_search() {
    let catalog = loinc::LoincCatalog::load();

    let results = catalog.search("2093-3", 3);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].loinc_code, "2093-3");

    let results = catalog.search("Cholesterol", 5);
    assert!(!results.is_empty());

    let results = catalog.search("xyzzy12345", 3);
    assert!(results.is_empty());
}

#[tokio::test]
async fn test_unrecognized_unit_stored_as_is() {
    // Unknown units should be stored in original unit, not rejected
    let (pool, catalog) = setup().await;
    let obs = NewObservation {
        biomarker: "2093-3".to_string(),
        value: 4.8,
        unit: "bananas/L".to_string(),
        observed_at: "2026-03-15".to_string(),
        lab_name: None,
        fasting: None,
        notes: None,
        report_id: None,
        import_id: None,
        original_value: None,
    };
    let result = observation::add_observation(&pool, &catalog, &obs).await;
    assert!(result.is_ok());
    let r = result.unwrap();
    assert_eq!(r.value, 4.8); // stored as-is, no conversion
}

#[tokio::test]
async fn test_cholesterol_mmol_to_mgdl() {
    let (pool, catalog) = setup().await;
    let obs = NewObservation {
        biomarker: "2093-3".to_string(),
        value: 5.51,  // 3 sig figs: 5.51 * 38.67 = 213.07, rounds to 213
        unit: "mmol/L".to_string(),
        observed_at: "2026-03-15".to_string(),
        lab_name: None,
        fasting: None,
        notes: None,
        report_id: None,
        import_id: None,
        original_value: None,
    };
    let result = observation::add_observation(&pool, &catalog, &obs).await.unwrap();
    // 5.51 * 38.67 = 213.07, rounded to 3 sig figs = 213
    assert!(result.converted);
    assert!(
        (result.value - 213.0).abs() < 1.0,
        "Expected ~213, got {}",
        result.value
    );
}

#[tokio::test]
async fn test_glucose_mmol_to_mgdl() {
    let (pool, catalog) = setup().await;
    let obs = NewObservation {
        biomarker: "Glucose".to_string(),
        value: 5.5,
        unit: "mmol/L".to_string(),
        observed_at: "2026-03-15".to_string(),
        lab_name: None,
        fasting: None,
        notes: None,
        report_id: None,
        import_id: None,
        original_value: None,
    };
    let result = observation::add_observation(&pool, &catalog, &obs).await.unwrap();
    assert!(result.converted);
    // 5.5 * 18.018 = 99.099, rounded to 2 sig figs = 99
    assert!((result.value - 99.0).abs() < 1.0);
}

#[tokio::test]
async fn test_hba1c_mmolmol_to_percent() {
    let (pool, catalog) = setup().await;
    let obs = NewObservation {
        biomarker: "HbA1c".to_string(),
        value: 42.0,
        unit: "mmol/mol".to_string(),
        observed_at: "2026-03-15".to_string(),
        lab_name: None,
        fasting: None,
        notes: None,
        report_id: None,
        import_id: None,
        original_value: None,
    };
    let result = observation::add_observation(&pool, &catalog, &obs).await.unwrap();
    assert!(result.converted);
    // 42 * 0.0915 + 2.15 = 5.993, rounded to 2 sig figs = 6.0
    assert!((result.value - 6.0).abs() < 0.1);
}

#[tokio::test]
async fn test_cbc_x10e9_normalizes_to_canonical() {
    let (pool, catalog) = setup().await;
    // "x 10^9/L" should normalize to "10*3/uL" (same unit, different notation)
    let obs = NewObservation {
        biomarker: "WBC".to_string(),
        value: 6.5,
        unit: "x 10^9/L".to_string(),
        observed_at: "2026-03-15".to_string(),
        lab_name: None,
        fasting: None,
        notes: None,
        report_id: None,
        import_id: None,
        original_value: None,
    };
    let result = observation::add_observation(&pool, &catalog, &obs).await.unwrap();
    // Should NOT be converted - units are equivalent
    assert_eq!(result.value, 6.5);
}

#[tokio::test]
async fn test_rbc_x10e12_normalizes_to_canonical() {
    let (pool, catalog) = setup().await;
    let obs = NewObservation {
        biomarker: "RBC".to_string(),
        value: 4.8,
        unit: "x 10^12/L".to_string(),
        observed_at: "2026-03-15".to_string(),
        lab_name: None,
        fasting: None,
        notes: None,
        report_id: None,
        import_id: None,
        original_value: None,
    };
    let result = observation::add_observation(&pool, &catalog, &obs).await.unwrap();
    assert_eq!(result.value, 4.8);
}

#[tokio::test]
async fn test_same_unit_no_conversion() {
    let (pool, catalog) = setup().await;
    let obs = NewObservation {
        biomarker: "2093-3".to_string(),
        value: 185.0,
        unit: "mg/dL".to_string(),
        observed_at: "2026-03-15".to_string(),
        lab_name: None,
        fasting: None,
        notes: None,
        report_id: None,
        import_id: None,
        original_value: None,
    };
    let result = observation::add_observation(&pool, &catalog, &obs).await.unwrap();
    assert!(!result.converted);
    assert_eq!(result.value, 185.0);
}

#[tokio::test]
async fn test_case_insensitive_unit_match() {
    let (pool, catalog) = setup().await;
    // "mg/dl" should match "mg/dL" - value stored unchanged
    let obs = NewObservation {
        biomarker: "2093-3".to_string(),
        value: 185.0,
        unit: "mg/dl".to_string(),
        observed_at: "2026-03-15".to_string(),
        lab_name: None,
        fasting: None,
        notes: None,
        report_id: None,
        import_id: None,
        original_value: None,
    };
    let result = observation::add_observation(&pool, &catalog, &obs).await.unwrap();
    // Value should be stored unchanged (no numeric conversion)
    assert_eq!(result.value, 185.0);
}

#[tokio::test]
async fn test_observation_precision_preserved() {
    let (pool, catalog) = setup().await;

    // Add observation with high precision
    let obs = NewObservation {
        biomarker: "HbA1c".to_string(),
        value: 5.20,
        unit: "%".to_string(),
        observed_at: "2026-03-15".to_string(),
        lab_name: None,
        fasting: None,
        notes: None,
        report_id: None,
        import_id: None,
        original_value: None,
    };
    let result = observation::add_observation(&pool, &catalog, &obs)
        .await
        .unwrap();
    assert_eq!(result.value, 5.2);
}

#[tokio::test]
async fn test_dashboard_summary() {
    let (pool, catalog) = setup().await;

    // Add an observation that's out of reference range for HDL (low is 40, optimal is 60)
    let obs = NewObservation {
        biomarker: "HDL-C".to_string(),
        value: 35.0, // below reference low of 40
        unit: "mg/dL".to_string(),
        observed_at: "2026-03-15".to_string(),
        lab_name: None,
        fasting: None,
        notes: None,
        report_id: None,
        import_id: None,
        original_value: None,
    };
    observation::add_observation(&pool, &catalog, &obs)
        .await
        .unwrap();

    let summary = biomarker::dashboard_summary(&pool).await.unwrap();
    assert_eq!(summary.total_tracked, 49);
    assert!(summary.out_of_range >= 1); // HDL at 35 is out of reference range
}
