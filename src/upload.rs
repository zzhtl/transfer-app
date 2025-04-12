use anyhow::{Result, anyhow};
use hyper::{Body, Request, Response, StatusCode};
use memmap2::MmapMut;
use multer::parse_boundary;
use std::path::PathBuf;

pub async fn handle_upload(req: Request<Body>, target_dir: PathBuf) -> Result<Response<Body>> {
    let uri_path = req.uri().path().to_string();
    let content_type = req
        .headers()
        .get(hyper::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| anyhow!("缺少 Content-Type 头"))?;

    // 解析 multipart 表单的 boundary
    let boundary =
        parse_boundary(content_type).map_err(|e| anyhow!("解析 boundary 失败: {}", e))?;

    let body = req.into_body();
    let mut multipart = multer::Multipart::new(body, boundary);

    // 处理每个上传字段
    while let Some(mut field) = multipart.next_field().await? {
        // 获取并清理文件名
        let filename = match field.file_name() {
            Some(f) => sanitize_filename::sanitize(&f.to_string()),
            None => continue, // 如果没有文件名则跳过
        };

        let file_path = target_dir.join(&filename);

        // 创建目标文件并保留原始文件句柄
        let file = tokio::fs::File::create(&file_path).await?;

        // 初始内存映射 - 需要先克隆文件句柄
        let file_for_mmap = file.try_clone().await?;
        let mut mmap = tokio::task::spawn_blocking(move || unsafe {
            MmapMut::map_mut(&file_for_mmap).map_err(|e| anyhow!("内存映射失败: {}", e))
        })
        .await??;

        let mut offset = 0; // 当前写入位置

        // 处理每个数据块
        while let Some(chunk) = field.chunk().await? {
            let chunk_len = chunk.len();

            // 如果需要扩展内存映射区域
            if offset + chunk_len > mmap.len() {
                // 首先释放当前内存映射
                drop(mmap);

                // 调整文件大小
                file.set_len((offset + chunk_len) as u64).await?;

                // 为新的内存映射克隆文件句柄
                let file_for_mmap = file.try_clone().await?;
                mmap = tokio::task::spawn_blocking(move || unsafe {
                    MmapMut::map_mut(&file_for_mmap)
                        .map_err(|e| anyhow!("内存映射重新创建失败: {}", e))
                })
                .await??;
            }

            // 将数据块写入内存映射区域
            mmap[offset..offset + chunk_len].copy_from_slice(&chunk);
            offset += chunk_len;
        }

        // 最终调整文件大小
        file.set_len(offset as u64).await?;

        // 刷新内存映射到磁盘
        tokio::task::spawn_blocking(move || {
            mmap.flush().map_err(|e| anyhow!("内存映射刷新失败: {}", e))
        })
        .await??;
    }

    // 返回 303 重定向响应
    Ok(Response::builder()
        .status(StatusCode::SEE_OTHER)
        .header(hyper::header::LOCATION, uri_path)
        .body(Body::empty())?)
}
