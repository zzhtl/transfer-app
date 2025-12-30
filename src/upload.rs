use anyhow::{anyhow, Result};
use hyper::{Body, Request, Response, StatusCode};
use multer::parse_boundary;
use serde::Serialize;
use std::path::PathBuf;
use tokio::fs::OpenOptions;
use tokio::io::{AsyncSeekExt, AsyncWriteExt, BufWriter};

// 缓冲区大小: 4MB - 适合大文件传输
const WRITE_BUFFER_SIZE: usize = 4 * 1024 * 1024;

#[derive(Debug, Serialize)]
pub struct UploadResponse {
    pub success: bool,
    pub message: String,
    pub filename: Option<String>,
    pub size: Option<u64>,
}

/// 处理文件上传（支持普通上传和分块上传）
pub async fn handle_upload(req: Request<Body>, target_dir: PathBuf) -> Result<Response<Body>> {
    let uri_path = req.uri().path().to_string();
    
    // 检查是否是分块上传
    let is_chunk_upload = req.headers()
        .get("X-Chunk-Upload")
        .and_then(|v| v.to_str().ok())
        .map(|v| v == "true")
        .unwrap_or(false);

    if is_chunk_upload {
        return handle_chunk_upload(req, target_dir).await;
    }

    // 普通上传（用于小文件或单文件）
    handle_normal_upload(req, target_dir, &uri_path).await
}

/// 处理普通文件上传（优化版）
async fn handle_normal_upload(
    req: Request<Body>,
    target_dir: PathBuf,
    _uri_path: &str,
) -> Result<Response<Body>> {
    let content_type = req
        .headers()
        .get(hyper::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| anyhow!("Missing Content-Type"))?;

    let boundary = parse_boundary(content_type)
        .map_err(|e| anyhow!("解析 boundary 失败: {}", e))?;
    
    let body = req.into_body();
    let mut multipart = multer::Multipart::new(body, boundary);

    let mut uploaded_files = Vec::new();

    while let Some(mut field) = multipart.next_field().await? {
        let filename = match field.file_name() {
            Some(f) => sanitize_filename::sanitize(f),
            None => continue,
        };
        
        let file_path = target_dir.join(&filename);

        // 创建文件并使用带缓冲的写入器
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&file_path)
            .await?;

        let mut writer = BufWriter::with_capacity(WRITE_BUFFER_SIZE, file);
        let mut total_size: u64 = 0;

        // 流式写入数据
        while let Some(chunk) = field.chunk().await? {
            writer.write_all(&chunk).await?;
            total_size += chunk.len() as u64;
        }

        // 确保所有数据写入磁盘
        writer.flush().await?;
        writer.into_inner().sync_all().await?;

        uploaded_files.push(UploadResponse {
            success: true,
            message: "上传成功".to_string(),
            filename: Some(filename),
            size: Some(total_size),
        });
    }

    // 返回JSON响应
    let response_body = serde_json::to_string(&uploaded_files)?;
    
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Body::from(response_body))?)
}

/// 处理分块上传（用于大文件）
async fn handle_chunk_upload(req: Request<Body>, target_dir: PathBuf) -> Result<Response<Body>> {
    // 从请求头获取分块信息
    let chunk_index: usize = req.headers()
        .get("X-Chunk-Index")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    let total_chunks: usize = req.headers()
        .get("X-Total-Chunks")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);

    let file_id = req.headers()
        .get("X-File-Id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    let filename = req.headers()
        .get("X-Filename")
        .and_then(|v| v.to_str().ok())
        .map(|s| percent_encoding::percent_decode_str(s).decode_utf8_lossy().to_string())
        .map(|f| sanitize_filename::sanitize(&f))
        .unwrap_or_else(|| format!("file_{}", file_id));

    let total_size: u64 = req.headers()
        .get("X-Total-Size")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    let chunk_start: u64 = req.headers()
        .get("X-Chunk-Start")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    // 临时文件路径（用于分块上传）
    let temp_path = target_dir.join(format!(".{}.tmp", file_id));
    let final_path = target_dir.join(&filename);

    // 打开或创建临时文件
    let file = if chunk_index == 0 {
        // 第一个分块，创建新文件
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temp_path)
            .await?;
        
        // 预分配文件大小以优化写入性能
        if total_size > 0 {
            file.set_len(total_size).await?;
        }
        file
    } else {
        // 后续分块，打开已存在的文件
        OpenOptions::new()
            .write(true)
            .open(&temp_path)
            .await?
    };

    // 移动到正确的位置
    let mut file = file;
    file.seek(std::io::SeekFrom::Start(chunk_start)).await?;

    // 读取请求体并写入
    let body_bytes = hyper::body::to_bytes(req.into_body()).await?;
    file.write_all(&body_bytes).await?;
    file.sync_all().await?;

    // 如果是最后一个分块，重命名文件
    if chunk_index == total_chunks - 1 {
        drop(file); // 关闭文件句柄
        
        // 如果目标文件已存在，先删除
        if final_path.exists() {
            tokio::fs::remove_file(&final_path).await?;
        }
        
        tokio::fs::rename(&temp_path, &final_path).await?;
    }

    let response = UploadResponse {
        success: true,
        message: if chunk_index == total_chunks - 1 {
            "文件上传完成".to_string()
        } else {
            format!("分块 {}/{} 上传成功", chunk_index + 1, total_chunks)
        },
        filename: Some(filename),
        size: Some(body_bytes.len() as u64),
    };

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&response)?))?)
}

/// 清理未完成的临时文件
pub async fn cleanup_temp_files(dir: &PathBuf) -> Result<()> {
    let mut entries = tokio::fs::read_dir(dir).await?;
    
    while let Some(entry) = entries.next_entry().await? {
        let filename = entry.file_name();
        let filename_str = filename.to_string_lossy();
        
        if filename_str.starts_with('.') && filename_str.ends_with(".tmp") {
            let _ = tokio::fs::remove_file(entry.path()).await;
        }
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyper::header;
    use std::io::Write;
    use tempfile::tempdir;
    use tokio::fs;

    fn create_test_request(data: &[u8], filename: &str) -> Result<Request<Body>> {
        let boundary = "------------------------test_boundary";
        let mut content = Vec::new();
        
        write!(
            content,
            "--{}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{}\"\r\nContent-Type: application/octet-stream\r\n\r\n",
            boundary,
            filename
        )?;
        
        content.extend_from_slice(data);
        write!(content, "\r\n--{}--\r\n", boundary)?;
        
        let request = Request::builder()
            .uri("/upload")
            .method("POST")
            .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={}", boundary))
            .body(Body::from(content))?;
        
        Ok(request)
    }

    #[tokio::test]
    async fn test_handle_upload() -> Result<()> {
        let temp_dir = tempdir()?;
        let target_dir = temp_dir.path().to_path_buf();
        
        let test_data = b"Hello, this is a test file content!";
        let filename = "test_file.txt";
        
        let request = create_test_request(test_data, filename)?;
        let response = handle_upload(request, target_dir.clone()).await?;
        
        assert_eq!(response.status(), StatusCode::OK);
        
        let uploaded_file_path = target_dir.join(filename);
        let file_exists = fs::try_exists(&uploaded_file_path).await?;
        assert!(file_exists, "上传的文件应该存在");
        
        let file_content = fs::read(&uploaded_file_path).await?;
        assert_eq!(file_content, test_data, "文件内容应该与上传的数据匹配");
        
        Ok(())
    }
}
