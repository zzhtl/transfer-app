use anyhow::{Result, anyhow};
use hyper::{Body, Request, Response, StatusCode};
use memmap2::MmapMut;
use multer::parse_boundary;
use std::fs::OpenOptions;
use std::path::PathBuf;

pub async fn handle_upload(req: Request<Body>, target_dir: PathBuf) -> Result<Response<Body>> {
    let uri_path = req.uri().path().to_string();
    let content_type = req
        .headers()
        .get(hyper::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| anyhow!("Missing Content-Type"))?;

    let boundary =
        parse_boundary(content_type).map_err(|e| anyhow!("解析 boundary 失败: {}", e))?;
    let body = req.into_body();
    let mut multipart = multer::Multipart::new(body, boundary);

    while let Some(mut field) = multipart.next_field().await? {
        let filename = match field.file_name() {
            Some(f) => sanitize_filename::sanitize(f),
            None => continue,
        };
        let file_path = target_dir.join(&filename);

        // 使用 spawn_blocking 处理同步文件操作
        let (file, mut cursor) = tokio::task::spawn_blocking({
            let file_path = file_path.clone();
            move || -> Result<(std::fs::File, u64)> {
                let file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true)
                    .open(file_path)?;
                file.set_len(0)?; // 初始文件大小为0
                Ok((file, 0))
            }
        })
        .await??;

        while let Some(chunk) = field.chunk().await? {
            let chunk_len = chunk.len();
            let new_cursor = cursor + chunk_len as u64;

            // 扩展文件并重新映射内存
            let mut mmap = tokio::task::spawn_blocking({
                let file = file.try_clone()?;
                move || {
                    file.set_len(new_cursor)?;
                    unsafe { MmapMut::map_mut(&file) }
                }
            })
            .await??;

            // 将 chunk 写入内存映射
            tokio::task::spawn_blocking({
                let chunk = chunk.clone();
                move || {
                    (&mut mmap[cursor as usize..new_cursor as usize]).copy_from_slice(&chunk);
                    mmap.flush()?;
                    Ok::<_, std::io::Error>(())
                }
            })
            .await??;

            cursor = new_cursor;
        }

        // 最终文件截断到实际大小
        tokio::task::spawn_blocking(move || file.set_len(cursor)).await??;
    }

    Ok(Response::builder()
        .status(StatusCode::SEE_OTHER)
        .header(hyper::header::LOCATION, uri_path)
        .body(Body::empty())?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyper::{header, HeaderMap, StatusCode};
    use std::io::Write;
    use tempfile::tempdir;
    use tokio::fs;

    fn create_test_request(data: &[u8], filename: &str) -> Result<Request<Body>> {
        // 创建测试所需的boundary
        let boundary = "------------------------test_boundary";
        
        // 创建multipart内容
        let mut content = Vec::new();
        
        // 添加多部分表单头部
        write!(
            content,
            "--{}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{}\"\r\nContent-Type: application/octet-stream\r\n\r\n",
            boundary,
            filename
        )?;
        
        // 写入文件内容
        content.extend_from_slice(data);
        
        // 添加多部分表单尾部
        write!(content, "\r\n--{}--\r\n", boundary)?;
        
        // 创建请求头
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={}", boundary).parse()?,
        );
        
        // 构建请求
        let request = Request::builder()
            .uri("/upload")
            .method("POST")
            .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={}", boundary))
            .body(Body::from(content))?;
        
        Ok(request)
    }

    #[tokio::test]
    async fn test_handle_upload() -> Result<()> {
        // 创建临时目录
        let temp_dir = tempdir()?;
        let target_dir = temp_dir.path().to_path_buf();
        
        // 创建测试数据
        let test_data = b"Hello, this is a test file content!";
        let filename = "test_file.txt";
        
        // 创建请求
        let request = create_test_request(test_data, filename)?;
        
        // 调用上传处理函数
        let response = handle_upload(request, target_dir.clone()).await?;
        
        // 验证响应状态
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        
        // 检查文件是否被正确创建
        let uploaded_file_path = target_dir.join(filename);
        let file_exists = fs::try_exists(&uploaded_file_path).await?;
        assert!(file_exists, "上传的文件应该存在");
        
        // 验证文件内容
        let file_content = fs::read(&uploaded_file_path).await?;
        assert_eq!(file_content, test_data, "文件内容应该与上传的数据匹配");
        
        Ok(())
    }
}
