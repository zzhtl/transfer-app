/**
 * 分享弹窗：对文件/目录创建带有效期/提取码/仅下载的公开链接。
 * 复用 .dialog 样式。二维码在放置了 vendored QR 库（window.QRCode）时渲染，否则仅显示链接。
 */

import { createShare } from '../api.js';
import { showToast } from './toast.js';

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
        <div class="dialog share-dialog">
            <h3 class="share-title">创建分享</h3>

            <div class="share-form">
                <div class="share-target"></div>
                <label class="share-field">有效期
                    <select class="share-expiry">
                        ${EXPIRY_OPTIONS.map((o, i) => `<option value="${o.secs}"${i === 2 ? ' selected' : ''}>${o.label}</option>`).join('')}
                    </select>
                </label>
                <label class="share-field">提取码（可选）
                    <input type="text" class="share-code" placeholder="留空则无需提取码">
                </label>
                <label class="share-check">
                    <input type="checkbox" class="share-download-only"> 仅下载（不暴露目录浏览）
                </label>
                <div class="dialog-actions">
                    <button class="btn btn-ghost share-cancel">取消</button>
                    <button class="btn btn-primary share-create">创建链接</button>
                </div>
            </div>

            <div class="share-result" style="display:none">
                <div class="share-link-row">
                    <input type="text" class="share-link" readonly>
                    <button class="btn btn-sm btn-primary share-copy">复制</button>
                </div>
                <div class="share-code-hint"></div>
                <div class="share-qr"></div>
                <div class="dialog-actions">
                    <button class="btn btn-primary share-done">完成</button>
                </div>
            </div>
        </div>`;
    document.body.appendChild(overlayEl);

    overlayEl.addEventListener('click', (e) => {
        if (e.target === overlayEl) close();
    });
    overlayEl.querySelector('.share-cancel').addEventListener('click', close);
    overlayEl.querySelector('.share-done').addEventListener('click', close);
    overlayEl.querySelector('.share-create').addEventListener('click', doCreate);
    overlayEl.querySelector('.share-copy').addEventListener('click', doCopy);
}

export function openShareModal(path) {
    ensureDom();
    targetPath = path;
    overlayEl.querySelector('.share-target').textContent = `目标：${path}`;
    overlayEl.querySelector('.share-form').style.display = '';
    overlayEl.querySelector('.share-result').style.display = 'none';
    overlayEl.querySelector('.share-code').value = '';
    overlayEl.querySelector('.share-download-only').checked = false;
    overlayEl.querySelector('.share-qr').innerHTML = '';
    overlayEl.classList.add('active');
}

function close() {
    overlayEl.classList.remove('active');
}

async function doCreate() {
    const secs = Number(overlayEl.querySelector('.share-expiry').value);
    const code = overlayEl.querySelector('.share-code').value.trim();
    const downloadOnly = overlayEl.querySelector('.share-download-only').checked;
    const btn = overlayEl.querySelector('.share-create');
    btn.disabled = true;
    try {
        const info = await createShare({
            path: targetPath,
            expires_in_secs: secs,
            code: code || undefined,
            download_only: downloadOnly,
        });
        showResult(info, code);
    } catch (e) {
        showToast(`创建分享失败: ${e.message}`, 'error');
    } finally {
        btn.disabled = false;
    }
}

function showResult(info, code) {
    const fullUrl = `${location.origin}${info.url}`;
    overlayEl.querySelector('.share-form').style.display = 'none';
    overlayEl.querySelector('.share-result').style.display = '';
    overlayEl.querySelector('.share-link').value = fullUrl;
    overlayEl.querySelector('.share-code-hint').textContent = code ? `提取码：${code}` : '';

    // 二维码：仅当放置了 vendored QR 库时渲染
    const qrEl = overlayEl.querySelector('.share-qr');
    qrEl.innerHTML = '';
    if (typeof window.QRCode === 'function') {
        try {
            new window.QRCode(qrEl, { text: fullUrl, width: 176, height: 176 });
        } catch { /* 忽略二维码渲染失败 */ }
    }
}

async function doCopy() {
    const url = overlayEl.querySelector('.share-link').value;
    const ok = await copyText(url);
    showToast(ok ? '链接已复制' : '复制失败，请手动复制', ok ? 'success' : 'error');
}

/** 复制文本：优先 Clipboard API，回退到 execCommand（http 明文场景） */
async function copyText(text) {
    try {
        if (navigator.clipboard && window.isSecureContext) {
            await navigator.clipboard.writeText(text);
            return true;
        }
    } catch { /* 回退 */ }
    try {
        const input = overlayEl.querySelector('.share-link');
        input.focus();
        input.select();
        return document.execCommand('copy');
    } catch {
        return false;
    }
}
