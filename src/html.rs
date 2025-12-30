use anyhow::Result;
use hyper::{Body, Response};
use std::path::Path;

pub async fn handle_directory(path: &Path, request_path: &str) -> Result<Response<Body>> {
    let mut dir_entries = tokio::fs::read_dir(path).await?;
    let mut folders = Vec::new();
    let mut files = Vec::new();

    // 添加返回上级目录链接（如果不是根目录）
    if request_path != "/" {
        folders.push(format!(
            r#"<li class="entry folder parent-dir" onclick="window.location.href='{}'">
                <div class="entry-icon"><i class="fas fa-level-up-alt"></i></div>
                <div class="entry-info">
                    <span class="entry-name">..</span>
                    <span class="entry-meta">返回上级目录</span>
                </div>
            </li>"#,
            parent_path(request_path)
        ));
    }

    while let Some(entry) = dir_entries.next_entry().await? {
        let file_name = entry.file_name();
        let file_name_str = file_name.to_string_lossy();
        let metadata = entry.metadata().await?;
        let is_dir = metadata.is_dir();

        let display_name = file_name_str.to_string();
        let path = format!(
            "{}{}",
            request_path,
            if request_path.ends_with('/') { "" } else { "/" },
        ) + &file_name_str;

        let escaped_path = html_escape::encode_text(&path);
        let escaped_name = html_escape::encode_text(&display_name);

        if is_dir {
            folders.push(format!(
                r#"<li class="entry folder" onclick="window.location.href='{}'">
                    <div class="entry-icon"><i class="fas fa-folder"></i></div>
                    <div class="entry-info">
                        <span class="entry-name">{}</span>
                        <span class="entry-meta">文件夹</span>
                    </div>
                    <button class="delete-btn" onclick="event.stopPropagation(); deleteItem('{}', true)" title="删除文件夹">
                        <i class="fas fa-trash"></i>
                    </button>
                </li>"#,
                escaped_path, escaped_name, escaped_path
            ));
        } else {
            let size = metadata.len();
            let size_str = human_readable_size(size);
            let ext = get_file_extension(&file_name_str);
            let icon = get_file_icon(&ext);

            files.push(format!(
                r#"<li class="entry file" data-path="{}" data-size="{}" data-name="{}">
                    <div class="entry-icon"><i class="fas {}"></i></div>
                    <div class="entry-info">
                        <span class="entry-name" onclick="event.stopPropagation(); downloadFile('{}', '{}', {})">{}</span>
                        <span class="entry-meta">{}</span>
                    </div>
                    <div class="entry-actions">
                        <button class="action-btn download-btn" onclick="event.stopPropagation(); downloadFile('{}', '{}', {})" title="下载">
                            <i class="fas fa-download"></i>
                        </button>
                        <button class="action-btn delete-btn" onclick="event.stopPropagation(); deleteItem('{}', false)" title="删除">
                            <i class="fas fa-trash"></i>
                        </button>
                    </div>
                </li>"#,
                escaped_path,
                size,
                escaped_name,
                icon,
                escaped_path,
                escaped_name,
                size,
                escaped_name,
                size_str,
                escaped_path,
                escaped_name,
                size,
                escaped_path
            ));
        }
    }

    // 合并文件夹和文件列表
    let all_entries = [folders, files].concat().join("");

    let html = format!(
        r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>文件传输 - {}</title>
    <link rel="preconnect" href="https://fonts.googleapis.com">
    <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
    <link href="https://fonts.googleapis.com/css2?family=JetBrains+Mono:wght@400;500;600&family=Noto+Sans+SC:wght@400;500;600;700&display=swap" rel="stylesheet">
    <link rel="stylesheet" href="https://cdnjs.cloudflare.com/ajax/libs/font-awesome/6.4.0/css/all.min.css">
    <style>{css}</style>
</head>
<body>
    <div class="app">
        <aside class="sidebar">
            <div class="logo">
                <i class="fas fa-bolt"></i>
                <span>FileTransfer</span>
            </div>
            <nav class="nav-breadcrumb">
                {breadcrumb}
            </nav>
            <div class="stats">
                <div class="stat-item">
                    <i class="fas fa-folder"></i>
                    <span id="folder-count">0</span>
                    <label>文件夹</label>
                </div>
                <div class="stat-item">
                    <i class="fas fa-file"></i>
                    <span id="file-count">0</span>
                    <label>文件</label>
                </div>
            </div>
        </aside>
        
        <main class="content">
            <header class="header">
                <h1><i class="fas fa-folder-open"></i> {title}</h1>
                <div class="header-actions">
                    <button class="btn btn-secondary" onclick="location.reload()">
                        <i class="fas fa-sync-alt"></i> 刷新
                    </button>
                </div>
            </header>

            <section class="upload-zone" id="upload-zone">
                <div class="upload-content">
                    <div class="upload-icon">
                        <i class="fas fa-cloud-upload-alt"></i>
                    </div>
                    <h3>拖拽文件到这里上传</h3>
                    <p>或者点击下方按钮选择文件</p>
                    <div class="upload-actions">
                        <label class="btn btn-primary" for="file-input">
                            <i class="fas fa-plus"></i> 选择文件
                        </label>
                        <label class="btn btn-secondary" for="folder-input">
                            <i class="fas fa-folder-plus"></i> 选择文件夹
                        </label>
                    </div>
                    <input type="file" id="file-input" multiple hidden>
                    <input type="file" id="folder-input" webkitdirectory mozdirectory hidden>
                </div>
            </section>

            <!-- 传输任务面板 -->
            <section class="transfer-panel" id="transfer-panel" style="display: none;">
                <div class="panel-header">
                    <div class="panel-tabs">
                        <button class="tab-btn active" data-tab="all" onclick="filterTasks('all')">
                            全部 <span class="tab-count" id="count-all">0</span>
                        </button>
                        <button class="tab-btn" data-tab="upload" onclick="filterTasks('upload')">
                            <i class="fas fa-arrow-up"></i> 上传 <span class="tab-count" id="count-upload">0</span>
                        </button>
                        <button class="tab-btn" data-tab="download" onclick="filterTasks('download')">
                            <i class="fas fa-arrow-down"></i> 下载 <span class="tab-count" id="count-download">0</span>
                        </button>
                    </div>
                    <div class="panel-actions">
                        <button class="btn btn-sm btn-secondary" onclick="clearCompletedTasks()">
                            <i class="fas fa-broom"></i> 清除已完成
                        </button>
                        <button class="btn btn-sm btn-danger" onclick="cancelAllTasks()">
                            <i class="fas fa-stop"></i> 取消全部
                        </button>
                    </div>
                </div>
                <div class="panel-progress">
                    <div class="total-progress-bar">
                        <div class="total-progress-fill" id="total-progress"></div>
                    </div>
                    <div class="progress-stats">
                        <span id="progress-text">0 / 0</span>
                        <span id="speed-text"></span>
                    </div>
                </div>
                <ul class="task-list" id="task-list"></ul>
            </section>

            <section class="file-list">
                <ul class="entries">{entries}</ul>
                <div class="empty-state" id="empty-state" style="display: none;">
                    <i class="fas fa-inbox"></i>
                    <p>此文件夹为空</p>
                </div>
            </section>
        </main>
    </div>

    <div class="toast-container" id="toast-container"></div>

    <script>{js}</script>
</body>
</html>"#,
        request_path,
        css = generate_css(),
        js = generate_js(),
        breadcrumb = generate_breadcrumbs(request_path),
        title = request_path,
        entries = all_entries,
    );

    Ok(Response::new(Body::from(html)))
}

fn generate_css() -> &'static str {
    r#"
:root {
    --bg-primary: #0a0e14;
    --bg-secondary: #11151c;
    --bg-tertiary: #1a1f2a;
    --bg-hover: #252b38;
    --border-color: #2a3140;
    --text-primary: #e6edf3;
    --text-secondary: #8b949e;
    --text-muted: #6e7681;
    --accent-blue: #58a6ff;
    --accent-green: #3fb950;
    --accent-yellow: #d29922;
    --accent-red: #f85149;
    --accent-purple: #a371f7;
    --accent-cyan: #39c5cf;
    --accent-orange: #f0883e;
    --gradient-primary: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
    --gradient-success: linear-gradient(135deg, #11998e 0%, #38ef7d 100%);
    --gradient-upload: linear-gradient(90deg, #667eea, #764ba2);
    --gradient-download: linear-gradient(90deg, #11998e, #38ef7d);
    --shadow: 0 8px 32px rgba(0,0,0,0.4);
    --radius: 12px;
    --radius-sm: 8px;
}

* {
    margin: 0;
    padding: 0;
    box-sizing: border-box;
}

body {
    font-family: 'Noto Sans SC', -apple-system, BlinkMacSystemFont, sans-serif;
    background: var(--bg-primary);
    color: var(--text-primary);
    line-height: 1.6;
    min-height: 100vh;
}

/* App Layout */
.app {
    display: flex;
    min-height: 100vh;
}

/* Sidebar */
.sidebar {
    width: 260px;
    background: var(--bg-secondary);
    border-right: 1px solid var(--border-color);
    padding: 24px;
    display: flex;
    flex-direction: column;
    position: sticky;
    top: 0;
    height: 100vh;
}

.logo {
    display: flex;
    align-items: center;
    gap: 12px;
    font-size: 1.4rem;
    font-weight: 700;
    color: var(--accent-cyan);
    margin-bottom: 32px;
    padding-bottom: 24px;
    border-bottom: 1px solid var(--border-color);
}

.logo i {
    font-size: 1.6rem;
    filter: drop-shadow(0 0 8px var(--accent-cyan));
}

.nav-breadcrumb {
    margin-bottom: 24px;
    font-size: 0.9rem;
}

.nav-breadcrumb a {
    color: var(--accent-blue);
    text-decoration: none;
    transition: color 0.2s;
}

.nav-breadcrumb a:hover {
    color: var(--text-primary);
}

.nav-breadcrumb span {
    color: var(--text-secondary);
}

.stats {
    margin-top: auto;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
}

.stat-item {
    background: var(--bg-tertiary);
    padding: 14px;
    border-radius: var(--radius-sm);
    text-align: center;
    border: 1px solid var(--border-color);
}

.stat-item i {
    font-size: 1.3rem;
    color: var(--accent-blue);
    margin-bottom: 6px;
    display: block;
}

.stat-item span {
    font-size: 1.6rem;
    font-weight: 700;
    font-family: 'JetBrains Mono', monospace;
    color: var(--text-primary);
    display: block;
}

.stat-item label {
    font-size: 0.75rem;
    color: var(--text-secondary);
}

/* Main Content */
.content {
    flex: 1;
    padding: 28px;
    overflow-y: auto;
}

.header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 24px;
    padding-bottom: 20px;
    border-bottom: 1px solid var(--border-color);
}

.header h1 {
    font-size: 1.4rem;
    font-weight: 600;
    display: flex;
    align-items: center;
    gap: 10px;
    color: var(--text-primary);
}

.header h1 i {
    color: var(--accent-yellow);
}

/* Buttons */
.btn {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 10px 18px;
    border-radius: var(--radius-sm);
    font-size: 0.9rem;
    font-weight: 500;
    cursor: pointer;
    border: none;
    transition: all 0.2s ease;
    font-family: inherit;
}

.btn-primary {
    background: var(--gradient-primary);
    color: white;
}

.btn-primary:hover {
    transform: translateY(-2px);
    box-shadow: 0 4px 20px rgba(102, 126, 234, 0.4);
}

.btn-secondary {
    background: var(--bg-tertiary);
    color: var(--text-primary);
    border: 1px solid var(--border-color);
}

.btn-secondary:hover {
    background: var(--bg-hover);
    border-color: var(--text-secondary);
}

.btn-danger {
    background: var(--accent-red);
    color: white;
}

.btn-danger:hover {
    filter: brightness(1.1);
}

.btn-sm {
    padding: 6px 12px;
    font-size: 0.8rem;
}

/* Upload Zone */
.upload-zone {
    background: linear-gradient(135deg, var(--bg-secondary), var(--bg-tertiary));
    border: 2px dashed var(--border-color);
    border-radius: var(--radius);
    padding: 40px;
    text-align: center;
    margin-bottom: 20px;
    transition: all 0.3s ease;
    position: relative;
    overflow: hidden;
}

.upload-zone::before {
    content: '';
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: radial-gradient(circle at center, rgba(102,126,234,0.08) 0%, transparent 70%);
    pointer-events: none;
}

.upload-zone.dragover {
    border-color: var(--accent-blue);
    background: linear-gradient(135deg, rgba(102,126,234,0.15), rgba(118,75,162,0.15));
    transform: scale(1.01);
}

.upload-content {
    position: relative;
    z-index: 1;
}

.upload-icon {
    margin-bottom: 16px;
}

.upload-icon i {
    font-size: 3.5rem;
    color: var(--text-secondary);
    transition: all 0.3s ease;
}

.upload-zone.dragover .upload-icon i {
    transform: scale(1.2);
    color: var(--accent-blue);
}

.upload-zone h3 {
    font-size: 1.2rem;
    margin-bottom: 6px;
    color: var(--text-primary);
}

.upload-zone p {
    color: var(--text-secondary);
    margin-bottom: 20px;
    font-size: 0.9rem;
}

.upload-actions {
    display: flex;
    gap: 12px;
    justify-content: center;
    flex-wrap: wrap;
}

/* Transfer Panel */
.transfer-panel {
    background: var(--bg-secondary);
    border-radius: var(--radius);
    border: 1px solid var(--border-color);
    margin-bottom: 20px;
    overflow: hidden;
    animation: slideDown 0.3s ease;
}

@keyframes slideDown {
    from { opacity: 0; transform: translateY(-20px); }
    to { opacity: 1; transform: translateY(0); }
}

.panel-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 14px 18px;
    border-bottom: 1px solid var(--border-color);
    background: var(--bg-tertiary);
    flex-wrap: wrap;
    gap: 12px;
}

.panel-tabs {
    display: flex;
    gap: 4px;
}

.tab-btn {
    background: transparent;
    border: none;
    color: var(--text-secondary);
    padding: 8px 14px;
    border-radius: 6px;
    cursor: pointer;
    font-size: 0.85rem;
    font-family: inherit;
    transition: all 0.2s;
    display: flex;
    align-items: center;
    gap: 6px;
}

.tab-btn:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
}

.tab-btn.active {
    background: var(--bg-primary);
    color: var(--text-primary);
}

.tab-count {
    background: var(--bg-hover);
    padding: 2px 8px;
    border-radius: 10px;
    font-size: 0.75rem;
    font-family: 'JetBrains Mono', monospace;
}

.panel-actions {
    display: flex;
    gap: 8px;
}

.panel-progress {
    padding: 12px 18px;
    border-bottom: 1px solid var(--border-color);
}

.total-progress-bar {
    height: 6px;
    background: var(--bg-primary);
    border-radius: 3px;
    overflow: hidden;
    margin-bottom: 8px;
}

.total-progress-fill {
    height: 100%;
    background: var(--gradient-primary);
    width: 0%;
    transition: width 0.3s ease;
    border-radius: 3px;
}

.progress-stats {
    display: flex;
    justify-content: space-between;
    font-size: 0.8rem;
    color: var(--text-secondary);
    font-family: 'JetBrains Mono', monospace;
}

.task-list {
    list-style: none;
    max-height: 320px;
    overflow-y: auto;
}

.task-item {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 14px 18px;
    border-bottom: 1px solid var(--border-color);
    transition: background 0.2s;
    animation: fadeIn 0.3s ease;
}

@keyframes fadeIn {
    from { opacity: 0; transform: translateX(-10px); }
    to { opacity: 1; transform: translateX(0); }
}

.task-item:last-child {
    border-bottom: none;
}

.task-item:hover {
    background: var(--bg-tertiary);
}

.task-item.hidden {
    display: none;
}

.task-icon {
    width: 38px;
    height: 38px;
    border-radius: 8px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 1rem;
    flex-shrink: 0;
}

.task-icon.upload {
    background: linear-gradient(135deg, rgba(102,126,234,0.2), rgba(118,75,162,0.2));
    color: #667eea;
}

.task-icon.download {
    background: linear-gradient(135deg, rgba(17,153,142,0.2), rgba(56,239,125,0.2));
    color: #38ef7d;
}

.task-info {
    flex: 1;
    min-width: 0;
}

.task-name {
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--text-primary);
    font-size: 0.9rem;
    margin-bottom: 4px;
}

.task-meta {
    font-size: 0.75rem;
    color: var(--text-secondary);
    font-family: 'JetBrains Mono', monospace;
    display: flex;
    gap: 12px;
}

.task-progress {
    width: 140px;
    flex-shrink: 0;
}

.progress-bar {
    height: 6px;
    background: var(--bg-primary);
    border-radius: 3px;
    overflow: hidden;
    margin-bottom: 4px;
}

.progress-fill {
    height: 100%;
    width: 0%;
    transition: width 0.2s ease;
    border-radius: 3px;
}

.progress-fill.upload {
    background: var(--gradient-upload);
}

.progress-fill.download {
    background: var(--gradient-download);
}

.progress-fill.success {
    background: var(--accent-green);
}

.progress-fill.error {
    background: var(--accent-red);
}

.progress-percent {
    font-size: 0.75rem;
    font-family: 'JetBrains Mono', monospace;
    color: var(--text-secondary);
    text-align: right;
}

.task-status {
    width: 32px;
    text-align: center;
    flex-shrink: 0;
}

.status-icon {
    font-size: 1rem;
}

.status-icon.pending { color: var(--text-muted); }
.status-icon.uploading { color: #667eea; }
.status-icon.downloading { color: #38ef7d; }
.status-icon.success { color: var(--accent-green); }
.status-icon.error { color: var(--accent-red); }

.task-cancel {
    background: transparent;
    border: none;
    color: var(--text-muted);
    cursor: pointer;
    padding: 6px;
    border-radius: 4px;
    transition: all 0.2s;
    opacity: 0;
}

.task-item:hover .task-cancel {
    opacity: 1;
}

.task-cancel:hover {
    background: rgba(248,81,73,0.1);
    color: var(--accent-red);
}

/* File List */
.file-list {
    background: var(--bg-secondary);
    border-radius: var(--radius);
    border: 1px solid var(--border-color);
    overflow: hidden;
}

.entries {
    list-style: none;
}

.entry {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 14px 18px;
    border-bottom: 1px solid var(--border-color);
    cursor: pointer;
    transition: all 0.2s ease;
}

.entry:last-child {
    border-bottom: none;
}

.entry:hover {
    background: var(--bg-tertiary);
}

.entry:hover .entry-actions {
    opacity: 1;
}

.entry-icon {
    width: 42px;
    height: 42px;
    border-radius: 10px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 1.1rem;
    flex-shrink: 0;
}

.folder .entry-icon {
    background: linear-gradient(135deg, rgba(139,148,158,0.15), rgba(139,148,158,0.08));
    color: var(--text-secondary);
}

.folder:hover .entry-icon {
    background: linear-gradient(135deg, rgba(88,166,255,0.2), rgba(163,113,247,0.2));
    color: var(--accent-blue);
}

.file .entry-icon {
    background: linear-gradient(135deg, rgba(88,166,255,0.15), rgba(88,166,255,0.08));
    color: var(--accent-blue);
}

.parent-dir .entry-icon {
    background: linear-gradient(135deg, rgba(210,153,34,0.2), rgba(210,153,34,0.1));
    color: var(--accent-yellow);
}

.entry-info {
    flex: 1;
    min-width: 0;
}

.entry-name {
    display: block;
    font-weight: 500;
    color: var(--text-primary);
    text-decoration: none;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    transition: color 0.2s;
    cursor: pointer;
}

.entry-name:hover {
    color: var(--accent-blue);
}

.entry-meta {
    font-size: 0.8rem;
    color: var(--text-secondary);
    font-family: 'JetBrains Mono', monospace;
}

.entry-actions {
    display: flex;
    gap: 6px;
    opacity: 0;
    transition: opacity 0.2s;
}

.action-btn {
    background: transparent;
    border: 1px solid var(--border-color);
    color: var(--text-secondary);
    padding: 8px 10px;
    border-radius: 6px;
    cursor: pointer;
    transition: all 0.2s ease;
}

.download-btn:hover {
    background: rgba(56,239,125,0.1);
    border-color: var(--accent-green);
    color: var(--accent-green);
}

.delete-btn:hover {
    background: rgba(248,81,73,0.1);
    border-color: var(--accent-red);
    color: var(--accent-red);
}

/* Empty State */
.empty-state {
    text-align: center;
    padding: 60px;
    color: var(--text-secondary);
}

.empty-state i {
    font-size: 3.5rem;
    margin-bottom: 14px;
    opacity: 0.5;
}

/* Toast Notifications */
.toast-container {
    position: fixed;
    bottom: 24px;
    right: 24px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    z-index: 1000;
}

.toast {
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    padding: 14px 18px;
    display: flex;
    align-items: center;
    gap: 12px;
    box-shadow: var(--shadow);
    animation: slideIn 0.3s ease;
    min-width: 280px;
}

@keyframes slideIn {
    from { opacity: 0; transform: translateX(100px); }
    to { opacity: 1; transform: translateX(0); }
}

.toast.success { border-left: 4px solid var(--accent-green); }
.toast.error { border-left: 4px solid var(--accent-red); }
.toast.info { border-left: 4px solid var(--accent-blue); }

.toast-icon { font-size: 1.1rem; }
.toast.success .toast-icon { color: var(--accent-green); }
.toast.error .toast-icon { color: var(--accent-red); }
.toast.info .toast-icon { color: var(--accent-blue); }

.toast-content { flex: 1; }
.toast-title { font-weight: 600; margin-bottom: 2px; font-size: 0.9rem; }
.toast-message { font-size: 0.8rem; color: var(--text-secondary); }

/* Scrollbar */
::-webkit-scrollbar { width: 6px; height: 6px; }
::-webkit-scrollbar-track { background: var(--bg-primary); }
::-webkit-scrollbar-thumb { background: var(--bg-hover); border-radius: 3px; }
::-webkit-scrollbar-thumb:hover { background: var(--text-muted); }

/* Responsive */
@media (max-width: 1024px) {
    .sidebar { display: none; }
    .content { padding: 16px; }
}

@media (max-width: 640px) {
    .upload-zone { padding: 28px 16px; }
    .upload-actions { flex-direction: column; }
    .btn { width: 100%; justify-content: center; }
    .panel-header { flex-direction: column; align-items: stretch; }
    .task-progress { width: 100px; }
}
"#
}

fn generate_js() -> &'static str {
    r#"
// ===== 配置 =====
const CONFIG = {
    CHUNK_SIZE: 5 * 1024 * 1024,           // 5MB 分块大小
    MAX_CONCURRENT_UPLOADS: 3,              // 最大并发上传数
    MAX_CONCURRENT_DOWNLOADS: 3,            // 最大并发下载数
    MAX_RETRIES: 3,                         // 最大重试次数
    RETRY_DELAY: 1000,                      // 重试延迟(ms)
    LARGE_FILE_THRESHOLD: 10 * 1024 * 1024  // 10MB 以上使用分块上传
};

// ===== 传输管理器 =====
class TransferManager {
    constructor() {
        this.tasks = [];
        this.activeUploads = 0;
        this.activeDownloads = 0;
        this.currentFilter = 'all';
    }

    addTask(type, file, path, size) {
        const id = Date.now() + '-' + Math.random().toString(36).substr(2, 9);
        const task = {
            id,
            type,               // 'upload' or 'download'
            name: file?.name || path.split('/').pop(),
            path,
            size: size || file?.size || 0,
            file,
            status: 'pending',  // pending, active, success, error
            progress: 0,
            loaded: 0,
            speed: 0,
            retries: 0,
            startTime: null,
            xhr: null,
            abortController: null
        };
        this.tasks.push(task);
        this.renderTask(task);
        this.showPanel();
        this.updateCounts();
        return task;
    }

    async processUploads() {
        while (true) {
            const pendingTask = this.tasks.find(t => t.type === 'upload' && t.status === 'pending');
            if (!pendingTask) break;
            
            if (this.activeUploads >= CONFIG.MAX_CONCURRENT_UPLOADS) {
                await this.sleep(100);
                continue;
            }
            
            this.uploadFile(pendingTask);
        }
    }

    async processDownloads() {
        while (true) {
            const pendingTask = this.tasks.find(t => t.type === 'download' && t.status === 'pending');
            if (!pendingTask) break;
            
            if (this.activeDownloads >= CONFIG.MAX_CONCURRENT_DOWNLOADS) {
                await this.sleep(100);
                continue;
            }
            
            this.downloadFile(pendingTask);
        }
    }

    async uploadFile(task) {
        this.activeUploads++;
        task.status = 'active';
        task.startTime = Date.now();
        this.updateTask(task);

        try {
            if (task.size > CONFIG.LARGE_FILE_THRESHOLD) {
                await this.uploadLargeFile(task);
            } else {
                await this.uploadSmallFile(task);
            }
            
            task.status = 'success';
            task.progress = 100;
            showToast('success', '上传成功', task.name);
        } catch (error) {
            if (error.message === 'cancelled') {
                task.status = 'error';
            } else if (task.retries < CONFIG.MAX_RETRIES) {
                task.retries++;
                task.status = 'pending';
                task.progress = 0;
                task.loaded = 0;
                await this.sleep(CONFIG.RETRY_DELAY);
            } else {
                task.status = 'error';
                showToast('error', '上传失败', `${task.name}: ${error.message}`);
            }
        }

        this.activeUploads--;
        this.updateTask(task);
        this.updateCounts();
        this.updateTotalProgress();
        
        // 继续处理队列
        if (task.status !== 'pending') {
            this.processUploads();
        }
    }

    async uploadSmallFile(task) {
        return new Promise((resolve, reject) => {
            const formData = new FormData();
            formData.append('file', task.file);

            const xhr = new XMLHttpRequest();
            task.xhr = xhr;
            xhr.open('POST', window.location.pathname);

            xhr.upload.onprogress = (e) => {
                if (e.lengthComputable) {
                    task.loaded = e.loaded;
                    task.progress = Math.round((e.loaded / e.total) * 100);
                    task.speed = this.calculateSpeed(task);
                    this.updateTask(task);
                    this.updateTotalProgress();
                }
            };

            xhr.onload = () => {
                if (xhr.status >= 200 && xhr.status < 300) {
                    resolve();
                } else {
                    reject(new Error(`HTTP ${xhr.status}`));
                }
            };

            xhr.onerror = () => reject(new Error('网络错误'));
            xhr.onabort = () => reject(new Error('cancelled'));
            xhr.send(formData);
        });
    }

    async uploadLargeFile(task) {
        const file = task.file;
        const fileId = this.generateId();
        const totalChunks = Math.ceil(file.size / CONFIG.CHUNK_SIZE);
        let uploadedBytes = 0;

        for (let i = 0; i < totalChunks; i++) {
            if (task.status === 'error') {
                throw new Error('cancelled');
            }

            const start = i * CONFIG.CHUNK_SIZE;
            const end = Math.min(start + CONFIG.CHUNK_SIZE, file.size);
            const chunk = file.slice(start, end);

            task.abortController = new AbortController();

            await fetch(window.location.pathname, {
                method: 'POST',
                headers: {
                    'X-Chunk-Upload': 'true',
                    'X-File-Id': fileId,
                    'X-Filename': encodeURIComponent(file.name),
                    'X-Chunk-Index': i.toString(),
                    'X-Total-Chunks': totalChunks.toString(),
                    'X-Total-Size': file.size.toString(),
                    'X-Chunk-Start': start.toString()
                },
                body: chunk,
                signal: task.abortController.signal
            });

            uploadedBytes += chunk.size;
            task.loaded = uploadedBytes;
            task.progress = Math.round((uploadedBytes / file.size) * 100);
            task.speed = this.calculateSpeed(task);
            this.updateTask(task);
            this.updateTotalProgress();
        }
    }

    async downloadFile(task) {
        this.activeDownloads++;
        task.status = 'active';
        task.startTime = Date.now();
        this.updateTask(task);

        try {
            await this.downloadWithProgress(task);
            task.status = 'success';
            task.progress = 100;
            showToast('success', '下载成功', task.name);
        } catch (error) {
            if (error.message === 'cancelled') {
                task.status = 'error';
            } else if (task.retries < CONFIG.MAX_RETRIES) {
                task.retries++;
                task.status = 'pending';
                task.progress = 0;
                task.loaded = 0;
                await this.sleep(CONFIG.RETRY_DELAY);
            } else {
                task.status = 'error';
                showToast('error', '下载失败', `${task.name}: ${error.message}`);
            }
        }

        this.activeDownloads--;
        this.updateTask(task);
        this.updateCounts();
        this.updateTotalProgress();
        
        if (task.status !== 'pending') {
            this.processDownloads();
        }
    }

    async downloadWithProgress(task) {
        return new Promise((resolve, reject) => {
            const xhr = new XMLHttpRequest();
            task.xhr = xhr;
            xhr.open('GET', task.path, true);
            xhr.responseType = 'blob';

            xhr.onprogress = (e) => {
                if (e.lengthComputable) {
                    task.loaded = e.loaded;
                    task.size = e.total;
                    task.progress = Math.round((e.loaded / e.total) * 100);
                    task.speed = this.calculateSpeed(task);
                    this.updateTask(task);
                    this.updateTotalProgress();
                }
            };

            xhr.onload = () => {
                if (xhr.status >= 200 && xhr.status < 300) {
                    // 触发下载
                    const blob = xhr.response;
                    const url = URL.createObjectURL(blob);
                    const a = document.createElement('a');
                    a.href = url;
                    a.download = task.name;
                    document.body.appendChild(a);
                    a.click();
                    document.body.removeChild(a);
                    URL.revokeObjectURL(url);
                    resolve();
                } else {
                    reject(new Error(`HTTP ${xhr.status}`));
                }
            };

            xhr.onerror = () => reject(new Error('网络错误'));
            xhr.onabort = () => reject(new Error('cancelled'));
            xhr.send();
        });
    }

    cancelTask(id) {
        const task = this.tasks.find(t => t.id === id);
        if (task) {
            if (task.xhr) {
                task.xhr.abort();
            }
            if (task.abortController) {
                task.abortController.abort();
            }
            task.status = 'error';
            this.updateTask(task);
            this.updateCounts();
        }
    }

    cancelAll() {
        this.tasks.forEach(task => {
            if (task.status === 'pending' || task.status === 'active') {
                this.cancelTask(task.id);
            }
        });
    }

    clearCompleted() {
        this.tasks = this.tasks.filter(t => t.status !== 'success');
        this.renderAllTasks();
        this.updateCounts();
        if (this.tasks.length === 0) {
            this.hidePanel();
        }
    }

    calculateSpeed(task) {
        if (!task.startTime) return 0;
        const elapsed = (Date.now() - task.startTime) / 1000;
        return elapsed > 0 ? task.loaded / elapsed : 0;
    }

    renderTask(task) {
        const taskList = document.getElementById('task-list');
        const icon = task.type === 'upload' ? 'fa-arrow-up' : 'fa-arrow-down';
        const progressClass = task.type;
        
        const li = document.createElement('li');
        li.className = `task-item ${task.type}`;
        li.id = `task-${task.id}`;
        li.dataset.type = task.type;
        li.innerHTML = `
            <div class="task-icon ${task.type}"><i class="fas ${icon}"></i></div>
            <div class="task-info">
                <div class="task-name">${escapeHtml(task.name)}</div>
                <div class="task-meta">
                    <span class="task-size">${formatSize(task.size)}</span>
                    <span class="task-speed"></span>
                </div>
            </div>
            <div class="task-progress">
                <div class="progress-bar">
                    <div class="progress-fill ${progressClass}" style="width: 0%"></div>
                </div>
                <div class="progress-percent">0%</div>
            </div>
            <div class="task-status">
                <i class="status-icon fas fa-clock pending"></i>
            </div>
            <button class="task-cancel" onclick="transferManager.cancelTask('${task.id}')" title="取消">
                <i class="fas fa-times"></i>
            </button>
        `;
        taskList.appendChild(li);
        this.applyFilter();
    }

    updateTask(task) {
        const li = document.getElementById(`task-${task.id}`);
        if (!li) return;

        const progressFill = li.querySelector('.progress-fill');
        const progressPercent = li.querySelector('.progress-percent');
        const statusIcon = li.querySelector('.status-icon');
        const speedSpan = li.querySelector('.task-speed');

        progressFill.style.width = task.progress + '%';
        progressPercent.textContent = task.progress + '%';
        
        // 更新速度显示
        if (task.speed > 0 && task.status === 'active') {
            speedSpan.textContent = formatSize(task.speed) + '/s';
        } else {
            speedSpan.textContent = '';
        }

        // 更新状态图标
        statusIcon.className = 'status-icon fas';
        progressFill.classList.remove('success', 'error');
        
        switch (task.status) {
            case 'pending':
                statusIcon.classList.add('fa-clock', 'pending');
                break;
            case 'active':
                statusIcon.classList.add('fa-spinner', 'fa-spin', task.type === 'upload' ? 'uploading' : 'downloading');
                break;
            case 'success':
                statusIcon.classList.add('fa-check-circle', 'success');
                progressFill.classList.add('success');
                break;
            case 'error':
                statusIcon.classList.add('fa-times-circle', 'error');
                progressFill.classList.add('error');
                break;
        }
    }

    renderAllTasks() {
        const taskList = document.getElementById('task-list');
        taskList.innerHTML = '';
        this.tasks.forEach(task => this.renderTask(task));
    }

    updateCounts() {
        const all = this.tasks.length;
        const uploads = this.tasks.filter(t => t.type === 'upload').length;
        const downloads = this.tasks.filter(t => t.type === 'download').length;
        
        document.getElementById('count-all').textContent = all;
        document.getElementById('count-upload').textContent = uploads;
        document.getElementById('count-download').textContent = downloads;
    }

    updateTotalProgress() {
        const activeTasks = this.tasks.filter(t => t.status !== 'success' && t.status !== 'error');
        const completedTasks = this.tasks.filter(t => t.status === 'success');
        
        let totalProgress = 0;
        if (this.tasks.length > 0) {
            totalProgress = this.tasks.reduce((sum, t) => sum + t.progress, 0) / this.tasks.length;
        }
        
        document.getElementById('total-progress').style.width = totalProgress + '%';
        document.getElementById('progress-text').textContent = `${completedTasks.length} / ${this.tasks.length} 完成`;
        
        // 计算总速度
        const totalSpeed = activeTasks.reduce((sum, t) => sum + (t.speed || 0), 0);
        document.getElementById('speed-text').textContent = totalSpeed > 0 ? formatSize(totalSpeed) + '/s' : '';
    }

    applyFilter() {
        const filter = this.currentFilter;
        document.querySelectorAll('.task-item').forEach(item => {
            if (filter === 'all' || item.dataset.type === filter) {
                item.classList.remove('hidden');
            } else {
                item.classList.add('hidden');
            }
        });
    }

    showPanel() {
        document.getElementById('transfer-panel').style.display = 'block';
    }

    hidePanel() {
        document.getElementById('transfer-panel').style.display = 'none';
    }

    generateId() {
        return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, c => {
            const r = Math.random() * 16 | 0;
            const v = c === 'x' ? r : (r & 0x3 | 0x8);
            return v.toString(16);
        });
    }

    sleep(ms) {
        return new Promise(resolve => setTimeout(resolve, ms));
    }
}

// 全局传输管理器
const transferManager = new TransferManager();

// ===== 页面初始化 =====
document.addEventListener('DOMContentLoaded', () => {
    initializeUpload();
    updateStats();
    checkEmptyState();
});

function initializeUpload() {
    const uploadZone = document.getElementById('upload-zone');
    const fileInput = document.getElementById('file-input');
    const folderInput = document.getElementById('folder-input');

    // 拖拽事件
    uploadZone.addEventListener('dragover', (e) => {
        e.preventDefault();
        uploadZone.classList.add('dragover');
    });

    uploadZone.addEventListener('dragleave', (e) => {
        e.preventDefault();
        uploadZone.classList.remove('dragover');
    });

    uploadZone.addEventListener('drop', async (e) => {
        e.preventDefault();
        uploadZone.classList.remove('dragover');
        
        const items = e.dataTransfer.items;
        const files = [];
        
        for (const item of items) {
            if (item.kind === 'file') {
                const entry = item.webkitGetAsEntry();
                if (entry) {
                    await traverseEntry(entry, files);
                }
            }
        }
        
        if (files.length > 0) {
            startUpload(files);
        }
    });

    fileInput.addEventListener('change', (e) => {
        const files = Array.from(e.target.files);
        if (files.length > 0) {
            startUpload(files);
            e.target.value = '';
        }
    });

    folderInput.addEventListener('change', (e) => {
        const files = Array.from(e.target.files);
        if (files.length > 0) {
            startUpload(files);
            e.target.value = '';
        }
    });
}

async function traverseEntry(entry, files, path = '') {
    if (entry.isFile) {
        const file = await new Promise(resolve => entry.file(resolve));
        files.push(file);
    } else if (entry.isDirectory) {
        const reader = entry.createReader();
        const entries = await new Promise(resolve => reader.readEntries(resolve));
        for (const e of entries) {
            await traverseEntry(e, files, path + entry.name + '/');
        }
    }
}

function startUpload(files) {
    files.forEach(file => {
        transferManager.addTask('upload', file, window.location.pathname, file.size);
    });
    transferManager.processUploads().then(() => {
        const completed = transferManager.tasks.filter(t => t.type === 'upload' && t.status === 'success').length;
        const total = transferManager.tasks.filter(t => t.type === 'upload').length;
        if (completed === total && total > 0) {
            setTimeout(() => location.reload(), 1500);
        }
    });
}

function downloadFile(path, name, size) {
    transferManager.addTask('download', null, path, size);
    transferManager.processDownloads();
}

function filterTasks(filter) {
    transferManager.currentFilter = filter;
    document.querySelectorAll('.tab-btn').forEach(btn => {
        btn.classList.toggle('active', btn.dataset.tab === filter);
    });
    transferManager.applyFilter();
}

function cancelAllTasks() {
    if (confirm('确定要取消所有传输任务吗？')) {
        transferManager.cancelAll();
        showToast('info', '已取消', '所有传输任务已取消');
    }
}

function clearCompletedTasks() {
    transferManager.clearCompleted();
}

// ===== 文件操作 =====
async function deleteItem(path, isDir) {
    const type = isDir ? '文件夹' : '文件';
    if (!confirm(`确定要删除这个${type}吗？此操作不可恢复。`)) {
        return;
    }

    try {
        const response = await fetch(path, { method: 'DELETE' });
        if (response.ok) {
            showToast('success', '删除成功', `${type}已删除`);
            const entry = document.querySelector(`[data-path="${path}"]`) ||
                          document.querySelector(`[onclick*="${path}"]`);
            if (entry) {
                entry.style.animation = 'fadeOut 0.3s ease forwards';
                setTimeout(() => entry.remove(), 300);
            }
            setTimeout(() => location.reload(), 500);
        } else {
            showToast('error', '删除失败', await response.text());
        }
    } catch (error) {
        showToast('error', '请求失败', error.message);
    }
}

// ===== 辅助函数 =====
function showToast(type, title, message) {
    const container = document.getElementById('toast-container');
    const icons = {
        success: 'fa-check-circle',
        error: 'fa-times-circle',
        info: 'fa-info-circle'
    };

    const toast = document.createElement('div');
    toast.className = `toast ${type}`;
    toast.innerHTML = `
        <div class="toast-icon"><i class="fas ${icons[type]}"></i></div>
        <div class="toast-content">
            <div class="toast-title">${escapeHtml(title)}</div>
            <div class="toast-message">${escapeHtml(message)}</div>
        </div>
    `;

    container.appendChild(toast);
    setTimeout(() => {
        toast.style.animation = 'slideIn 0.3s ease reverse forwards';
        setTimeout(() => toast.remove(), 300);
    }, 4000);
}

function updateStats() {
    const entries = document.querySelectorAll('.entries .entry');
    let folderCount = 0;
    let fileCount = 0;

    entries.forEach(entry => {
        if (entry.classList.contains('folder') && !entry.classList.contains('parent-dir')) {
            folderCount++;
        } else if (entry.classList.contains('file')) {
            fileCount++;
        }
    });

    document.getElementById('folder-count').textContent = folderCount;
    document.getElementById('file-count').textContent = fileCount;
}

function checkEmptyState() {
    const entries = document.querySelectorAll('.entries .entry:not(.parent-dir)');
    document.getElementById('empty-state').style.display = entries.length === 0 ? 'block' : 'none';
}

function formatSize(bytes) {
    if (bytes === 0) return '0 B';
    const units = ['B', 'KB', 'MB', 'GB', 'TB'];
    let i = 0;
    let size = bytes;
    while (size >= 1024 && i < units.length - 1) {
        size /= 1024;
        i++;
    }
    return size.toFixed(i > 0 ? 1 : 0) + ' ' + units[i];
}

function escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}

// CSS 动画
const style = document.createElement('style');
style.textContent = `
    @keyframes fadeOut {
        from { opacity: 1; transform: translateX(0); }
        to { opacity: 0; transform: translateX(20px); }
    }
`;
document.head.appendChild(style);
"#
}

fn generate_breadcrumbs(path: &str) -> String {
    let mut crumbs = vec![(String::from("/"), String::from("根目录"))];
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
                format!("<span>{}</span>", html_escape::encode_text(name))
            } else {
                format!(
                    "<a href='{}'>{}</a> / ",
                    html_escape::encode_text(link),
                    html_escape::encode_text(name)
                )
            }
        })
        .collect()
}

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

fn get_file_extension(filename: &str) -> String {
    filename.rsplit('.').next().unwrap_or("").to_lowercase()
}

fn get_file_icon(ext: &str) -> &'static str {
    match ext {
        "pdf" => "fa-file-pdf",
        "doc" | "docx" => "fa-file-word",
        "xls" | "xlsx" => "fa-file-excel",
        "ppt" | "pptx" => "fa-file-powerpoint",
        "zip" | "rar" | "7z" | "tar" | "gz" => "fa-file-archive",
        "jpg" | "jpeg" | "png" | "gif" | "svg" | "webp" | "bmp" => "fa-file-image",
        "mp3" | "wav" | "flac" | "aac" | "ogg" => "fa-file-audio",
        "mp4" | "avi" | "mkv" | "mov" | "wmv" | "flv" => "fa-file-video",
        "js" | "ts" | "py" | "java" | "c" | "cpp" | "rs" | "go" | "html" | "css" | "php" | "rb" => {
            "fa-file-code"
        }
        "txt" | "md" | "log" | "csv" => "fa-file-alt",
        "json" | "xml" | "yaml" | "yml" | "toml" => "fa-file-code",
        _ => "fa-file",
    }
}
