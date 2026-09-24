/**
 * Hash-based 路由
 * 格式: #/path/to/dir，每一段单独 encodeURIComponent
 */

import { state } from './store.js';
import { loadFiles, clearSearch } from './actions.js';

/**
 * 从 hash 提取路径。按段解码并容错：之前对整个 hash 做 decodeURIComponent，
 * 名字里有 `100%` 这种不成对的百分号时直接抛 URIError，导航就断了。
 */
function hashToPath() {
    return location.hash
        .replace(/^#\/?/, '')
        .split('/')
        .filter(Boolean)
        .map((segment) => {
            try {
                return decodeURIComponent(segment);
            } catch {
                return segment;
            }
        })
        .join('/');
}

/** 路径 → hash（每段编码，名字里的 `%`、`#`、`?` 都安全） */
export function pathToHash(path) {
    return '#/' + path.split('/').filter(Boolean).map(encodeURIComponent).join('/');
}

/** 导航到指定路径 */
export function navigate(path) {
    location.hash = pathToHash(path);
}

/** 上级目录 */
export function parentPath(path) {
    return path.includes('/') ? path.slice(0, path.lastIndexOf('/')) : '';
}

/** 初始化路由监听 */
export function initRouter() {
    const handler = (force) => {
        const path = hashToPath();
        if (force || state.currentPath !== path) {
            state.currentPath = path;
            state.selected = [];
            state.contextMenu = null;
            // 进入新目录就退出搜索：之前关键词会留下来继续过滤新目录
            clearSearch();
            document.title = path ? `${path.split('/').pop()} · FileTransfer` : 'FileTransfer';
            loadFiles(path);
        }
    };
    window.addEventListener('hashchange', () => handler(false));
    // 首次强制加载（根目录 hash 为空，currentPath 初值也为空，需强制触发）
    handler(true);
}
