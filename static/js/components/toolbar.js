/**
 * 工具栏组件：搜索、排序、视图切换、新建、上传入口、批量操作
 */

import { state, subscribe, getRaw } from '../store.js';
import {
    toggleSort, searchFiles, clearSearch, createFolder, downloadAsZip, refresh, clearSelection,
} from '../actions.js';
import { saveFile, friendlyError } from '../api.js';
import { openFileEditor } from './preview-modal.js';
import { showToast } from './toast.js';
import { promptDialog, validateName } from './dialog.js';
import { runMenuAction, siblingNames } from './context-menu.js';
import { icon } from '../utils/dom.js';

let searchTimer = null;

/** 新建空文件并立即打开编辑器 */
async function createNewFile(name) {
    const path = state.currentPath ? `${state.currentPath}/${name}` : name;
    try {
        await saveFile(path, '');
        await refresh();
        openFileEditor({ name, path, is_dir: false, size: 0 });
    } catch (e) {
        showToast(`新建失败：${friendlyError(e)}`, 'error');
    }
}

export function initToolbar() {
    const toolbar = document.getElementById('toolbar');
    if (!toolbar) return;

    // 搜索框：本地过滤立即生效；两个字以上时再去服务端递归搜索
    const searchInput = toolbar.querySelector('.search-input');
    searchInput.addEventListener('input', () => {
        clearTimeout(searchTimer);
        const q = searchInput.value;
        state.filterText = q;
        searchTimer = setTimeout(() => {
            if (q.trim().length >= 2) {
                searchFiles(q.trim());
            } else if (getRaw().searchResults !== null || getRaw().searching) {
                // 删到一个字以下：退回本地过滤，丢掉还在路上的搜索
                const text = state.filterText;
                clearSearch();
                state.filterText = text;
            }
        }, 300);
    });
    searchInput.addEventListener('keydown', (e) => {
        if (e.key === 'Escape') {
            e.preventDefault();
            clearTimeout(searchTimer);
            clearSearch();
            searchInput.blur();
        }
    });
    // 路由切换、Esc 清空时同步输入框
    subscribe('filterText', () => {
        if (!state.filterText && searchInput.value) searchInput.value = '';
    });

    // 排序：桌面是按钮组，窄屏是下拉
    toolbar.querySelectorAll('[data-sort]').forEach((btn) => {
        btn.addEventListener('click', () => toggleSort(btn.dataset.sort));
    });
    const sortSelect = toolbar.querySelector('.sort-select');
    sortSelect?.addEventListener('change', () => {
        const [by, dir] = sortSelect.value.split(':');
        state.sortBy = by;
        state.sortAsc = dir === 'asc';
    });

    toolbar.querySelector('.btn-mkdir')?.addEventListener('click', async () => {
        const name = await promptDialog({
            title: '新建文件夹',
            placeholder: '文件夹名称',
            confirmLabel: '创建',
            validate: v => validateName(v, siblingNames()),
        });
        if (name) createFolder(name);
    });

    toolbar.querySelector('.btn-newfile')?.addEventListener('click', async () => {
        // 同名会直接覆盖掉原文件，这里一定要查重
        const name = await promptDialog({
            title: '新建文件',
            placeholder: '例如 notes.md',
            confirmLabel: '创建并编辑',
            validate: v => validateName(v, siblingNames()),
        });
        if (name) createNewFile(name);
    });

    toolbar.querySelector('.btn-upload')?.addEventListener('click', () => {
        state.uploadPanelOpen = !state.uploadPanelOpen;
    });

    toolbar.querySelector('.btn-delete')?.addEventListener('click', () => runMenuAction('delete'));
    toolbar.querySelector('.btn-zip')?.addEventListener('click', () => downloadAsZip([...getRaw().selected]));
    toolbar.querySelector('.btn-clear-selection')?.addEventListener('click', clearSelection);

    // 视图切换
    const viewBtn = toolbar.querySelector('.btn-view-toggle');
    const renderViewBtn = () => {
        const grid = state.viewMode === 'grid';
        viewBtn.innerHTML = icon(grid ? 'list' : 'grid');
        viewBtn.title = grid ? '列表视图' : '网格视图';
        viewBtn.setAttribute('aria-label', viewBtn.title);
    };
    viewBtn?.addEventListener('click', () => {
        state.viewMode = state.viewMode === 'list' ? 'grid' : 'list';
        try {
            localStorage.setItem('viewMode', state.viewMode);
        } catch { /* 存不了就只在本次会话生效 */ }
    });
    if (viewBtn) {
        subscribe('viewMode', renderViewBtn);
        renderViewBtn();
    }

    // 选中状态：显示已选数量与批量操作
    subscribe('selected', () => {
        const count = state.selected.length;
        toolbar.classList.toggle('has-selection', count > 0);
        const countEl = toolbar.querySelector('.selected-count');
        if (countEl) countEl.textContent = count ? `已选 ${count} 项` : '';
    });

    // 排序状态指示
    const updateSortIndicator = () => {
        toolbar.querySelectorAll('[data-sort]').forEach((btn) => {
            const active = btn.dataset.sort === state.sortBy;
            btn.classList.toggle('active', active);
            btn.dataset.dir = active ? (state.sortAsc ? 'asc' : 'desc') : '';
        });
        if (sortSelect) sortSelect.value = `${state.sortBy}:${state.sortAsc ? 'asc' : 'desc'}`;
    };
    subscribe('sortBy', updateSortIndicator);
    subscribe('sortAsc', updateSortIndicator);
    updateSortIndicator();
}
