/**
 * 业务动作：连接 store 和 api
 */

import { state, getRaw } from './store.js';
import * as api from './api.js';
import { friendlyError } from './api.js';
import { showToast } from './components/toast.js';

/** 路径的最后一段 */
export function baseName(path) {
    return path.split('/').filter(Boolean).pop() || '';
}

// ===== 列表加载 =====

let listSeq = 0;
let listAbort = null;
let skeletonTimer = null;

/**
 * 加载目录。
 * - navigate：进入新目录。150ms 内没返回才显示骨架屏，局域网里多数时候直接出结果，不闪。
 * - refresh：原地刷新（改名、删除、上传完成后）。列表保持不动，只亮顶部细进度条。
 *
 * 每次加载都会取消上一个请求并丢弃过期响应：快速连点目录时，慢的旧响应不能覆盖新目录。
 */
export async function loadFiles(path, { mode = 'navigate' } = {}) {
    const seq = ++listSeq;
    listAbort?.abort();
    const controller = new AbortController();
    listAbort = controller;
    clearTimeout(skeletonTimer);

    if (mode === 'navigate') {
        skeletonTimer = setTimeout(() => {
            if (seq === listSeq) state.loading = true;
        }, 150);
    } else {
        state.refreshing = true;
    }

    try {
        const data = await api.listFiles(path, { signal: controller.signal });
        if (seq !== listSeq) return;
        state.filesPath = path;
        state.files = data.entries || [];
    } catch (e) {
        if (e.name === 'AbortError' || seq !== listSeq) return;
        showToast(`加载失败：${friendlyError(e)}`, 'error');
        if (mode === 'navigate') {
            state.filesPath = path;
            state.files = [];
        }
    } finally {
        if (seq === listSeq) {
            clearTimeout(skeletonTimer);
            state.loading = false;
            state.refreshing = false;
            listAbort = null;
        }
    }
}

/** 原地刷新当前目录 */
export function refresh() {
    return loadFiles(state.currentPath, { mode: 'refresh' });
}

// ===== 搜索 =====

let searchSeq = 0;
let searchAbort = null;

/** 服务端递归搜索；新的搜索会取消还没返回的旧搜索 */
export async function searchFiles(query) {
    const seq = ++searchSeq;
    searchAbort?.abort();
    if (!query.trim()) {
        state.searching = false;
        state.searchResults = null;
        return;
    }
    const controller = new AbortController();
    searchAbort = controller;
    state.searching = true;
    try {
        const data = await api.search(state.currentPath, query, { signal: controller.signal });
        if (seq !== searchSeq) return;
        state.searchResults = Array.isArray(data) ? data : (data.results || []);
    } catch (e) {
        if (e.name === 'AbortError' || seq !== searchSeq) return;
        showToast(`搜索失败：${friendlyError(e)}`, 'error');
    } finally {
        if (seq === searchSeq) state.searching = false;
    }
}

/** 退出搜索：取消进行中的请求，清掉结果和关键词 */
export function clearSearch() {
    searchSeq++;
    searchAbort?.abort();
    searchAbort = null;
    state.searching = false;
    state.searchResults = null;
    state.filterText = '';
}

// ===== 排序视图 =====

// numeric：「第2集」排在「第10集」前面；zh-CN 下中文按拼音排
const collator = new Intl.Collator('zh-CN', { numeric: true, sensitivity: 'base' });
let sortedMemo = { files: null, sortBy: null, sortAsc: null, filterText: null, result: [] };

/**
 * 排序、过滤后的当前目录。
 * 以 (files 引用, 排序字段, 方向, 过滤词) 为 key 缓存：选中状态变化不会触发重新排序，
 * 两万个文件的目录里每次点选都重排一遍是之前卡顿的主因之一。
 */
export function getSortedFiles() {
    const raw = getRaw();
    const memo = sortedMemo;
    if (memo.files === raw.files && memo.sortBy === raw.sortBy
        && memo.sortAsc === raw.sortAsc && memo.filterText === raw.filterText) {
        return memo.result;
    }

    let list;
    if (raw.filterText) {
        const kw = raw.filterText.toLowerCase();
        list = raw.files.filter(f => f.name.toLowerCase().includes(kw));
    } else {
        list = [...raw.files];
    }

    const direction = raw.sortAsc ? 1 : -1;
    const by = raw.sortBy;
    list.sort((a, b) => {
        // 目录始终在前
        if (a.is_dir !== b.is_dir) return a.is_dir ? -1 : 1;
        let cmp = 0;
        if (by === 'size') cmp = (a.size || 0) - (b.size || 0);
        else if (by === 'modified') cmp = (a.modified || 0) - (b.modified || 0);
        if (cmp === 0) cmp = collator.compare(a.name, b.name);
        return direction * cmp;
    });

    sortedMemo = {
        files: raw.files, sortBy: raw.sortBy, sortAsc: raw.sortAsc,
        filterText: raw.filterText, result: list,
    };
    return list;
}

/** 当前展示的条目：搜索结果优先，否则是排序后的当前目录 */
export function activeFiles() {
    const raw = getRaw();
    return raw.searchResults !== null ? raw.searchResults : getSortedFiles();
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

// ===== 选择 =====

export function selectOnly(path) {
    state.selected = path == null ? [] : [path];
}

export function selectPaths(paths) {
    state.selected = [...new Set(paths)];
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

export function clearSelection() {
    if (getRaw().selected.length) state.selected = [];
}

/** 全选/取消全选 */
export function toggleSelectAll() {
    const all = activeFiles();
    const allSelected = all.length > 0 && getRaw().selected.length === all.length;
    state.selected = allSelected ? [] : all.map(f => f.path);
}

// ===== 文件操作 =====

/** 创建目录，成功返回 true */
export async function createFolder(name) {
    try {
        await api.mkdir(state.currentPath, name);
        showToast(`已创建文件夹「${name}」`, 'success');
        await refresh();
        return true;
    } catch (e) {
        showToast(`创建失败：${friendlyError(e)}`, 'error');
        return false;
    }
}

/** 重命名，成功后选中新名字 */
export async function renameEntry(path, newName) {
    try {
        await api.rename(path, newName);
        const parent = path.includes('/') ? path.slice(0, path.lastIndexOf('/')) : '';
        state.selected = [parent ? `${parent}/${newName}` : newName];
        showToast('已重命名', 'success');
        await refresh();
    } catch (e) {
        showToast(`重命名失败：${friendlyError(e)}`, 'error');
    }
}

/** 批量删除 */
export async function deleteEntries(paths) {
    if (!paths.length) return;
    try {
        await api.batchDelete(paths);
        showToast(`已删除 ${paths.length} 项`, 'success');
        state.selected = [];
    } catch (e) {
        // 服务端遇到第一个失败就停，前面的可能已经删了，所以无论如何都要刷新
        showToast(`删除失败：${friendlyError(e)}`, 'error');
    }
    await refresh();
}

/** 逐项移动/复制：单项失败不中断整批，结束后汇总 */
async function transferEach(paths, destination, fn, verb) {
    const failures = [];
    for (const path of paths) {
        try {
            await fn(path, destination);
        } catch (e) {
            failures.push(`${baseName(path)}：${friendlyError(e)}`);
        }
    }
    const ok = paths.length - failures.length;
    if (!failures.length) {
        showToast(`已${verb} ${ok} 项`, 'success');
    } else {
        const detail = failures.slice(0, 3).join('；') + (failures.length > 3 ? '…' : '');
        showToast(`${verb}成功 ${ok} 项，失败 ${failures.length} 项。${detail}`, ok ? 'warning' : 'error');
    }
    state.selected = [];
    await refresh();
}

export function moveEntries(paths, destination) {
    return transferEach(paths, destination, api.moveEntry, '移动');
}

export function copyEntries(paths, destination) {
    return transferEach(paths, destination, api.copyEntry, '复制');
}

// ===== 下载 =====

function triggerDownload(url) {
    const a = document.createElement('a');
    a.href = url;
    a.download = '';
    document.body.appendChild(a);
    a.click();
    a.remove();
}

/** 下载文件 */
export function downloadFile(path) {
    triggerDownload(api.downloadUrl(path, true));
}

/** 打包下载：只有一项时 zip 用它的名字，多项用当前目录名 */
export function downloadAsZip(paths) {
    if (!paths.length) return;
    let name;
    if (paths.length === 1) name = `${baseName(paths[0])}.zip`;
    else if (getRaw().searchResults !== null) name = '搜索结果.zip';
    else name = `${baseName(state.currentPath) || '共享文件'}.zip`;
    triggerDownload(api.zipDownloadUrl(paths, name));
}

// ===== 预览 =====

/** 打开预览 */
export function openPreview(file) {
    state.preview = file;
}

/** 关闭预览 */
export function closePreview() {
    state.preview = null;
}
