use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;

use crate::upload::session::UploadSession;

/// 管理所有上传会话
pub struct UploadManager {
    sessions: parking_lot::RwLock<HashMap<String, Arc<RwLock<UploadSession>>>>,
    tmp_dir: PathBuf,
    expiration: Duration,
}

impl UploadManager {
    pub fn new(tmp_dir: PathBuf, expiration: Duration) -> Self {
        Self {
            sessions: parking_lot::RwLock::new(HashMap::new()),
            tmp_dir,
            expiration,
        }
    }

    pub fn tmp_dir(&self) -> &PathBuf {
        &self.tmp_dir
    }

    pub fn expiration(&self) -> Duration {
        self.expiration
    }

    /// 创建新的上传会话
    pub fn create(&self, session: UploadSession) -> Arc<RwLock<UploadSession>> {
        let arc = Arc::new(RwLock::new(session.clone()));
        self.sessions.write().insert(session.file_id.clone(), arc.clone());
        arc
    }

    /// 获取上传会话
    pub fn get(&self, file_id: &str) -> Option<Arc<RwLock<UploadSession>>> {
        self.sessions.read().get(file_id).cloned()
    }

    /// 移除上传会话
    pub fn remove(&self, file_id: &str) {
        self.sessions.write().remove(file_id);
    }

    /// 启动时恢复未完成的上传会话
    pub async fn boot_recover(&self) -> anyhow::Result<usize> {
        let mut count = 0;

        if !self.tmp_dir.exists() {
            return Ok(0);
        }

        let mut entries = tokio::fs::read_dir(&self.tmp_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension() != Some(OsStr::new("meta")) {
                continue;
            }
            match UploadSession::load_from(&path).await {
                Ok(session) => {
                    tracing::info!(
                        file_id = %session.file_id,
                        filename = %session.filename,
                        uploaded = session.uploaded,
                        total = session.total_size,
                        "recovered upload session"
                    );
                    self.sessions
                        .write()
                        .insert(session.file_id.clone(), Arc::new(RwLock::new(session)));
                    count += 1;
                }
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "skip corrupt meta");
                }
            }
        }

        Ok(count)
    }

    /// 清理过期的会话
    pub async fn cleanup_expired(&self) -> usize {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let expiry = self.expiration.as_secs();

        let expired: Vec<String> = {
            let sessions = self.sessions.read();
            let mut ids = Vec::new();
            for (id, arc) in sessions.iter() {
                let s = arc.blocking_read();
                if now.saturating_sub(s.last_active) > expiry {
                    ids.push(id.clone());
                }
            }
            ids
        };

        for id in &expired {
            let arc = {
                self.sessions.write().remove(id)
            };
            if let Some(arc) = arc {
                let s = arc.read().await;
                let _ = tokio::fs::remove_file(s.part_path(&self.tmp_dir)).await;
                let _ = tokio::fs::remove_file(s.meta_path(&self.tmp_dir)).await;
                tracing::info!(file_id = %id, "cleaned expired upload session");
            }
        }

        expired.len()
    }
}
