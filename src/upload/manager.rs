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

        // 先在 parking_lot 锁里把 Arc 拷出来，放锁后再逐个 `.read().await`：
        // 这里跑在 tokio 任务里，`blocking_read` 会直接 panic（release 下即进程退出），
        // 而 parking_lot 的 guard 又不能跨 await 持有。
        let sessions: Vec<(String, Arc<RwLock<UploadSession>>)> = self
            .sessions
            .read()
            .iter()
            .map(|(id, arc)| (id.clone(), arc.clone()))
            .collect();

        let mut expired = Vec::new();
        for (id, arc) in sessions {
            if now.saturating_sub(arc.read().await.last_active) > expiry {
                expired.push(id);
            }
        }

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

#[cfg(test)]
mod tests {
    use super::*;

    fn session(file_id: &str, last_active: u64) -> UploadSession {
        UploadSession {
            file_id: file_id.to_string(),
            filename: format!("{file_id}.bin"),
            relative_path: None,
            target_dir: PathBuf::from("/"),
            total_size: 10,
            uploaded: 0,
            created_at: last_active,
            last_active,
            expected_checksum: None,
            mime_hint: None,
        }
    }

    /// janitor 是在 tokio 任务里调用它的。之前这里用 `blocking_read`，在运行时线程上
    /// 会直接 panic，release 下 `panic = "abort"` 就是整个进程退出——只要重启时
    /// 有一个没传完的上传会话，服务就起不来（首个 interval tick 立即触发）。
    #[tokio::test]
    async fn cleanup_runs_inside_the_runtime_and_only_drops_stale_sessions() {
        let dir = tempfile::tempdir().expect("临时目录");
        let manager = UploadManager::new(dir.path().to_path_buf(), Duration::from_secs(60));
        let now = crate::auth::now_secs();
        manager.create(session("stale", now - 3600));
        manager.create(session("fresh", now));

        assert_eq!(manager.cleanup_expired().await, 1);
        assert!(manager.get("stale").is_none());
        assert!(manager.get("fresh").is_some());
    }
}
