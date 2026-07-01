//! 分享记录：绑定一个目标路径 + 有效期 + 可选提取码 + 仅下载标志。
//! 仿 UploadSession 的 serde 落盘范式，持久化到 .transfer-tmp/shares/<id>.share。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareRecord {
    /// 管理用 id（uuid，用于 DELETE）
    pub id: String,
    /// 公开不可猜 token（uuid v4 去横线）
    pub token: String,
    /// 目标：root 相对路径（访问时重新用 PathSafety 解析）
    pub target: String,
    pub is_dir: bool,
    pub created_at: u64,
    pub expires_at: u64,
    /// 提取码的 SHA-256 hex（绝不存明文）；None 表示无需提取码
    pub code_hash: Option<String>,
    /// 仅下载：不暴露目录浏览/子路径，仅整体下载/zip
    pub download_only: bool,
}

impl ShareRecord {
    pub fn meta_path(&self, dir: &Path) -> PathBuf {
        dir.join(format!("{}.share", self.id))
    }

    pub async fn persist(&self, dir: &Path) -> std::io::Result<()> {
        let json = serde_json::to_vec(self).map_err(std::io::Error::other)?;
        tokio::fs::write(self.meta_path(dir), json).await
    }

    pub async fn load_from(path: &Path) -> std::io::Result<Self> {
        let data = tokio::fs::read(path).await?;
        serde_json::from_slice(&data)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    pub fn is_expired(&self, now: u64) -> bool {
        self.expires_at <= now
    }

    pub fn requires_code(&self) -> bool {
        self.code_hash.is_some()
    }

    /// 校验提取码（无码分享恒 true；有码则常数时间比对 SHA-256）
    pub fn verify_code(&self, input: Option<&str>) -> bool {
        match &self.code_hash {
            None => true,
            Some(expected) => match input {
                Some(code) if !code.is_empty() => {
                    let actual = sha256_hex(code.as_bytes());
                    crate::auth::token::ct_eq(actual.as_bytes(), expected.as_bytes())
                }
                _ => false,
            },
        }
    }
}

/// 计算数据的 SHA-256，返回小写 hex
pub fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(data))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(code_hash: Option<String>) -> ShareRecord {
        ShareRecord {
            id: "id".into(),
            token: "tok".into(),
            target: "a/b".into(),
            is_dir: false,
            created_at: 0,
            expires_at: 100,
            code_hash,
            download_only: false,
        }
    }

    #[test]
    fn no_code_always_passes() {
        let r = rec(None);
        assert!(r.verify_code(None));
        assert!(r.verify_code(Some("anything")));
    }

    #[test]
    fn code_must_match() {
        let r = rec(Some(sha256_hex(b"1234")));
        assert!(r.verify_code(Some("1234")));
        assert!(!r.verify_code(Some("0000")));
        assert!(!r.verify_code(None));
        assert!(!r.verify_code(Some("")));
    }

    #[test]
    fn expiry() {
        let r = rec(None);
        assert!(!r.is_expired(50));
        assert!(r.is_expired(100));
        assert!(r.is_expired(200));
    }
}
