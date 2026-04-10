use std::fs::Metadata;
use std::time::UNIX_EPOCH;

/// 计算 ETag: "<mtime_ns>-<size>"
pub fn compute_etag(meta: &Metadata) -> String {
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let size = meta.len();
    format!("\"{:x}-{:x}\"", mtime, size)
}

/// 检查 If-None-Match 头是否匹配 ETag
pub fn matches_etag(if_none_match: Option<&str>, etag: &str) -> bool {
    if_none_match.map(|v| v.trim() == etag).unwrap_or(false)
}
