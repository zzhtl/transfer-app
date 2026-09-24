use std::io::SeekFrom;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::header::*;
use axum::http::{HeaderMap, HeaderValue, Response, StatusCode};
use serde::Deserialize;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

use crate::download::disposition::content_disposition;
use crate::download::{etag, range};
use crate::error::AppError;
use crate::state::AppState;
use crate::util::mime::guess_mime;

#[derive(Deserialize, Default)]
pub struct DownloadParams {
    #[serde(default)]
    pub download: Option<String>,
}

/// GET /api/download/{*path} — 文件下载 + Range + ETag
pub async fn get(
    State(state): State<AppState>,
    Path(rel): Path<String>,
    Query(params): Query<DownloadParams>,
    headers: HeaderMap,
) -> Result<Response<Body>, AppError> {
    let abs = state.path_safety.resolve(&rel)?;
    serve_file(&abs, params.download.is_some(), &headers).await
}

/// 流式服务单个文件（Range/ETag/304/Content-Disposition）。
/// 供受保护下载与公开分享下载共用；调用方负责路径安全校验。
pub async fn serve_file(
    abs: &std::path::Path,
    is_download: bool,
    headers: &HeaderMap,
) -> Result<Response<Body>, AppError> {
    if abs.is_dir() {
        return Err(AppError::IsADirectory);
    }

    let meta = tokio::fs::metadata(abs).await?;
    let size = meta.len();
    let etag_val = etag::compute_etag(&meta);
    let mime_type = guess_mime(abs);
    let modified = meta.modified().ok();

    // 304 Not Modified
    let if_none_match = headers.get(IF_NONE_MATCH).and_then(|v| v.to_str().ok());
    if etag::matches_etag(if_none_match, &etag_val) {
        return Ok(Response::builder()
            .status(StatusCode::NOT_MODIFIED)
            .header(ETAG, &etag_val)
            .body(Body::empty())
            .unwrap());
    }

    // If-Range 与当前版本不符说明文件在两次请求之间变了，照 Range 续传会把新旧内容
    // 拼在一起。按 RFC 9110 忽略 Range，回完整的 200。
    let range_header = headers
        .get(RANGE)
        .filter(|_| if_range_matches(headers.get(IF_RANGE), &etag_val, modified));
    let range_result = range::parse_range(range_header, size);

    let (status, start, end) = match range_result {
        None => (StatusCode::OK, 0, size.saturating_sub(1)),
        Some((s, e)) => (StatusCode::PARTIAL_CONTENT, s, e),
    };

    // Range 无效 -> 416
    if range_header.is_some() && range_result.is_none() && size > 0 {
        return Ok(Response::builder()
            .status(StatusCode::RANGE_NOT_SATISFIABLE)
            .header(CONTENT_RANGE, format!("bytes */{}", size))
            .body(Body::empty())
            .unwrap());
    }

    let length = if size == 0 { 0 } else { end - start + 1 };

    // 完全流式，不缓存到内存
    let mut file = tokio::fs::File::open(abs).await?;
    if start > 0 {
        file.seek(SeekFrom::Start(start)).await?;
    }
    let limited = file.take(length);
    let stream = ReaderStream::with_capacity(limited, 256 * 1024); // 256KB
    let body = Body::from_stream(stream);

    let filename = abs.file_name().unwrap_or_default().to_string_lossy();

    let mut builder = Response::builder()
        .status(status)
        .header(CONTENT_TYPE, &mime_type)
        .header(CONTENT_LENGTH, length)
        .header(ACCEPT_RANGES, "bytes")
        .header(ETAG, &etag_val)
        .header(CACHE_CONTROL, "public, max-age=0, must-revalidate")
        .header(
            CONTENT_DISPOSITION,
            content_disposition(!is_download, &filename),
        )
        // 用户上传的文件，不让浏览器按内容去猜类型
        .header(X_CONTENT_TYPE_OPTIONS, "nosniff")
        .header("X-File-Size", size.to_string());

    // inline 打开用户上传的 html/svg/xml 时，里面的脚本会以本站的源执行、能直接调
    // 本站接口。sandbox 让这个文档变成不透明源；<img> 引用 svg 做预览不受影响。
    if !is_download && is_active_content(&mime_type) {
        builder = builder.header(CONTENT_SECURITY_POLICY, "sandbox");
    }

    if let Some(lm) = modified {
        builder = builder.header(LAST_MODIFIED, httpdate::fmt_http_date(lm));
    }

    if status == StatusCode::PARTIAL_CONTENT {
        builder = builder.header(
            CONTENT_RANGE,
            format!("bytes {}-{}/{}", start, end, size),
        );
    }

    Ok(builder.body(body).unwrap())
}

/// If-Range 是否与当前版本一致；没带这个头视为一致。
///
/// 实体标签只认强比较；日期按 HTTP-date 的秒级精度和文件修改时间比较。
fn if_range_matches(
    if_range: Option<&HeaderValue>,
    etag: &str,
    modified: Option<SystemTime>,
) -> bool {
    let Some(value) = if_range else {
        return true;
    };
    let Ok(value) = value.to_str().map(str::trim) else {
        return false;
    };
    if value.starts_with('"') {
        return value == etag;
    }
    if value.starts_with("W/") {
        return false;
    }
    let secs = |t: SystemTime| t.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).ok();
    match (httpdate::parse_http_date(value), modified) {
        (Ok(date), Some(mtime)) => secs(date).is_some() && secs(date) == secs(mtime),
        _ => false,
    }
}

/// 以 inline 方式返回时会被浏览器当作文档执行脚本的类型
fn is_active_content(mime: &str) -> bool {
    matches!(
        mime,
        "text/html" | "application/xhtml+xml" | "image/svg+xml" | "text/xml" | "application/xml"
    )
}
