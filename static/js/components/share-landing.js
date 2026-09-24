/**
 * 公开分享落地页：浏览器访问 /s/{token} 时命中 SPA fallback，由本组件渲染。
 * 与常规应用隔离：文件→下载；目录→打包下载 +（非仅下载时）浏览。
 */

import { shareMeta, shareList, shareDownloadUrl, shareZipUrl } from '../api.js';
import { formatSize, formatRemaining, fileIcon } from '../utils/format.js';
import { escapeHtml, icon } from '../utils/dom.js';

const ctx = { token: '', meta: null, code: '', subpath: '' };
// 与主界面一致：目录在前，名字按自然顺序（第2集在第10集前面）
const collator = new Intl.Collator('zh-CN', { numeric: true, sensitivity: 'base' });

export async function renderShareLanding() {
    ctx.token = decodeURIComponent(location.pathname.split('/')[2] || '');
    const app = document.getElementById('app');
    app.className = 'share-landing-page';
    app.innerHTML = `<div class="share-landing"><div class="landing-card"><div class="spinner"></div></div></div>`;

    try {
        ctx.meta = await shareMeta(ctx.token);
    } catch (e) {
        renderError(e.status === 404 ? '链接不存在或已过期' : '暂时无法打开这个分享，请稍后再试');
        return;
    }
    document.title = `${ctx.meta.name} · 分享`;
    renderCard();
}

function renderError(msg) {
    document.getElementById('app').innerHTML = `
        <div class="share-landing"><div class="landing-card">
            <div class="landing-icon">${icon('link-off', 'landing-icon-muted')}</div>
            <h2>分享不可用</h2>
            <p class="landing-sub">${escapeHtml(msg)}</p>
        </div></div>`;
}

function renderCard() {
    const m = ctx.meta;
    const app = document.getElementById('app');
    const expires = formatRemaining(m.expires_at - Date.now() / 1000);
    const sub = `${m.is_dir ? '文件夹' : formatSize(m.size)} · ${expires === '已过期' ? expires : `${expires}后过期`}`;
    const codeField = m.requires_code
        ? `<input type="text" class="landing-code" placeholder="请输入提取码" autocomplete="off" value="${escapeHtml(ctx.code)}" aria-label="提取码">`
        : '';
    const action = m.is_dir
        ? `<button class="btn btn-primary landing-primary landing-zip">${icon('archive')}打包下载</button>`
        : `<button class="btn btn-primary landing-primary landing-download">${icon('download')}下载</button>`;

    app.innerHTML = `
        <div class="share-landing">
            <div class="landing-card">
                <div class="landing-icon">${m.is_dir ? icon('folder', 'ic-folder') : fileIcon({ name: m.name, is_dir: false })}</div>
                <h2 class="landing-name">${escapeHtml(m.name)}</h2>
                <p class="landing-sub">${escapeHtml(sub)}</p>
                ${codeField}
                <div class="landing-actions">${action}</div>
                <div class="landing-msg" role="status"></div>
                <div class="landing-browse"></div>
            </div>
        </div>`;

    const codeInput = app.querySelector('.landing-code');
    if (codeInput) {
        codeInput.addEventListener('input', () => {
            ctx.code = codeInput.value.trim();
        });
        codeInput.addEventListener('keydown', (e) => {
            if (e.key === 'Enter') {
                // 回车：目录就用新提取码重新加载列表，文件直接下载
                if (m.is_dir && !m.download_only) loadBrowse();
                else app.querySelector('.landing-primary')?.click();
            }
        });
        codeInput.focus();
    }
    app.querySelector('.landing-download')?.addEventListener('click', () =>
        triggerDownload(shareDownloadUrl(ctx.token, ctx.code)));
    app.querySelector('.landing-zip')?.addEventListener('click', () =>
        triggerDownload(shareZipUrl(ctx.token, ctx.code)));

    // 目录且非仅下载：加载可浏览列表
    if (m.is_dir && !m.download_only && (!m.requires_code || ctx.code)) {
        loadBrowse();
    } else if (m.is_dir && !m.download_only) {
        document.querySelector('.landing-browse').innerHTML = '<div class="landing-hint">输入提取码后回车即可浏览目录</div>';
    }
}

async function loadBrowse() {
    const browseEl = document.querySelector('.landing-browse');
    if (!browseEl) return;
    browseEl.innerHTML = '<div class="landing-hint">加载目录…</div>';
    try {
        const data = await shareList(ctx.token, ctx.code, ctx.subpath);
        const entries = (data.entries || []).sort((a, b) =>
            (a.is_dir === b.is_dir ? 0 : a.is_dir ? -1 : 1) || collator.compare(a.name, b.name));
        const up = ctx.subpath
            ? `<button type="button" class="landing-item" data-up="1">
                <span class="landing-item-icon">${icon('arrow-up')}</span>
                <span class="landing-item-name">上级文件夹</span>
            </button>`
            : '';
        const rows = entries.map(e => `
            <button type="button" class="landing-item" data-path="${escapeHtml(e.path)}" data-dir="${e.is_dir}">
                <span class="landing-item-icon">${fileIcon(e)}</span>
                <span class="landing-item-name">${escapeHtml(e.name)}</span>
                <span class="landing-item-size">${e.is_dir ? '' : formatSize(e.size)}</span>
            </button>`).join('');
        const where = ctx.subpath ? `<div class="landing-where">${escapeHtml(ctx.subpath)}</div>` : '';
        browseEl.innerHTML = where + up + (rows || '<div class="landing-hint">空文件夹</div>');

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
        browseEl.innerHTML = `<div class="landing-hint">${e.status === 403 ? '提取码不正确，无法浏览' : escapeHtml(e.message)}</div>`;
    }
}

/** 先用轻量请求校验（提取码/存在性），通过后再触发真实下载 */
async function triggerDownload(url) {
    const msgEl = document.querySelector('.landing-msg');
    if (msgEl) msgEl.textContent = '校验中…';
    try {
        const resp = await fetch(url, { headers: { Range: 'bytes=0-0' }, credentials: 'same-origin' });
        if (resp.body) { try { await resp.body.cancel(); } catch { /* ignore */ } }
        if (resp.status === 403) {
            if (msgEl) msgEl.textContent = '提取码不正确';
            document.querySelector('.landing-code')?.focus();
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
        document.body.appendChild(a);
        a.click();
        a.remove();
    } catch (e) {
        if (msgEl) msgEl.textContent = `下载失败：${e.message}`;
    }
}
