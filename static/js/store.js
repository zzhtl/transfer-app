/**
 * Proxy-based 响应式状态管理
 * 订阅 state 变化，自动触发 UI 更新
 */

const listeners = new Map();
let batchQueue = null;

/** 创建响应式代理 */
function createReactive(target, path = '') {
    return new Proxy(target, {
        get(obj, key) {
            const val = obj[key];
            if (val && typeof val === 'object' && !Array.isArray(val)) {
                return createReactive(val, path ? `${path}.${key}` : key);
            }
            return val;
        },
        set(obj, key, value) {
            const old = obj[key];
            if (old === value) return true;
            obj[key] = value;
            const fullKey = path ? `${path}.${key}` : key;
            notify(fullKey);
            return true;
        }
    });
}

/** 批量更新：合并同一微任务内的多次变更 */
function notify(key) {
    if (!batchQueue) {
        batchQueue = new Set();
        queueMicrotask(flush);
    }
    batchQueue.add(key);
}

function flush() {
    const keys = batchQueue;
    batchQueue = null;
    for (const key of keys) {
        const parts = key.split('.');
        for (let i = parts.length; i > 0; i--) {
            const prefix = parts.slice(0, i).join('.');
            const fns = listeners.get(prefix);
            if (fns) fns.forEach(fn => fn());
        }
        // 通配符监听
        const fns = listeners.get('*');
        if (fns) fns.forEach(fn => fn());
    }
}

/** 初始状态 */
const raw = {
    /** 当前路径 */
    currentPath: '',
    /** 文件列表 */
    files: [],
    /** 加载状态 */
    loading: false,
    /** 排序：name | size | modified */
    sortBy: 'name',
    /** 排序方向 */
    sortAsc: true,
    /** 过滤关键词 */
    filterText: '',
    /** 搜索结果 */
    searchResults: null,
    /** 选中的文件（Set 序列化为数组） */
    selected: [],
    /** 上传队列 */
    uploads: [],
    /** 上传面板是否展开 */
    uploadPanelOpen: false,
    /** 预览文件信息 */
    preview: null,
    /** 主题: light | dark | auto */
    theme: localStorage.getItem('theme') || 'auto',
    /** 右键菜单 */
    contextMenu: null,
    /** 视图模式: list | grid */
    viewMode: localStorage.getItem('viewMode') || 'list',
};

export const state = createReactive(raw);

/** 订阅状态变化 */
export function subscribe(key, fn) {
    if (!listeners.has(key)) listeners.set(key, new Set());
    listeners.get(key).add(fn);
    return () => listeners.get(key).delete(fn);
}

/** 获取原始（非代理）状态 */
export function getRaw() {
    return raw;
}
