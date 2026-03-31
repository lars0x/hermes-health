use thiserror::Error;

#[derive(Debug, Error)]
pub enum HermesError {
    #[error("not found: {0}")]
    NotFound(String),

    #[error("validation error: {0}")]
    Validation(String),

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("unit conversion error: {0}")]
    Conversion(String),

    #[error("configuration error: {0}")]
    Config(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("CSV error: {0}")]
    Csv(#[from] csv::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("PDF extraction error: {0}")]
    Pdf(String),

    #[error("agent error: {0}")]
    Agent(String),

    #[error("duplicate: {0}")]
    Duplicate(String),
}

pub type Result<T> = std::result::Result<T, HermesError>;

impl axum::response::IntoResponse for HermesError {
    fn into_response(self) -> axum::response::Response {
        let status = match &self {
            HermesError::NotFound(_) => axum::http::StatusCode::NOT_FOUND,
            HermesError::Validation(_) => axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            HermesError::Conversion(_) => axum::http::StatusCode::BAD_REQUEST,
            _ => axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        };
        let body = format!(
            r#"<html><body style="font-family: sans-serif; padding: 2rem;">
            <h2>Error {}</h2><p>{}</p>
            <a href="/">Back to dashboard</a>
            </body></html>"#,
            status.as_u16(),
            self
        );
        (status, axum::response::Html(body)).into_response()
    }
}
