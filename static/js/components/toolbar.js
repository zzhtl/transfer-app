/**
 * 工具栏组件：搜索、排序、过滤、视图切换、新建文件夹、上传按钮
 */

import { state, subscribe } from '../store.js';
import { toggleSort, searchFiles, createFolder, deleteSelected, downloadSelectedAsZip } from '../actions.js';

let searchTimer = null;

export function initToolbar() {
    const toolbar = document.getElementById('toolbar');
    if (!toolbar) return;

    // 搜索框
    const searchInput = toolbar.querySelector('.search-input');
    if (searchInput) {
        searchInput.addEventListener('input', () => {
            clearTimeout(searchTimer);
            const q = searchInput.value;
            state.filterText = q;
            searchTimer = setTimeout(() => {
                if (q.length >= 2) {
                    searchFiles(q);
                } else {
                    state.searchResults = null;
                }
            }, 300);
        });
    }

    // 排序按钮
    toolbar.querySelectorAll('[data-sort]').forEach(btn => {
        btn.addEventListener('click', () => toggleSort(btn.dataset.sort));
    });

    // 新建文件夹
    const mkdirBtn = toolbar.querySelector('.btn-mkdir');
    if (mkdirBtn) {
        mkdirBtn.addEventListener('click', () => {
            const name = prompt('文件夹名称:');
            if (name && name.trim()) createFolder(name.trim());
        });
    }

    // 上传按钮
    const uploadBtn = toolbar.querySelector('.btn-upload');
    if (uploadBtn) {
        uploadBtn.addEventListener('click', () => {
            state.uploadPanelOpen = !state.uploadPanelOpen;
        });
    }

    // 删除按钮
    const deleteBtn = toolbar.querySelector('.btn-delete');
    if (deleteBtn) {
        deleteBtn.addEventListener('click', () => {
            const count = state.selected.length;
            if (count && confirm(`确定删除 ${count} 个项目？`)) {
                deleteSelected();
            }
        });
    }

    // ZIP 下载
    const zipBtn = toolbar.querySelector('.btn-zip');
    if (zipBtn) {
        zipBtn.addEventListener('click', downloadSelectedAsZip);
    }

    // 视图切换
    const viewBtn = toolbar.querySelector('.btn-view-toggle');
    if (viewBtn) {
        viewBtn.addEventListener('click', () => {
            state.viewMode = state.viewMode === 'list' ? 'grid' : 'list';
            localStorage.setItem('viewMode', state.viewMode);
        });
    }

    // 响应选中状态变化，显示/隐藏批量操作按钮
    subscribe('selected', () => {
        const hasSelection = state.selected.length > 0;
        toolbar.querySelectorAll('.batch-action').forEach(el => {
            el.style.display = hasSelection ? '' : 'none';
        });
        const countEl = toolbar.querySelector('.selected-count');
        if (countEl) {
            countEl.textContent = hasSelection ? `已选 ${state.selected.length} 项` : '';
        }
    });

    // 排序状态指示
    subscribe('sortBy', updateSortIndicator);
    subscribe('sortAsc', updateSortIndicator);

    function updateSortIndicator() {
        toolbar.querySelectorAll('[data-sort]').forEach(btn => {
            btn.classList.toggle('active', btn.dataset.sort === state.sortBy);
            if (btn.dataset.sort === state.sortBy) {
                btn.dataset.dir = state.sortAsc ? 'asc' : 'desc';
            }
        });
    }

    updateSortIndicator();
}
