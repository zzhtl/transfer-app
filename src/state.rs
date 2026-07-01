use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::Semaphore;

use crate::auth::AuthContext;
use crate::config::AppConfig;
use crate::fs::path_safety::PathSafety;
use crate::share::ShareManager;
use crate::upload::manager::UploadManager;

/// 应用共享状态
pub type AppState = Arc<AppStateInner>;

pub struct AppStateInner {
    pub config: AppConfig,
    pub root: PathBuf,
    pub path_safety: PathSafety,
    pub upload_manager: UploadManager,
    /// 上传并发上限（对应 --max-concurrent-transfers）
    pub transfer_semaphore: Arc<Semaphore>,
    /// 鉴权上下文（仅在设置了 --auth-password 时存在）
    pub auth: Option<AuthContext>,
    /// 分享链接管理
    pub share_manager: ShareManager,
}

impl AppStateInner {
    pub fn new(config: AppConfig) -> anyhow::Result<Self> {
        let root = config.path.clone();
        let tmp_dir = root.join(".transfer-tmp");
        std::fs::create_dir_all(&tmp_dir)?;

        let path_safety = PathSafety::new(root.clone());
        let shares_dir = tmp_dir.join("shares");
        std::fs::create_dir_all(&shares_dir)?;
        let share_manager = ShareManager::new(shares_dir);

        let upload_manager = UploadManager::new(
            tmp_dir,
            std::time::Duration::from_secs(config.upload_expiration_secs),
        );
        // 至少允许 1 个并发，避免 0 导致永久阻塞
        let permits = config.max_concurrent_transfers.max(1);
        let transfer_semaphore = Arc::new(Semaphore::new(permits));

        // 设置了密码才启用鉴权；key 每进程随机（重启即全局失效）
        let auth = config.auth_password.as_ref().map(|_| AuthContext {
            key: crate::auth::token::random_key(),
            ttl: std::time::Duration::from_secs(config.session_ttl_secs),
            cookie_secure: config.tls_cert.is_some(),
        });

        Ok(Self {
            config,
            root,
            path_safety,
            upload_manager,
            transfer_semaphore,
            auth,
            share_manager,
        })
    }
}
