/**
 * FileTransfer v0.3 — 应用入口
 */

import { initRouter } from './router.js';
import { initThemeToggle } from './components/theme-toggle.js';
import { initBreadcrumb } from './components/breadcrumb.js';
import { initToolbar } from './components/toolbar.js';
import { initFileList } from './components/file-list.js';
import { initUploadPanel } from './components/upload-panel.js';
import { initPreviewModal } from './components/preview-modal.js';
import { initContextMenu } from './components/context-menu.js';
import { initLoginModal } from './components/login-modal.js';
import { renderShareLanding } from './components/share-landing.js';
import { authStatus } from './api.js';
import { refresh } from './actions.js';
import { state } from './store.js';

let appStarted = false;

/** 首次登录成功/免登录时启动数据加载；会话恢复时仅刷新当前目录 */
function startApp() {
    if (appStarted) {
        refresh();
        return;
    }
    appStarted = true;
    initRouter();
}

/** 应用初始化 */
async function init() {
    initThemeToggle();

    // 公开分享落地页：与常规应用隔离，直接渲染后返回
    if (location.pathname.startsWith('/s/')) {
        renderShareLanding();
        return;
    }

    initBreadcrumb();
    initToolbar();
    initFileList();
    initUploadPanel();
    initPreviewModal();
    initContextMenu();
    initLoginModal(startApp);

    // 移动端浮动上传按钮
    const floatBtn = document.getElementById('upload-float');
    if (floatBtn) {
        floatBtn.addEventListener('click', () => {
            state.uploadPanelOpen = !state.uploadPanelOpen;
        });
    }

    // 先查鉴权状态：需要登录则弹登录框，否则直接加载
    let status;
    try {
        status = await authStatus();
    } catch {
        status = { auth_required: false, authenticated: true };
    }
    if (status.auth_required && !status.authenticated) {
        state.authRequired = true;
    } else {
        startApp();
    }
}

// DOM 就绪后启动
if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', init);
} else {
    init();
}
