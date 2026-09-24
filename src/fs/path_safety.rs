use std::path::{Path, PathBuf};

use crate::error::AppError;

/// 共享根下的内部目录：上传分片、会话 meta、分享记录都在里面
const RESERVED_DIR: &str = ".transfer-tmp";

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
    ///
    /// 入参已经是解码后的字符串（Query / Json / Path 提取器、tus metadata 都解过码），
    /// 这里不再 percent-decode：之前多解一次，名字里本来就带 `%41` 的文件会被解析成
    /// 另一个文件。越界防护靠的是下面 canonicalize 之后的包含检查，与解码无关。
    pub fn resolve(&self, relative: &str) -> Result<PathBuf, AppError> {
        // 清理路径组件，拒绝 .. 和绝对路径
        let cleaned: PathBuf = relative
            .split('/')
            .filter(|s| !s.is_empty() && *s != "." && *s != "..")
            .collect();

        let full_path = self.root.join(&cleaned);

        // canonicalize 存在的路径（处理符号链接）
        let canonical = if full_path.exists() {
            dunce::canonicalize(&full_path).map_err(|_| AppError::NotFound(relative.to_string()))?
        } else {
            // 对于不存在的路径，canonicalize 父目录
            if let Some(parent) = full_path.parent() {
                if parent.exists() {
                    let canonical_parent = dunce::canonicalize(parent)
                        .map_err(|_| AppError::NotFound(relative.to_string()))?;
                    let file_name = full_path
                        .file_name()
                        .ok_or_else(|| AppError::BadRequest("invalid path".into()))?;
                    canonical_parent.join(file_name)
                } else {
                    return Err(AppError::NotFound(relative.to_string()));
                }
            } else {
                return Err(AppError::NotFound(relative.to_string()));
            }
        };

        self.ensure_inside(&canonical)?;
        Ok(canonical)
    }

    /// 为「即将创建」的子目录做越界校验，返回最终目录（调用方负责 `create_dir_all`）。
    ///
    /// `base` 也要重新校验：它可能来自重启恢复的 `.meta`，不能假设仍在 root 内。
    /// 按段往下走：已存在的段 canonicalize 后必须仍在 root 内且是目录，这样挡住了
    /// 借根目录里指向外部的符号链接跳出去；从第一个不存在的段开始都是新建的，
    /// 后面不可能再经过符号链接。`components` 必须来自 [`split_relative`]。
    pub fn resolve_for_create(
        &self,
        base: &Path,
        components: &[String],
    ) -> Result<PathBuf, AppError> {
        let mut current = dunce::canonicalize(base)
            .map_err(|_| AppError::NotFound(base.display().to_string()))?;
        self.ensure_inside(&current)?;

        let mut creating = false;
        for component in components {
            let next = current.join(component);
            if creating {
                current = next;
                continue;
            }
            match std::fs::symlink_metadata(&next) {
                Ok(_) => {
                    let canonical = dunce::canonicalize(&next)
                        .map_err(|_| AppError::NotFound(component.clone()))?;
                    self.ensure_inside(&canonical)?;
                    if !canonical.is_dir() {
                        return Err(AppError::NotADirectory);
                    }
                    current = canonical;
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    creating = true;
                    current = next;
                }
                Err(e) => return Err(e.into()),
            }
        }
        Ok(current)
    }

    /// 核心安全检查：必须在 root 下，且不能进入 root 下的内部目录。
    ///
    /// 内部目录能被 API 读写的话，就能伪造一个 target_dir 指向外部的上传 `.meta`，
    /// 重启恢复后由 finalize 写到共享根之外。
    fn ensure_inside(&self, canonical: &Path) -> Result<(), AppError> {
        let relative = canonical
            .strip_prefix(&self.root)
            .map_err(|_| AppError::PathTraversal)?;
        if relative
            .components()
            .next()
            .is_some_and(|c| c.as_os_str() == RESERVED_DIR)
        {
            return Err(AppError::Forbidden("reserved path"));
        }
        Ok(())
    }

    /// 检查路径是否是 .transfer-tmp 目录（listing 时跳过）
    pub fn is_transfer_tmp(&self, path: &Path) -> bool {
        path.file_name().map(|n| n == RESERVED_DIR).unwrap_or(false)
    }
}

/// 把客户端给的相对路径（tus 的 relativePath）拆成干净的路径段。
///
/// 含 `..` 或是绝对路径时直接拒绝而不是悄悄丢掉：这种输入只会来自恶意或有 bug 的
/// 客户端，静默纠正会让文件落到对方意料之外的位置。其余每段都过一遍
/// `sanitize_filename`，与上传文件名的处理一致。
pub fn split_relative(relative: &str) -> Result<Vec<String>, AppError> {
    if relative.starts_with(['/', '\\']) {
        return Err(AppError::BadRequest("relativePath must be relative".into()));
    }
    let mut components = Vec::new();
    for segment in relative.split(['/', '\\']) {
        match segment {
            "" | "." => {}
            ".." => {
                return Err(AppError::BadRequest(
                    "relativePath must not contain ..".into(),
                ))
            }
            s => {
                let clean = sanitize_filename::sanitize(s);
                if !clean.is_empty() {
                    components.push(clean);
                }
            }
        }
    }
    Ok(components)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, PathSafety) {
        let dir = TempDir::new().unwrap();
        let root = dunce::canonicalize(dir.path()).unwrap();
        let safety = PathSafety::new(root);
        (dir, safety)
    }

    #[test]
    fn test_resolve_normal_path() {
        let (dir, safety) = setup();
        std::fs::create_dir_all(dir.path().join("sub")).unwrap();
        let result = safety.resolve("sub").unwrap();
        assert!(result.starts_with(safety.root()));
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

    /// 入参已经解过码，`%20` 这类字面量必须原样保留，不能再被当成转义
    #[test]
    fn literal_percent_sequences_are_not_decoded_again() {
        let (dir, safety) = setup();
        std::fs::create_dir_all(dir.path().join("my%20dir")).unwrap();
        std::fs::create_dir_all(dir.path().join("my dir")).unwrap();
        let result = safety.resolve("my%20dir").unwrap();
        assert!(result.ends_with("my%20dir"));
    }

    /// 不再解码之后，`%2e%2e` 只是一个普通名字，不能被还原成 `..` 越界
    #[test]
    fn encoded_dot_dot_is_just_a_name() {
        let (_dir, safety) = setup();
        match safety.resolve("%2e%2e/%2e%2e/etc") {
            Ok(p) => assert!(p.starts_with(safety.root())),
            Err(e) => assert!(matches!(e, AppError::NotFound(_)), "{e:?}"),
        }
    }

    #[test]
    fn reserved_dir_is_not_reachable_through_the_api() {
        let (dir, safety) = setup();
        std::fs::create_dir_all(dir.path().join(".transfer-tmp/shares")).unwrap();
        for p in [
            ".transfer-tmp",
            ".transfer-tmp/shares",
            ".transfer-tmp/x.meta",
            "./.transfer-tmp",
        ] {
            assert!(
                matches!(safety.resolve(p), Err(AppError::Forbidden(_))),
                "{p}"
            );
        }
        // 子目录里同名的普通目录不受影响
        std::fs::create_dir_all(dir.path().join("sub/.transfer-tmp")).unwrap();
        assert!(safety.resolve("sub/.transfer-tmp").is_ok());
    }

    #[test]
    fn test_transfer_tmp_detection() {
        let (_dir, safety) = setup();
        assert!(safety.is_transfer_tmp(Path::new("/some/path/.transfer-tmp")));
        assert!(!safety.is_transfer_tmp(Path::new("/some/path/normal")));
    }

    #[test]
    fn split_relative_rejects_escapes() {
        for bad in [
            "../x.txt",
            "a/../../x.txt",
            "/etc/cron.d/x",
            "\\evil\\x",
            "a\\..\\..\\x",
        ] {
            assert!(split_relative(bad).is_err(), "{bad}");
        }
        assert_eq!(
            split_relative("folder/./sub//f.txt").unwrap(),
            vec!["folder", "sub", "f.txt"]
        );
    }

    #[test]
    fn resolve_for_create_stays_inside_root() {
        let (dir, safety) = setup();
        let root = safety.root().to_path_buf();

        // 正常：已存在 + 新建混合
        std::fs::create_dir_all(dir.path().join("a")).unwrap();
        let comps = split_relative("a/b/c/f.txt").unwrap();
        let target = safety
            .resolve_for_create(&root, &comps[..comps.len() - 1])
            .unwrap();
        assert_eq!(target, root.join("a/b/c"));

        // base 本身在 root 外（来自被篡改的 meta）
        let outside = TempDir::new().unwrap();
        assert!(matches!(
            safety.resolve_for_create(outside.path(), &[]),
            Err(AppError::PathTraversal)
        ));

        // base 指向内部目录
        std::fs::create_dir_all(dir.path().join(".transfer-tmp")).unwrap();
        assert!(safety
            .resolve_for_create(&root.join(".transfer-tmp"), &[])
            .is_err());
    }

    #[cfg(unix)]
    #[test]
    fn resolve_for_create_does_not_follow_symlinks_out_of_root() {
        let (dir, safety) = setup();
        let outside = TempDir::new().unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("link")).unwrap();

        let comps = vec!["link".to_string(), "x".to_string()];
        assert!(matches!(
            safety.resolve_for_create(safety.root(), &comps),
            Err(AppError::PathTraversal)
        ));
        assert!(!outside.path().join("x").exists());
    }
}
