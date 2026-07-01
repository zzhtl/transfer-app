/**
 * Hash-based 路由
 * 格式: #/path/to/dir
 */

import { state } from './store.js';
import { loadFiles } from './actions.js';

/** 从 hash 提取路径 */
function hashToPath() {
    const hash = location.hash.slice(1) || '/';
    return decodeURIComponent(hash).replace(/^\/+/, '').replace(/\/+$/, '');
}

/** 导航到指定路径 */
export function navigate(path) {
    const clean = path.replace(/^\/+/, '').replace(/\/+$/, '');
    location.hash = `/${clean}`;
}

/** 初始化路由监听 */
export function initRouter() {
    const handler = (force) => {
        const path = hashToPath();
        if (force || state.currentPath !== path) {
            state.currentPath = path;
            state.selected = [];
            state.searchResults = null;
            state.contextMenu = null;
            loadFiles(path);
        }
    };
    window.addEventListener('hashchange', () => handler(false));
    // 首次强制加载（根目录 hash 为空，currentPath 初值也为空，需强制触发）
    handler(true);
}
