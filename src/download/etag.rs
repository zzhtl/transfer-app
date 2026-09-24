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
///
/// 按 RFC 9110 用弱比较：头里可以是逗号分隔的列表、带 `W/` 前缀，或者 `*`。
pub fn matches_etag(if_none_match: Option<&str>, etag: &str) -> bool {
    let Some(value) = if_none_match else {
        return false;
    };
    value.split(',').map(str::trim).any(|candidate| {
        candidate == "*" || candidate.strip_prefix("W/").unwrap_or(candidate) == etag
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn if_none_match_accepts_lists_weak_tags_and_star() {
        let etag = "\"abc-1\"";
        assert!(matches_etag(Some("\"abc-1\""), etag));
        assert!(matches_etag(Some("\"x\", \"abc-1\""), etag));
        assert!(matches_etag(Some("W/\"abc-1\""), etag));
        assert!(matches_etag(Some("*"), etag));
        assert!(!matches_etag(Some("\"abc-2\""), etag));
        assert!(!matches_etag(None, etag));
    }
}
