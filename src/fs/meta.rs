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
    pub mime_type: Option<String>,
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

        let modified = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs());

        let extension = path
            .extension()
            .map(|e| e.to_string_lossy().to_string());

        let mime_type = if metadata.is_file() {
            Some(
                mime_guess::from_path(path)
                    .first_or_octet_stream()
                    .to_string(),
            )
        } else {
            None
        };

        Ok(Self {
            name,
            path: String::new(), // 由调用方填充
            is_dir: metadata.is_dir(),
            size: metadata.len(),
            modified,
            mime_type,
            extension,
        })
    }
}
