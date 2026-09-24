/**
 * 二维码弹窗：手机扫码访问本机、扫码打开分享链接
 */

import { renderQr } from '../utils/qr.js';
import { copyText } from '../utils/clipboard.js';
import { showToast } from './toast.js';
import { bringToFront, closeOnEscape } from '../utils/dom.js';

let overlayEl = null;

function ensureDom() {
    if (overlayEl) return;
    overlayEl = document.createElement('div');
    overlayEl.className = 'dialog-overlay';
    overlayEl.innerHTML = `
        <div class="dialog qr-dialog" role="dialog" aria-modal="true" aria-labelledby="qr-dialog-title">
            <h3 id="qr-dialog-title" class="qr-dialog-title"></h3>
            <div class="qr-box"></div>
            <div class="qr-url"></div>
            <p class="qr-note"></p>
            <div class="dialog-actions">
                <button type="button" class="btn btn-ghost qr-copy">复制链接</button>
                <button type="button" class="btn btn-primary qr-close">完成</button>
            </div>
        </div>`;
    document.body.appendChild(overlayEl);
    overlayEl.addEventListener('mousedown', (e) => {
        if (e.target === overlayEl) close();
    });
    overlayEl.querySelector('.qr-close').addEventListener('click', close);
    overlayEl.querySelector('.qr-copy').addEventListener('click', async () => {
        const ok = await copyText(overlayEl.querySelector('.qr-url').textContent);
        showToast(ok ? '链接已复制' : '复制失败，请手动复制', ok ? 'success' : 'error');
    });
    closeOnEscape(overlayEl, close);
}

function close() {
    overlayEl.classList.remove('active');
}

/** @param {{title: string, url: string, note?: string}} opts */
export function openQrDialog({ title, url, note = '' }) {
    ensureDom();
    overlayEl.querySelector('.qr-dialog-title').textContent = title;
    overlayEl.querySelector('.qr-url').textContent = url;
    const noteEl = overlayEl.querySelector('.qr-note');
    noteEl.textContent = note;
    noteEl.hidden = !note;
    const box = overlayEl.querySelector('.qr-box');
    box.innerHTML = '<div class="spinner"></div>';
    renderQr(box, url).catch(() => {
        box.textContent = '二维码生成失败';
    });
    bringToFront(overlayEl);
    overlayEl.classList.add('active');
    overlayEl.querySelector('.qr-close').focus();
}
