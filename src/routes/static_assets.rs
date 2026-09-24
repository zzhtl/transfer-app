use std::borrow::Cow;

use axum::body::{Body, Bytes};
use axum::extract::Path;
use axum::http::header::*;
use axum::http::{HeaderMap, Response, StatusCode};
use rust_embed::Embed;

use crate::download::etag::matches_etag;

#[derive(Embed)]
#[folder = "static/"]
struct StaticAssets;

/// GET / 及前端路由的 SPA 兜底 — 返回 index.html
pub async fn index(headers: HeaderMap) -> Response<Body> {
    serve_embedded("index.html", &headers).unwrap_or_else(not_found)
}

/// GET /static/{*path} — 静态资源
///
/// 找不到就是 404，不回落到 index.html：否则缺失的模块会拿到一段 HTML，
/// 浏览器只报一个费解的 MIME 类型错误。
pub async fn serve(Path(path): Path<String>, headers: HeaderMap) -> Response<Body> {
    serve_embedded(&path, &headers).unwrap_or_else(not_found)
}

fn serve_embedded(path: &str, headers: &HeaderMap) -> Option<Response<Body>> {
    let asset = StaticAssets::get(path)?;

    // URL 里没有内容哈希，只能靠 ETag 协商：no-cache 表示每次都回源校验，没变就是
    // 一个空的 304。之前的 max-age=3600 会让升级后的一小时里新旧 ES module 混着用。
    let etag = format!("\"{}\"", hex::encode(&asset.metadata.sha256_hash()[..16]));
    if matches_etag(
        headers.get(IF_NONE_MATCH).and_then(|v| v.to_str().ok()),
        &etag,
    ) {
        return Some(
            Response::builder()
                .status(StatusCode::NOT_MODIFIED)
                .header(ETAG, etag)
                .header(CACHE_CONTROL, "no-cache")
                .body(Body::empty())
                .unwrap(),
        );
    }

    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let body = match asset.data {
        Cow::Borrowed(bytes) => Body::from(Bytes::from_static(bytes)),
        Cow::Owned(bytes) => Body::from(bytes),
    };
    Some(
        Response::builder()
            .status(StatusCode::OK)
            .header(CONTENT_TYPE, mime.as_ref())
            .header(ETAG, etag)
            .header(CACHE_CONTROL, "no-cache")
            .body(body)
            .unwrap(),
    )
}

fn not_found() -> Response<Body> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::from("not found"))
        .unwrap()
}
