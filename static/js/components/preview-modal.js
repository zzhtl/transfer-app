/**
 * 文件预览模态框
 * 支持：图片、视频、音频、文本、Markdown、PDF
 */

import { state, subscribe } from '../store.js';
import { closePreview } from '../actions.js';
import { previewUrl, downloadUrl } from '../api.js';

let modalEl = null;
let contentEl = null;

export function initPreviewModal() {
    modalEl = document.getElementById('preview-modal');
    if (!modalEl) return;

    contentEl = modalEl.querySelector('.preview-content');

    // 关闭按钮
    modalEl.querySelector('.preview-close')?.addEventListener('click', closePreview);

    // 背景点击关闭
    modalEl.addEventListener('click', (e) => {
        if (e.target === modalEl) closePreview();
    });

    // 下载按钮
    modalEl.querySelector('.preview-download')?.addEventListener('click', () => {
        if (state.preview) {
            const a = document.createElement('a');
            a.href = downloadUrl(state.preview.path, true);
            a.download = '';
            a.click();
        }
    });

    // ESC 关闭
    document.addEventListener('keydown', (e) => {
        if (e.key === 'Escape' && state.preview) closePreview();
    });

    subscribe('preview', render);
}

async function render() {
    const file = state.preview;
    if (!file) {
        modalEl.classList.remove('open');
        if (contentEl) contentEl.innerHTML = '';
        return;
    }

    modalEl.classList.add('open');
    const titleEl = modalEl.querySelector('.preview-title');
    if (titleEl) titleEl.textContent = file.name;

    const mime = file.mime_type || guessMime(file.name);
    const url = previewUrl(file.path);
    const rawUrl = downloadUrl(file.path, false);

    if (mime.startsWith('image/')) {
        contentEl.innerHTML = `<img src="${rawUrl}" alt="${escapeAttr(file.name)}" class="preview-image">`;
    } else if (mime.startsWith('video/')) {
        contentEl.innerHTML = `<video controls autoplay class="preview-video"><source src="${rawUrl}" type="${mime}"></video>`;
    } else if (mime.startsWith('audio/')) {
        contentEl.innerHTML = `<audio controls autoplay class="preview-audio"><source src="${rawUrl}" type="${mime}"></audio>`;
    } else if (mime === 'application/pdf') {
        contentEl.innerHTML = `<iframe src="${rawUrl}" class="preview-pdf"></iframe>`;
    } else if (mime.startsWith('text/') || isTextLike(file.name)) {
        // 文本/Markdown：服务端预览 API
        try {
            const resp = await fetch(url);
            if (!resp.ok) throw new Error(resp.statusText);
            const ct = resp.headers.get('content-type') || '';
            if (ct.includes('text/html')) {
                // Markdown 渲染结果
                const html = await resp.text();
                contentEl.innerHTML = `<div class="preview-markdown">${html}</div>`;
            } else {
                const text = await resp.text();
                contentEl.innerHTML = `<pre class="preview-text"><code>${escapeHtml(text)}</code></pre>`;
            }
        } catch {
            contentEl.innerHTML = `<div class="preview-error">预览加载失败</div>`;
        }
    } else {
        contentEl.innerHTML = `<div class="preview-unsupported">
            <p>此文件类型暂不支持预览</p>
            <button class="btn btn-primary preview-download-alt">下载文件</button>
        </div>`;
        contentEl.querySelector('.preview-download-alt')?.addEventListener('click', () => {
            const a = document.createElement('a');
            a.href = downloadUrl(file.path, true);
            a.download = '';
            a.click();
        });
    }
}

function guessMime(name) {
    const ext = name.split('.').pop()?.toLowerCase() || '';
    const map = {
        jpg: 'image/jpeg', jpeg: 'image/jpeg', png: 'image/png', gif: 'image/gif',
        webp: 'image/webp', svg: 'image/svg+xml', bmp: 'image/bmp',
        mp4: 'video/mp4', webm: 'video/webm', mkv: 'video/x-matroska', avi: 'video/x-msvideo',
        mp3: 'audio/mpeg', wav: 'audio/wav', ogg: 'audio/ogg', flac: 'audio/flac',
        pdf: 'application/pdf',
        txt: 'text/plain', md: 'text/markdown', json: 'text/plain',
        js: 'text/plain', ts: 'text/plain', rs: 'text/plain', go: 'text/plain',
        py: 'text/plain', java: 'text/plain', c: 'text/plain', cpp: 'text/plain',
        h: 'text/plain', css: 'text/plain', html: 'text/html', xml: 'text/xml',
        yaml: 'text/plain', yml: 'text/plain', toml: 'text/plain',
        sh: 'text/plain', bash: 'text/plain', zsh: 'text/plain',
        sql: 'text/plain', log: 'text/plain', csv: 'text/plain',
    };
    return map[ext] || 'application/octet-stream';
}

function isTextLike(name) {
    const ext = name.split('.').pop()?.toLowerCase() || '';
    const textExts = new Set([
        'txt', 'md', 'json', 'js', 'ts', 'rs', 'go', 'py', 'java', 'c', 'cpp',
        'h', 'css', 'html', 'xml', 'yaml', 'yml', 'toml', 'sh', 'bash', 'zsh',
        'sql', 'log', 'csv', 'ini', 'conf', 'cfg', 'env', 'gitignore', 'dockerfile',
        'makefile', 'cmake', 'gradle', 'properties', 'lock',
    ]);
    return textExts.has(ext);
}

function escapeHtml(text) {
    const d = document.createElement('div');
    d.textContent = text;
    return d.innerHTML;
}

function escapeAttr(text) {
    return text.replace(/"/g, '&quot;');
}
