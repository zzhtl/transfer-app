//! 分享管理器：内存双表（token→记录 / id→token）+ 落盘 + 重启恢复 + 过期清理。
//! 记录创建后不可变，用 Arc<ShareRecord> 免内层锁。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::RwLock;

use crate::share::record::ShareRecord;

pub struct ShareManager {
    by_token: RwLock<HashMap<String, Arc<ShareRecord>>>,
    by_id: RwLock<HashMap<String, String>>,
    dir: PathBuf,
}

impl ShareManager {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            by_token: RwLock::new(HashMap::new()),
            by_id: RwLock::new(HashMap::new()),
            dir,
        }
    }

    pub fn dir(&self) -> &PathBuf {
        &self.dir
    }

    /// 落盘 + 双表插入
    pub async fn create(&self, rec: ShareRecord) -> anyhow::Result<Arc<ShareRecord>> {
        rec.persist(&self.dir).await?;
        let arc = Arc::new(rec);
        self.by_id.write().insert(arc.id.clone(), arc.token.clone());
        self.by_token.write().insert(arc.token.clone(), arc.clone());
        Ok(arc)
    }

    pub fn get(&self, token: &str) -> Option<Arc<ShareRecord>> {
        self.by_token.read().get(token).cloned()
    }

    pub fn list(&self) -> Vec<Arc<ShareRecord>> {
        self.by_token.read().values().cloned().collect()
    }

    /// 吊销：删表项 + 删文件
    pub async fn revoke(&self, id: &str) -> bool {
        let Some(token) = self.by_id.write().remove(id) else {
            return false;
        };
        // 注意：写锁必须在 await 前释放（parking_lot guard 非 Send）
        let rec = self.by_token.write().remove(&token);
        let Some(rec) = rec else {
            return false;
        };
        let _ = tokio::fs::remove_file(rec.meta_path(&self.dir)).await;
        true
    }

    /// 重启恢复：扫描 dir 下 *.share
    pub async fn boot_recover(&self) -> anyhow::Result<usize> {
        let mut read_dir = match tokio::fs::read_dir(&self.dir).await {
            Ok(rd) => rd,
            Err(_) => return Ok(0),
        };
        let mut count = 0;
        while let Some(entry) = read_dir.next_entry().await? {
            let path = entry.path();
            if path.extension().map(|e| e == "share").unwrap_or(false) {
                match ShareRecord::load_from(&path).await {
                    Ok(rec) => {
                        let arc = Arc::new(rec);
                        self.by_id.write().insert(arc.id.clone(), arc.token.clone());
                        self.by_token.write().insert(arc.token.clone(), arc.clone());
                        count += 1;
                    }
                    Err(e) => {
                        tracing::warn!(path = %path.display(), error = %e, "skip corrupt share meta");
                    }
                }
            }
        }
        Ok(count)
    }

    /// 清理过期分享：删表项 + 删文件，返回清理数
    pub async fn cleanup_expired(&self, now: u64) -> usize {
        let expired: Vec<Arc<ShareRecord>> = self
            .by_token
            .read()
            .values()
            .filter(|r| r.is_expired(now))
            .cloned()
            .collect();

        for rec in &expired {
            self.by_id.write().remove(&rec.id);
            self.by_token.write().remove(&rec.token);
            let _ = tokio::fs::remove_file(rec.meta_path(&self.dir)).await;
        }
        expired.len()
    }
}
