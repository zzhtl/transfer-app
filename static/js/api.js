/**
 * API 请求封装
 */

const BASE = '/api';

class ApiError extends Error {
    constructor(status, code, message) {
        super(message);
        this.status = status;
        this.code = code;
    }
}

async function request(method, path, opts = {}) {
    const { body, params, headers: extra } = opts;
    let url = `${BASE}${path}`;
    if (params) {
        const qs = new URLSearchParams(params).toString();
        if (qs) url += `?${qs}`;
    }
    const headers = { ...extra };
    let reqBody = body;
    if (body && typeof body === 'object' && !(body instanceof FormData) && !(body instanceof Blob)) {
        headers['Content-Type'] = 'application/json';
        reqBody = JSON.stringify(body);
    }
    const resp = await fetch(url, { method, headers, body: reqBody });
    if (!resp.ok) {
        let code = 'unknown';
        let msg = resp.statusText;
        try {
            const data = await resp.json();
            code = data.code || code;
            msg = data.message || msg;
        } catch { /* ignore */ }
        throw new ApiError(resp.status, code, msg);
    }
    const ct = resp.headers.get('content-type') || '';
    if (ct.includes('application/json')) return resp.json();
    return resp;
}

/** 文件列表 */
export function listFiles(path = '') {
    return request('GET', '/files', { params: { path } });
}

/** 创建目录 */
export function mkdir(path, name) {
    return request('POST', '/files/mkdir', { body: { path, name } });
}

/** 重命名 */
export function rename(path, newName) {
    return request('POST', '/files/rename', { body: { path, new_name: newName } });
}

/** 移动 */
export function moveEntry(src, dest) {
    return request('POST', '/files/move', { body: { src, dest } });
}

/** 复制 */
export function copyEntry(src, dest) {
    return request('POST', '/files/copy', { body: { src, dest } });
}

/** 批量删除 */
export function batchDelete(paths) {
    return request('POST', '/files/delete', { body: { paths } });
}

/** 搜索 */
export function search(path, query) {
    return request('GET', '/files/search', { params: { path, q: query } });
}

/** 获取下载 URL */
export function downloadUrl(path, asAttachment = true) {
    const encoded = path.split('/').map(encodeURIComponent).join('/');
    return asAttachment
        ? `${BASE}/download/${encoded}?download=1`
        : `${BASE}/download/${encoded}`;
}

/** ZIP 下载 URL */
export function zipDownloadUrl(paths) {
    const params = paths.map(p => `paths=${encodeURIComponent(p)}`).join('&');
    return `${BASE}/download-zip?${params}`;
}

/** 预览 URL */
export function previewUrl(path) {
    const encoded = path.split('/').map(encodeURIComponent).join('/');
    return `${BASE}/preview/${encoded}`;
}

export { ApiError };
