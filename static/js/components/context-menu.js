/**
 * 右键菜单 / 行尾 ⋯ 菜单，以及它们共用的文件操作（快捷键也走这里）
 */

import { state, subscribe, getRaw } from '../store.js';
import {
    activeFiles, baseName, renameEntry, deleteEntries, downloadFile, downloadAsZip,
    openPreview, moveEntries, copyEntries,
} from '../actions.js';
import { navigate, parentPath } from '../router.js';
import { escapeHtml, icon } from '../utils/dom.js';
import { pickFolder } from './folder-picker.js';
import { openShareModal } from './share-modal.js';
import { promptDialog, confirmDialog, validateName } from './dialog.js';

let menuEl = null;

export function initContextMenu() {
    menuEl = document.createElement('div');
    menuEl.className = 'context-menu';
    menuEl.setAttribute('role', 'menu');
    document.body.appendChild(menuEl);

    menuEl.addEventListener('click', (e) => {
        const item = e.target.closest('[data-action]');
        if (!item) return;
        e.stopPropagation();
        closeMenu();
        runMenuAction(item.dataset.action);
    });
    menuEl.addEventListener('keydown', onMenuKeydown);

    // 点其他地方、滚动、窗口变化都关闭
    document.addEventListener('click', closeMenu);
    window.addEventListener('resize', closeMenu);
    window.addEventListener('blur', closeMenu);
    document.addEventListener('scroll', closeMenu, true);

    // 路由切换会把它置空
    subscribe('contextMenu', () => {
        if (!state.contextMenu) closeMenu();
    });
}

export function isContextMenuOpen() {
    return !!menuEl?.classList.contains('open');
}

function closeMenu() {
    if (!isContextMenuOpen()) return;
    menuEl.classList.remove('open');
    if (state.contextMenu) state.contextMenu = null;
}

/** 当前选中的条目对象 */
function selectedFiles() {
    const selected = new Set(getRaw().selected);
    return activeFiles().filter(f => selected.has(f.path));
}

function buildItems(files) {
    const single = files.length === 1 ? files[0] : null;
    const items = [];
    if (single?.is_dir) {
        items.push({ action: 'open', label: '打开', icon: 'folder-open' });
        items.push({ action: 'zip', label: '下载（ZIP）', icon: 'download' });
    } else if (single) {
        items.push({ action: 'preview', label: '预览', icon: 'eye' });
        items.push({ action: 'download', label: '下载', icon: 'download' });
    } else {
        items.push({ action: 'zip', label: `打包下载 ${files.length} 项`, icon: 'archive' });
    }
    if (single && getRaw().searchResults !== null) {
        items.push({ action: 'reveal', label: '打开所在文件夹', icon: 'folder' });
    }
    if (single) items.push({ action: 'share', label: '分享…', icon: 'share' });
    items.push({ divider: true });
    items.push({ action: 'move', label: '移动到…', icon: 'move' });
    items.push({ action: 'copy', label: '复制到…', icon: 'copy' });
    if (single) items.push({ action: 'rename', label: '重命名', icon: 'edit', hint: 'F2' });
    items.push({ action: 'delete', label: '删除', icon: 'trash', danger: true, hint: 'Del' });
    return items;
}

/**
 * 打开菜单。
 * @param {{x?: number, y?: number, anchor?: Element}} at 鼠标位置，或以某个按钮为锚点
 */
export function openContextMenu(at) {
    const files = selectedFiles();
    if (!files.length) return;

    menuEl.innerHTML = buildItems(files).map((item) => {
        if (item.divider) return '<div class="context-menu-divider" role="separator"></div>';
        const cls = item.danger ? 'context-menu-item danger' : 'context-menu-item';
        const hint = item.hint ? `<span class="context-menu-hint">${item.hint}</span>` : '';
        return `<button type="button" class="${cls}" role="menuitem" data-action="${item.action}">${icon(item.icon)}<span>${escapeHtml(item.label)}</span>${hint}</button>`;
    }).join('');

    menuEl.classList.add('open');
    state.contextMenu = true;

    const rect = menuEl.getBoundingClientRect();
    let left;
    let top;
    if (at.anchor) {
        const anchor = at.anchor.getBoundingClientRect();
        left = anchor.right - rect.width;
        top = anchor.bottom + 4;
        if (top + rect.height > window.innerHeight - 8) top = anchor.top - rect.height - 4;
    } else {
        left = at.x;
        top = at.y;
    }
    menuEl.style.left = `${Math.max(8, Math.min(left, window.innerWidth - rect.width - 8))}px`;
    menuEl.style.top = `${Math.max(8, Math.min(top, window.innerHeight - rect.height - 8))}px`;
    menuEl.querySelector('.context-menu-item')?.focus({ preventScroll: true });
}

function onMenuKeydown(e) {
    const items = [...menuEl.querySelectorAll('.context-menu-item')];
    const current = items.indexOf(document.activeElement);
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        const delta = e.key === 'ArrowDown' ? 1 : -1;
        items[(current + delta + items.length) % items.length]?.focus();
    } else if (e.key === 'Home') {
        items[0]?.focus();
    } else if (e.key === 'End') {
        items[items.length - 1]?.focus();
    } else if (e.key === 'Escape' || e.key === 'Tab') {
        closeMenu();
        document.querySelector('.file-list-scroll')?.focus({ preventScroll: true });
    } else {
        return;
    }
    e.preventDefault();
    e.stopPropagation();
}

/** 列表里现有的名字（重命名、新建时查重用） */
export function siblingNames() {
    return getRaw().searchResults === null ? getRaw().files.map(f => f.name) : [];
}

/**
 * 对当前选中项执行操作；菜单点击和快捷键（Delete、F2）共用
 * @param {string} action
 */
export async function runMenuAction(action) {
    const files = selectedFiles();
    if (!files.length) return;
    const paths = files.map(f => f.path);
    const single = files.length === 1 ? files[0] : null;

    switch (action) {
        case 'open':
            if (single?.is_dir) navigate(single.path);
            break;
        case 'preview':
            if (single && !single.is_dir) openPreview(single);
            break;
        case 'download':
            if (single && !single.is_dir) downloadFile(single.path);
            break;
        case 'zip':
            downloadAsZip(paths);
            break;
        case 'reveal':
            if (single) navigate(parentPath(single.path));
            break;
        case 'share':
            if (single) openShareModal(single.path);
            break;
        case 'move': {
            const dest = await pickFolder({ title: `移动 ${files.length} 项到`, confirmLabel: '移动到这里', initialPath: state.currentPath });
            if (dest !== null) await moveEntries(paths, dest);
            break;
        }
        case 'copy': {
            const dest = await pickFolder({ title: `复制 ${files.length} 项到`, confirmLabel: '复制到这里', initialPath: state.currentPath });
            if (dest !== null) await copyEntries(paths, dest);
            break;
        }
        case 'rename': {
            if (!single) return;
            const others = siblingNames().filter(n => n !== single.name);
            const name = await promptDialog({
                title: single.is_dir ? '重命名文件夹' : '重命名文件',
                value: single.name,
                confirmLabel: '重命名',
                selectStem: !single.is_dir,
                validate: v => validateName(v, others),
            });
            if (name && name !== single.name) await renameEntry(single.path, name);
            break;
        }
        case 'delete': {
            const ok = await confirmDialog({
                title: files.length === 1 ? '删除这一项？' : `删除这 ${files.length} 项？`,
                message: '删除后无法恢复，文件夹会连同里面的内容一起删除。',
                items: files.map(f => (f.is_dir ? `${baseName(f.path)}/` : baseName(f.path))),
                confirmLabel: '删除',
                danger: true,
            });
            if (ok) await deleteEntries(paths);
            break;
        }
    }
}
