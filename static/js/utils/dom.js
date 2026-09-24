/**
 * DOM 相关的小工具
 */

const ESCAPES = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' };

/**
 * HTML 转义，文本和属性值通用。
 * 用字符串替换而不是借 DOM 节点：列表一次要转义上万个名字，每次建节点太慢；
 * 属性值里的 `&` 也必须转义，否则名字里的 `&quot;` 会被浏览器还原成引号。
 */
export function escapeHtml(value) {
    return String(value ?? '').replace(/[&<>"']/g, ch => ESCAPES[ch]);
}

/** 焦点是否在可输入的控件里（此时不响应全局快捷键） */
export function isTypingTarget(el) {
    return !!el && (el.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(el.tagName));
}

/** 是否有模态层（对话框、预览、登录）盖在列表上 */
export function hasModalOpen() {
    return !!document.querySelector('.dialog-overlay.active, .preview-modal.open, .login-overlay.visible');
}

const scripts = new Map();

/** 按需加载经典脚本（如 tus），同一地址只加载一次 */
export function loadScript(src) {
    if (!scripts.has(src)) {
        scripts.set(src, new Promise((resolve, reject) => {
            const el = document.createElement('script');
            el.src = src;
            el.onload = resolve;
            el.onerror = () => {
                scripts.delete(src);
                reject(new Error(`加载 ${src} 失败`));
            };
            document.head.appendChild(el);
        }));
    }
    return scripts.get(src);
}

/** SVG sprite 图标 */
export function icon(name, cls = '') {
    return `<svg class="ic ${cls}" aria-hidden="true"><use href="#i-${name}"></use></svg>`;
}

/**
 * 打开对话框时移到 body 末尾：几个对话框的 z-index 相同，叠放次序取决于 DOM 顺序，
 * 先创建的确认框否则会被后打开的「我的分享」压在下面。
 */
export function bringToFront(overlayEl) {
    if (overlayEl.nextElementSibling) document.body.appendChild(overlayEl);
}

/**
 * Esc 关闭最上层的对话框。在 document 捕获阶段处理，而不是只监听 overlay：
 * 焦点不一定在对话框里（比如触发它的按钮已经从列表里移除），那时 overlay 收不到按键。
 */
export function closeOnEscape(overlayEl, close) {
    document.addEventListener('keydown', (e) => {
        if (e.key !== 'Escape' || !overlayEl.classList.contains('active')) return;
        const open = document.querySelectorAll('.dialog-overlay.active');
        if (open[open.length - 1] !== overlayEl) return;
        e.preventDefault();
        e.stopImmediatePropagation();
        close();
    }, true);
}
