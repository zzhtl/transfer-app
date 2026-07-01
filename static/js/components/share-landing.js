/**
 * 公开分享落地页：浏览器访问 /s/{token} 时命中 SPA fallback，由本组件渲染。
 * 与常规应用隔离：文件→下载；目录→打包下载 +（非仅下载时）浏览。
 */

import { shareMeta, shareList, shareDownloadUrl, shareZipUrl } from '../api.js';
import { formatSize } from '../utils/format.js';

const ctx = { token: '', meta: null, code: '', subpath: '' };

export async function renderShareLanding() {
    ctx.token = decodeURIComponent(location.pathname.split('/')[2] || '');
    const app = document.getElementById('app');
    app.className = 'share-landing-page';
    app.innerHTML = `<div class="share-landing"><div class="landing-card"><p>加载中...</p></div></div>`;

    try {
        ctx.meta = await shareMeta(ctx.token);
    } catch (e) {
        renderError(e.status === 404 ? '链接不存在或已过期' : e.message);
        return;
    }
    renderCard();
}

function renderError(msg) {
    document.getElementById('app').innerHTML = `
        <div class="share-landing"><div class="landing-card">
            <h2>分享不可用</h2>
            <p class="landing-sub">${escapeHtml(msg)}</p>
        </div></div>`;
}

function renderCard() {
    const m = ctx.meta;
    const app = document.getElementById('app');
    const sizeStr = m.is_dir ? '目录' : formatSize(m.size);
    const codeField = m.requires_code
        ? `<input type="text" class="landing-code" placeholder="请输入提取码" value="${escapeAttr(ctx.code)}">`
        : '';

    let actions;
    if (m.is_dir) {
        actions = `<button class="btn btn-primary landing-zip">打包下载 ZIP</button>`;
    } else {
        actions = `<button class="btn btn-primary landing-download">下载</button>`;
    }

    app.innerHTML = `
        <div class="share-landing">
            <div class="landing-card">
                <div class="landing-icon">${m.is_dir ? '📁' : '📄'}</div>
                <h2 class="landing-name">${escapeHtml(m.name)}</h2>
                <p class="landing-sub">${escapeHtml(sizeStr)}</p>
                ${codeField}
                <div class="landing-actions">${actions}</div>
                <div class="landing-msg"></div>
                <div class="landing-browse"></div>
            </div>
        </div>`;

    const codeInput = app.querySelector('.landing-code');
    if (codeInput) {
        codeInput.addEventListener('input', () => { ctx.code = codeInput.value.trim(); });
    }
    app.querySelector('.landing-download')?.addEventListener('click', () =>
        triggerDownload(shareDownloadUrl(ctx.token, ctx.code)));
    app.querySelector('.landing-zip')?.addEventListener('click', () =>
        triggerDownload(shareZipUrl(ctx.token, ctx.code)));

    // 目录且非仅下载：加载可浏览列表
    if (m.is_dir && !m.download_only) {
        loadBrowse();
    }
}

async function loadBrowse() {
    const browseEl = document.querySelector('.landing-browse');
    if (!browseEl) return;
    browseEl.innerHTML = '<div class="landing-hint">加载目录...</div>';
    try {
        const data = await shareList(ctx.token, ctx.code, ctx.subpath);
        const entries = data.entries || [];
        const up = ctx.subpath
            ? `<div class="landing-item" data-up="1">
                <span class="landing-item-icon">⬆</span>
                <span class="landing-item-name">上级目录</span>
            </div>`
            : '';
        const rows = entries.map(e => `
            <div class="landing-item" data-path="${escapeAttr(e.path)}" data-dir="${e.is_dir}">
                <span class="landing-item-icon">${e.is_dir ? '📁' : '📄'}</span>
                <span class="landing-item-name">${escapeHtml(e.name)}</span>
                <span class="landing-item-size">${e.is_dir ? '' : formatSize(e.size)}</span>
            </div>`).join('');
        browseEl.innerHTML = up + (rows || '<div class="landing-hint">空目录</div>');

        browseEl.querySelectorAll('.landing-item').forEach(it => {
            it.addEventListener('click', () => {
                if (it.dataset.up) {
                    ctx.subpath = ctx.subpath.split('/').slice(0, -1).join('/');
                    loadBrowse();
                } else if (it.dataset.dir === 'true') {
                    ctx.subpath = it.dataset.path;
                    loadBrowse();
                } else {
                    triggerDownload(shareDownloadUrl(ctx.token, ctx.code, it.dataset.path));
                }
            });
        });
    } catch (e) {
        browseEl.innerHTML = `<div class="landing-hint">${e.status === 403 ? '提取码错误，无法浏览' : escapeHtml(e.message)}</div>`;
    }
}

/** 先用轻量请求校验（提取码/存在性），通过后再触发真实下载 */
async function triggerDownload(url) {
    const msgEl = document.querySelector('.landing-msg');
    if (msgEl) msgEl.textContent = '校验中...';
    try {
        const resp = await fetch(url, { headers: { Range: 'bytes=0-0' }, credentials: 'same-origin' });
        if (resp.body) { try { await resp.body.cancel(); } catch { /* ignore */ } }
        if (resp.status === 403) {
            if (msgEl) msgEl.textContent = '提取码错误';
            return;
        }
        if (resp.status >= 400) {
            if (msgEl) msgEl.textContent = '无法下载（链接可能已失效）';
            return;
        }
        if (msgEl) msgEl.textContent = '';
        const a = document.createElement('a');
        a.href = url;
        a.download = '';
        a.click();
    } catch (e) {
        if (msgEl) msgEl.textContent = `下载失败: ${e.message}`;
    }
}

function escapeHtml(text) {
    const d = document.createElement('div');
    d.textContent = text == null ? '' : text;
    return d.innerHTML;
}

function escapeAttr(text) {
    return String(text == null ? '' : text).replace(/"/g, '&quot;').replace(/</g, '&lt;');
}
