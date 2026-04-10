/**
 * 主题切换组件
 */

import { state, subscribe } from '../store.js';

const ICONS = {
    light: `<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="5"/><path d="M12 1v2M12 21v2M4.22 4.22l1.42 1.42M18.36 18.36l1.42 1.42M1 12h2M21 12h2M4.22 19.78l1.42-1.42M18.36 5.64l1.42-1.42"/></svg>`,
    dark: `<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 12.79A9 9 0 1111.21 3 7 7 0 0021 12.79z"/></svg>`,
    auto: `<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><path d="M12 2a10 10 0 000 20V2z" fill="currentColor" opacity="0.3"/></svg>`,
};

const CYCLE = ['auto', 'light', 'dark'];

function applyTheme(theme) {
    const root = document.documentElement;
    if (theme === 'auto') {
        root.removeAttribute('data-theme');
    } else {
        root.setAttribute('data-theme', theme);
    }
    localStorage.setItem('theme', theme);
}

export function initThemeToggle() {
    const btn = document.getElementById('theme-toggle');
    if (!btn) return;

    const render = () => {
        const theme = state.theme;
        btn.innerHTML = ICONS[theme] || ICONS.auto;
        btn.title = `当前: ${theme === 'auto' ? '跟随系统' : theme === 'light' ? '浅色' : '深色'}`;
        applyTheme(theme);
    };

    btn.addEventListener('click', () => {
        const idx = CYCLE.indexOf(state.theme);
        state.theme = CYCLE[(idx + 1) % CYCLE.length];
    });

    subscribe('theme', render);
    render();
}
