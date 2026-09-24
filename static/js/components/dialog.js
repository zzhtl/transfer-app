/**
 * 应用内对话框：替代 prompt / confirm。
 * 原生弹窗会阻塞页面、样式无法统一，也做不了实时校验。
 * 用法：const name = await promptDialog({...})  // 取消返回 null
 *      const ok = await confirmDialog({...})    // 取消返回 false
 */

import { escapeHtml, bringToFront, closeOnEscape } from '../utils/dom.js';

let overlayEl = null;
let dialogEl = null;
let active = null;

function ensureDom() {
    if (overlayEl) return;
    overlayEl = document.createElement('div');
    overlayEl.className = 'dialog-overlay';
    overlayEl.innerHTML = `
        <div class="dialog app-dialog" role="dialog" aria-modal="true" aria-labelledby="app-dialog-title">
            <h3 id="app-dialog-title" class="app-dialog-title"></h3>
            <div class="app-dialog-body"></div>
            <input class="app-dialog-input" type="text" autocomplete="off" spellcheck="false">
            <div class="app-dialog-error" aria-live="polite"></div>
            <div class="dialog-actions">
                <button type="button" class="btn btn-ghost app-dialog-cancel">取消</button>
                <button type="button" class="btn btn-primary app-dialog-confirm">确定</button>
            </div>
        </div>`;
    document.body.appendChild(overlayEl);
    dialogEl = overlayEl.querySelector('.app-dialog');

    overlayEl.addEventListener('mousedown', (e) => {
        if (e.target === overlayEl) finish(active?.input ? null : false);
    });
    overlayEl.querySelector('.app-dialog-cancel').addEventListener('click', () => finish(active?.input ? null : false));
    overlayEl.querySelector('.app-dialog-confirm').addEventListener('click', submit);
    overlayEl.querySelector('.app-dialog-input').addEventListener('input', () => {
        if (active) showError(active.validate?.(inputEl().value.trim()) || '');
    });
    overlayEl.addEventListener('keydown', onKeydown);
    closeOnEscape(overlayEl, () => finish(active?.input ? null : false));
}

const inputEl = () => overlayEl.querySelector('.app-dialog-input');

function showError(msg) {
    overlayEl.querySelector('.app-dialog-error').textContent = msg;
    overlayEl.querySelector('.app-dialog-confirm').disabled = !!msg;
}

function onKeydown(e) {
    if (!active) return;
    // 对话框打开时按键只归它处理，别让页面级快捷键（Esc 关预览、Delete 删文件）也响应
    e.stopPropagation();
    if (e.key === 'Enter' && !e.isComposing) {
        e.preventDefault();
        submit();
    } else if (e.key === 'Tab') {
        // 焦点只在对话框内循环
        const focusables = [...dialogEl.querySelectorAll('input, button')]
            .filter(el => el.offsetParent !== null && !el.disabled);
        const first = focusables[0];
        const last = focusables[focusables.length - 1];
        if (e.shiftKey && document.activeElement === first) {
            e.preventDefault();
            last.focus();
        } else if (!e.shiftKey && document.activeElement === last) {
            e.preventDefault();
            first.focus();
        }
    }
}

function submit() {
    if (!active) return;
    if (!active.input) {
        finish(true);
        return;
    }
    const value = inputEl().value.trim();
    const error = active.validate?.(value) || '';
    if (error) {
        showError(error);
        return;
    }
    finish(value);
}

function finish(result) {
    if (!active) return;
    const { resolve, restoreFocus } = active;
    active = null;
    overlayEl.classList.remove('active');
    restoreFocus?.focus?.();
    resolve(result);
}

function open({ title, message, items, input, confirmLabel, danger, validate }) {
    ensureDom();
    // 上一个还没关就当作取消
    if (active) finish(active.input ? null : false);

    overlayEl.querySelector('.app-dialog-title').textContent = title;
    const bodyEl = overlayEl.querySelector('.app-dialog-body');
    let body = message ? `<p>${escapeHtml(message)}</p>` : '';
    if (items?.length) {
        const shown = items.slice(0, 5).map(name => `<li>${escapeHtml(name)}</li>`).join('');
        const more = items.length > 5 ? `<li class="app-dialog-more">等共 ${items.length} 项</li>` : '';
        body += `<ul class="app-dialog-items">${shown}${more}</ul>`;
    }
    bodyEl.innerHTML = body;
    bodyEl.hidden = !body;

    const confirmBtn = overlayEl.querySelector('.app-dialog-confirm');
    confirmBtn.textContent = confirmLabel;
    confirmBtn.classList.toggle('btn-danger', !!danger);
    confirmBtn.classList.toggle('btn-primary', !danger);

    const field = inputEl();
    field.hidden = !input;
    overlayEl.querySelector('.app-dialog-error').hidden = !input;
    showError('');

    return new Promise((resolve) => {
        active = { resolve, input: !!input, validate, restoreFocus: document.activeElement };
        bringToFront(overlayEl);
        overlayEl.classList.add('active');
        if (input) {
            field.value = input.value || '';
            field.placeholder = input.placeholder || '';
            field.focus();
            // 重命名时只选中主名，改名时通常不动扩展名
            const dot = input.selectStem ? field.value.lastIndexOf('.') : -1;
            field.setSelectionRange(0, dot > 0 ? dot : field.value.length);
        } else {
            confirmBtn.focus();
        }
    });
}

/**
 * 输入对话框
 * @returns {Promise<string|null>} 确认返回去掉首尾空白的输入，取消返回 null
 */
export function promptDialog({
    title, message = '', value = '', placeholder = '', confirmLabel = '确定',
    selectStem = false, validate,
}) {
    return open({
        title, message, confirmLabel, validate,
        input: { value, placeholder, selectStem },
    });
}

/**
 * 确认对话框
 * @returns {Promise<boolean>}
 */
export function confirmDialog({ title, message = '', items = [], confirmLabel = '确定', danger = false }) {
    return open({ title, message, items, confirmLabel, danger });
}

/**
 * 文件/文件夹名校验，返回错误提示，合法时返回空串
 * @param {string} name
 * @param {string[]} existing 当前目录里已有的名字
 */
export function validateName(name, existing = []) {
    if (!name) return '名称不能为空';
    if (/[/\\]/.test(name)) return '名称里不能有 / 或 \\';
    if (name === '.' || name === '..') return '这个名称不能用';
    if (existing.includes(name)) return '当前文件夹里已有同名的项';
    return '';
}
