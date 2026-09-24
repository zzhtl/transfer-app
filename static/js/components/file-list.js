/**
 * 文件列表：列表 / 网格 / 搜索结果三种视图，统一窗口化渲染。
 *
 * 只渲染可见范围（上下各多 OVERSCAN 行）。之前一次性 innerHTML 全部条目，
 * 两万个文件就是二十万个节点，每点选一行都要重建整个列表，一次卡好几秒。
 *
 * 交互：
 * - 鼠标：单击只选当前项，Ctrl/⌘ 单击切换，Shift 单击连选，双击或 Enter 打开
 * - 触屏：没有选中项时单击直接打开；点复选框进入选择模式，之后单击切换选中
 * - 每行末尾的 ⋯ 打开与右键相同的菜单（iOS 长按不触发 contextmenu，全靠它）
 */

import { state, subscribe, getRaw } from '../store.js';
import {
    activeFiles, selectOnly, selectPaths, toggleSelect, clearSelection,
    toggleSelectAll, toggleSort, openPreview, clearSearch,
} from '../actions.js';
import { navigate, parentPath } from '../router.js';
import { formatSize, formatTime, formatDateTime, fileIcon } from '../utils/format.js';
import { escapeHtml, isTypingTarget, hasModalOpen, icon } from '../utils/dom.js';
import { openContextMenu, runMenuAction, isContextMenuOpen } from './context-menu.js';

const OVERSCAN = 8;
const coarsePointer = matchMedia('(pointer: coarse)');

let rootEl;
let headerEl;
let headerCheck;
let hintEl;
let progressEl;
let scrollEl;
let spacerEl;
let windowEl;
let emptyEl;

/** 当前视图里的条目（排序过滤后的目录，或搜索结果） */
let view = [];
/** list | grid | search */
let mode = 'list';
let cols = 1;
/** 一「行」的高度；网格模式下是一行格子加上行间距 */
let pitch = 48;
let rendered = { start: -1, end: -1 };
let scrollRaf = 0;

let selectedRef = null;
let selectedSet = new Set();
let focusIndex = -1;
let anchorPath = null;

/** 按目录记住滚动位置，回到这个目录时恢复 */
const scrollMemory = new Map();
/** 当前渲染的是哪个目录的数据 */
let shownPath = null;
/** 触屏上刚打开过条目的时间：双击的第二下不能落到新页面的条目上 */
let openedAt = 0;

export function initFileList() {
    rootEl = document.getElementById('file-list');
    if (!rootEl) return;

    rootEl.innerHTML = `
        <div class="list-progress" hidden></div>
        <div class="file-list-header">
            <div class="file-cell file-cell-check"><input type="checkbox" class="select-all" tabindex="-1" aria-label="全选"></div>
            <div class="file-cell file-cell-icon"></div>
            <button type="button" class="file-cell file-cell-name header-sort" data-sort="name">名称</button>
            <button type="button" class="file-cell file-cell-size header-sort" data-sort="size">大小</button>
            <button type="button" class="file-cell file-cell-time header-sort" data-sort="modified">修改时间</button>
            <div class="file-cell file-cell-more"></div>
        </div>
        <div class="file-list-hint" hidden></div>
        <div class="file-list-scroll" tabindex="0" role="listbox" aria-multiselectable="true" aria-label="文件列表">
            <div class="file-list-spacer"><div class="file-list-window"></div></div>
            <div class="file-list-empty" hidden></div>
        </div>`;

    progressEl = rootEl.querySelector('.list-progress');
    headerEl = rootEl.querySelector('.file-list-header');
    headerCheck = rootEl.querySelector('.select-all');
    hintEl = rootEl.querySelector('.file-list-hint');
    scrollEl = rootEl.querySelector('.file-list-scroll');
    spacerEl = rootEl.querySelector('.file-list-spacer');
    windowEl = rootEl.querySelector('.file-list-window');
    emptyEl = rootEl.querySelector('.file-list-empty');

    headerCheck.addEventListener('click', () => toggleSelectAll());
    headerEl.addEventListener('click', (e) => {
        const cell = e.target.closest('[data-sort]');
        if (cell) toggleSort(cell.dataset.sort);
    });

    scrollEl.addEventListener('scroll', onScroll, { passive: true });
    scrollEl.addEventListener('click', onClick);
    scrollEl.addEventListener('dblclick', onDblClick);
    scrollEl.addEventListener('contextmenu', onContextMenu);
    document.addEventListener('keydown', onKeydown);
    new ResizeObserver(onResize).observe(scrollEl);

    for (const key of ['files', 'filterText', 'sortBy', 'sortAsc', 'viewMode', 'searchResults']) {
        subscribe(key, render);
    }
    subscribe('selected', syncRows);
    subscribe('loading', render);
    subscribe('refreshing', updateProgress);
    subscribe('searching', () => {
        updateProgress();
        updateHint();
    });
    subscribe('currentPath', () => {
        // 离开目录前记下滚动位置；新数据到达前旧列表还在，这时 scrollTop 仍然有效
        if (shownPath !== null) scrollMemory.set(shownPath, scrollEl.scrollTop);
        focusIndex = -1;
        anchorPath = null;
    });

    render();
}

// ===== 渲染 =====

function render() {
    const raw = getRaw();
    const searchMode = raw.searchResults !== null;
    mode = searchMode ? 'search' : state.viewMode === 'grid' ? 'grid' : 'list';
    view = activeFiles();
    rootEl.dataset.mode = mode;
    headerEl.hidden = mode !== 'list';
    updateSortIndicator();
    updateHint();
    updateProgress();

    if (focusIndex >= view.length) focusIndex = view.length - 1;
    measure();
    spacerEl.style.height = `${Math.ceil(view.length / cols) * pitch}px`;

    const skeleton = state.loading;
    const empty = !skeleton && view.length === 0;
    spacerEl.hidden = skeleton || empty;
    emptyEl.hidden = !(skeleton || empty);
    if (skeleton) emptyEl.innerHTML = skeletonHtml();
    else if (empty) emptyEl.innerHTML = emptyHtml(searchMode);

    // 新目录的数据到了：恢复这个目录上次的滚动位置（没有就回到顶部）
    if (!skeleton && raw.filesPath !== null && raw.filesPath !== shownPath) {
        shownPath = raw.filesPath;
        scrollEl.scrollTop = scrollMemory.get(shownPath) || 0;
    }

    rendered = { start: -1, end: -1 };
    renderWindow();
}

/** 读取行高、网格列数（都来自 CSS 变量，移动端断点下的值会不同） */
function measure() {
    const style = getComputedStyle(scrollEl);
    if (mode === 'grid') {
        const cell = parseFloat(style.getPropertyValue('--grid-cell-height')) || 132;
        const gap = parseFloat(style.getPropertyValue('--grid-gap')) || 12;
        const minWidth = parseFloat(style.getPropertyValue('--grid-cell-min')) || 120;
        const width = scrollEl.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight);
        cols = Math.max(1, Math.floor((width + gap) / (minWidth + gap)));
        pitch = cell + gap;
        windowEl.style.gridTemplateColumns = `repeat(${cols}, minmax(0, 1fr))`;
    } else {
        cols = 1;
        pitch = parseFloat(style.getPropertyValue('--row-height')) || 48;
        windowEl.style.gridTemplateColumns = '';
    }
}

/** 只重绘可见窗口；可见区间没变就什么都不做 */
function renderWindow() {
    const rows = Math.ceil(view.length / cols);
    const top = scrollEl.scrollTop;
    const height = scrollEl.clientHeight || window.innerHeight;
    const firstRow = Math.max(0, Math.floor(top / pitch) - OVERSCAN);
    const lastRow = Math.min(rows, Math.ceil((top + height) / pitch) + OVERSCAN);
    const start = firstRow * cols;
    const end = Math.min(view.length, lastRow * cols);
    if (start === rendered.start && end === rendered.end) return;
    rendered = { start, end };

    windowEl.style.transform = `translateY(${firstRow * pitch}px)`;
    let html = '';
    for (let i = start; i < end; i++) html += rowHtml(view[i], i);
    windowEl.innerHTML = html;
    syncRows();
}

function rowHtml(f, i) {
    const path = escapeHtml(f.path);
    const name = escapeHtml(f.name);
    const kind = f.is_dir ? 'is-dir' : 'is-file';
    const check = '<div class="file-cell file-cell-check"><input type="checkbox" tabindex="-1" aria-label="选择"></div>';
    const more = `<button type="button" class="row-more" tabindex="-1" aria-label="更多操作">${icon('more')}</button>`;

    if (mode === 'grid') {
        return `<div class="file-row grid-cell ${kind}" id="row-${i}" role="option" data-index="${i}" data-path="${path}" title="${name}">
            ${check}${more}
            <div class="grid-icon">${fileIcon(f)}</div>
            <div class="grid-name">${name}</div>
            <div class="grid-meta">${f.is_dir ? '文件夹' : formatSize(f.size)}</div>
        </div>`;
    }

    let meta;
    let sub;
    if (mode === 'search') {
        const dir = escapeHtml(parentPath(f.path) || '/');
        meta = `<div class="file-cell file-cell-path" title="${dir}">${dir}</div>`;
        sub = dir;
    } else {
        const size = f.is_dir ? '—' : formatSize(f.size);
        const time = formatTime(f.modified);
        meta = `<div class="file-cell file-cell-size">${size}</div>
            <div class="file-cell file-cell-time" title="${formatDateTime(f.modified)}">${time}</div>`;
        sub = f.is_dir ? time : `${size} · ${time}`;
    }
    // file-sub 只在窄屏显示：那时大小、时间两列放不下，挪到名字下面
    return `<div class="file-row ${kind}" id="row-${i}" role="option" data-index="${i}" data-path="${path}">
        ${check}
        <div class="file-cell file-cell-icon">${fileIcon(f)}</div>
        <div class="file-cell file-cell-name" title="${name}"><span class="file-name-text">${name}</span><span class="file-sub">${sub}</span></div>
        ${meta}
        <div class="file-cell file-cell-more">${more}</div>
    </div>`;
}

function skeletonHtml() {
    return Array.from({ length: 8 }, () => '<div class="skeleton-row"><span></span><span></span></div>').join('');
}

function emptyHtml(searchMode) {
    const raw = getRaw();
    let msg = '此文件夹为空';
    let sub = '把文件拖到这里，或点「上传」';
    if (searchMode) {
        msg = '没有匹配的文件';
        sub = '换个关键词试试';
    } else if (raw.filterText) {
        msg = `当前文件夹里没有名字包含「${escapeHtml(raw.filterText)}」的项`;
        sub = '输入两个字以上会搜索所有子文件夹';
    }
    return `<div class="empty-state">${icon('folder', 'empty-icon')}<p>${msg}</p><p class="empty-sub">${sub}</p></div>`;
}

/** 窗口里各行的选中态、焦点态；表头复选框的全选/半选态 */
function syncRows() {
    const selected = currentSelection();
    for (const el of windowEl.children) {
        const on = selected.has(el.dataset.path);
        el.classList.toggle('selected', on);
        el.setAttribute('aria-selected', String(on));
        el.classList.toggle('focused', Number(el.dataset.index) === focusIndex);
        const box = el.querySelector('input');
        if (box) box.checked = on;
    }
    const count = view.reduce((n, f) => n + (selected.has(f.path) ? 1 : 0), 0);
    headerCheck.checked = count > 0 && count === view.length;
    headerCheck.indeterminate = count > 0 && count < view.length;
    rootEl.classList.toggle('has-selection', selected.size > 0);
    if (focusIndex >= 0) scrollEl.setAttribute('aria-activedescendant', `row-${focusIndex}`);
    else scrollEl.removeAttribute('aria-activedescendant');
}

function currentSelection() {
    const selected = getRaw().selected;
    if (selected !== selectedRef) {
        selectedRef = selected;
        selectedSet = new Set(selected);
    }
    return selectedSet;
}

function updateSortIndicator() {
    for (const cell of headerEl.querySelectorAll('[data-sort]')) {
        const active = cell.dataset.sort === state.sortBy;
        cell.classList.toggle('active', active);
        cell.dataset.dir = active ? (state.sortAsc ? 'asc' : 'desc') : '';
    }
}

function updateHint() {
    const raw = getRaw();
    let text = '';
    if (raw.searchResults !== null) {
        text = raw.searching ? '搜索中…' : `在当前文件夹及子文件夹中找到 ${raw.searchResults.length} 项`;
    } else if (raw.searching) {
        text = '搜索中…';
    }
    hintEl.textContent = text;
    hintEl.hidden = !text;
}

function updateProgress() {
    progressEl.hidden = !(state.refreshing || state.searching);
}

function onScroll() {
    if (scrollRaf) return;
    scrollRaf = requestAnimationFrame(() => {
        scrollRaf = 0;
        renderWindow();
    });
}

function onResize() {
    const firstIndex = Math.floor(scrollEl.scrollTop / pitch) * cols;
    const before = `${cols}:${pitch}`;
    measure();
    if (`${cols}:${pitch}` !== before) {
        // 列数或行高变了：总高度要重算，并让原来第一个可见的条目仍在顶部
        spacerEl.style.height = `${Math.ceil(view.length / cols) * pitch}px`;
        scrollEl.scrollTop = Math.floor(firstIndex / cols) * pitch;
        rendered = { start: -1, end: -1 };
    }
    renderWindow();
}

// ===== 交互 =====

function rowOf(target) {
    const row = target.closest('.file-row');
    if (!row) return null;
    const index = Number(row.dataset.index);
    return view[index] ? { row, index, file: view[index] } : null;
}

function openEntry(file) {
    openedAt = performance.now();
    if (file.is_dir) navigate(file.path);
    else openPreview(file);
}

function onClick(e) {
    const hit = rowOf(e.target);
    if (!hit) {
        // 点空白处：取消选择
        if (!e.target.closest('.file-list-empty')) clearSelection();
        return;
    }
    const { index, file } = hit;

    const moreBtn = e.target.closest('.row-more');
    if (moreBtn) {
        // 不让 document 上关闭菜单的监听把刚打开的菜单又关掉
        e.stopPropagation();
        if (!currentSelection().has(file.path)) {
            selectOnly(file.path);
            anchorPath = file.path;
        }
        setFocus(index, false);
        openContextMenu({ anchor: moreBtn });
        return;
    }

    if (e.target.closest('.file-cell-check')) {
        toggleSelect(file.path);
        anchorPath = file.path;
        setFocus(index, false);
        return;
    }

    if (coarsePointer.matches) {
        if (performance.now() - openedAt < 400) return;
        if (currentSelection().size) toggleSelect(file.path);
        else openEntry(file);
        return;
    }

    if (e.shiftKey && anchorPath !== null) {
        selectRangeTo(index);
    } else if (e.ctrlKey || e.metaKey) {
        toggleSelect(file.path);
        anchorPath = file.path;
    } else {
        selectOnly(file.path);
        anchorPath = file.path;
    }
    setFocus(index, false);
    scrollEl.focus({ preventScroll: true });
}

function onDblClick(e) {
    if (coarsePointer.matches || e.target.closest('.row-more, .file-cell-check')) return;
    const hit = rowOf(e.target);
    if (hit) openEntry(hit.file);
}

function onContextMenu(e) {
    const hit = rowOf(e.target);
    if (!hit) return;
    e.preventDefault();
    if (!currentSelection().has(hit.file.path)) {
        selectOnly(hit.file.path);
        anchorPath = hit.file.path;
    }
    setFocus(hit.index, false);
    openContextMenu({ x: e.clientX, y: e.clientY });
}

function selectRangeTo(index) {
    const anchor = view.findIndex(f => f.path === anchorPath);
    if (anchor < 0) {
        selectOnly(view[index].path);
        anchorPath = view[index].path;
        return;
    }
    const [from, to] = anchor < index ? [anchor, index] : [index, anchor];
    selectPaths(view.slice(from, to + 1).map(f => f.path));
}

function setFocus(index, reveal) {
    focusIndex = index;
    if (reveal) ensureVisible(index);
    syncRows();
}

function ensureVisible(index) {
    const top = Math.floor(index / cols) * pitch;
    const bottom = top + pitch;
    if (top < scrollEl.scrollTop) scrollEl.scrollTop = top;
    else if (bottom > scrollEl.scrollTop + scrollEl.clientHeight) scrollEl.scrollTop = bottom - scrollEl.clientHeight;
    renderWindow();
}

/** 方向键等：移动焦点。默认选中跟随焦点，Shift 连选，Ctrl/⌘ 只移动焦点 */
function moveFocusTo(index, e) {
    if (!view.length) return;
    index = Math.max(0, Math.min(view.length - 1, index));
    const path = view[index].path;
    if (e.shiftKey && anchorPath !== null) {
        selectRangeTo(index);
    } else if (!(e.ctrlKey || e.metaKey)) {
        selectOnly(path);
        anchorPath = path;
    }
    setFocus(index, true);
    scrollEl.focus({ preventScroll: true });
}

function moveFocus(delta, e) {
    const from = focusIndex < 0 ? (delta > 0 ? -1 : view.length) : focusIndex;
    moveFocusTo(from + delta, e);
}

function pageStep() {
    return Math.max(1, Math.floor(scrollEl.clientHeight / pitch) - 1) * cols;
}

function goUp() {
    if (state.currentPath) navigate(parentPath(state.currentPath));
}

function onKeydown(e) {
    if (e.defaultPrevented || e.isComposing || isTypingTarget(e.target)) return;
    if (hasModalOpen() || isContextMenuOpen() || state.authRequired) return;
    if (!rootEl.offsetParent) return;
    const mod = e.ctrlKey || e.metaKey;
    const step = mode === 'grid' ? cols : 1;

    switch (e.key) {
        case 'ArrowDown': moveFocus(step, e); break;
        case 'ArrowUp':
            if (e.altKey) goUp();
            else moveFocus(-step, e);
            break;
        case 'ArrowRight':
            if (mode !== 'grid') return;
            moveFocus(1, e);
            break;
        case 'ArrowLeft':
            if (mode !== 'grid') return;
            moveFocus(-1, e);
            break;
        case 'Home': moveFocusTo(0, e); break;
        case 'End': moveFocusTo(view.length - 1, e); break;
        case 'PageDown': moveFocus(pageStep(), e); break;
        case 'PageUp': moveFocus(-pageStep(), e); break;
        case 'Enter':
            if (focusIndex < 0) return;
            openEntry(view[focusIndex]);
            break;
        case ' ':
            if (focusIndex < 0) return;
            toggleSelect(view[focusIndex].path);
            anchorPath = view[focusIndex].path;
            break;
        case 'Backspace': goUp(); break;
        case 'Delete': runMenuAction('delete'); break;
        case 'F2': runMenuAction('rename'); break;
        case 'a':
        case 'A':
            if (!mod) return;
            selectPaths(view.map(f => f.path));
            break;
        case 'Escape':
            if (currentSelection().size) clearSelection();
            else if (getRaw().searchResults !== null || getRaw().filterText) clearSearch();
            else return;
            break;
        case '/':
            document.querySelector('.search-input')?.focus();
            break;
        default:
            return;
    }
    e.preventDefault();
}
