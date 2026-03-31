use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "src/web/static/"]
pub struct StaticAssets;

pub async fn static_file(
    axum::extract::Path(path): axum::extract::Path<String>,
) -> Response {
    match StaticAssets::get(&path) {
        Some(file) => {
            let mime = mime_guess::from_path(&path).first_or_octet_stream();
            let mut response = (StatusCode::OK, file.data.to_vec()).into_response();
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                mime.as_ref().parse().unwrap(),
            );
            // No caching in debug mode so changes are picked up immediately
            response.headers_mut().insert(
                header::CACHE_CONTROL,
                "no-cache, must-revalidate".parse().unwrap(),
            );
            response
        }
        None => (StatusCode::NOT_FOUND, "Not found").into_response(),
    }
}
