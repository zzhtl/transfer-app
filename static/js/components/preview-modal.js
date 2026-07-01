/**
 * 文件预览模态框
 * 支持：图片、视频、音频、文本、Markdown、PDF
 */

import { state, subscribe } from '../store.js';
import { closePreview, refresh } from '../actions.js';
import { previewUrl, downloadUrl, getContent, saveFile, renderMarkdown } from '../api.js';
import { showToast } from './toast.js';

let modalEl = null;
let contentEl = null;
let editing = false;
let autoEditNext = false;

export function initPreviewModal() {
    modalEl = document.getElementById('preview-modal');
    if (!modalEl) return;

    contentEl = modalEl.querySelector('.preview-content');

    // 关闭按钮
    modalEl.querySelector('.preview-close')?.addEventListener('click', closePreview);

    // 编辑按钮
    modalEl.querySelector('.preview-edit')?.addEventListener('click', enterEditMode);

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
    editing = false;
    const titleEl = modalEl.querySelector('.preview-title');
    if (titleEl) titleEl.textContent = file.name;

    const editBtn = modalEl.querySelector('.preview-edit');
    if (editBtn) editBtn.style.display = isEditable(file) ? '' : 'none';

    // 新建文件：直接进入编辑器，跳过只读渲染（避免竞态）
    if (autoEditNext) {
        autoEditNext = false;
        enterEditMode();
        return;
    }

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

function isEditable(file) {
    const mime = file.mime_type || guessMime(file.name);
    return mime.startsWith('text/') || isTextLike(file.name);
}

/** 进入编辑模式：拉取完整内容 → 渲染编辑器（markdown 带实时预览） */
async function enterEditMode() {
    const file = state.preview;
    if (!file) return;
    editing = true;
    const editBtn = modalEl.querySelector('.preview-edit');
    if (editBtn) editBtn.style.display = 'none';
    contentEl.innerHTML = '<div class="editor-loading">加载中...</div>';

    let text = '';
    try {
        text = await getContent(file.path);
    } catch (e) {
        showToast(`无法编辑: ${e.message}`, 'error');
        editing = false;
        render();
        return;
    }

    const isMd = /\.(md|markdown)$/i.test(file.name);
    contentEl.innerHTML = `
        <div class="editor-wrap">
            <div class="editor ${isMd ? 'editor-split' : ''}">
                <textarea class="editor-textarea" spellcheck="false"></textarea>
                ${isMd ? '<div class="editor-preview preview-markdown"></div>' : ''}
            </div>
            <div class="editor-actions">
                <span class="editor-status"></span>
                <button class="btn btn-ghost btn-sm editor-cancel">取消</button>
                <button class="btn btn-primary btn-sm editor-save">保存</button>
            </div>
        </div>`;

    const ta = contentEl.querySelector('.editor-textarea');
    ta.value = text;
    contentEl.querySelector('.editor-cancel').addEventListener('click', () => {
        editing = false;
        render();
    });
    contentEl.querySelector('.editor-save').addEventListener('click', doSave);

    if (isMd) {
        const preview = contentEl.querySelector('.editor-preview');
        let timer;
        const update = async () => {
            try { preview.innerHTML = await renderMarkdown(ta.value); } catch { /* ignore */ }
        };
        ta.addEventListener('input', () => {
            clearTimeout(timer);
            timer = setTimeout(update, 400);
        });
        update();
    }
    ta.focus();
}

async function doSave() {
    const file = state.preview;
    if (!file) return;
    const ta = contentEl.querySelector('.editor-textarea');
    const statusEl = contentEl.querySelector('.editor-status');
    const btn = contentEl.querySelector('.editor-save');
    btn.disabled = true;
    if (statusEl) statusEl.textContent = '保存中...';
    try {
        await saveFile(file.path, ta.value);
        if (statusEl) statusEl.textContent = '已保存';
        showToast('已保存', 'success');
        refresh();
    } catch (e) {
        if (statusEl) statusEl.textContent = '';
        showToast(`保存失败: ${e.message}`, 'error');
    } finally {
        btn.disabled = false;
    }
}

/** 直接以编辑模式打开文件（新建文件后立即编辑） */
export function openFileEditor(file) {
    autoEditNext = true;
    state.preview = file;
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
