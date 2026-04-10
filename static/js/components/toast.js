/**
 * Toast 通知组件
 */

let container = null;

function getContainer() {
    if (!container) {
        container = document.createElement('div');
        container.className = 'toast-container';
        document.body.appendChild(container);
    }
    return container;
}

/**
 * 显示 toast 通知
 * @param {string} message
 * @param {'info'|'success'|'error'|'warning'} type
 * @param {number} duration ms
 */
export function showToast(message, type = 'info', duration = 3000) {
    const el = document.createElement('div');
    el.className = `toast toast-${type}`;
    el.textContent = message;

    const c = getContainer();
    c.appendChild(el);

    // 触发进入动画
    requestAnimationFrame(() => el.classList.add('toast-show'));

    setTimeout(() => {
        el.classList.remove('toast-show');
        el.addEventListener('transitionend', () => el.remove(), { once: true });
        // 兜底移除
        setTimeout(() => el.remove(), 500);
    }, duration);
}
