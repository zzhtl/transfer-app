use anyhow::Result;
use hyper::{Body, Response};
use std::path::Path;

pub async fn handle_directory(path: &Path, request_path: &str) -> Result<Response<Body>> {
    let mut dir_entries = tokio::fs::read_dir(path).await?;
    let mut entries = Vec::new();

    // 添加返回上级目录链接（如果不是根目录）
    if request_path != "/" {
        entries.push(format!(
            r#"<li class="directory">
                <a href="{}" class="entry">
                    <i class="fas fa-fw fa-level-up-alt"></i>
                    <span class="name">..（返回上级）</span>
                </a>
            </li>"#,
            parent_path(request_path)
        ));
    }

    while let Some(entry) = dir_entries.next_entry().await? {
        let file_name = entry.file_name();
        let file_name_str = file_name.to_string_lossy();
        let metadata = entry.metadata().await?;
        let is_dir = metadata.is_dir();

        // 美化文件名显示
        let display_name = file_name_str
            .replace('_', " ")
            .replace('-', " ")
            .replacen('.', "", 1);

        // 图标类型
        let icon_class = if is_dir { "fa-folder" } else { "fa-file" };

        let path = format!(
            "{}{}",
            request_path,
            if request_path.ends_with('/') { "" } else { "/" },
        ) + &file_name_str;

        let escaped_path = html_escape::encode_text(&path);
        let escaped_name = html_escape::encode_text(&display_name);

        entries.push(format!(
            r#"<li class="{}">
                <a href="{}" class="entry">
                    <i class="fas fa-fw {}"></i>
                    <span class="name">{}</span>
                    <span class="size">{}</span>
                </a>
            </li>"#,
            if is_dir { "directory" } else { "file" },
            escaped_path,
            icon_class,
            escaped_name,
            human_readable_size(metadata.len())
        ));
    }

    // 生成完整HTML
    let html = format!(
        r#"<!DOCTYPE html>
        <html lang="zh-CN">
        <head>
            <meta charset="UTF-8">
            <meta name="viewport" content="width=device-width, initial-scale=1.0">
            <title>目录列表 - {}</title>
            <link rel="stylesheet" href="https://cdnjs.cloudflare.com/ajax/libs/font-awesome/5.15.4/css/all.min.css">
            <style>
                {css}
            </style>
        </head>
        <body>
            <div class="container">
                <header>
                    <h1><i class="fas fa-folder-open"></i> {}</h1>
                    <nav class="breadcrumb">{}</nav>
                </header>
                <div class="file-list">
                    <ul>{}</ul>
                </div>
                <footer>
                    <p>由 Rust HTTP 服务器强力驱动</p>
                </footer>
            </div>
        </body>
        </html>"#,
        request_path,
        request_path,
        generate_breadcrumbs(request_path),
        entries.join(""),
        css = r#"
            * { margin: 0; padding: 0; box-sizing: border-box; }
            body {
                font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto,
                    "Helvetica Neue", Arial, sans-serif;
                line-height: 1.6;
                background: #f5f5f5;
                color: #333;
            }
            .container {
                max-width: 1000px;
                margin: 2rem auto;
                padding: 1rem;
                background: white;
                border-radius: 8px;
                box-shadow: 0 2px 8px rgba(0,0,0,0.1);
            }
            header {
                border-bottom: 1px solid #eee;
                padding-bottom: 1rem;
                margin-bottom: 1.5rem;
            }
            h1 {
                font-size: 1.8rem;
                color: #2c3e50;
                margin-bottom: 0.5rem;
            }
            h1 i { margin-right: 0.5rem; }
            .file-list ul { list-style: none; }
            .entry {
                display: flex;
                align-items: center;
                padding: 0.8rem 1rem;
                text-decoration: none;
                color: #34495e;
                border-radius: 4px;
                transition: all 0.2s;
            }
            .entry:hover {
                background: #f8f9fa;
                transform: translateX(4px);
            }
            .entry i {
                width: 1.5rem;
                color: #7f8c8d;
            }
            .directory .entry i { color: #3498db; }
            .name {
                flex: 1;
                margin: 0 1rem;
                word-break: break-all;
            }
            .size {
                color: #95a5a6;
                font-size: 0.9em;
                min-width: 80px;
                text-align: right;
            }
            .breadcrumb {
                color: #7f8c8d;
                font-size: 0.9em;
            }
            .breadcrumb a {
                color: #3498db;
                text-decoration: none;
            }
            .breadcrumb a:hover { text-decoration: underline; }
            footer {
                margin-top: 2rem;
                padding-top: 1rem;
                border-top: 1px solid #eee;
                text-align: center;
                color: #95a5a6;
                font-size: 0.9em;
            }
            @media (max-width: 600px) {
                .container { margin: 1rem; }
                .entry { padding: 0.6rem; }
                .size { display: none; }
            }
        "#
    );

    Ok(Response::new(Body::from(html)))
}

// 生成面包屑导航
fn generate_breadcrumbs(path: &str) -> String {
    let mut crumbs = vec![(String::from("/"), String::from("首页"))];
    let mut current = String::new();

    for part in path.split('/').filter(|p| !p.is_empty()) {
        current.push('/');
        current.push_str(part);
        crumbs.push((current.clone(), part.to_string()));
    }

    crumbs
        .iter()
        .enumerate()
        .map(|(i, (link, name))| {
            if i == crumbs.len() - 1 {
                format!("<span>{}</span>", name)
            } else {
                format!("<a href='{}'>{}</a> / ", link, name)
            }
        })
        .collect()
}

// 生成返回上级目录路径
fn parent_path(current_path: &str) -> String {
    let mut path = current_path
        .trim_end_matches('/')
        .rsplitn(2, '/')
        .nth(1)
        .unwrap_or("/")
        .to_string();

    if !path.starts_with('/') {
        path.insert(0, '/');
    }
    path
}

// 生成人类可读的文件大小
fn human_readable_size(size: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    let mut size = size as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    format!("{:.1} {}", size, UNITS[unit_index])
}
