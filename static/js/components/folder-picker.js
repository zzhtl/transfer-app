/**
 * 目标文件夹选择器（用于移动 / 复制）
 * 复用 .dialog 样式与 listFiles API，导航到目标目录后确认。
 * 用法：const dest = await pickFolder({ title, confirmLabel, initialPath });
 *       dest 为选中的目标目录相对路径（root 为 ''），取消则为 null。
 */

import { listFiles } from '../api.js';

const FOLDER_SVG = `<svg viewBox="0 0 24 24" fill="var(--accent)" stroke="none"><path d="M2 6a2 2 0 012-2h5l2 2h9a2 2 0 012 2v10a2 2 0 01-2 2H4a2 2 0 01-2-2V6z"/></svg>`;
const UP_SVG = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 19V5"/><path d="M5 12l7-7 7 7"/></svg>`;

let overlayEl = null;
let currentPath = '';
let resolver = null;

function ensureDom() {
    if (overlayEl) return;
    overlayEl = document.createElement('div');
    overlayEl.className = 'dialog-overlay';
    overlayEl.innerHTML = `
        <div class="dialog folder-picker">
            <h3 class="folder-picker-title">选择目标文件夹</h3>
            <div class="folder-picker-current"></div>
            <div class="folder-picker-list"></div>
            <div class="dialog-actions">
                <button class="btn btn-ghost folder-picker-cancel">取消</button>
                <button class="btn btn-primary folder-picker-confirm">选择此文件夹</button>
            </div>
        </div>`;
    document.body.appendChild(overlayEl);

    overlayEl.querySelector('.folder-picker-cancel').addEventListener('click', () => finish(null));
    overlayEl.querySelector('.folder-picker-confirm').addEventListener('click', () => finish(currentPath));
    overlayEl.addEventListener('click', (e) => {
        if (e.target === overlayEl) finish(null);
    });
    document.addEventListener('keydown', (e) => {
        if (e.key === 'Escape' && resolver) finish(null);
    });
}

function finish(result) {
    overlayEl.classList.remove('active');
    const r = resolver;
    resolver = null;
    if (r) r(result);
}

async function load(path) {
    currentPath = path;
    const listEl = overlayEl.querySelector('.folder-picker-list');
    const curEl = overlayEl.querySelector('.folder-picker-current');
    curEl.textContent = `当前位置：/${path}`;
    listEl.innerHTML = '<div class="folder-picker-empty">加载中...</div>';
    try {
        const data = await listFiles(path);
        const dirs = (data.entries || []).filter(e => e.is_dir);
        const rows = [];
        if (path) {
            const parent = path.split('/').slice(0, -1).join('/');
            rows.push(`<div class="folder-picker-item" data-path="${escapeAttr(parent)}">${UP_SVG}<span>.. 上级目录</span></div>`);
        }
        for (const d of dirs) {
            rows.push(`<div class="folder-picker-item" data-path="${escapeAttr(d.path)}">${FOLDER_SVG}<span>${escapeHtml(d.name)}</span></div>`);
        }
        listEl.innerHTML = rows.length ? rows.join('') : '<div class="folder-picker-empty">没有子文件夹</div>';
        listEl.querySelectorAll('.folder-picker-item').forEach(it => {
            it.addEventListener('click', () => load(it.dataset.path));
        });
    } catch (e) {
        listEl.innerHTML = `<div class="folder-picker-empty">加载失败：${escapeHtml(e.message)}</div>`;
    }
}

export function pickFolder({ title = '选择目标文件夹', confirmLabel = '选择此文件夹', initialPath = '' } = {}) {
    ensureDom();
    overlayEl.querySelector('.folder-picker-title').textContent = title;
    overlayEl.querySelector('.folder-picker-confirm').textContent = confirmLabel;
    overlayEl.classList.add('active');
    load(initialPath);
    return new Promise((resolve) => { resolver = resolve; });
}

function escapeHtml(text) {
    const d = document.createElement('div');
    d.textContent = text;
    return d.innerHTML;
}

function escapeAttr(text) {
    return String(text).replace(/"/g, '&quot;').replace(/</g, '&lt;');
}
