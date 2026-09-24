/**
 * 分享弹窗：对文件/目录创建带有效期/提取码/仅下载的公开链接。
 * 复用 .dialog 样式；创建成功后给出链接和二维码。
 */

import { createShare, friendlyError } from '../api.js';
import { showToast } from './toast.js';
import { baseName } from '../actions.js';
import { publicOrigin } from '../utils/origin.js';
import { copyText } from '../utils/clipboard.js';
import { renderQr } from '../utils/qr.js';
import { bringToFront, closeOnEscape } from '../utils/dom.js';

const EXPIRY_OPTIONS = [
    { label: '1 小时', secs: 3600 },
    { label: '1 天', secs: 86400 },
    { label: '7 天', secs: 604800 },
    { label: '30 天', secs: 2592000 },
];

let overlayEl = null;
let targetPath = '';

function ensureDom() {
    if (overlayEl) return;
    overlayEl = document.createElement('div');
    overlayEl.className = 'dialog-overlay';
    overlayEl.innerHTML = `
        <div class="dialog share-dialog" role="dialog" aria-modal="true" aria-labelledby="share-title">
            <h3 class="share-title" id="share-title">创建分享</h3>

            <div class="share-form">
                <div class="share-target"></div>
                <label class="share-field">有效期
                    <select class="share-expiry">
                        ${EXPIRY_OPTIONS.map((o, i) => `<option value="${o.secs}"${i === 2 ? ' selected' : ''}>${o.label}</option>`).join('')}
                    </select>
                </label>
                <label class="share-field">提取码（可选）
                    <input type="text" class="share-code" placeholder="留空则无需提取码" autocomplete="off">
                </label>
                <label class="share-check">
                    <input type="checkbox" class="share-download-only"> 仅下载（不允许浏览目录内容）
                </label>
                <div class="dialog-actions">
                    <button type="button" class="btn btn-ghost share-cancel">取消</button>
                    <button type="button" class="btn btn-primary share-create">创建链接</button>
                </div>
            </div>

            <div class="share-result" hidden>
                <div class="share-link-row">
                    <input type="text" class="share-link" readonly aria-label="分享链接">
                    <button type="button" class="btn btn-sm btn-primary share-copy">复制</button>
                </div>
                <div class="share-code-hint"></div>
                <div class="qr-box share-qr"></div>
                <div class="dialog-actions">
                    <button type="button" class="btn btn-primary share-done">完成</button>
                </div>
            </div>
        </div>`;
    document.body.appendChild(overlayEl);

    overlayEl.addEventListener('mousedown', (e) => {
        if (e.target === overlayEl) close();
    });
    overlayEl.addEventListener('keydown', (e) => {
        if (e.key === 'Enter' && e.target.classList.contains('share-code')) doCreate();
    });
    closeOnEscape(overlayEl, close);
    overlayEl.querySelector('.share-cancel').addEventListener('click', close);
    overlayEl.querySelector('.share-done').addEventListener('click', close);
    overlayEl.querySelector('.share-create').addEventListener('click', doCreate);
    overlayEl.querySelector('.share-copy').addEventListener('click', doCopy);
}

export function openShareModal(path) {
    ensureDom();
    targetPath = path;
    overlayEl.querySelector('.share-target').textContent = `「${baseName(path)}」`;
    overlayEl.querySelector('.share-target').title = path;
    overlayEl.querySelector('.share-form').hidden = false;
    overlayEl.querySelector('.share-result').hidden = true;
    overlayEl.querySelector('.share-code').value = '';
    overlayEl.querySelector('.share-download-only').checked = false;
    overlayEl.querySelector('.share-qr').innerHTML = '';
    bringToFront(overlayEl);
    overlayEl.classList.add('active');
    overlayEl.querySelector('.share-expiry').focus();
}

function close() {
    overlayEl.classList.remove('active');
}

async function doCreate() {
    const secs = Number(overlayEl.querySelector('.share-expiry').value);
    const code = overlayEl.querySelector('.share-code').value.trim();
    const downloadOnly = overlayEl.querySelector('.share-download-only').checked;
    const btn = overlayEl.querySelector('.share-create');
    if (btn.disabled) return;
    btn.disabled = true;
    try {
        const info = await createShare({
            path: targetPath,
            expires_in_secs: secs,
            code: code || undefined,
            download_only: downloadOnly,
        });
        await showResult(info, code);
    } catch (e) {
        showToast(`创建分享失败：${friendlyError(e)}`, 'error');
    } finally {
        btn.disabled = false;
    }
}

async function showResult(info, code) {
    const fullUrl = `${await publicOrigin()}${info.url}`;
    overlayEl.querySelector('.share-form').hidden = true;
    overlayEl.querySelector('.share-result').hidden = false;
    const linkEl = overlayEl.querySelector('.share-link');
    linkEl.value = fullUrl;
    overlayEl.querySelector('.share-code-hint').textContent = code ? `提取码：${code}（请与链接一起发给对方）` : '';
    overlayEl.querySelector('.share-copy').focus();
    linkEl.select();

    const qrEl = overlayEl.querySelector('.share-qr');
    renderQr(qrEl, fullUrl).catch(() => {
        qrEl.innerHTML = '';
    });
}

async function doCopy() {
    const ok = await copyText(overlayEl.querySelector('.share-link').value);
    showToast(ok ? '链接已复制' : '复制失败，请手动复制', ok ? 'success' : 'error');
}
