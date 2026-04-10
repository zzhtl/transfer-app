use std::io::SeekFrom;

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::header::*;
use axum::http::{HeaderMap, Response, StatusCode};
use serde::Deserialize;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

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

    if abs.is_dir() {
        return Err(AppError::IsADirectory);
    }

    let meta = tokio::fs::metadata(&abs).await?;
    let size = meta.len();
    let etag_val = etag::compute_etag(&meta);
    let mime_type = guess_mime(&abs);

    // 304 Not Modified
    if let Some(inm) = headers.get(IF_NONE_MATCH) {
        if etag::matches_etag(inm.to_str().ok(), &etag_val) {
            return Ok(Response::builder()
                .status(StatusCode::NOT_MODIFIED)
                .body(Body::empty())
                .unwrap());
        }
    }

    let range_result = range::parse_range(headers.get(RANGE), size);

    let (status, start, end) = match range_result {
        None => (StatusCode::OK, 0, size.saturating_sub(1)),
        Some((s, e)) => (StatusCode::PARTIAL_CONTENT, s, e),
    };

    // Range 无效 -> 416
    if headers.get(RANGE).is_some() && range_result.is_none() && size > 0 {
        return Ok(Response::builder()
            .status(StatusCode::RANGE_NOT_SATISFIABLE)
            .header(CONTENT_RANGE, format!("bytes */{}", size))
            .body(Body::empty())
            .unwrap());
    }

    let length = if size == 0 { 0 } else { end - start + 1 };

    // 完全流式，不缓存到内存
    let mut file = tokio::fs::File::open(&abs).await?;
    if start > 0 {
        file.seek(SeekFrom::Start(start)).await?;
    }
    let limited = file.take(length);
    let stream = ReaderStream::with_capacity(limited, 256 * 1024); // 256KB
    let body = Body::from_stream(stream);

    // Content-Disposition
    let filename = abs
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let is_download = params.download.is_some();
    let disposition = if is_download {
        format!("attachment; filename=\"{}\"", filename)
    } else {
        format!("inline; filename=\"{}\"", filename)
    };

    let last_modified = meta
        .modified()
        .ok()
        .and_then(httpdate_format);

    let mut builder = Response::builder()
        .status(status)
        .header(CONTENT_TYPE, &mime_type)
        .header(CONTENT_LENGTH, length)
        .header(ACCEPT_RANGES, "bytes")
        .header(ETAG, &etag_val)
        .header(CACHE_CONTROL, "public, max-age=0, must-revalidate")
        .header(CONTENT_DISPOSITION, &disposition)
        .header("X-File-Size", size.to_string());

    if let Some(lm) = &last_modified {
        builder = builder.header(LAST_MODIFIED, lm);
    }

    if status == StatusCode::PARTIAL_CONTENT {
        builder = builder.header(
            CONTENT_RANGE,
            format!("bytes {}-{}/{}", start, end, size),
        );
    }

    Ok(builder.body(body).unwrap())
}

fn httpdate_format(time: std::time::SystemTime) -> Option<String> {
    let duration = time.duration_since(std::time::UNIX_EPOCH).ok()?;
    let secs = duration.as_secs();
    // 简单的 HTTP date 格式
    Some(format!("{}", secs))
}
