use std::path::Path;

/// 根据文件路径猜测 MIME 类型
pub fn guess_mime(path: &Path) -> String {
    mime_guess::from_path(path)
        .first_or_octet_stream()
        .to_string()
}
