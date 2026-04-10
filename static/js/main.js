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
import { state } from './store.js';

/** 应用初始化 */
function init() {
    initThemeToggle();
    initBreadcrumb();
    initToolbar();
    initFileList();
    initUploadPanel();
    initPreviewModal();
    initContextMenu();

    // 移动端浮动上传按钮
    const floatBtn = document.getElementById('upload-float');
    if (floatBtn) {
        floatBtn.addEventListener('click', () => {
            state.uploadPanelOpen = !state.uploadPanelOpen;
        });
    }

    // 路由最后初始化（触发首次加载）
    initRouter();
}

// DOM 就绪后启动
if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', init);
} else {
    init();
}
