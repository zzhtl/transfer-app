use std::path::Path;

use crate::fs::meta::FileMeta;

/// 列出目录内容，跳过 .transfer-tmp
pub async fn list_directory(dir: &Path) -> std::io::Result<Vec<FileMeta>> {
    let mut entries = Vec::new();
    let mut read_dir = tokio::fs::read_dir(dir).await?;

    while let Some(entry) = read_dir.next_entry().await? {
        let name = entry.file_name().to_string_lossy().to_string();

        // 跳过隐藏的临时目录
        if name == ".transfer-tmp" {
            continue;
        }

        match FileMeta::from_path(&entry.path()).await {
            Ok(meta) => entries.push(meta),
            Err(e) => {
                tracing::warn!(path = %entry.path().display(), error = %e, "skip entry");
            }
        }
    }

    // 目录在前，文件在后；各自按名称排序
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Ok(entries)
}
