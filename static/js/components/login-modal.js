/**
 * 登录遮罩：仅在 state.authRequired 为 true 时显示。
 * 登录成功后清除 authRequired 并回调 onAuthed（首次启动加载 / 会话恢复）。
 */

import { state, subscribe } from '../store.js';
import { login } from '../api.js';

let overlayEl = null;

export function initLoginModal(onAuthed) {
    overlayEl = document.createElement('div');
    overlayEl.className = 'login-overlay';
    overlayEl.innerHTML = `
        <div class="login-card">
            <h2 class="login-title">需要登录</h2>
            <p class="login-hint">请输入访问密码</p>
            <input type="password" class="login-input" placeholder="密码" autocomplete="current-password">
            <div class="login-error"></div>
            <button class="btn btn-primary login-submit">登录</button>
        </div>`;
    document.body.appendChild(overlayEl);

    const input = overlayEl.querySelector('.login-input');
    const submit = overlayEl.querySelector('.login-submit');
    const errEl = overlayEl.querySelector('.login-error');

    async function doLogin() {
        const pw = input.value;
        if (!pw) return;
        submit.disabled = true;
        errEl.textContent = '';
        try {
            await login(pw);
            input.value = '';
            state.authRequired = false;
            if (onAuthed) onAuthed();
        } catch (e) {
            errEl.textContent = e.status === 401 ? '密码错误' : `登录失败: ${e.message}`;
        } finally {
            submit.disabled = false;
        }
    }

    submit.addEventListener('click', doLogin);
    input.addEventListener('keydown', (e) => {
        if (e.key === 'Enter') doLogin();
    });

    subscribe('authRequired', () => {
        overlayEl.classList.toggle('visible', state.authRequired);
        if (state.authRequired) setTimeout(() => input.focus(), 50);
    });
}
