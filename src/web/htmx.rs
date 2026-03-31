use axum::http::HeaderMap;

/// Check if the request is an HTMX request by looking for the HX-Request header.
pub fn is_htmx_request(headers: &HeaderMap) -> bool {
    headers
        .get("HX-Request")
        .and_then(|v| v.to_str().ok())
        .map(|s| s == "true")
        .unwrap_or(false)
}
