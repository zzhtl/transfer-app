/**
 * FileTransfer — 应用入口
 */

import { initRouter, pathToHash } from './router.js';
import { initThemeToggle } from './components/theme-toggle.js';
import { initBreadcrumb } from './components/breadcrumb.js';
import { initToolbar } from './components/toolbar.js';
import { initFileList } from './components/file-list.js';
import { initUploadPanel } from './components/upload-panel.js';
import { initPreviewModal } from './components/preview-modal.js';
import { initContextMenu } from './components/context-menu.js';
import { initLoginModal } from './components/login-modal.js';
import { renderShareLanding } from './components/share-landing.js';
import { openSharesDialog } from './components/shares-dialog.js';
import { openQrDialog } from './components/qr-dialog.js';
import { authStatus, logout } from './api.js';
import { refresh } from './actions.js';
import { state } from './store.js';
import { publicOrigin, isLoopback } from './utils/origin.js';

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

/** 顶栏：手机访问二维码、我的分享、登出 */
function initTopbar(authRequired) {
    document.querySelector('.btn-shares')?.addEventListener('click', openSharesDialog);

    document.querySelector('.btn-phone')?.addEventListener('click', async () => {
        const origin = await publicOrigin();
        const url = `${origin}/${pathToHash(state.currentPath)}`;
        // 服务只绑定在回环地址上时，别的设备根本连不上
        const unreachable = isLoopback() && new URL(origin).hostname === location.hostname;
        openQrDialog({
            title: '用手机扫码打开',
            url,
            note: unreachable
                ? '服务只监听了本机地址，其他设备无法访问；启动时去掉 --bind 127.0.0.1 或改成 0.0.0.0'
                : '手机需要和这台电脑在同一个局域网内',
        });
    });

    const logoutBtn = document.querySelector('.btn-logout');
    if (logoutBtn) {
        logoutBtn.hidden = !authRequired;
        logoutBtn.addEventListener('click', () => {
            logout().catch(() => {}).finally(() => location.reload());
        });
    }
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
    document.getElementById('upload-float')?.addEventListener('click', () => {
        state.uploadPanelOpen = !state.uploadPanelOpen;
    });

    // 先查鉴权状态：需要登录则弹登录框，否则直接加载
    let status;
    try {
        status = await authStatus();
    } catch {
        status = { auth_required: false, authenticated: true };
    }
    initTopbar(status.auth_required);
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
