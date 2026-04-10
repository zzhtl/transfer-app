use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::header::*;
use axum::http::{Response, StatusCode};
use serde::Deserialize;
use tokio_util::compat::TokioAsyncWriteCompatExt;

use crate::error::AppError;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct ZipParams {
    pub paths: String,
    pub name: Option<String>,
}

/// GET /api/download-zip?paths=a,b,c — 流式 zip 打包下载
pub async fn get(
    State(state): State<AppState>,
    Query(params): Query<ZipParams>,
) -> Result<Response<Body>, AppError> {
    let entries: Vec<std::path::PathBuf> = params
        .paths
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|p| state.path_safety.resolve(p.trim()))
        .collect::<Result<_, _>>()?;

    if entries.is_empty() {
        return Err(AppError::BadRequest("no paths specified".into()));
    }

    let root = state.root.clone();
    let (writer, reader) = tokio::io::duplex(256 * 1024);
    let reader_stream = tokio_util::io::ReaderStream::new(reader);
    let body = Body::from_stream(reader_stream);

    // 后台写 zip
    tokio::spawn(async move {
        if let Err(e) = write_zip(writer, entries, &root).await {
            tracing::warn!(error = %e, "zip stream failed");
        }
    });

    let filename = params.name.unwrap_or_else(|| {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        format!("transfer-{}.zip", ts)
    });

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "application/zip")
        .header(
            CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", filename),
        )
        .body(body)
        .unwrap())
}

async fn write_zip(
    sink: tokio::io::DuplexStream,
    entries: Vec<std::path::PathBuf>,
    root: &std::path::Path,
) -> anyhow::Result<()> {
    use async_zip::base::write::ZipFileWriter;

    // tokio DuplexStream -> futures_io::AsyncWrite via compat
    let compat = sink.compat_write();
    let mut zip = ZipFileWriter::new(compat);

    for entry_path in &entries {
        if entry_path.is_dir() {
            let dir = entry_path.clone();
            let files: Vec<std::path::PathBuf> =
                tokio::task::spawn_blocking(move || {
                    walkdir::WalkDir::new(&dir)
                        .into_iter()
                        .filter_map(Result::ok)
                        .filter(|e| e.file_type().is_file())
                        .map(|e| e.into_path())
                        .collect()
                })
                .await?;

            for file in files {
                add_file_entry(&mut zip, &file, root).await?;
            }
        } else {
            add_file_entry(&mut zip, entry_path, root).await?;
        }
    }

    zip.close().await?;
    Ok(())
}

async fn add_file_entry<W>(
    zip: &mut async_zip::base::write::ZipFileWriter<W>,
    file: &std::path::Path,
    root: &std::path::Path,
) -> anyhow::Result<()>
where
    W: futures_util::io::AsyncWrite + Unpin,
{
    use async_zip::{Compression, ZipEntryBuilder};
    use futures_util::io::AsyncWriteExt;

    let rel = file
        .strip_prefix(root)
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
