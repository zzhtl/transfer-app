/**
 * 目标文件夹选择器（用于移动 / 复制）
 * 复用 .dialog 样式与 listFiles API，导航到目标目录后确认。
 * 用法：const dest = await pickFolder({ title, confirmLabel, initialPath });
 *       dest 为选中的目标目录相对路径（root 为 ''），取消则为 null。
 */

import { listFiles, friendlyError } from '../api.js';
import { escapeHtml, icon, bringToFront, closeOnEscape } from '../utils/dom.js';

const FOLDER_SVG = icon('folder', 'ic-folder');
const UP_SVG = icon('arrow-up');

let overlayEl = null;
let currentPath = '';
let resolver = null;

function ensureDom() {
    if (overlayEl) return;
    overlayEl = document.createElement('div');
    overlayEl.className = 'dialog-overlay';
    overlayEl.innerHTML = `
        <div class="dialog folder-picker" role="dialog" aria-modal="true" aria-labelledby="folder-picker-title">
            <h3 class="folder-picker-title" id="folder-picker-title">选择目标文件夹</h3>
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
    closeOnEscape(overlayEl, () => finish(null));
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
            rows.push(`<div class="folder-picker-item" data-path="${escapeHtml(parent)}">${UP_SVG}<span>上级文件夹</span></div>`);
        }
        for (const d of dirs) {
            rows.push(`<div class="folder-picker-item" data-path="${escapeHtml(d.path)}">${FOLDER_SVG}<span>${escapeHtml(d.name)}</span></div>`);
        }
        listEl.innerHTML = rows.length ? rows.join('') : '<div class="folder-picker-empty">没有子文件夹</div>';
        listEl.querySelectorAll('.folder-picker-item').forEach(it => {
            it.addEventListener('click', () => load(it.dataset.path));
        });
    } catch (e) {
        listEl.innerHTML = `<div class="folder-picker-empty">加载失败：${escapeHtml(friendlyError(e))}</div>`;
    }
}

export function pickFolder({ title = '选择目标文件夹', confirmLabel = '选择此文件夹', initialPath = '' } = {}) {
    ensureDom();
    overlayEl.querySelector('.folder-picker-title').textContent = title;
    overlayEl.querySelector('.folder-picker-confirm').textContent = confirmLabel;
    bringToFront(overlayEl);
    overlayEl.classList.add('active');
    overlayEl.querySelector('.folder-picker-confirm').focus();
    load(initialPath);
    return new Promise((resolve) => { resolver = resolve; });
}

