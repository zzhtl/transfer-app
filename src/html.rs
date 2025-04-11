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
        let display_name = file_name_str.to_string();

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
                <div class="entry-container">
                    <a href="{}" class="entry-link">
                        <i class="fas fa-fw {}"></i>
                        <span class="name">{}</span>
                        <span class="size">{}</span>
                    </a>
                    <button class="delete-btn" onclick="deleteFile('{}')">
                        <i class="fas fa-trash-alt"></i>
                    </button>
                </div>
            </li>"#,
            if is_dir { "directory" } else { "file" },
            escaped_path,
            icon_class,
            escaped_name,
            human_readable_size(metadata.len()),
            escaped_path // 新增删除按钮参数
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

            <script>
                // 删除操作
                async function deleteFile(path) {{
                    if (!confirm('确定要永久删除该文件吗？')) return;

                    try {{
                        const response = await fetch(path, {{ method: 'DELETE' }});
                        if (response.ok) {{
                            location.reload();
                        }} else {{
                            alert('删除失败: ' + await response.text());
                        }}
                    }} catch (error) {{
                        alert('请求失败: ' + error.message);
                    }}
                }}

                // 修改后的上传逻辑
                document.addEventListener('DOMContentLoaded', () => {{
                    const form = document.querySelector('.upload-form');
                    const progressContainer = document.querySelector('.progress-container');

                    // 新增的文件名显示代码
                    const fileInput = document.getElementById('file-input');
                    const fileNames = document.getElementById('file-names');

                    fileInput.addEventListener('change', (e) => {{
                        const files = Array.from(e.target.files);
                        fileNames.textContent = files.length > 3
                            ? `${{files.length}}个文件已选择`
                            : files.map(f => f.name).join(', ');
                    }});

                    form.addEventListener('submit', async (e) => {{
                        e.preventDefault();

                        const formData = new FormData();
                        const files = document.getElementById('file-input').files;
                        const progressBar = document.querySelector('.progress-bar');
                        const progressText = document.querySelector('.progress-text');

                        // 显示进度条容器
                        progressContainer.style.display = 'block';
                        progressBar.style.width = '0%';
                        progressText.textContent = '0%';

                        for (const file of files) {{
                            formData.append('file', file);
                        }}

                        const xhr = new XMLHttpRequest();
                        xhr.open('POST', window.location.pathname);

                        xhr.upload.onprogress = (event) => {{
                            if (event.lengthComputable) {{
                                const percent = Math.round((event.loaded / event.total) * 100);
                                progressBar.style.width = `${{percent}}%`;
                                progressText.textContent = `${{percent}}%`;
                                if (percent === 100) {{
                                    progressText.textContent = '处理中...';
                                }}
                            }}
                        }};

                        xhr.onload = () => {{
                            if (xhr.status === 200) {{
                                progressBar.style.backgroundColor = '#2ecc71';
                                setTimeout(() => location.reload(), 500);
                            }}
                        }};

                        xhr.send(formData);
                    }});
                }});
            </script>
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
                <div class="upload-section">
                    <h2><i class="fas fa-upload"></i> 上传文件</h2>
                    <div class="progress-container">
                        <div class="progress-bar"></div>
                        <div class="progress-text">0%</div>
                    </div>
                    <form method="post" enctype="multipart/form-data" class="upload-form">
                        <div class="form-group">
                            <input display="false" type="file" name="file" id="file-input" multiple required>
                            <label for="file-input" class="file-label">
                                <i class="fas fa-folder-open"></i>
                                <span class="file-text">选择文件...</span>
                                <span class="file-names" id="file-names"></span>
                            </label>
                        </div>
                        <button type="submit" class="upload-button">
                            <i class="fas fa-cloud-upload-alt"></i> 开始上传
                        </button>
                    </form>
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
            * {
                margin: 0;
                padding: 0;
                box-sizing: border-box;
                font-family: 'Segoe UI', system-ui, -apple-system, sans-serif;
            }
            body {
                background: #f8fafc;
                color: #334155;
                line-height: 1.6;
            }
            .container {
                max-width: 1200px;
                margin: 2rem auto;
                padding: 2rem;
                background: white;
                border-radius: 12px;
                box-shadow: 0 4px 24px rgba(0,0,0,0.08);
            }

            /* 进度条样式修正 */
            .progress-container {
                margin: 1.5rem 0;
                background: #e2e8f0;
                height: 24px;
                border-radius: 12px;
                position: relative;
                overflow: hidden;
                display: none; /* 初始隐藏 */
            }
            .progress-bar {
                height: 100%;
                background: linear-gradient(90deg, #3b82f6 0%, #60a5fa 100%);
                transition: width 0.3s cubic-bezier(0.4, 0, 0.2, 1);
                width: 0%;
            }
            .progress-text {
                position: absolute;
                top: 50%;
                left: 50%;
                transform: translate(-50%, -50%);
                color: #ffffff;
                font-weight: 600;
                font-size: 0.85rem;
                text-shadow: 0 1px 2px rgba(0,0,0,0.1);
            }

            /* 文件列表优化 */
            .file-list ul {
                border: 1px solid #e2e8f0;
                border-radius: 8px;
                overflow: hidden;
                margin: 1.5rem 0;
            }
            .directory, .file {
                border-bottom: 1px solid #f1f5f9;
                transition: background 0.2s;
            }
            .directory:hover, .file:hover {
                background: #f8fafc;
            }
            .entry-container {
                display: flex;
                align-items: center;
                justify-content: space-between;
                padding: 1rem;
            }
            .entry-link {
                flex: 1;
                display: flex;
                align-items: center;
                text-decoration: none;
                color: inherit;
            }
            .entry-link i {
                width: 28px;
                font-size: 1.1rem;
            }
            .directory .entry-link i {
                color: #3b82f6;
            }
            .name {
                flex-grow: 1;
                margin: 0 1rem;
                font-weight: 500;
            }
            .size {
                color: #64748b;
                font-size: 0.9em;
                min-width: 80px;
                text-align: right;
            }

            /* 上传区域优化 */
            #file-input {
                display: none !important;
                opacity: 0;
                width: 0;
                height: 0;
                position: absolute;
            }
            /* 新增文件名称显示 */
            .file-names {
                color: #475569;
                font-size: 0.9rem;
                max-width: 300px;
                white-space: nowrap;
                overflow: hidden;
                text-overflow: ellipsis;
            }
            .upload-section {
                margin-top: 2.5rem;
                padding: 2rem;
                background: #f8fafc;
                border-radius: 12px;
                border: 2px dashed #cbd5e1;
                transition: border-color 0.3s;
            }
            .upload-section:hover {
                border-color: #94a3b8;
            }
            .upload-section h2 {
                color: #1e293b;
                margin-bottom: 1.5rem;
                font-size: 1.4rem;
                display: flex;
                align-items: center;
                gap: 0.75rem;
            }
            .file-label {
                background: white;
                border: 2px solid #3b82f6;
                color: #3b82f6;
                padding: 0.8rem 1.5rem;
                border-radius: 8px;
                cursor: pointer;
                transition: all 0.2s;
                display: inline-flex;
                align-items: center;
                gap: 0.5rem;
            }
            .file-label:hover {
                background: #3b82f6;
                color: white;
                transform: translateY(-1px);
            }
            .upload-button {
                background: #10b981;
                color: white;
                border: none;
                padding: 0.8rem 1.5rem;
                border-radius: 8px;
                cursor: pointer;
                transition: all 0.2s;
                display: inline-flex;
                align-items: center;
                gap: 0.5rem;
            }
            .upload-button:hover {
                background: #059669;
                transform: translateY(-1px);
            }

            /* 面包屑导航优化 */
            .breadcrumb {
                color: #64748b;
                font-size: 0.95em;
                display: flex;
                gap: 0.5rem;
                align-items: center;
            }
            .breadcrumb a {
                color: #3b82f6;
                text-decoration: none;
                transition: color 0.2s;
            }
            .breadcrumb a:hover {
                color: #2563eb;
                text-decoration: underline;
            }
            .breadcrumb span {
                color: #475569;
            }

            /* 删除按钮优化 */
            .delete-btn {
                background: #fee2e2;
                border: none;
                color: #dc2626;
                cursor: pointer;
                padding: 0.5rem 0.75rem;
                border-radius: 6px;
                transition: all 0.2s;
            }
            .delete-btn:hover {
                background: #fecaca;
                transform: scale(1.05);
            }

            @media (max-width: 768px) {
                .container {
                    margin: 1rem;
                    padding: 1.5rem;
                }
                .entry-container {
                    padding: 0.75rem;
                }
                .size {
                    display: none;
                }
                .upload-section {
                    padding: 1.5rem;
                }
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
