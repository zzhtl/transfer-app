use std::path::Path;

use crate::error::AppError;

/// 创建目录
pub async fn mkdir(path: &Path) -> Result<(), AppError> {
    tokio::fs::create_dir_all(path).await?;
    Ok(())
}

/// 重命名文件/目录
pub async fn rename(from: &Path, to: &Path) -> Result<(), AppError> {
    if to.exists() {
        return Err(AppError::BadRequest(format!(
            "target already exists: {}",
            to.display()
        )));
    }
    tokio::fs::rename(from, to).await?;
    Ok(())
}

/// 复制文件
pub async fn copy_file(from: &Path, to: &Path) -> Result<(), AppError> {
    if to.exists() {
        return Err(AppError::BadRequest(format!(
            "target already exists: {}",
            to.display()
        )));
    }
    if from.is_dir() {
        copy_dir_recursive(from, to).await?;
    } else {
        tokio::fs::copy(from, to).await?;
    }
    Ok(())
}

/// 移动文件/目录
pub async fn move_entry(from: &Path, to: &Path) -> Result<(), AppError> {
    if to.exists() {
        return Err(AppError::BadRequest(format!(
            "target already exists: {}",
            to.display()
        )));
    }
    // 先尝试 rename（同文件系统），失败则 copy + delete
    if tokio::fs::rename(from, to).await.is_err() {
        copy_file(from, to).await?;
        delete(from).await?;
    }
    Ok(())
}

/// 删除文件或目录
pub async fn delete(path: &Path) -> Result<(), AppError> {
    if path.is_dir() {
        tokio::fs::remove_dir_all(path).await?;
    } else {
        tokio::fs::remove_file(path).await?;
    }
    Ok(())
}

/// 递归复制目录
async fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), AppError> {
    tokio::fs::create_dir_all(dst).await?;

    let src = src.to_path_buf();
    let dst = dst.to_path_buf();

    // walkdir 是同步的，放到 spawn_blocking
    tokio::task::spawn_blocking(move || -> Result<(), AppError> {
        for entry in walkdir::WalkDir::new(&src).min_depth(1) {
            let entry = entry.map_err(|e| {
                AppError::Internal(anyhow::anyhow!("walk error: {}", e))
            })?;
            let relative = entry.path().strip_prefix(&src).map_err(|e| {
                AppError::Internal(anyhow::anyhow!("strip prefix: {}", e))
            })?;
            let target = dst.join(relative);

            if entry.file_type().is_dir() {
                std::fs::create_dir_all(&target)?;
            } else {
                std::fs::copy(entry.path(), &target)?;
            }
        }
        Ok(())
    })
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("join: {}", e)))??;

    Ok(())
}
