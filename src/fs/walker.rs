use std::path::Path;

use crate::fs::meta::FileMeta;

/// 列出目录内容，跳过 .transfer-tmp。
///
/// 整个目录只用一次 spawn_blocking：之前每个条目单独一次 tokio::fs::metadata，每次都是
/// 一趟 blocking 线程池往返，两万个文件就是两万次调度。`rel_prefix` 是这个目录在调用方
/// 视角下（共享根或分享根）的相对路径，用来直接拼出每个条目的 `path`。
pub async fn list_directory(dir: &Path, rel_prefix: &str) -> std::io::Result<Vec<FileMeta>> {
    let dir = dir.to_path_buf();
    let prefix = rel_prefix.trim_matches('/').to_string();
    tokio::task::spawn_blocking(move || read_entries(&dir, &prefix))
        .await
        .map_err(std::io::Error::other)?
}

fn read_entries(dir: &Path, prefix: &str) -> std::io::Result<Vec<FileMeta>> {
    let mut entries = Vec::new();

    for entry in std::fs::read_dir(dir)? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                tracing::warn!(dir = %dir.display(), error = %e, "skip entry");
                continue;
            }
        };
        let name = entry.file_name().to_string_lossy().to_string();

        // 跳过隐藏的临时目录
        if name == ".transfer-tmp" {
            continue;
        }

        // DirEntry::metadata 不跟随符号链接，而列表要按链接目标展示（目录能点进去、
        // 大小是目标文件的大小），所以只有符号链接才额外 stat 一次
        let metadata = match entry.file_type() {
            Ok(file_type) if file_type.is_symlink() => std::fs::metadata(entry.path()),
            _ => entry.metadata(),
        };
        match metadata {
            Ok(metadata) => {
                let path = if prefix.is_empty() {
                    name.clone()
                } else {
                    format!("{prefix}/{name}")
                };
                entries.push(FileMeta::new(name, path, &metadata));
            }
            Err(e) => {
                tracing::warn!(path = %entry.path().display(), error = %e, "skip entry");
            }
        }
    }

    // 目录在前，文件在后；各自按名称排序
    entries.sort_by_cached_key(|e| (!e.is_dir, e.name.to_lowercase()));

    Ok(entries)
}
