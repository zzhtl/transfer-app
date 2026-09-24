/**
 * 面包屑导航组件
 */

import { state, subscribe } from '../store.js';
import { navigate, pathToHash } from '../router.js';
import { escapeHtml, icon } from '../utils/dom.js';

export function initBreadcrumb() {
    const el = document.getElementById('breadcrumb');
    if (!el) return;

    const render = () => {
        const path = state.currentPath;
        const parts = path ? path.split('/').filter(Boolean) : [];

        let html = `<a class="breadcrumb-item breadcrumb-root" href="#/" data-path="" aria-label="根目录">${icon('home')}</a>`;

        let cumulative = '';
        for (const part of parts) {
            cumulative += (cumulative ? '/' : '') + part;
            html += `<span class="breadcrumb-sep" aria-hidden="true">/</span>`;
            html += `<a class="breadcrumb-item" href="${escapeHtml(pathToHash(cumulative))}" data-path="${escapeHtml(cumulative)}">${escapeHtml(part)}</a>`;
        }

        el.innerHTML = html;
        el.lastElementChild?.setAttribute('aria-current', 'page');
        // 深层目录时让最后一段可见
        el.scrollLeft = el.scrollWidth;
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
