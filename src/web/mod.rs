pub mod assets;
pub mod extraction_queue;
pub mod handlers;
pub mod htmx;
pub mod routes;
pub mod templates;

use std::sync::Arc;

use sqlx::SqlitePool;

use crate::config::HermesConfig;
use crate::services::loinc::LoincCatalog;
use templates::TemplateEngine;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub catalog: Arc<LoincCatalog>,
    pub config: Arc<HermesConfig>,
    pub templates: TemplateEngine,
    pub extraction_queue: extraction_queue::ExtractionSender,
}
