/**
 * 复制文本：优先 Clipboard API；它只在安全上下文（https 或 localhost）可用，
 * 局域网里用 http://IP 访问时退回 execCommand。
 */
export async function copyText(text) {
    try {
        if (navigator.clipboard && window.isSecureContext) {
            await navigator.clipboard.writeText(text);
            return true;
        }
    } catch { /* 回退 */ }
    const ta = document.createElement('textarea');
    ta.value = text;
    ta.setAttribute('readonly', '');
    ta.style.cssText = 'position:fixed;top:-1000px;opacity:0';
    document.body.appendChild(ta);
    ta.select();
    try {
        return document.execCommand('copy');
    } catch {
        return false;
    } finally {
        ta.remove();
    }
}
