use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// 上传会话，持久化为 .meta JSON 文件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadSession {
    pub file_id: String,
    pub filename: String,
    pub relative_path: Option<String>,
    pub target_dir: PathBuf,
    pub total_size: u64,
    pub uploaded: u64,
    pub created_at: u64,
    pub last_active: u64,
    pub expected_checksum: Option<String>,
    pub mime_hint: Option<String>,
}

impl UploadSession {
    pub fn part_path(&self, tmp_dir: &Path) -> PathBuf {
        tmp_dir.join(format!("{}.part", self.file_id))
    }

    pub fn meta_path(&self, tmp_dir: &Path) -> PathBuf {
        tmp_dir.join(format!("{}.meta", self.file_id))
    }

    /// 持久化 meta 到磁盘
    pub async fn persist_meta(&self, tmp_dir: &Path) -> std::io::Result<()> {
        let json = serde_json::to_vec(self)
            .map_err(std::io::Error::other)?;
        tokio::fs::write(self.meta_path(tmp_dir), json).await
    }

    /// 从 .meta 文件加载
    pub async fn load_from(path: &Path) -> std::io::Result<Self> {
        let data = tokio::fs::read(path).await?;
        serde_json::from_slice(&data)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    pub fn is_complete(&self) -> bool {
        self.uploaded >= self.total_size
    }
}
