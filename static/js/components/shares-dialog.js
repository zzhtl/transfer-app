/**
 * 我的分享：列出全部分享，复制链接、看二维码、撤销。
 * 服务端只存提取码的哈希，这里只能标注「有提取码」，没法再显示明文。
 */

import { listShares, revokeShare, friendlyError } from '../api.js';
import { formatRemaining } from '../utils/format.js';
import { escapeHtml, icon, bringToFront, closeOnEscape } from '../utils/dom.js';
import { publicOrigin } from '../utils/origin.js';
import { copyText } from '../utils/clipboard.js';
import { showToast } from './toast.js';
import { confirmDialog } from './dialog.js';
import { openQrDialog } from './qr-dialog.js';

let overlayEl = null;
let listEl = null;
let shares = [];
let origin = location.origin;

function ensureDom() {
    if (overlayEl) return;
    overlayEl = document.createElement('div');
    overlayEl.className = 'dialog-overlay';
    overlayEl.innerHTML = `
        <div class="dialog shares-dialog" role="dialog" aria-modal="true" aria-labelledby="shares-title">
            <h3 id="shares-title">我的分享</h3>
            <div class="shares-list"></div>
            <div class="dialog-actions">
                <button type="button" class="btn btn-primary shares-close">关闭</button>
            </div>
        </div>`;
    document.body.appendChild(overlayEl);
    listEl = overlayEl.querySelector('.shares-list');

    overlayEl.addEventListener('mousedown', (e) => {
        if (e.target === overlayEl) close();
    });
    closeOnEscape(overlayEl, close);
    overlayEl.querySelector('.shares-close').addEventListener('click', close);
    listEl.addEventListener('click', onAction);
}

function close() {
    overlayEl.classList.remove('active');
}

export async function openSharesDialog() {
    ensureDom();
    bringToFront(overlayEl);
    overlayEl.classList.add('active');
    overlayEl.querySelector('.shares-close').focus();
    listEl.innerHTML = '<div class="shares-empty"><div class="spinner"></div></div>';
    try {
        [shares, origin] = await Promise.all([listShares(), publicOrigin()]);
        render();
    } catch (e) {
        listEl.innerHTML = `<div class="shares-empty">加载失败：${escapeHtml(friendlyError(e))}</div>`;
    }
}

function render() {
    if (!shares.length) {
        listEl.innerHTML = `<div class="shares-empty">${icon('share', 'empty-icon')}<p>还没有分享</p><p class="empty-sub">在文件上点右键或 ⋯，选「分享…」</p></div>`;
        return;
    }
    const now = Date.now() / 1000;
    listEl.innerHTML = shares.map((s) => {
        const remaining = s.expires_at - now;
        const expired = remaining <= 0;
        const tags = [
            expired ? '<span class="tag tag-muted">已过期</span>' : `<span class="tag">剩余 ${formatRemaining(remaining)}</span>`,
            s.has_code ? '<span class="tag">有提取码</span>' : '',
            s.download_only ? '<span class="tag">仅下载</span>' : '',
        ].join('');
        return `<div class="share-row${expired ? ' is-expired' : ''}" data-id="${escapeHtml(s.id)}">
            <div class="share-row-icon">${icon(s.is_dir ? 'folder' : 'file', s.is_dir ? 'ic-folder' : '')}</div>
            <div class="share-row-main">
                <div class="share-row-name" title="${escapeHtml(s.target)}">${escapeHtml(s.name)}</div>
                <div class="share-row-meta"><span class="share-row-path">${escapeHtml(s.target)}</span>${tags}</div>
            </div>
            <div class="share-row-actions">
                ${expired ? '' : `<button type="button" class="icon-btn" data-action="copy" title="复制链接" aria-label="复制链接">${icon('link')}</button>
                <button type="button" class="icon-btn" data-action="qr" title="二维码" aria-label="二维码">${icon('qr')}</button>`}
                <button type="button" class="icon-btn danger" data-action="revoke" title="撤销分享" aria-label="撤销分享">${icon('trash')}</button>
            </div>
        </div>`;
    }).join('');
}

async function onAction(e) {
    const btn = e.target.closest('[data-action]');
    if (!btn) return;
    const share = shares.find(s => s.id === btn.closest('.share-row').dataset.id);
    if (!share) return;
    const url = `${origin}${share.url}`;

    if (btn.dataset.action === 'copy') {
        const ok = await copyText(url);
        showToast(ok ? '链接已复制' : '复制失败，请手动复制', ok ? 'success' : 'error');
    } else if (btn.dataset.action === 'qr') {
        openQrDialog({ title: `分享「${share.name}」`, url, note: share.has_code ? '对方打开后还需要输入提取码' : '' });
    } else if (btn.dataset.action === 'revoke') {
        const ok = await confirmDialog({
            title: '撤销这个分享？',
            message: `撤销后，已经发出去的「${share.name}」链接会立即失效。`,
            confirmLabel: '撤销',
            danger: true,
        });
        if (!ok) return;
        try {
            await revokeShare(share.id);
            shares = shares.filter(s => s.id !== share.id);
            render();
            showToast('分享已撤销', 'success');
        } catch (err) {
            showToast(`撤销失败：${friendlyError(err)}`, 'error');
        }
    }
}
