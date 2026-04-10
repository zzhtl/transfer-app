/**
 * 文件列表组件
 * 支持列表/网格视图、多选、双击导航/预览
 */

import { state, subscribe, getRaw } from '../store.js';
import { getSortedFiles, toggleSelect, openPreview, downloadFile } from '../actions.js';
import { navigate } from '../router.js';
import { formatSize, formatTime, fileIcon } from '../utils/format.js';

let listEl = null;

export function initFileList() {
    listEl = document.getElementById('file-list');
    if (!listEl) return;

    // 点击事件代理
    listEl.addEventListener('click', handleClick);
    listEl.addEventListener('dblclick', handleDblClick);
    listEl.addEventListener('contextmenu', handleContextMenu);

    subscribe('files', render);
    subscribe('selected', render);
    subscribe('filterText', render);
    subscribe('sortBy', render);
    subscribe('sortAsc', render);
    subscribe('viewMode', render);
    subscribe('loading', render);

    render();
}

function render() {
    if (!listEl) return;

    if (state.loading) {
        listEl.innerHTML = `<div class="empty-state">
            <div class="spinner"></div>
            <p>加载中...</p>
        </div>`;
        return;
    }

    const files = getSortedFiles();
    const raw = getRaw();
    const selected = new Set(raw.selected);
    const isGrid = state.viewMode === 'grid';

    listEl.className = `file-list ${isGrid ? 'file-list-grid' : ''}`;

    if (!files.length) {
        listEl.innerHTML = `<div class="empty-state">
            <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="var(--text-tertiary)" stroke-width="1.5">
                <path d="M22 19a2 2 0 01-2 2H4a2 2 0 01-2-2V5a2 2 0 012-2h5l2 3h9a2 2 0 012 2v11z"/>
            </svg>
            <p>此文件夹为空</p>
        </div>`;
        return;
    }

    const html = files.map(f => {
        const isSelected = selected.has(f.path);
        const cls = [
            'file-row',
            f.is_dir ? 'is-dir' : 'is-file',
            isSelected ? 'selected' : ''
        ].filter(Boolean).join(' ');

        if (isGrid) {
            return `<div class="${cls}" data-path="${escapeAttr(f.path)}" data-dir="${f.is_dir}">
                <div class="file-icon">${fileIcon(f)}</div>
                <div class="file-name" title="${escapeAttr(f.name)}">${escapeHtml(f.name)}</div>
            </div>`;
        }

        return `<div class="${cls}" data-path="${escapeAttr(f.path)}" data-dir="${f.is_dir}">
            <div class="file-cell file-cell-check">
                <input type="checkbox" ${isSelected ? 'checked' : ''} tabindex="-1">
            </div>
            <div class="file-cell file-cell-icon">${fileIcon(f)}</div>
            <div class="file-cell file-cell-name" title="${escapeAttr(f.name)}">${escapeHtml(f.name)}</div>
            <div class="file-cell file-cell-size">${f.is_dir ? '-' : formatSize(f.size)}</div>
            <div class="file-cell file-cell-time">${formatTime(f.modified)}</div>
        </div>`;
    }).join('');

    listEl.innerHTML = html;
}

function handleClick(e) {
    const row = e.target.closest('.file-row');
    if (!row) return;
    const path = row.dataset.path;
    // checkbox 或行点击 → 切换选中
    toggleSelect(path);
}

function handleDblClick(e) {
    const row = e.target.closest('.file-row');
    if (!row) return;
    const path = row.dataset.path;
    const isDir = row.dataset.dir === 'true';

    if (isDir) {
        navigate(path);
    } else {
        // 文件双击 → 预览
        const raw = getRaw();
        const file = raw.files.find(f => f.path === path);
        if (file) openPreview(file);
    }
}

function handleContextMenu(e) {
    const row = e.target.closest('.file-row');
    if (!row) return;
    e.preventDefault();
    const path = row.dataset.path;

    // 确保该项被选中
    const raw = getRaw();
    if (!raw.selected.includes(path)) {
        state.selected = [path];
    }

    state.contextMenu = { x: e.clientX, y: e.clientY };
}

function escapeHtml(text) {
    const d = document.createElement('div');
    d.textContent = text;
    return d.innerHTML;
}

function escapeAttr(text) {
    return text.replace(/"/g, '&quot;').replace(/</g, '&lt;');
}
