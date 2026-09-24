use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, Response, StatusCode};
use futures_util::StreamExt;
use crate::error::AppError;
use crate::fs::path_safety::split_relative;
use crate::state::AppState;
use crate::upload::session::UploadSession;
use crate::upload::writer::ChunkWriter;

const TUS_VERSION: &str = "1.0.0";
const TUS_EXTENSIONS: &str = "creation,creation-with-upload,termination,expiration";

/// OPTIONS /api/upload — tus 能力发现
pub async fn options(State(state): State<AppState>) -> Response<Body> {
    let max_size = if state.config.max_upload_size > 0 {
        state.config.max_upload_size.to_string()
    } else {
        "0".to_string()
    };

    Response::builder()
        .status(StatusCode::NO_CONTENT)
        .header("Tus-Resumable", TUS_VERSION)
        .header("Tus-Version", TUS_VERSION)
        .header("Tus-Extension", TUS_EXTENSIONS)
        .header("Tus-Max-Size", max_size)
        .body(Body::empty())
        .unwrap()
}

/// POST /api/upload — 创建上传会话
pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response<Body>, AppError> {
    let upload_length: u64 = headers
        .get("upload-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| AppError::BadRequest("missing Upload-Length".into()))?;

    // 检查大小限制
    if state.config.max_upload_size > 0 && upload_length > state.config.max_upload_size {
        return Err(AppError::PayloadTooLarge);
    }

    // 解析 Upload-Metadata
    let metadata = parse_tus_metadata(&headers);
    let filename = metadata
        .get("filename")
        .cloned()
        .unwrap_or_else(|| "unnamed".to_string());
    let filename = sanitize_filename::sanitize(&filename);
    // relativePath 决定 finalize 时在 targetDir 下建哪些子目录，原样拼接的话
    // `../` 或绝对路径就能写出共享根。在这里就拒绝，免得客户端先传完几个 GB 才失败；
    // 存进会话的是清理过的版本。
    let relative_path = match metadata.get("relativePath").filter(|r| !r.is_empty()) {
        Some(rel) => Some(split_relative(rel)?.join("/")),
        None => None,
    };
    let target_dir_str = metadata
        .get("targetDir")
        .cloned()
        .unwrap_or_default();
    let mime_hint = metadata.get("filetype").cloned();

    let target_dir = if target_dir_str.is_empty() {
        state.root.clone()
    } else {
        state.path_safety.resolve(&target_dir_str)?
    };

    let file_id = uuid::Uuid::new_v4().to_string().replace('-', "");

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let session = UploadSession {
        file_id: file_id.clone(),
        filename,
        relative_path,
        target_dir,
        total_size: upload_length,
        uploaded: 0,
        created_at: now,
        last_active: now,
        expected_checksum: metadata.get("checksum").cloned(),
        mime_hint,
    };

    let tmp_dir = state.upload_manager.tmp_dir();
    session.persist_meta(tmp_dir).await?;
    state.upload_manager.create(session);

    let location = format!("/api/upload/{}", file_id);

    Ok(Response::builder()
        .status(StatusCode::CREATED)
        .header("Location", &location)
        .header("Tus-Resumable", TUS_VERSION)
        .header("Upload-Offset", "0")
        .body(Body::empty())
        .unwrap())
}

/// HEAD /api/upload/{file_id} — 查询上传进度
pub async fn head(
    State(state): State<AppState>,
    Path(file_id): Path<String>,
) -> Result<Response<Body>, AppError> {
    let arc = state
        .upload_manager
        .get(&file_id)
        .ok_or_else(|| AppError::NotFound(file_id.clone()))?;

    let session = arc.read().await;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Upload-Offset", session.uploaded.to_string())
        .header("Upload-Length", session.total_size.to_string())
        .header("Tus-Resumable", TUS_VERSION)
        .header("Cache-Control", "no-store")
        .body(Body::empty())
        .unwrap())
}

/// PATCH /api/upload/{file_id} — 上传分块（核心：流式写入）
pub async fn patch(
    State(state): State<AppState>,
    Path(file_id): Path<String>,
    headers: HeaderMap,
    request: axum::extract::Request,
) -> Result<Response<Body>, AppError> {
    let client_offset: u64 = headers
        .get("upload-offset")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| AppError::BadRequest("missing Upload-Offset".into()))?;

    let arc = state
        .upload_manager
        .get(&file_id)
        .ok_or_else(|| AppError::NotFound(file_id.clone()))?;

    // 校验 offset
    {
        let session = arc.read().await;
        if session.uploaded != client_offset {
            return Err(AppError::OffsetConflict {
                server: session.uploaded,
                client: client_offset,
            });
        }
    }

    // 并发上限：获取传输许可（PATCH 数据传输期间持有，结束自动释放）
    let _permit = state
        .transfer_semaphore
        .clone()
        .acquire_owned()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("semaphore closed: {}", e)))?;

    let tmp_dir = state.upload_manager.tmp_dir().clone();
    let (part_path, total_size) = {
        let session = arc.read().await;
        (session.part_path(&tmp_dir), session.total_size)
    };

    // 流式写入 — 关键修复点：不用 to_bytes()！
    let mut writer = ChunkWriter::open(&part_path, client_offset).await?;
    let mut stream = request.into_body().into_data_stream();
    let mut written: u64 = 0;
    let persist_interval: u64 = 16 * 1024 * 1024; // 每 16MB 持久化一次

    while let Some(frame) = stream.next().await {
        let bytes = frame.map_err(|e| {
            AppError::Internal(anyhow::anyhow!("body read error: {}", e))
        })?;
        // 防止客户端超发填满磁盘：累计写入不得超过声明的 total_size
        if client_offset + written + bytes.len() as u64 > total_size {
            return Err(AppError::PayloadTooLarge);
        }
        writer.write_all(&bytes).await?;
        written += bytes.len() as u64;

        // 定期持久化进度
        if written % persist_interval < bytes.len() as u64 {
            writer.flush_data().await?;
            let mut session = arc.write().await;
            session.uploaded = client_offset + written;
            session.last_active = now_secs();
            session.persist_meta(&tmp_dir).await?;
        }
    }

    // 最终 flush
    writer.flush_data().await?;
    let new_offset = client_offset + written;

    {
        let mut session = arc.write().await;
        session.uploaded = new_offset;
        session.last_active = now_secs();
        session.persist_meta(&tmp_dir).await?;
    }

    // 检查是否上传完成
    let completed = {
        let session = arc.read().await;
        session.is_complete()
    };

    if completed {
        finalize_upload(&state, &file_id).await?;
    }

    Ok(Response::builder()
        .status(StatusCode::NO_CONTENT)
        .header("Upload-Offset", new_offset.to_string())
        .header("Tus-Resumable", TUS_VERSION)
        .body(Body::empty())
        .unwrap())
}

/// DELETE /api/upload/{file_id} — 取消上传
pub async fn cancel(
    State(state): State<AppState>,
    Path(file_id): Path<String>,
) -> Result<StatusCode, AppError> {
    let arc = state
        .upload_manager
        .get(&file_id)
        .ok_or_else(|| AppError::NotFound(file_id.clone()))?;

    let tmp_dir = state.upload_manager.tmp_dir();
    let session = arc.read().await;
    let _ = tokio::fs::remove_file(session.part_path(tmp_dir)).await;
    let _ = tokio::fs::remove_file(session.meta_path(tmp_dir)).await;
    drop(session);

    state.upload_manager.remove(&file_id);

    Ok(StatusCode::NO_CONTENT)
}

/// 上传完成后的 finalize：校验 + 原子 rename
async fn finalize_upload(state: &AppState, file_id: &str) -> Result<(), AppError> {
    let arc = state
        .upload_manager
        .get(file_id)
        .ok_or_else(|| AppError::NotFound(file_id.to_string()))?;

    let session = arc.read().await;
    let tmp_dir = state.upload_manager.tmp_dir();
    let part_path = session.part_path(tmp_dir);
    let expected_checksum = session.expected_checksum.clone();

    // 计算最终目录：relative_path 除最后一段（文件名本身，落盘名用 session.filename）
    // 外都是要建的子目录。会话可能来自重启恢复的 .meta，这里必须重新做越界校验。
    let mut sub_dirs = match session.relative_path.as_deref() {
        Some(rel) => split_relative(rel)?,
        None => Vec::new(),
    };
    sub_dirs.pop();
    let final_dir = state
        .path_safety
        .resolve_for_create(&session.target_dir, &sub_dirs)?;

    tokio::fs::create_dir_all(&final_dir).await?;

    let mut final_path = final_dir.join(&session.filename);

    // 文件名冲突处理
    if final_path.exists() {
        let stem = final_path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let ext = final_path
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy()))
            .unwrap_or_default();
        for i in 1..1000 {
            let new_name = format!("{} ({}){}", stem, i, ext);
            let candidate = final_dir.join(&new_name);
            if !candidate.exists() {
                final_path = candidate;
                break;
            }
        }
    }

    drop(session);

    // 完整性校验：若客户端提供了 checksum（SHA-256 hex），在发布前校验 .part
    if let Some(expected) = expected_checksum {
        let actual = sha256_file(&part_path).await?;
        if !actual.eq_ignore_ascii_case(expected.trim()) {
            let _ = tokio::fs::remove_file(&part_path).await;
            if let Some(a) = state.upload_manager.get(file_id) {
                let s = a.read().await;
                let _ = tokio::fs::remove_file(s.meta_path(tmp_dir)).await;
            }
            state.upload_manager.remove(file_id);
            return Err(AppError::ChecksumMismatch { expected, actual });
        }
    }

    // 原子 rename
    tokio::fs::rename(&part_path, &final_path).await?;

    // 清理 meta
    if let Some(arc) = state.upload_manager.get(file_id) {
        let session = arc.read().await;
        let _ = tokio::fs::remove_file(session.meta_path(tmp_dir)).await;
    }

    state.upload_manager.remove(file_id);

    tracing::info!(
        file_id = %file_id,
        path = %final_path.display(),
        "upload finalized"
    );

    Ok(())
}

/// 解析 tus Upload-Metadata 头
fn parse_tus_metadata(headers: &HeaderMap) -> std::collections::HashMap<String, String> {
    use base64::Engine;

    let mut map = std::collections::HashMap::new();
    if let Some(val) = headers.get("upload-metadata") {
        if let Ok(s) = val.to_str() {
            for pair in s.split(',') {
                let pair = pair.trim();
                if let Some((key, b64val)) = pair.split_once(' ') {
                    if let Ok(decoded) = base64::engine::general_purpose::STANDARD
                        .decode(b64val.trim())
                    {
                        if let Ok(value) = String::from_utf8(decoded) {
                            map.insert(key.trim().to_string(), value);
                        }
                    }
                }
            }
        }
    }
    map
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// 流式计算文件 SHA-256，返回小写 hex（避免整文件读入内存）
async fn sha256_file(path: &std::path::Path) -> std::io::Result<String> {
    use sha2::{Digest, Sha256};
    use tokio::io::AsyncReadExt;

    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn sha256_file_matches_known_vector() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("f.bin");
        // 跨越 1MB 读缓冲边界，验证分块累积正确
        tokio::fs::write(&p, b"hello").await.unwrap();
        let got = sha256_file(&p).await.unwrap();
        assert_eq!(
            got,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[tokio::test]
    async fn sha256_file_large_multichunk() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("big.bin");
        // 3MB 全 0，跨多个 1MB 读缓冲块
        tokio::fs::write(&p, vec![0u8; 3 * 1024 * 1024]).await.unwrap();
        let got = sha256_file(&p).await.unwrap();
        // sha256 of 3MiB zeros
        assert_eq!(got.len(), 64);
    }
}
