use axum::body::Body;
use axum::extract::Path;
use axum::http::header::*;
use axum::http::{Response, StatusCode};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "static/"]
struct StaticAssets;

/// GET / — SPA 入口
pub async fn index() -> Response<Body> {
    serve_embedded("index.html")
}

/// GET /static/{*path} — 静态资源
pub async fn serve(Path(path): Path<String>) -> Response<Body> {
    serve_embedded(&path)
}

fn serve_embedded(path: &str) -> Response<Body> {
    match StaticAssets::get(path) {
        Some(asset) => {
            let mime = mime_guess::from_path(path)
                .first_or_octet_stream()
                .to_string();

            Response::builder()
                .status(StatusCode::OK)
                .header(CONTENT_TYPE, mime)
                .header(CACHE_CONTROL, "public, max-age=3600")
                .body(Body::from(asset.data.to_vec()))
                .unwrap()
        }
        None => {
            // SPA fallback: 返回 index.html
            if let Some(index) = StaticAssets::get("index.html") {
                Response::builder()
                    .status(StatusCode::OK)
                    .header(CONTENT_TYPE, "text/html; charset=utf-8")
                    .body(Body::from(index.data.to_vec()))
                    .unwrap()
            } else {
                Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Body::from("not found"))
                    .unwrap()
            }
        }
    }
}
