/**
 * 业务动作：连接 store 和 api
 */

import { state, getRaw } from './store.js';
import * as api from './api.js';
import { showToast } from './components/toast.js';

/** 加载文件列表 */
export async function loadFiles(path) {
    state.loading = true;
    try {
        const data = await api.listFiles(path);
        state.files = data.entries || [];
    } catch (e) {
        showToast(`加载失败: ${e.message}`, 'error');
        state.files = [];
    } finally {
        state.loading = false;
    }
}

/** 刷新当前目录 */
export function refresh() {
    return loadFiles(state.currentPath);
}

/** 创建目录 */
export async function createFolder(name) {
    try {
        await api.mkdir(state.currentPath, name);
        showToast(`已创建文件夹: ${name}`);
        await refresh();
    } catch (e) {
        showToast(`创建失败: ${e.message}`, 'error');
    }
}

/** 重命名 */
export async function renameEntry(path, newName) {
    try {
        await api.rename(path, newName);
        showToast('重命名成功');
        await refresh();
    } catch (e) {
        showToast(`重命名失败: ${e.message}`, 'error');
    }
}

/** 删除选中文件 */
export async function deleteSelected() {
    const raw = getRaw();
    const paths = [...raw.selected];
    if (!paths.length) return;
    try {
        await api.batchDelete(paths);
        showToast(`已删除 ${paths.length} 个项目`);
        state.selected = [];
        await refresh();
    } catch (e) {
        showToast(`删除失败: ${e.message}`, 'error');
    }
}

/** 下载文件 */
export function downloadFile(path) {
    const url = api.downloadUrl(path, true);
    const a = document.createElement('a');
    a.href = url;
    a.download = '';
    a.click();
}

/** 下载选中文件为 ZIP */
export function downloadSelectedAsZip() {
    const raw = getRaw();
    const paths = [...raw.selected];
    if (!paths.length) return;
    const url = api.zipDownloadUrl(paths);
    const a = document.createElement('a');
    a.href = url;
    a.download = '';
    a.click();
}

/** 搜索 */
export async function searchFiles(query) {
    if (!query.trim()) {
        state.searchResults = null;
        return;
    }
    state.loading = true;
    try {
        const data = await api.search(state.currentPath, query);
        state.searchResults = Array.isArray(data) ? data : (data.results || []);
    } catch (e) {
        showToast(`搜索失败: ${e.message}`, 'error');
    } finally {
        state.loading = false;
    }
}

/** 排序切换 */
export function toggleSort(field) {
    if (state.sortBy === field) {
        state.sortAsc = !state.sortAsc;
    } else {
        state.sortBy = field;
        state.sortAsc = true;
    }
}

/** 切换选中 */
export function toggleSelect(path) {
    const raw = getRaw();
    const set = new Set(raw.selected);
    if (set.has(path)) {
        set.delete(path);
    } else {
        set.add(path);
    }
    state.selected = [...set];
}

/** 全选/取消全选 */
export function toggleSelectAll() {
    const raw = getRaw();
    if (raw.selected.length === raw.files.length) {
        state.selected = [];
    } else {
        state.selected = raw.files.map(f => f.path);
    }
}

/** 打开预览 */
export function openPreview(file) {
    state.preview = file;
}

/** 关闭预览 */
export function closePreview() {
    state.preview = null;
}

/** 获取排序后过滤后的文件列表 */
export function getSortedFiles() {
    const raw = getRaw();
    let list = [...raw.files];

    // 过滤
    if (raw.filterText) {
        const kw = raw.filterText.toLowerCase();
        list = list.filter(f => f.name.toLowerCase().includes(kw));
    }

    // 排序：目录优先
    list.sort((a, b) => {
        if (a.is_dir !== b.is_dir) return a.is_dir ? -1 : 1;

        let cmp = 0;
        switch (raw.sortBy) {
            case 'name':
                cmp = a.name.localeCompare(b.name, 'zh');
                break;
            case 'size':
                cmp = (a.size || 0) - (b.size || 0);
                break;
            case 'modified':
                cmp = (a.modified || 0) - (b.modified || 0);
                break;
            default:
                cmp = a.name.localeCompare(b.name, 'zh');
        }
        return raw.sortAsc ? cmp : -cmp;
    });

    return list;
}
