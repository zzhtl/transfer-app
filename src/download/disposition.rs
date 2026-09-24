use axum::http::HeaderValue;
use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};

/// RFC 5987 `attr-char` 之外的字符都要百分号编码
const ATTR_CHAR: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'!')
    .remove(b'#')
    .remove(b'$')
    .remove(b'&')
    .remove(b'+')
    .remove(b'-')
    .remove(b'.')
    .remove(b'^')
    .remove(b'_')
    .remove(b'`')
    .remove(b'|')
    .remove(b'~');

/// 构造 `Content-Disposition`，同时给 ASCII 兜底的 `filename` 和 RFC 5987 的 `filename*`。
///
/// 之前是把原始文件名直接塞进 `filename="…"`：中文名在部分浏览器里乱码，名字里的 `"`
/// 会截断这个头，而控制字符（ext4/APFS 上是合法文件名）会让 HeaderValue 构造失败——
/// 那里是 `unwrap`，release 下 `panic = "abort"` 就是整个进程退出。
/// 这里的输出只含可见 ASCII 和空格，一定是合法的 HeaderValue。
pub fn content_disposition(inline: bool, filename: &str) -> HeaderValue {
    let kind = if inline { "inline" } else { "attachment" };
    let fallback: String = filename
        .chars()
        .map(|c| match c {
            ' ' => ' ',
            '"' | '\\' => '_',
            c if c.is_ascii_graphic() => c,
            _ => '_',
        })
        .collect();
    let encoded = utf8_percent_encode(filename, ATTR_CHAR);
    HeaderValue::from_str(&format!(
        "{kind}; filename=\"{fallback}\"; filename*=UTF-8''{encoded}"
    ))
    .unwrap_or_else(|_| HeaderValue::from_static("attachment"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(inline: bool, name: &str) -> String {
        content_disposition(inline, name)
            .to_str()
            .expect("只含可见 ASCII")
            .to_string()
    }

    #[test]
    fn unicode_names_get_an_rfc5987_filename_star() {
        assert_eq!(
            render(false, "报告 v2.pdf"),
            "attachment; filename=\"__ v2.pdf\"; filename*=UTF-8''%E6%8A%A5%E5%91%8A%20v2.pdf"
        );
    }

    #[test]
    fn quotes_backslashes_and_control_chars_cannot_break_the_header() {
        for name in [
            "a\"b.txt",
            "a\\b.txt",
            "line\nbreak.txt",
            "tab\there.txt",
            "nul\0.txt",
        ] {
            let value = render(true, name);
            assert!(value.starts_with("inline; filename=\""), "{value}");
            // 兜底部分里只剩一对引号
            let fallback = value.split("; filename*=").next().expect("兜底部分");
            assert_eq!(fallback.matches('"').count(), 2, "{value}");
        }
    }

    #[test]
    fn plain_ascii_names_round_trip() {
        assert_eq!(
            render(false, "notes-1.txt"),
            "attachment; filename=\"notes-1.txt\"; filename*=UTF-8''notes-1.txt"
        );
    }
}
