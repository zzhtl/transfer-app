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
    const handler = () => {
        const path = hashToPath();
        if (state.currentPath !== path) {
            state.currentPath = path;
            state.selected = [];
            state.searchResults = null;
            state.contextMenu = null;
            loadFiles(path);
        }
    };
    window.addEventListener('hashchange', handler);
    // 首次加载
    handler();
}
