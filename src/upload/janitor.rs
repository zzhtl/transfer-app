use std::time::Duration;

use crate::state::AppState;

/// 启动后台清理任务，定期清理过期的上传会话
pub fn spawn(state: AppState) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(3600));
        loop {
            interval.tick().await;
            let cleaned = state.upload_manager.cleanup_expired().await;
            if cleaned > 0 {
                tracing::info!(count = cleaned, "cleaned expired upload sessions");
            }
        }
    });
}
