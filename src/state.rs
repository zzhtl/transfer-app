use std::path::PathBuf;
use std::sync::Arc;

use crate::config::AppConfig;
use crate::fs::path_safety::PathSafety;
use crate::upload::manager::UploadManager;

/// 应用共享状态
pub type AppState = Arc<AppStateInner>;

pub struct AppStateInner {
    pub config: AppConfig,
    pub root: PathBuf,
    pub path_safety: PathSafety,
    pub upload_manager: UploadManager,
}

impl AppStateInner {
    pub fn new(config: AppConfig) -> anyhow::Result<Self> {
        let root = config.path.clone();
        let tmp_dir = root.join(".transfer-tmp");
        std::fs::create_dir_all(&tmp_dir)?;

        let path_safety = PathSafety::new(root.clone());
        let upload_manager = UploadManager::new(
            tmp_dir,
            std::time::Duration::from_secs(config.upload_expiration_secs),
        );

        Ok(Self {
            config,
            root,
            path_safety,
            upload_manager,
        })
    }
}
