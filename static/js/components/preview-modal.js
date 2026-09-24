/**
 * 文件预览模态框
 * 支持：图片、视频、音频、文本、Markdown、PDF；←/→ 切换同目录文件，文本可在线编辑
 */

import { state, subscribe } from '../store.js';
import { activeFiles, closePreview, refresh, downloadFile } from '../actions.js';
import { previewUrl, downloadUrl, getContent, saveFile, renderMarkdown, friendlyError } from '../api.js';
import { formatSize } from '../utils/format.js';
import { escapeHtml, icon } from '../utils/dom.js';
import { showToast } from './toast.js';
import { confirmDialog } from './dialog.js';

/** 服务端文本预览最多返回的字节数 */
const TEXT_PREVIEW_LIMIT = 1024 * 1024;

let modalEl = null;
let contentEl = null;
let titleEl = null;
let subEl = null;
let editBtn = null;
let prevBtn = null;
let nextBtn = null;

let editing = false;
let dirty = false;
let autoEditNext = false;
/** 每次渲染递增：切到下一个文件后，上一个文件迟到的响应直接丢弃 */
let renderSeq = 0;
let restoreFocus = null;

export function initPreviewModal() {
    modalEl = document.getElementById('preview-modal');
    if (!modalEl) return;

    contentEl = modalEl.querySelector('.preview-content');
    titleEl = modalEl.querySelector('.preview-title');
    subEl = modalEl.querySelector('.preview-sub');
    editBtn = modalEl.querySelector('.preview-edit');
    prevBtn = modalEl.querySelector('.preview-prev');
    nextBtn = modalEl.querySelector('.preview-next');

    modalEl.querySelector('.preview-close')?.addEventListener('click', requestClose);
    editBtn?.addEventListener('click', enterEditMode);
    prevBtn?.addEventListener('click', () => go(-1));
    nextBtn?.addEventListener('click', () => go(1));
    modalEl.querySelector('.preview-download')?.addEventListener('click', () => {
        if (state.preview) downloadFile(state.preview.path);
    });

    // 背景点击关闭
    modalEl.addEventListener('click', (e) => {
        if (e.target === modalEl) requestClose();
    });

    document.addEventListener('keydown', onKeydown);
    initSwipe();

    subscribe('preview', render);
}

/** 当前视图里可以预览的文件（跳过文件夹），顺序与列表一致 */
function siblings() {
    return activeFiles().filter(f => !f.is_dir);
}

function indexOf(list, file) {
    return file ? list.findIndex(f => f.path === file.path) : -1;
}

async function go(delta) {
    const list = siblings();
    const next = list[indexOf(list, state.preview) + delta];
    if (!next || !(await confirmDiscard())) return;
    state.preview = next;
}

async function requestClose() {
    if (await confirmDiscard()) closePreview();
}

/** 编辑中有未保存的修改时先确认 */
async function confirmDiscard() {
    if (!editing || !dirty) return true;
    return confirmDialog({
        title: '放弃未保存的修改？',
        message: '这次编辑的内容还没有保存。',
        confirmLabel: '放弃修改',
        danger: true,
    });
}

function onKeydown(e) {
    if (!state.preview || e.defaultPrevented) return;
    const mod = e.ctrlKey || e.metaKey;
    if (mod && e.key.toLowerCase() === 's') {
        e.preventDefault();
        if (editing) doSave();
        return;
    }
    if (e.key === 'Escape') {
        e.preventDefault();
        requestClose();
        return;
    }
    // 编辑时方向键留给光标
    if (editing || e.target.closest?.('textarea, input, video, audio')) return;
    if (e.key === 'ArrowLeft') {
        e.preventDefault();
        go(-1);
    } else if (e.key === 'ArrowRight') {
        e.preventDefault();
        go(1);
    }
}

/** 图片预览上左右滑动切换 */
function initSwipe() {
    let start = null;
    contentEl.addEventListener('pointerdown', (e) => {
        start = e.isPrimary && contentEl.querySelector('.preview-image') ? { x: e.clientX, y: e.clientY } : null;
    });
    contentEl.addEventListener('pointerup', (e) => {
        if (!start) return;
        const dx = e.clientX - start.x;
        const dy = e.clientY - start.y;
        start = null;
        if (Math.abs(dx) > 50 && Math.abs(dx) > Math.abs(dy) * 1.5) go(dx < 0 ? 1 : -1);
    });
    contentEl.addEventListener('pointercancel', () => {
        start = null;
    });
}

function renderHeader(file) {
    titleEl.textContent = file.name;
    titleEl.title = file.name;
    const list = siblings();
    const index = indexOf(list, file);
    const parts = [];
    if (index >= 0 && list.length > 1) parts.push(`${index + 1} / ${list.length}`);
    if (file.size != null) parts.push(formatSize(file.size));
    subEl.textContent = parts.join(' · ');
    prevBtn.hidden = !(index > 0);
    nextBtn.hidden = !(index >= 0 && index < list.length - 1);

    // 预加载相邻的图片，左右切换时基本不用等
    for (const neighbor of [list[index - 1], list[index + 1]]) {
        if (neighbor && previewKind(neighbor) === 'image') new Image().src = downloadUrl(neighbor.path, false);
    }
}

async function render() {
    const file = state.preview;
    const seq = ++renderSeq;
    editing = false;
    dirty = false;

    if (!file) {
        modalEl.classList.remove('open');
        contentEl.innerHTML = '';
        restoreFocus?.focus?.({ preventScroll: true });
        restoreFocus = null;
        return;
    }

    if (!modalEl.classList.contains('open')) {
        restoreFocus = document.activeElement;
        modalEl.classList.add('open');
        modalEl.querySelector('.preview-close')?.focus({ preventScroll: true });
    }
    renderHeader(file);
    editBtn.hidden = !isEditable(file);

    // 新建文件：直接进入编辑器
    if (autoEditNext) {
        autoEditNext = false;
        enterEditMode();
        return;
    }

    const kind = previewKind(file);
    const rawUrl = downloadUrl(file.path, false);
    const alt = escapeHtml(file.name);

    if (kind === 'image') {
        contentEl.innerHTML = `<div class="preview-spinner spinner"></div><img src="${rawUrl}" alt="${alt}" class="preview-image" draggable="false">`;
        watchMedia(contentEl.querySelector('img'), 'load');
    } else if (kind === 'video') {
        contentEl.innerHTML = `<div class="preview-spinner spinner"></div><video src="${rawUrl}" controls autoplay playsinline class="preview-video"></video>`;
        watchMedia(contentEl.querySelector('video'), 'loadeddata');
    } else if (kind === 'audio') {
        contentEl.innerHTML = `<div class="preview-audio-wrap">${icon('music', 'preview-audio-icon')}<audio src="${rawUrl}" controls autoplay class="preview-audio"></audio></div>`;
    } else if (kind === 'pdf') {
        contentEl.innerHTML = `<iframe src="${rawUrl}" class="preview-pdf" title="${alt}"></iframe>`;
    } else if (kind === 'text' || kind === 'markdown') {
        contentEl.innerHTML = '<div class="preview-spinner spinner"></div>';
        try {
            const resp = await fetch(previewUrl(file.path), { credentials: 'same-origin' });
            if (!resp.ok) throw new Error(resp.statusText);
            const isHtml = (resp.headers.get('content-type') || '').includes('text/html');
            const body = await resp.text();
            if (seq !== renderSeq) return;
            const truncated = file.size > TEXT_PREVIEW_LIMIT
                ? `<div class="preview-truncated">文件较大，只显示前 1 MB。${isEditable(file) ? '' : '完整内容请下载查看。'}</div>`
                : '';
            contentEl.innerHTML = isHtml
                ? `${truncated}<div class="preview-markdown">${body}</div>`
                : `${truncated}<pre class="preview-text"><code>${escapeHtml(body)}</code></pre>`;
        } catch {
            if (seq !== renderSeq) return;
            contentEl.innerHTML = '<div class="preview-error">预览加载失败</div>';
        }
    } else {
        contentEl.innerHTML = `<div class="preview-unsupported">
            ${icon('file', 'preview-unsupported-icon')}
            <p>这种文件暂时没法在线预览</p>
            <button class="btn btn-primary preview-download-alt">${icon('download')}下载文件</button>
        </div>`;
        contentEl.querySelector('.preview-download-alt')?.addEventListener('click', () => downloadFile(file.path));
    }
}

/** 媒体加载完成前显示 spinner，失败时给出提示 */
function watchMedia(el, readyEvent) {
    const done = () => contentEl.querySelector('.preview-spinner')?.remove();
    el.addEventListener(readyEvent, done, { once: true });
    el.addEventListener('error', () => {
        done();
        el.insertAdjacentHTML('afterend', '<div class="preview-error">无法加载这个文件，浏览器可能不支持这种格式</div>');
        el.remove();
    }, { once: true });
}

function previewKind(file) {
    const mime = file.mime_type || guessMime(file.name);
    if (mime.startsWith('image/')) return 'image';
    if (mime.startsWith('video/')) return 'video';
    if (mime.startsWith('audio/')) return 'audio';
    if (mime === 'application/pdf') return 'pdf';
    if (/\.(md|markdown)$/i.test(file.name)) return 'markdown';
    if (mime.startsWith('text/') || isTextLike(file.name)) return 'text';
    return 'other';
}

function isEditable(file) {
    const kind = previewKind(file);
    return kind === 'text' || kind === 'markdown';
}

/** 进入编辑模式：拉取完整内容 → 渲染编辑器（markdown 带实时预览） */
async function enterEditMode() {
    const file = state.preview;
    if (!file) return;
    const seq = renderSeq;
    editing = true;
    dirty = false;
    editBtn.hidden = true;
    contentEl.innerHTML = '<div class="editor-loading">加载中…</div>';

    let text = '';
    try {
        text = await getContent(file.path);
    } catch (e) {
        if (seq !== renderSeq) return;
        showToast(`无法编辑：${friendlyError(e)}`, 'error');
        render();
        return;
    }
    if (seq !== renderSeq) return;

    const isMd = /\.(md|markdown)$/i.test(file.name);
    contentEl.innerHTML = `
        <div class="editor-wrap">
            <div class="editor ${isMd ? 'editor-split' : ''}">
                <textarea class="editor-textarea" spellcheck="false" aria-label="编辑 ${escapeHtml(file.name)}"></textarea>
                ${isMd ? '<div class="editor-preview preview-markdown"></div>' : ''}
            </div>
            <div class="editor-actions">
                <span class="editor-status"></span>
                <button type="button" class="btn btn-ghost btn-sm editor-cancel">取消</button>
                <button type="button" class="btn btn-primary btn-sm editor-save" title="Ctrl / ⌘ + S">保存</button>
            </div>
        </div>`;

    const ta = contentEl.querySelector('.editor-textarea');
    const statusEl = contentEl.querySelector('.editor-status');
    ta.value = text;
    ta.addEventListener('input', () => {
        if (!dirty) {
            dirty = true;
            statusEl.textContent = '未保存';
        }
    });
    ta.addEventListener('keydown', (e) => {
        // Tab 缩进而不是跳走焦点；Shift+Tab 仍然可以把焦点移出编辑框
        if (e.key === 'Tab' && !e.shiftKey && !e.ctrlKey && !e.altKey && !e.metaKey) {
            e.preventDefault();
            ta.setRangeText('    ', ta.selectionStart, ta.selectionEnd, 'end');
            ta.dispatchEvent(new Event('input'));
        }
    });
    contentEl.querySelector('.editor-cancel').addEventListener('click', async () => {
        if (await confirmDiscard()) render();
    });
    contentEl.querySelector('.editor-save').addEventListener('click', doSave);

    if (isMd) {
        const preview = contentEl.querySelector('.editor-preview');
        let timer;
        const update = async () => {
            try {
                preview.innerHTML = await renderMarkdown(ta.value);
            } catch { /* ignore */ }
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
    const ta = contentEl.querySelector('.editor-textarea');
    if (!file || !ta) return;
    const statusEl = contentEl.querySelector('.editor-status');
    const btn = contentEl.querySelector('.editor-save');
    if (btn.disabled) return;
    btn.disabled = true;
    statusEl.textContent = '保存中…';
    try {
        await saveFile(file.path, ta.value);
        dirty = false;
        statusEl.textContent = '已保存';
        showToast('已保存', 'success');
        refresh();
    } catch (e) {
        statusEl.textContent = dirty ? '未保存' : '';
        showToast(`保存失败：${friendlyError(e)}`, 'error');
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
        webp: 'image/webp', svg: 'image/svg+xml', bmp: 'image/bmp', avif: 'image/avif',
        mp4: 'video/mp4', webm: 'video/webm', mkv: 'video/x-matroska', mov: 'video/quicktime', m4v: 'video/mp4',
        mp3: 'audio/mpeg', wav: 'audio/wav', ogg: 'audio/ogg', flac: 'audio/flac', m4a: 'audio/mp4', aac: 'audio/aac',
        pdf: 'application/pdf',
        txt: 'text/plain', md: 'text/markdown', html: 'text/html', xml: 'text/xml',
    };
    return map[ext] || 'application/octet-stream';
}

function isTextLike(name) {
    const ext = name.split('.').pop()?.toLowerCase() || '';
    const textExts = new Set([
        'txt', 'md', 'json', 'js', 'mjs', 'ts', 'rs', 'go', 'py', 'java', 'c', 'cpp',
        'h', 'css', 'html', 'xml', 'yaml', 'yml', 'toml', 'sh', 'bash', 'zsh',
        'sql', 'log', 'csv', 'ini', 'conf', 'cfg', 'env', 'gitignore', 'dockerfile',
        'makefile', 'cmake', 'gradle', 'properties', 'lock', 'vue', 'svelte', 'jsx', 'tsx',
        'kt', 'swift', 'rb', 'php', 'lua',
    ]);
    return textExts.has(ext);
}
