use std::path::Path;

use crate::error::AppError;

/// 创建目录
pub async fn mkdir(path: &Path) -> Result<(), AppError> {
    tokio::fs::create_dir_all(path).await?;
    Ok(())
}

/// 目标已存在。消息里只带名字：之前带的是 `to.display()`，会把服务器上的绝对路径
/// 带到浏览器的错误提示里。
fn already_exists(to: &Path) -> AppError {
    AppError::Conflict(
        to.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
    )
}

/// 目标落在源目录自身之内。放行的话，rename 会因 EINVAL 失败并退回复制，
/// 而 copy_dir_recursive 边走边往自己里面复制，一路长到 ENAMETOOLONG 才停，留下一大堆垃圾。
fn ensure_not_into_itself(from: &Path, to: &Path) -> Result<(), AppError> {
    if to.starts_with(from) {
        return Err(AppError::BadRequest(
            "cannot move or copy a folder into itself".into(),
        ));
    }
    Ok(())
}

/// 重命名文件/目录
pub async fn rename(from: &Path, to: &Path) -> Result<(), AppError> {
    if to.exists() {
        return Err(already_exists(to));
    }
    tokio::fs::rename(from, to).await?;
    Ok(())
}

/// 复制文件
pub async fn copy_file(from: &Path, to: &Path) -> Result<(), AppError> {
    ensure_not_into_itself(from, to)?;
    if to.exists() {
        return Err(already_exists(to));
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
    ensure_not_into_itself(from, to)?;
    if to.exists() {
        return Err(already_exists(to));
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
