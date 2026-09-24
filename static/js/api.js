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
    const { body, params, headers: extra, signal } = opts;
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
    let resp;
    try {
        resp = await fetch(url, { method, headers, body: reqBody, credentials: 'same-origin', signal });
    } catch (e) {
        if (e.name === 'AbortError') throw e;
        throw new ApiError(0, 'network', '无法连接服务器');
    }
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

const MESSAGES = {
    network: '无法连接服务器，请检查网络',
    not_found: '文件或文件夹不存在',
    already_exists: '已存在同名的文件或文件夹',
    permission_denied: '服务器上没有这个文件的访问权限',
    forbidden: '没有权限执行此操作',
    path_traversal: '路径不合法',
    too_large: '文件太大',
    is_directory: '这是一个文件夹',
    not_directory: '这不是一个文件夹',
    unauthorized: '请先登录',
    checksum_mismatch: '文件校验失败，请重新上传',
    offset_conflict: '上传进度不一致，请重试',
};

/** 把错误转成给人看的中文提示；服务端的英文消息只作兜底 */
export function friendlyError(e) {
    if (e?.code === 'bad_request' && /into itself/.test(e.message)) {
        return '不能移动或复制到它自身或其子文件夹中';
    }
    return MESSAGES[e?.code] || e?.message || '操作失败';
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

/** 服务信息：{ version, lan_origin }，lan_origin 为局域网可访问地址 */
export function serverInfo() {
    return request('GET', '/server-info');
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
    return request('DELETE', `/share/${encodeURIComponent(id)}`);
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
export function listFiles(path = '', { signal } = {}) {
    return request('GET', '/files', { params: { path }, signal });
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
export function search(path, query, { signal } = {}) {
    return request('GET', '/files/search', { params: { path, q: query }, signal });
}

/** 获取下载 URL */
export function downloadUrl(path, asAttachment = true) {
    const encoded = path.split('/').map(encodeURIComponent).join('/');
    return asAttachment
        ? `${BASE}/download/${encoded}?download=1`
        : `${BASE}/download/${encoded}`;
}

/** ZIP 下载 URL：paths 用重复参数传，每个值是一个完整路径 */
export function zipDownloadUrl(paths, name) {
    const p = new URLSearchParams();
    for (const path of paths) p.append('paths', path);
    if (name) p.set('name', name);
    return `${BASE}/download-zip?${p}`;
}

/** 预览 URL */
export function previewUrl(path) {
    const encoded = path.split('/').map(encodeURIComponent).join('/');
    return `${BASE}/preview/${encoded}`;
}

export { ApiError };
