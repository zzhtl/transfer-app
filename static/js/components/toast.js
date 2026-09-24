/**
 * Toast 通知组件
 */

const MAX_VISIBLE = 4;

let container = null;

function getContainer() {
    if (!container) {
        container = document.createElement('div');
        container.className = 'toast-container';
        container.setAttribute('aria-live', 'polite');
        document.body.appendChild(container);
    }
    return container;
}

function dismiss(el) {
    if (el.dataset.leaving) return;
    el.dataset.leaving = '1';
    el.classList.remove('toast-show');
    el.addEventListener('transitionend', () => el.remove(), { once: true });
    // 兜底移除
    setTimeout(() => el.remove(), 500);
}

/**
 * 显示 toast 通知；点击可提前关闭，错误默认多停留一会儿
 * @param {string} message
 * @param {'info'|'success'|'error'|'warning'} type
 * @param {number} [duration] ms
 */
export function showToast(message, type = 'info', duration) {
    const el = document.createElement('div');
    el.className = `toast toast-${type}`;
    el.setAttribute('role', type === 'error' ? 'alert' : 'status');
    el.textContent = message;
    el.addEventListener('click', () => dismiss(el));

    const c = getContainer();
    c.appendChild(el);

    // 同时最多显示几条，多余的从最早的开始收起
    const live = [...c.children].filter(t => !t.dataset.leaving);
    for (const old of live.slice(0, Math.max(0, live.length - MAX_VISIBLE))) dismiss(old);

    // 触发进入动画
    requestAnimationFrame(() => el.classList.add('toast-show'));

    setTimeout(() => dismiss(el), duration ?? (type === 'error' || type === 'warning' ? 5000 : 3000));
}
