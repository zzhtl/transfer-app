use std::fs::Metadata;
use std::path::Path;

use serde::Serialize;

/// 文件元信息
#[derive(Debug, Clone, Serialize)]
pub struct FileMeta {
    pub name: String,
    /// 相对于 root 的路径
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<u64>,
    pub mime_type: Option<&'static str>,
    pub extension: Option<String>,
}

impl FileMeta {
    pub async fn from_path(path: &Path) -> std::io::Result<Self> {
        let metadata = tokio::fs::metadata(path).await?;
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        Ok(Self::new(name, String::new(), &metadata)) // path 由调用方填充
    }

    /// 由已经拿到的元数据构造；`path` 是调用方视角下的相对路径
    pub fn new(name: String, path: String, metadata: &Metadata) -> Self {
        let modified = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs());

        let extension = Path::new(&name)
            .extension()
            .map(|e| e.to_string_lossy().to_string());

        // first_raw 返回 &'static str：列目录时不必为每个条目分配一个 mime 字符串
        let mime_type = metadata.is_file().then(|| {
            mime_guess::from_path(&name)
                .first_raw()
                .unwrap_or("application/octet-stream")
        });

        Self {
            name,
            path,
            is_dir: metadata.is_dir(),
            size: metadata.len(),
            modified,
            mime_type,
            extension,
        }
    }
}
