use axum::http::HeaderValue;

/// 解析 Range 头，返回 (start, end)
/// 支持: bytes=start-end, bytes=start-, bytes=-suffix
pub fn parse_range(header: Option<&HeaderValue>, file_size: u64) -> Option<(u64, u64)> {
    let header = header?;
    let s = header.to_str().ok()?;

    if !s.starts_with("bytes=") {
        return None;
    }

    let range_str = &s["bytes=".len()..];
    let (start_str, end_str) = range_str.split_once('-')?;

    let (start, end) = if start_str.is_empty() {
        // bytes=-suffix (最后 N 字节)
        let suffix: u64 = end_str.parse().ok()?;
        if suffix == 0 || suffix > file_size {
            return None;
        }
        (file_size - suffix, file_size - 1)
    } else {
        let start: u64 = start_str.parse().ok()?;
        let end = if end_str.is_empty() {
            file_size - 1
        } else {
            let end: u64 = end_str.parse().ok()?;
            end.min(file_size - 1)
        };
        (start, end)
    };

    if start > end || start >= file_size {
        return None;
    }

    Some((start, end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_full_range() {
        let h = HeaderValue::from_static("bytes=0-999");
        assert_eq!(parse_range(Some(&h), 1000), Some((0, 999)));
    }

    #[test]
    fn test_open_end() {
        let h = HeaderValue::from_static("bytes=500-");
        assert_eq!(parse_range(Some(&h), 1000), Some((500, 999)));
    }

    #[test]
    fn test_suffix() {
        let h = HeaderValue::from_static("bytes=-100");
        assert_eq!(parse_range(Some(&h), 1000), Some((900, 999)));
    }

    #[test]
    fn test_invalid_range() {
        let h = HeaderValue::from_static("bytes=999-0");
        assert_eq!(parse_range(Some(&h), 1000), None);
    }

    #[test]
    fn test_out_of_bounds() {
        let h = HeaderValue::from_static("bytes=1000-2000");
        assert_eq!(parse_range(Some(&h), 1000), None);
    }

    #[test]
    fn test_none() {
        assert_eq!(parse_range(None, 1000), None);
    }
}
