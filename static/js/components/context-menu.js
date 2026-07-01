/**
 * 右键菜单组件
 */

import { state, subscribe, getRaw } from '../store.js';
import { renameEntry, deleteSelected, downloadFile, downloadSelectedAsZip, openPreview, moveSelected, copySelected } from '../actions.js';
import { downloadUrl } from '../api.js';
import { navigate } from '../router.js';
import { pickFolder } from './folder-picker.js';
import { openShareModal } from './share-modal.js';

let menuEl = null;

export function initContextMenu() {
    menuEl = document.createElement('div');
    menuEl.className = 'context-menu';
    menuEl.style.display = 'none';
    document.body.appendChild(menuEl);

    menuEl.addEventListener('click', handleAction);

    // 点击其他地方关闭
    document.addEventListener('click', () => {
        state.contextMenu = null;
    });

    document.addEventListener('keydown', (e) => {
        if (e.key === 'Escape') state.contextMenu = null;
    });

    subscribe('contextMenu', render);
}

function render() {
    const ctx = state.contextMenu;
    if (!ctx) {
        menuEl.style.display = 'none';
        return;
    }

    const raw = getRaw();
    const list = raw.searchResults !== null ? raw.searchResults : raw.files;
    const paths = raw.selected;
    const single = paths.length === 1;
    const file = single ? list.find(f => f.path === paths[0]) : null;
    const isDir = file?.is_dir;

    let items = [];

    if (single && isDir) {
        items.push({ action: 'open', label: '打开', icon: 'folder-open' });
    }
    if (single && !isDir) {
        items.push({ action: 'preview', label: '预览', icon: 'eye' });
        items.push({ action: 'download', label: '下载', icon: 'download' });
    }
    if (paths.length > 1) {
        items.push({ action: 'zip', label: '打包下载', icon: 'archive' });
    }
    if (single) {
        items.push({ action: 'share', label: '分享…', icon: 'share' });
    }
    items.push({ divider: true });
    items.push({ action: 'move', label: '移动到…', icon: 'move' });
    items.push({ action: 'copy', label: '复制到…', icon: 'copy' });
    if (single) {
        items.push({ action: 'rename', label: '重命名', icon: 'edit' });
    }
    items.push({ action: 'delete', label: '删除', icon: 'trash', danger: true });

    menuEl.innerHTML = items.map(item => {
        if (item.divider) return '<div class="context-menu-divider"></div>';
        const cls = item.danger ? 'context-menu-item danger' : 'context-menu-item';
        return `<div class="${cls}" data-action="${item.action}">${item.label}</div>`;
    }).join('');

    // 定位
    const { x, y } = ctx;
    menuEl.style.display = 'block';
    const rect = menuEl.getBoundingClientRect();
    const maxX = window.innerWidth - rect.width - 8;
    const maxY = window.innerHeight - rect.height - 8;
    menuEl.style.left = `${Math.min(x, maxX)}px`;
    menuEl.style.top = `${Math.min(y, maxY)}px`;
}

async function handleAction(e) {
    const item = e.target.closest('[data-action]');
    if (!item) return;

    const action = item.dataset.action;
    const raw = getRaw();
    const list = raw.searchResults !== null ? raw.searchResults : raw.files;
    const paths = [...raw.selected];
    const file = list.find(f => f.path === paths[0]);

    state.contextMenu = null;

    switch (action) {
        case 'open':
            navigate(paths[0]);
            break;
        case 'preview':
            if (file) openPreview(file);
            break;
        case 'download':
            if (paths[0]) downloadFile(paths[0]);
            break;
        case 'zip':
            downloadSelectedAsZip();
            break;
        case 'share':
            if (paths[0]) openShareModal(paths[0]);
            break;
        case 'move': {
            const dest = await pickFolder({ title: '移动到', confirmLabel: '移动到此文件夹', initialPath: state.currentPath });
            if (dest !== null) moveSelected(dest);
            break;
        }
        case 'copy': {
            const dest = await pickFolder({ title: '复制到', confirmLabel: '复制到此文件夹', initialPath: state.currentPath });
            if (dest !== null) copySelected(dest);
            break;
        }
        case 'rename': {
            if (!file) break;
            const newName = prompt('新名称:', file.name);
            if (newName && newName !== file.name) {
                renameEntry(paths[0], newName);
            }
            break;
        }
        case 'delete':
            if (confirm(`确定删除 ${paths.length} 个项目？`)) {
                deleteSelected();
            }
            break;
    }
}
