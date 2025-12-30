use anyhow::Result;
use hyper::header::CONTENT_LENGTH;
use hyper::{Body, Request, Response, StatusCode};
use mime_guess::from_path;
use percent_encoding::{percent_encode, NON_ALPHANUMERIC};
use std::cmp::min;
use std::io::SeekFrom;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

use crate::{html, not_found_response};

// 常量定义 - 优化传输性能
const CHUNK_SIZE: usize = 1024 * 1024; // 1MB 数据块，提高大文件传输效率

pub async fn handle_get(req: Request<Body>, dir_path: Arc<PathBuf>) -> Result<Response<Body>> {
    if req.method() != hyper::Method::GET {
        return Ok(Response::builder()
            .status(StatusCode::METHOD_NOT_ALLOWED)
            .body(Body::empty())?);
    }

    // 检查是否是范围请求（支持断点续传和进度条）
    let range_header = req.headers().get(hyper::header::RANGE);

    let request_path = req.uri().path();
    let decoded_path = percent_encoding::percent_decode_str(request_path)
        .decode_utf8_lossy()
        .to_string();
    let full_path = dir_path.join(&decoded_path[1..]); // 去掉前导斜杠

    // 安全验证
    let canonical_path = match tokio::fs::canonicalize(&full_path).await {
        Ok(p) => p,
        Err(_) => return Ok(not_found_response()),
    };

    if !canonical_path.starts_with(&*dir_path) {
        return Ok(Response::builder()
            .status(StatusCode::FORBIDDEN)
            .body(Body::empty())?);
    }

    // 处理目录请求
    if tokio::fs::metadata(&canonical_path).await?.is_dir() {
        return html::handle_directory(&canonical_path, &decoded_path).await;
    }

    // 处理文件请求
    match tokio::fs::metadata(&canonical_path).await {
        Ok(metadata) => {
            let file_size = metadata.len();
            let mime_type = from_path(&canonical_path).first_or_octet_stream();
            let original_filename = canonical_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("file");

            // 对文件名进行编码
            let encoded_filename =
                percent_encode(original_filename.as_bytes(), NON_ALPHANUMERIC);
            let content_disposition = format!(
                "attachment; filename=\"{}\"; filename*=UTF-8''{}",
                original_filename, encoded_filename
            );

            let response_builder = Response::builder()
                .header("Content-Type", mime_type.as_ref())
                .header("Content-Disposition", content_disposition)
                .header("Accept-Ranges", "bytes") // 支持断点续传
                .header("X-File-Size", file_size.to_string()); // 自定义头，用于前端显示进度

            // 处理范围请求（断点续传/进度下载）
            if let Some(range) = range_header {
                return handle_range_request(
                    range,
                    canonical_path,
                    file_size,
                    response_builder,
                    mime_type.as_ref(),
                )
                .await;
            }

            // 完整文件下载 - 使用流式传输
            handle_full_download(canonical_path, file_size, response_builder).await
        }
        Err(_) => Ok(not_found_response()),
    }
}

/// 处理完整文件下载
async fn handle_full_download(
    path: PathBuf,
    file_size: u64,
    response_builder: hyper::http::response::Builder,
) -> Result<Response<Body>> {
    let file = File::open(&path).await?;

    // 使用大缓冲区流式传输
    let stream = ReaderStream::with_capacity(file, CHUNK_SIZE);
    let body = Body::wrap_stream(stream);

    Ok(response_builder
        .header(CONTENT_LENGTH, file_size.to_string())
        .body(body)?)
}

/// 处理范围请求(断点续传/分块下载)
async fn handle_range_request(
    range: &hyper::header::HeaderValue,
    path: PathBuf,
    file_size: u64,
    response_builder: hyper::http::response::Builder,
    content_type: &str,
) -> Result<Response<Body>> {
    let range_str = range.to_str()?;

    if !range_str.starts_with("bytes=") {
        return Ok(response_builder
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from("Invalid Range header"))?);
    }

    let ranges_str = &range_str["bytes=".len()..];
    let range_pair = ranges_str
        .split_once('-')
        .ok_or_else(|| anyhow::anyhow!("Invalid range format"))?;

    let start = if range_pair.0.is_empty() {
        0
    } else {
        range_pair.0.parse::<u64>()?
    };

    let end = if range_pair.1.is_empty() {
        file_size - 1
    } else {
        min(range_pair.1.parse::<u64>()?, file_size - 1)
    };

    if start > end || start >= file_size {
        return Ok(response_builder
            .status(StatusCode::RANGE_NOT_SATISFIABLE)
            .header("Content-Range", format!("bytes */{}", file_size))
            .body(Body::empty())?);
    }

    let content_length = end - start + 1;

    let mut file = File::open(&path).await?;
    file.seek(SeekFrom::Start(start)).await?;

    // 对于小范围（< 4MB），直接读取到内存
    if content_length < 4 * 1024 * 1024 {
        let mut buffer = vec![0; content_length as usize];
        file.read_exact(&mut buffer).await?;

        return Ok(response_builder
            .status(StatusCode::PARTIAL_CONTENT)
            .header(
                "Content-Range",
                format!("bytes {}-{}/{}", start, end, file_size),
            )
            .header("Content-Length", content_length.to_string())
            .header("Content-Type", content_type)
            .body(Body::from(buffer))?);
    }

    // 对于大范围，使用流式传输
    let limited_reader = file.take(content_length);
    let stream = ReaderStream::with_capacity(limited_reader, CHUNK_SIZE);

    Ok(response_builder
        .status(StatusCode::PARTIAL_CONTENT)
        .header(
            "Content-Range",
            format!("bytes {}-{}/{}", start, end, file_size),
        )
        .header("Content-Length", content_length.to_string())
        .header("Content-Type", content_type)
        .body(Body::wrap_stream(stream))?)
}
