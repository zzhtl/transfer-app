use anyhow::Result;
use hyper::header::{CONTENT_LENGTH, TRANSFER_ENCODING};
use hyper::{Body, Request, Response, StatusCode};
use memmap2::Mmap;
use mime_guess::from_path;
use percent_encoding::{NON_ALPHANUMERIC, percent_encode};
use std::cmp::min;
use std::fs::File as StdFile;
use std::io::SeekFrom;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::codec::{BytesCodec, FramedRead};
use tokio_util::io::ReaderStream;

use crate::{html, not_found_response};

// 常量定义
const CHUNK_SIZE: usize = 256 * 1024; // 256KB 的数据块大小
const BUFFER_CAPACITY: usize = 1024 * 1024; // 1MB 的缓冲区大小
const SMALL_FILE_THRESHOLD: u64 = 32 * 1024 * 1024; // 32MB 以下考虑使用内存映射

pub async fn handle_get(req: Request<Body>, dir_path: Arc<PathBuf>) -> Result<Response<Body>> {
    if req.method() != hyper::Method::GET {
        return Ok(Response::builder()
            .status(StatusCode::METHOD_NOT_ALLOWED)
            .body(Body::empty())?);
    }

    // 检查客户端是否接受gzip编码
    let supports_gzip = req
        .headers()
        .get("Accept-Encoding")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.contains("gzip"))
        .unwrap_or(false);

    // 检查是否是范围请求（支持断点续传）
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
            let encoded_filename = percent_encode(original_filename.as_bytes(), NON_ALPHANUMERIC);
            let content_disposition = format!(
                "attachment; filename=\"{}\"; filename*=UTF-8''{}",
                original_filename,
                encoded_filename
            );

            let mut response_builder = Response::builder()
                .header("Content-Type", mime_type.as_ref())
                .header("Content-Disposition", content_disposition)
                .header("Accept-Ranges", "bytes"); // 支持断点续传

            // 处理范围请求（断点续传）
            if let Some(range) = range_header {
                return handle_range_request(range, canonical_path, file_size, response_builder, mime_type.as_ref()).await;
            }

            // 非范围请求处理
            if file_size < SMALL_FILE_THRESHOLD && !supports_gzip {
                // 小文件使用内存映射加速传输
                return handle_mmap_download(canonical_path, file_size, response_builder).await;
            }

            // 打开文件
            match File::open(&canonical_path).await {
                Ok(file) => {
                    if supports_gzip {
                        // 使用Gzip压缩 - 优化压缩级别
                        let buf_reader = tokio::io::BufReader::with_capacity(BUFFER_CAPACITY, file);
                        let encoder = async_compression::tokio::bufread::GzipEncoder::with_quality(
                            buf_reader,
                            async_compression::Level::Fastest, // 使用最快的压缩级别，牺牲一点压缩率换取速度
                        );
                        
                        // 使用高效的分块读取
                        response_builder = response_builder
                            .header(TRANSFER_ENCODING, "chunked")
                            .header("Content-Encoding", "gzip")
                            .header("Vary", "Accept-Encoding");
                            
                        let stream = tokio_util::io::ReaderStream::with_capacity(encoder, CHUNK_SIZE);
                        let body = Body::wrap_stream(stream);
                        
                        Ok(response_builder.body(body)?)
                    } else {
                        // 不压缩情况 - 使用更大的块大小提高传输效率
                        response_builder = response_builder
                            .header(CONTENT_LENGTH, file_size.to_string());
                        
                        let stream = FramedRead::with_capacity(file, BytesCodec::new(), CHUNK_SIZE);
                        let body = Body::wrap_stream(stream);
                        
                        Ok(response_builder.body(body)?)
                    }
                }
                Err(_) => Ok(not_found_response()),
            }
        }
        Err(_) => Ok(not_found_response()),
    }
}

// 使用内存映射技术处理小文件下载，提高性能
async fn handle_mmap_download(path: PathBuf, file_size: u64, response_builder: hyper::http::response::Builder) -> Result<Response<Body>> {
    // 使用tokio的阻塞线程池处理内存映射操作
    let mmap = tokio::task::spawn_blocking(move || -> Result<Mmap> {
        let file = StdFile::open(path)?;
        let mmap = unsafe { memmap2::Mmap::map(&file)? };
        Ok(mmap)
    }).await??;
    
    // 设置响应头
    let response = response_builder
        .header(CONTENT_LENGTH, file_size.to_string())
        .body(Body::from(mmap[..].to_vec()))?;
    
    Ok(response)
}

// 处理范围请求(断点续传)
async fn handle_range_request(
    range: &hyper::header::HeaderValue, 
    path: PathBuf, 
    file_size: u64,
    response_builder: hyper::http::response::Builder,
    content_type: &str
) -> Result<Response<Body>> {
    let range_str = range.to_str()?;
    
    if !range_str.starts_with("bytes=") {
        return Ok(response_builder
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from("Invalid Range header"))?);
    }
    
    let ranges_str = &range_str["bytes=".len()..];
    let range_pair = ranges_str.split_once('-')
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
    
    // 对于小范围，直接读取到内存
    if content_length < 4 * 1024 * 1024 { // 4MB
        let mut buffer = vec![0; content_length as usize];
        file.read_exact(&mut buffer).await?;
        
        return Ok(response_builder
            .status(StatusCode::PARTIAL_CONTENT)
            .header("Content-Range", format!("bytes {}-{}/{}", start, end, file_size))
            .header("Content-Length", content_length.to_string())
            .header("Content-Type", content_type)
            .body(Body::from(buffer))?);
    }
    
    // 对于大范围，使用流式传输
    let limited_reader = file.take(content_length);
    let stream = ReaderStream::with_capacity(limited_reader, CHUNK_SIZE);
    
    Ok(response_builder
        .status(StatusCode::PARTIAL_CONTENT)
        .header("Content-Range", format!("bytes {}-{}/{}", start, end, file_size))
        .header("Content-Length", content_length.to_string())
        .header("Content-Type", content_type)
        .body(Body::wrap_stream(stream))?)
}