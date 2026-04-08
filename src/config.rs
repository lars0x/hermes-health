use serde::Deserialize;
use std::path::{Path, PathBuf};

use crate::error::{HermesError, Result};

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct HermesConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub ollama: OllamaConfig,
    pub extraction: ExtractionConfig,
    pub user: UserConfig,
    pub display: DisplayConfig,
    pub trends: TrendConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DatabaseConfig {
    pub path: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct OllamaConfig {
    pub url: String,
    pub model: String,
    pub temperature: f64,
    pub timeout_seconds: u64,
    pub num_ctx: u32,
    pub num_predict: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ExtractionConfig {
    pub mode: String,
    pub max_agent_turns: u32,
    pub resolve_max_turns: u32,
    pub validation_strictness: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct UserConfig {
    pub date_of_birth: Option<String>,
    pub sex: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DisplayConfig {
    pub date_format: String,
    pub default_trend_window_days: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct TrendConfig {
    pub min_data_points: u32,
    pub rapid_change_threshold_pct: f64,
    pub projection_horizon_days: u32,
}


impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8080,
        }
    }
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            path: PathBuf::from("data/hermes.db"),
        }
    }
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            url: "http://100.95.56.105:11434".to_string(),
            model: "qwen3.5:27b-64k".to_string(),
            temperature: 0.0,
            timeout_seconds: 300,
            num_ctx: 131072,
            num_predict: 8192,
        }
    }
}

impl Default for ExtractionConfig {
    fn default() -> Self {
        Self {
            mode: "agentic".to_string(),
            max_agent_turns: 20,
            resolve_max_turns: 30,
            validation_strictness: "warn".to_string(),
        }
    }
}


impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            date_format: "%Y-%m-%d".to_string(),
            default_trend_window_days: 365,
        }
    }
}

impl Default for TrendConfig {
    fn default() -> Self {
        Self {
            min_data_points: 3,
            rapid_change_threshold_pct: 20.0,
            projection_horizon_days: 180,
        }
    }
}

impl HermesConfig {
    pub fn load(path: Option<&Path>) -> Result<Self> {
        let config_path = path
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("config.toml"));

        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            let config: HermesConfig =
                toml::from_str(&content).map_err(|e| HermesError::Config(e.to_string()))?;
            Ok(config)
        } else {
            Ok(HermesConfig::default())
        }
    }
}
