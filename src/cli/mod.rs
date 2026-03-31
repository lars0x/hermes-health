use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "hermes", about = "Hermes Health - Biomarker tracking for longevity")]
pub struct Cli {
    /// Path to config file
    #[arg(short, long, default_value = "config.toml")]
    pub config: String,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize the database and seed biomarkers
    Init,

    /// Manage biomarkers
    #[command(subcommand)]
    Biomarker(BiomarkerCmd),

    /// Manage observations
    #[command(subcommand)]
    Obs(ObsCmd),

    /// Show trend analysis for a biomarker
    Trend {
        /// Biomarker identifier (LOINC code, name, or alias)
        biomarker: String,

        /// Lookback window in days
        #[arg(short, long, default_value = "365")]
        window: u32,
    },

    /// Export data
    #[command(subcommand)]
    Export(ExportCmd),

    /// Start the web server
    Serve {
        /// Override host
        #[arg(long)]
        host: Option<String>,
        /// Override port
        #[arg(short, long)]
        port: Option<u16>,
    },

    /// Search the LOINC catalog
    Search {
        /// Search query (biomarker name)
        query: String,

        /// Maximum results
        #[arg(short, long, default_value = "5")]
        max: usize,
    },
}

#[derive(Subcommand)]
pub enum BiomarkerCmd {
    /// List all tracked biomarkers
    List {
        /// Filter by category
        #[arg(short = 'C', long)]
        category: Option<String>,
    },
    /// Show biomarker details
    Show {
        /// Biomarker identifier (LOINC code, name, or ID)
        identifier: String,
    },
}

#[derive(Subcommand)]
pub enum ObsCmd {
    /// Add a single observation
    Add {
        /// Biomarker identifier (LOINC code, name, or alias)
        biomarker: String,
        /// Measured value
        value: f64,
        /// Unit of measurement
        unit: String,
        /// Date of observation (YYYY-MM-DD)
        #[arg(short, long)]
        date: String,
        /// Lab name
        #[arg(short, long)]
        lab: Option<String>,
        /// Whether fasting
        #[arg(short, long)]
        fasting: Option<bool>,
        /// Notes
        #[arg(short, long)]
        notes: Option<String>,
    },
    /// List observations
    List {
        /// Filter by biomarker
        #[arg(short, long)]
        biomarker: Option<String>,
        /// From date (YYYY-MM-DD)
        #[arg(long)]
        from: Option<String>,
        /// To date (YYYY-MM-DD)
        #[arg(long)]
        to: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum ExportCmd {
    /// Export observations as CSV
    Csv {
        /// Output file path
        #[arg(short, long, default_value = "hermes_export.csv")]
        output: String,
        /// From date
        #[arg(long)]
        from: Option<String>,
        /// To date
        #[arg(long)]
        to: Option<String>,
    },
}
