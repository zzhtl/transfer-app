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

    // Markdown: 服务端渲染为 HTML
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

/// POST /api/preview/markdown — 渲染 markdown 片段（编辑实时预览用，不含文档包装/样式）
pub async fn render_md(axum::Json(req): axum::Json<MarkdownReq>) -> Response<Body> {
    let mut html = String::new();
    let parser = pulldown_cmark::Parser::new(&req.content);
    pulldown_cmark::html::push_html(&mut html, parser);
    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "text/html; charset=utf-8")
        .body(Body::from(html))
        .unwrap()
}

fn render_markdown(input: &str) -> String {
    let parser = pulldown_cmark::Parser::new(input);
    let mut html = String::with_capacity(input.len() * 2);
    html.push_str(
        r#"<!DOCTYPE html><html><head><meta charset="utf-8">
<style>body{font-family:system-ui;max-width:800px;margin:0 auto;padding:20px;color:#e6edf3;background:#0d1117}
pre{background:#161b22;padding:16px;border-radius:8px;overflow-x:auto}
code{font-family:'JetBrains Mono',monospace}
img{max-width:100%}a{color:#58a6ff}</style></head><body>"#,
    );
    pulldown_cmark::html::push_html(&mut html, parser);
    html.push_str("</body></html>");
    html
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
