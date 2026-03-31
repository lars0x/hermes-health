mod agent;
mod cli;
mod config;
mod db;
mod error;
mod export;
mod ingest;
mod services;
mod web;

use std::sync::Arc;

use clap::Parser;
use std::path::Path;

use cli::{BiomarkerCmd, Cli, Commands, ExportCmd, ObsCmd};
use config::HermesConfig;
use db::models::NewObservation;
use services::loinc::LoincCatalog;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "hermes_health=info".into()),
        )
        .init();

    let cli = Cli::parse();

    // Load config
    let cfg = HermesConfig::load(Some(Path::new(&cli.config)))?;

    // Connect to database
    let pool = db::create_pool(&cfg.database.path).await?;

    // Run migrations
    db::migrate::run_migrations(&pool).await?;

    // Load LOINC catalog
    let catalog = LoincCatalog::load();

    match cli.command {
        Commands::Init => {
            let count = services::seed::seed_biomarkers(&pool).await?;
            println!("Database initialized. Seeded {} biomarkers.", count);
            println!(
                "LOINC catalog loaded: {} entries.",
                catalog.entry_count()
            );
        }

        Commands::Biomarker(cmd) => match cmd {
            BiomarkerCmd::List { category } => {
                let biomarkers =
                    services::biomarker::list_biomarkers(&pool, category.as_deref()).await?;
                if biomarkers.is_empty() {
                    println!("No biomarkers found. Run 'hermes init' first.");
                    return Ok(());
                }
                println!(
                    "{:<12} {:<25} {:<10} {:<15} {:<12} {:<12}",
                    "LOINC", "Name", "Unit", "Category", "Ref Range", "Optimal"
                );
                println!("{}", "-".repeat(86));
                for bm in &biomarkers {
                    let ref_range = format!(
                        "{}-{}",
                        bm.reference_low.map(|v| format!("{v}")).unwrap_or_default(),
                        bm.reference_high.map(|v| format!("{v}")).unwrap_or_default()
                    );
                    let optimal = format!(
                        "{}-{}",
                        bm.optimal_low.map(|v| format!("{v}")).unwrap_or_default(),
                        bm.optimal_high.map(|v| format!("{v}")).unwrap_or_default()
                    );
                    println!(
                        "{:<12} {:<25} {:<10} {:<15} {:<12} {:<12}",
                        bm.loinc_code,
                        truncate(&bm.name, 24),
                        bm.unit,
                        truncate(&bm.category, 14),
                        ref_range,
                        optimal
                    );
                }
                println!("\nTotal: {} biomarkers", biomarkers.len());
            }
            BiomarkerCmd::Show { identifier } => {
                let bm =
                    services::biomarker::resolve_biomarker(&pool, &identifier, &catalog).await?;
                println!("Biomarker: {} ({})", bm.name, bm.loinc_code);
                println!("  Category: {}", bm.category);
                println!("  Unit: {}", bm.unit);
                println!(
                    "  Reference range: {} - {}",
                    bm.reference_low.map(|v| v.to_string()).unwrap_or("-".into()),
                    bm.reference_high
                        .map(|v| v.to_string())
                        .unwrap_or("-".into())
                );
                println!(
                    "  Optimal range: {} - {}",
                    bm.optimal_low.map(|v| v.to_string()).unwrap_or("-".into()),
                    bm.optimal_high
                        .map(|v| v.to_string())
                        .unwrap_or("-".into())
                );
                println!("  Aliases: {:?}", bm.aliases_vec());
                println!("  Source: {}", bm.source);

                // Show recent observations
                let obs =
                    services::observation::list_for_biomarker(&pool, bm.id, None, None).await?;
                if !obs.is_empty() {
                    println!("\n  Recent observations:");
                    for o in obs.iter().rev().take(10) {
                        let prec = o.precision as usize;
                        println!(
                            "    {} : {:.prec$} {} {}",
                            o.observed_at,
                            o.value,
                            bm.unit,
                            o.notes.as_deref().unwrap_or("")
                        );
                    }
                }
            }
        },

        Commands::Obs(cmd) => match cmd {
            ObsCmd::Add {
                biomarker,
                value,
                unit,
                date,
                lab,
                fasting,
                notes,
            } => {
                let obs = NewObservation {
                    biomarker,
                    value,
                    unit,
                    observed_at: date,
                    lab_name: lab,
                    fasting,
                    notes,
                };
                let result = services::observation::add_observation(&pool, &catalog, &obs).await?;
                println!(
                    "Observation added: {} = {} {} (id={}{})",
                    result.biomarker_name,
                    result.value,
                    result.unit,
                    result.id,
                    if result.converted {
                        " [unit converted]"
                    } else {
                        ""
                    }
                );
            }
            ObsCmd::List {
                biomarker,
                from,
                to,
            } => {
                let observations = if let Some(bm_id) = biomarker {
                    let bm =
                        services::biomarker::resolve_biomarker(&pool, &bm_id, &catalog).await?;
                    services::observation::list_for_biomarker(
                        &pool,
                        bm.id,
                        from.as_deref(),
                        to.as_deref(),
                    )
                    .await?
                } else {
                    services::observation::list_all(&pool, from.as_deref(), to.as_deref()).await?
                };

                if observations.is_empty() {
                    println!("No observations found.");
                    return Ok(());
                }

                println!(
                    "{:<6} {:<12} {:<10} {:<12} {:<10} {}",
                    "ID", "Date", "Value", "Unit", "Fasting", "Notes"
                );
                println!("{}", "-".repeat(65));
                for o in &observations {
                    let prec = o.precision as usize;
                    let fasting = match o.fasting {
                        Some(true) => "yes",
                        Some(false) => "no",
                        None => "-",
                    };
                    println!(
                        "{:<6} {:<12} {:<10} {:<12} {:<10} {}",
                        o.id,
                        o.observed_at,
                        format!("{:.prec$}", o.value),
                        o.original_unit,
                        fasting,
                        o.notes.as_deref().unwrap_or("")
                    );
                }
                println!("\nTotal: {} observations", observations.len());
            }
        },

        Commands::Trend { biomarker, window } => {
            let bm =
                services::biomarker::resolve_biomarker(&pool, &biomarker, &catalog).await?;
            let trend = services::trend::compute_trend(
                &pool,
                bm.id,
                window,
                cfg.trends.min_data_points,
                cfg.trends.rapid_change_threshold_pct,
                cfg.trends.projection_horizon_days,
            )
            .await?;

            println!("Trend: {} ({})", bm.name, bm.loinc_code);
            println!("  Window: {} days", window);
            println!("  Data points: {}", trend.observations.len());

            if let Some(t) = &trend.trend {
                println!("  Direction: {}", t.direction);
                println!("  Status: {}", t.status);
                println!("  Slope: {:.2} {}", t.slope, t.slope_unit);
                println!("  R-squared: {:.3}", t.r_squared);
                println!("  Rate of change: {:.1}%", t.rate_of_change_pct);
                println!("  Annualized rate: {:.1}%", t.annualized_rate_pct);
                println!("  Latest: {}", t.latest_value);
                if let Some(prev) = t.previous_value {
                    println!("  Previous: {}", prev);
                }
                if !t.alerts.is_empty() {
                    println!("  Alerts:");
                    for alert in &t.alerts {
                        println!("    [{}] {}", alert.alert_type, alert.message);
                    }
                }
            } else {
                println!(
                    "  Insufficient data (need at least {} points)",
                    cfg.trends.min_data_points
                );
            }
        }

        Commands::Export(cmd) => match cmd {
            ExportCmd::Csv { output, from, to } => {
                let mut file = std::fs::File::create(&output)?;
                let count = export::csv_export::export_csv(
                    &pool,
                    &mut file,
                    from.as_deref(),
                    to.as_deref(),
                )
                .await?;
                println!("Exported {} observations to {}", count, output);
            }
        },

        Commands::Serve { host, port } => {
            let host = host.unwrap_or(cfg.server.host.clone());
            let port = port.unwrap_or(cfg.server.port);
            let addr = format!("{host}:{port}");

            // Seed biomarkers if empty
            let count = crate::db::queries::count_biomarkers(&pool).await?;
            if count == 0 {
                services::seed::seed_biomarkers(&pool).await?;
                tracing::info!("Auto-seeded biomarkers on first run");
            }

            let state = web::AppState {
                pool,
                catalog: Arc::new(catalog),
                config: Arc::new(cfg),
                templates: web::templates::TemplateEngine::new(),
            };

            let app = web::routes::router().with_state(state);

            let listener = tokio::net::TcpListener::bind(&addr).await?;
            tracing::info!("Hermes Health server running at http://{addr}");
            println!("Hermes Health server running at http://{addr}");
            axum::serve(listener, app).await?;
        }

        Commands::Search { query, max } => {
            let results = catalog.search(&query, max);
            if results.is_empty() {
                println!("No matches found for '{}'", query);
            } else {
                println!(
                    "{:<12} {:<50} {:<10} {:<8}",
                    "LOINC", "Name", "Confidence", "Match"
                );
                println!("{}", "-".repeat(80));
                for r in &results {
                    println!(
                        "{:<12} {:<50} {:<10.2} {:<8}",
                        r.loinc_code,
                        truncate(&r.canonical_name, 49),
                        r.confidence,
                        r.match_type,
                    );
                }
            }
        }
    }

    Ok(())
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}
