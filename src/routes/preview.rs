use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::header::*;
use axum::http::{Response, StatusCode};

use crate::error::AppError;
use crate::state::AppState;
use crate::util::mime::guess_mime;

/// GET /api/preview/{*path} — 文件预览
pub async fn get(
    State(state): State<AppState>,
    Path(rel): Path<String>,
) -> Result<Response<Body>, AppError> {
    let abs = state.path_safety.resolve(&rel)?;

    if abs.is_dir() {
        return Err(AppError::IsADirectory);
    }

    let mime = guess_mime(&abs);

    // Markdown: 服务端渲染为 HTML 片段
    if mime == "text/markdown"
        || abs.extension().map(|e| e == "md").unwrap_or(false)
    {
        let content = tokio::fs::read_to_string(&abs).await?;
        let html = render_markdown(&content);
        return Ok(Response::builder()
            .status(StatusCode::OK)
            .header(CONTENT_TYPE, "text/html; charset=utf-8")
            .body(Body::from(html))
            .unwrap());
    }

    // 文本文件: 限读首 1MB（用 take 避免整文件读入内存）
    if mime.starts_with("text/") || is_code_file(&abs) {
        use tokio::io::AsyncReadExt;
        let file = tokio::fs::File::open(&abs).await?;
        let mut data = Vec::new();
        file.take(1024 * 1024).read_to_end(&mut data).await?;

        // 检测编码
        let text = if content_inspector::inspect(&data).is_text() {
            String::from_utf8_lossy(&data).to_string()
        } else {
            let (decoded, _, _) = encoding_rs::UTF_8.decode(&data);
            decoded.to_string()
        };

        return Ok(Response::builder()
            .status(StatusCode::OK)
            .header(CONTENT_TYPE, "text/plain; charset=utf-8")
            .body(Body::from(text))
            .unwrap());
    }

    // 其它类型（图片/视频/音频/PDF）: 流式透传，避免整文件读入内存
    let meta = tokio::fs::metadata(&abs).await?;
    let size = meta.len();

    let file = tokio::fs::File::open(&abs).await?;
    let stream = tokio_util::io::ReaderStream::new(file);

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, &mime)
        .header(CONTENT_LENGTH, size)
        .header("X-Preview-Type", preview_type(&mime))
        .body(Body::from_stream(stream))
        .unwrap())
}

#[derive(serde::Deserialize)]
pub struct MarkdownReq {
    pub content: String,
}

/// POST /api/preview/markdown — 渲染 markdown 片段（编辑实时预览用）
pub async fn render_md(axum::Json(req): axum::Json<MarkdownReq>) -> Response<Body> {
    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "text/html; charset=utf-8")
        .body(Body::from(render_markdown(&req.content)))
        .unwrap()
}

/// 渲染为 HTML 片段，前端直接 innerHTML 进预览区，排版由前端 CSS 按主题负责。
///
/// 之前返回的是带 `<style>body{…}</style>` 的完整文档：塞进 innerHTML 后那段样式
/// 作用于整个页面，浅色主题下正文变成白底白字。
///
/// 结果会进 innerHTML，所以不能信任 markdown 里的内容：原始 HTML 一律当文本显示
/// （否则 `<img onerror>` 这类写法会在本站源下执行），链接和图片只放行相对地址与
/// http/https/mailto。
fn render_markdown(input: &str) -> String {
    use pulldown_cmark::{Event, Options, Parser, Tag};

    let options =
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let parser = Parser::new_ext(input, options).map(|event| match event {
        Event::Html(raw) | Event::InlineHtml(raw) => Event::Text(raw),
        Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        }) => Event::Start(Tag::Link {
            link_type,
            dest_url: safe_url(dest_url),
            title,
            id,
        }),
        Event::Start(Tag::Image {
            link_type,
            dest_url,
            title,
            id,
        }) => Event::Start(Tag::Image {
            link_type,
            dest_url: safe_url(dest_url),
            title,
            id,
        }),
        other => other,
    });
    let mut html = String::with_capacity(input.len() * 2);
    pulldown_cmark::html::push_html(&mut html, parser);
    html
}

/// 链接地址白名单：相对地址、http、https、mailto；其余（javascript:、data: 等）置空。
///
/// 判断 scheme 前先去掉空白和控制字符：浏览器解析 URL 时会忽略它们，
/// `java\tscript:` 照样能执行。
fn safe_url(url: pulldown_cmark::CowStr<'_>) -> pulldown_cmark::CowStr<'_> {
    let normalized: String = url
        .chars()
        .filter(|c| !c.is_whitespace() && !c.is_control())
        .collect::<String>()
        .to_ascii_lowercase();
    let scheme_end = normalized.find([':', '/', '?', '#']);
    let has_scheme = scheme_end.is_some_and(|i| normalized[i..].starts_with(':'));
    let allowed = !has_scheme
        || ["http:", "https:", "mailto:"]
            .iter()
            .any(|scheme| normalized.starts_with(scheme));
    if allowed {
        url
    } else {
        pulldown_cmark::CowStr::Borrowed("")
    }
}

fn preview_type(mime: &str) -> &'static str {
    if mime.starts_with("image/") {
        "image"
    } else if mime.starts_with("video/") {
        "video"
    } else if mime.starts_with("audio/") {
        "audio"
    } else if mime == "application/pdf" {
        "pdf"
    } else if mime.starts_with("text/") {
        "text"
    } else {
        "unknown"
    }
}

fn is_code_file(path: &std::path::Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    matches!(
        ext,
        "rs" | "go" | "js" | "ts" | "py" | "java" | "c" | "cpp"
            | "h" | "hpp" | "css" | "html" | "json" | "yaml"
            | "yml" | "toml" | "xml" | "sh" | "bash" | "zsh"
            | "fish" | "sql" | "rb" | "php" | "swift" | "kt"
            | "scala" | "lua" | "r" | "m" | "vue" | "svelte"
            | "jsx" | "tsx"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_is_a_fragment_without_global_styles() {
        let html =
            render_markdown("# 标题\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n- [x] done\n\n~~old~~");
        assert!(!html.contains("<style"), "{html}");
        assert!(!html.contains("<!DOCTYPE"), "{html}");
        assert!(html.contains("<h1>标题</h1>"), "{html}");
        assert!(html.contains("<table>"), "{html}");
        assert!(html.contains("checkbox"), "{html}");
        assert!(html.contains("<del>old</del>"), "{html}");
    }

    #[test]
    fn raw_html_is_rendered_as_text() {
        let html = render_markdown("<script>alert(1)</script>\n\nhi <img src=x onerror=alert(1)>");
        assert!(!html.contains("<script"), "{html}");
        assert!(!html.contains("<img"), "{html}");
        assert!(html.contains("&lt;script&gt;"), "{html}");
    }

    /// 取出所有 href / src 属性值
    fn url_attrs(html: &str) -> Vec<String> {
        let mut urls = Vec::new();
        for attr in ["href=\"", "src=\""] {
            let mut rest = html;
            while let Some(start) = rest.find(attr) {
                rest = &rest[start + attr.len()..];
                let end = rest.find('"').unwrap_or(rest.len());
                urls.push(rest[..end].to_string());
                rest = &rest[end..];
            }
        }
        urls
    }

    #[test]
    fn dangerous_link_schemes_are_dropped() {
        for md in [
            "[x](javascript:alert(1))",
            "[x](JavaScript:alert(1))",
            "[x](<java\tscript:alert(1)>)",
            "[x](data:text/html;base64,PHNjcmlwdD4=)",
            "![x](javascript:alert(1))",
            "[x]: javascript:alert(1)\n\n[x]",
        ] {
            let html = render_markdown(md);
            let urls = url_attrs(&html);
            assert!(!urls.is_empty(), "{md} 应当渲染成链接: {html}");
            assert!(urls.iter().all(|u| u.is_empty()), "{md} -> {html}");
        }
        for (md, expect) in [
            (
                "[x](https://example.com/a?b#c)",
                "https://example.com/a?b#c",
            ),
            ("[x](mailto:a@b.c)", "mailto:a@b.c"),
            ("[x](docs/readme.md)", "docs/readme.md"),
            ("[x](#anchor)", "#anchor"),
            ("[x](./a:b.md)", "./a:b.md"),
        ] {
            assert_eq!(
                url_attrs(&render_markdown(md)),
                vec![expect.to_string()],
                "{md}"
            );
        }
    }

    #[test]
    fn scheme_check_ignores_whitespace_and_control_chars_like_browsers_do() {
        for url in [
            "java\tscript:alert(1)",
            " javascript:alert(1)",
            "JAVA\nSCRIPT:x",
            "vbscript:x",
        ] {
            assert_eq!(&*safe_url(url.into()), "", "{url:?}");
        }
    }
}
