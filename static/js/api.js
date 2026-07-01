/**
 * API 请求封装
 */

import { state } from './store.js';

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
    const resp = await fetch(url, { method, headers, body: reqBody, credentials: 'same-origin' });
    if (!resp.ok) {
        // 会话失效/未登录：唤出登录遮罩（登录端点自身除外，其错误由登录框展示）
        if (resp.status === 401 && path !== '/auth/login') {
            state.authRequired = true;
        }
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

/** 登录 */
export function login(password) {
    return request('POST', '/auth/login', { body: { password } });
}

/** 登出 */
export function logout() {
    return request('POST', '/auth/logout');
}

/** 鉴权状态：{ auth_required, authenticated } */
export function authStatus() {
    return request('GET', '/auth/status');
}

/** 创建分享 */
export function createShare(body) {
    return request('POST', '/share', { body });
}

/** 列出分享 */
export function listShares() {
    return request('GET', '/share');
}

/** 吊销分享 */
export function revokeShare(id) {
    return request('DELETE', `/share/${id}`);
}

/** 分享公开元信息 */
export function shareMeta(token) {
    return request('GET', `/s/${token}`);
}

/** 分享目录浏览 */
export function shareList(token, code, path = '') {
    return request('GET', `/s/${token}/list`, { params: { code: code || '', path } });
}

/** 分享下载 URL（浏览器直接导航） */
export function shareDownloadUrl(token, code, subpath) {
    const p = new URLSearchParams();
    if (code) p.set('code', code);
    if (subpath) p.set('path', subpath);
    const qs = p.toString();
    return `${BASE}/s/${token}/download${qs ? '?' + qs : ''}`;
}

/** 分享 ZIP 下载 URL */
export function shareZipUrl(token, code) {
    const qs = code ? `?code=${encodeURIComponent(code)}` : '';
    return `${BASE}/s/${token}/zip${qs}`;
}

/** 读取文本文件完整内容（在线编辑） */
export async function getContent(path) {
    const resp = await request('GET', '/files/content', { params: { path } });
    return resp.text();
}

/** 保存文本文件 */
export function saveFile(path, content) {
    return request('POST', '/files/save', { body: { path, content } });
}

/** 渲染 markdown 片段（编辑实时预览） */
export async function renderMarkdown(content) {
    const resp = await request('POST', '/preview/markdown', { body: { content } });
    return resp.text();
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

/** 移动：source 为源相对路径，destination 为目标目录相对路径 */
export function moveEntry(source, destination) {
    return request('POST', '/files/move', { body: { source, destination } });
}

/** 复制：source 为源相对路径，destination 为目标目录相对路径 */
export function copyEntry(source, destination) {
    return request('POST', '/files/copy', { body: { source, destination } });
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
