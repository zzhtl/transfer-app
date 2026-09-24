use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::header::*;
use axum::http::{Response, StatusCode};
use tokio_util::compat::TokioAsyncWriteCompatExt;

use crate::download::disposition::content_disposition;
use crate::error::AppError;
use crate::state::AppState;

/// GET /api/download-zip?paths=a&paths=b[&name=x.zip] — 流式 zip 打包下载
///
/// `paths` 用重复参数传，每个值就是一个完整路径。之前是 `paths: String` 加逗号拆分：
/// 前端发的一直是重复参数，serde 遇到第二个 `paths` 就报 duplicate field，多选打包
/// 必定 400；逗号拆分和 trim 还会切坏名字里带逗号、首尾带空格的文件。
pub async fn get(
    State(state): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Response<Body>, AppError> {
    let mut entries = Vec::new();
    let mut name = None;
    for (key, value) in pairs {
        match key.as_str() {
            "paths" if !value.is_empty() => entries.push(state.path_safety.resolve(&value)?),
            "name" if !value.is_empty() => name = Some(value),
            _ => {}
        }
    }

    if entries.is_empty() {
        return Err(AppError::BadRequest("no paths specified".into()));
    }

    let filename = name.unwrap_or_else(default_zip_name);
    Ok(zip_response(entries, filename))
}

/// 默认 zip 文件名 `transfer-<秒>.zip`
pub fn default_zip_name() -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("transfer-{}.zip", ts)
}

/// 构造流式 ZIP 响应（后台写、边写边传）。供直接 ZIP 下载与分享 ZIP 共用。
/// entries 的路径安全由调用方保证。
///
/// 每个条目在包内的路径相对于它自己的父目录：在 `photos/2024` 里多选打包，
/// 解压出来就是选中的那几项，而不是多套一层 `photos/2024/`。
pub fn zip_response(entries: Vec<std::path::PathBuf>, filename: String) -> Response<Body> {
    let (writer, reader) = tokio::io::duplex(256 * 1024);
    let reader_stream = tokio_util::io::ReaderStream::new(reader);
    let body = Body::from_stream(reader_stream);

    tokio::spawn(async move {
        if let Err(e) = write_zip(writer, entries).await {
            tracing::warn!(error = %e, "zip stream failed");
        }
    });

    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "application/zip")
        .header(CONTENT_DISPOSITION, content_disposition(false, &filename))
        .body(body)
        .unwrap()
}

async fn write_zip(
    sink: tokio::io::DuplexStream,
    entries: Vec<std::path::PathBuf>,
) -> anyhow::Result<()> {
    use async_zip::base::write::ZipFileWriter;

    // tokio DuplexStream -> futures_io::AsyncWrite via compat
    let compat = sink.compat_write();
    let mut zip = ZipFileWriter::new(compat);

    for entry_path in &entries {
        let base = entry_path
            .parent()
            .unwrap_or(entry_path.as_path())
            .to_path_buf();
        if entry_path.is_dir() {
            let dir = entry_path.clone();
            let files: Vec<std::path::PathBuf> = tokio::task::spawn_blocking(move || {
                walkdir::WalkDir::new(&dir)
                    .into_iter()
                    .filter_entry(|e| e.file_name() != ".transfer-tmp")
                    .filter_map(Result::ok)
                    .filter(|e| e.file_type().is_file())
                    .map(|e| e.into_path())
                    .collect()
            })
            .await?;

            for file in files {
                add_file_entry(&mut zip, &file, &base).await?;
            }
        } else {
            add_file_entry(&mut zip, entry_path, &base).await?;
        }
    }

    zip.close().await?;
    Ok(())
}

async fn add_file_entry<W>(
    zip: &mut async_zip::base::write::ZipFileWriter<W>,
    file: &std::path::Path,
    base: &std::path::Path,
) -> anyhow::Result<()>
where
    W: futures_util::io::AsyncWrite + Unpin,
{
    use async_zip::{Compression, ZipEntryBuilder};
    use futures_util::io::AsyncWriteExt;

    let rel = file
        .strip_prefix(base)
        .unwrap_or(file)
        .to_string_lossy()
        .to_string();

    let entry_builder = ZipEntryBuilder::new(
        rel.into(),
        Compression::Stored,
    );

    let mut entry_writer = zip.write_entry_stream(entry_builder).await?;

    // 流式读取，不全部加载到内存
    let mut f = tokio::fs::File::open(file).await?;
    let mut buf = vec![0u8; 256 * 1024]; // 256KB
    loop {
        let n = tokio::io::AsyncReadExt::read(&mut f, &mut buf).await?;
        if n == 0 {
            break;
        }
        entry_writer.write_all(&buf[..n]).await?;
    }

    entry_writer.close().await?;

    Ok(())
}
