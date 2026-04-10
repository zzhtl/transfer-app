use std::path::{Path, PathBuf};

use crate::error::AppError;

/// 路径安全检查器，防止目录穿越
#[derive(Debug, Clone)]
pub struct PathSafety {
    root: PathBuf,
}

impl PathSafety {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// 将相对路径解析为安全的绝对路径
    pub fn resolve(&self, relative: &str) -> Result<PathBuf, AppError> {
        let decoded = percent_encoding::percent_decode_str(relative)
            .decode_utf8_lossy()
            .to_string();

        // 清理路径组件，拒绝 .. 和绝对路径
        let cleaned: PathBuf = decoded
            .split('/')
            .filter(|s| !s.is_empty() && *s != "." && *s != "..")
            .collect();

        let full_path = self.root.join(&cleaned);

        // canonicalize 存在的路径（处理符号链接）
        let canonical = if full_path.exists() {
            dunce::canonicalize(&full_path)
                .map_err(|_| AppError::NotFound(decoded.clone()))?
        } else {
            // 对于不存在的路径，canonicalize 父目录
            if let Some(parent) = full_path.parent() {
                if parent.exists() {
                    let canonical_parent = dunce::canonicalize(parent)
                        .map_err(|_| AppError::NotFound(decoded.clone()))?;
                    let file_name = full_path
                        .file_name()
                        .ok_or_else(|| AppError::BadRequest("invalid path".into()))?;
                    canonical_parent.join(file_name)
                } else {
                    return Err(AppError::NotFound(decoded));
                }
            } else {
                return Err(AppError::NotFound(decoded));
            }
        };

        // 核心安全检查：必须在 root 下
        if !canonical.starts_with(&self.root) {
            return Err(AppError::PathTraversal);
        }

        Ok(canonical)
    }

    /// 检查路径是否是 .transfer-tmp 目录（listing 时跳过）
    pub fn is_transfer_tmp(&self, path: &Path) -> bool {
        path.file_name()
            .map(|n| n == ".transfer-tmp")
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, PathSafety) {
        let dir = TempDir::new().unwrap();
        let safety = PathSafety::new(dir.path().to_path_buf());
        (dir, safety)
    }

    #[test]
    fn test_resolve_normal_path() {
        let (dir, safety) = setup();
        std::fs::create_dir_all(dir.path().join("sub")).unwrap();
        let result = safety.resolve("sub").unwrap();
        assert!(result.starts_with(dir.path()));
    }

    #[test]
    fn test_reject_traversal() {
        let (_dir, safety) = setup();
        let result = safety.resolve("../../../etc/passwd");
        assert!(matches!(result, Err(AppError::PathTraversal) | Err(AppError::NotFound(_))));
    }

    #[test]
    fn test_reject_absolute_path() {
        let (_dir, safety) = setup();
        // 绝对路径的 / 会被 filter 掉，变成空路径 -> root
        let result = safety.resolve("/etc/passwd");
        // 应该返回 root 下的 etc/passwd，不存在 -> NotFound
        assert!(result.is_err() || result.unwrap().starts_with(safety.root()));
    }

    #[test]
    fn test_percent_decode() {
        let (dir, safety) = setup();
        std::fs::create_dir_all(dir.path().join("my dir")).unwrap();
        let result = safety.resolve("my%20dir").unwrap();
        assert!(result.ends_with("my dir"));
    }

    #[test]
    fn test_transfer_tmp_detection() {
        let (_dir, safety) = setup();
        assert!(safety.is_transfer_tmp(Path::new("/some/path/.transfer-tmp")));
        assert!(!safety.is_transfer_tmp(Path::new("/some/path/normal")));
    }
}
