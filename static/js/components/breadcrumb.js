/**
 * 面包屑导航组件
 */

import { state, subscribe } from '../store.js';
import { navigate } from '../router.js';

export function initBreadcrumb() {
    const el = document.getElementById('breadcrumb');
    if (!el) return;

    const render = () => {
        const path = state.currentPath;
        const parts = path ? path.split('/').filter(Boolean) : [];

        let html = `<a class="breadcrumb-item breadcrumb-root" href="#/" data-path="">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M3 9l9-7 9 7v11a2 2 0 01-2 2H5a2 2 0 01-2-2V9z"/>
            </svg>
        </a>`;

        let cumulative = '';
        for (const part of parts) {
            cumulative += (cumulative ? '/' : '') + part;
            html += `<span class="breadcrumb-sep">/</span>`;
            html += `<a class="breadcrumb-item" href="#/${cumulative}" data-path="${cumulative}">${escapeHtml(part)}</a>`;
        }

        el.innerHTML = html;
    };

    el.addEventListener('click', (e) => {
        const link = e.target.closest('[data-path]');
        if (link) {
            e.preventDefault();
            navigate(link.dataset.path);
        }
    });

    subscribe('currentPath', render);
    render();
}

function escapeHtml(text) {
    const d = document.createElement('div');
    d.textContent = text;
    return d.innerHTML;
}
