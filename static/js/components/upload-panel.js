/**
 * 上传面板组件
 * 集成 tus-js-client，支持拖拽上传、暂停/恢复、断点续传
 */

import { state, subscribe, getRaw } from '../store.js';
import { refresh } from '../actions.js';
import { showToast } from './toast.js';

let panelEl = null;
let listEl = null;

export function initUploadPanel() {
    panelEl = document.getElementById('upload-panel');
    if (!panelEl) return;
    listEl = panelEl.querySelector('.upload-list');

    // 关闭按钮
    panelEl.querySelector('.upload-panel-close')?.addEventListener('click', () => {
        state.uploadPanelOpen = false;
    });

    // 选择文件按钮
    panelEl.querySelector('.btn-select-files')?.addEventListener('click', () => {
        const input = document.createElement('input');
        input.type = 'file';
        input.multiple = true;
        input.addEventListener('change', () => {
            if (input.files.length) addFiles(input.files);
        });
        input.click();
    });

    // 选择文件夹
    panelEl.querySelector('.btn-select-folder')?.addEventListener('click', () => {
        const input = document.createElement('input');
        input.type = 'file';
        input.webkitdirectory = true;
        input.addEventListener('change', () => {
            if (input.files.length) addFiles(input.files);
        });
        input.click();
    });

    // 全局拖拽
    initDragDrop();

    subscribe('uploadPanelOpen', () => {
        panelEl.classList.toggle('open', state.uploadPanelOpen);
    });

    subscribe('uploads', renderList);
}

function initDragDrop() {
    const overlay = document.getElementById('drop-overlay');
    let dragCount = 0;

    document.addEventListener('dragenter', (e) => {
        e.preventDefault();
        dragCount++;
        if (overlay) overlay.classList.add('visible');
    });

    document.addEventListener('dragleave', (e) => {
        e.preventDefault();
        dragCount--;
        if (dragCount <= 0) {
            dragCount = 0;
            if (overlay) overlay.classList.remove('visible');
        }
    });

    document.addEventListener('dragover', (e) => e.preventDefault());

    document.addEventListener('drop', (e) => {
        e.preventDefault();
        dragCount = 0;
        if (overlay) overlay.classList.remove('visible');

        const files = e.dataTransfer?.files;
        if (files?.length) {
            state.uploadPanelOpen = true;
            addFiles(files);
        }
    });
}

/** 添加文件到上传队列并开始上传 */
function addFiles(fileList) {
    const raw = getRaw();
    const uploads = [...raw.uploads];

    for (const file of fileList) {
        const id = `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
        const entry = {
            id,
            file,
            name: file.name,
            size: file.size,
            relativePath: file.webkitRelativePath || '',
            progress: 0,
            speed: 0,
            status: 'pending', // pending | uploading | paused | done | error
            tusUpload: null,
            error: null,
        };
        uploads.push(entry);
        startUpload(entry);
    }

    state.uploads = uploads;
}

/** 使用 tus 协议上传单个文件 */
function startUpload(entry) {
    // tus-js-client 通过 vendor 全局加载
    if (typeof tus === 'undefined') {
        entry.status = 'error';
        entry.error = 'tus-js-client 未加载';
        updateEntry(entry);
        return;
    }

    const metadata = {
        filename: entry.file.name,
        filetype: entry.file.type,
        targetDir: state.currentPath,
    };
    if (entry.relativePath) {
        metadata.relativePath = entry.relativePath;
    }

    const upload = new tus.Upload(entry.file, {
        endpoint: '/api/upload',
        retryDelays: [0, 1000, 3000, 5000, 10000, 20000],
        chunkSize: 8 * 1024 * 1024, // 8MB
        metadata,
        storeFingerprintForResuming: true,
        removeFingerprintOnSuccess: true,

        onProgress: (bytesUploaded, bytesTotal) => {
            entry.progress = Math.round((bytesUploaded / bytesTotal) * 100);
            entry.status = 'uploading';
            updateEntry(entry);
        },
        onSuccess: () => {
            entry.progress = 100;
            entry.status = 'done';
            updateEntry(entry);
            showToast(`${entry.name} 上传完成`, 'success');
            refresh();
        },
        onError: (error) => {
            entry.status = 'error';
            entry.error = error.message || '上传失败';
            updateEntry(entry);
            showToast(`${entry.name} 上传失败`, 'error');
        },
    });

    entry.tusUpload = upload;

    // 尝试恢复之前的上传
    upload.findPreviousUploads().then(prev => {
        if (prev.length) {
            upload.resumeFromPreviousUpload(prev[0]);
        }
        upload.start();
        entry.status = 'uploading';
        updateEntry(entry);
    });
}

/** 更新上传条目状态 */
function updateEntry(entry) {
    const raw = getRaw();
    const uploads = raw.uploads.map(u => u.id === entry.id ? { ...entry } : u);
    state.uploads = uploads;
}

/** 暂停上传 */
export function pauseUpload(id) {
    const raw = getRaw();
    const entry = raw.uploads.find(u => u.id === id);
    if (entry?.tusUpload) {
        entry.tusUpload.abort();
        entry.status = 'paused';
        updateEntry(entry);
    }
}

/** 恢复上传 */
export function resumeUpload(id) {
    const raw = getRaw();
    const entry = raw.uploads.find(u => u.id === id);
    if (entry?.tusUpload) {
        entry.tusUpload.start();
        entry.status = 'uploading';
        updateEntry(entry);
    }
}

/** 取消上传 */
export function cancelUpload(id) {
    const raw = getRaw();
    const entry = raw.uploads.find(u => u.id === id);
    if (entry?.tusUpload) {
        entry.tusUpload.abort(true);
    }
    state.uploads = raw.uploads.filter(u => u.id !== id);
}

/** 渲染上传列表 */
function renderList() {
    if (!listEl) return;
    const raw = getRaw();
    const uploads = raw.uploads;

    if (!uploads.length) {
        listEl.innerHTML = '<div class="upload-empty">拖拽文件到此处或点击上方按钮选择文件</div>';
        return;
    }

    listEl.innerHTML = uploads.map(u => {
        const statusIcon = {
            pending: '⏳', uploading: '⬆️', paused: '⏸️', done: '✅', error: '❌'
        }[u.status] || '';

        const actions = [];
        if (u.status === 'uploading') {
            actions.push(`<button class="upload-action" data-action="pause" data-id="${u.id}">暂停</button>`);
        }
        if (u.status === 'paused') {
            actions.push(`<button class="upload-action" data-action="resume" data-id="${u.id}">继续</button>`);
        }
        if (u.status !== 'done') {
            actions.push(`<button class="upload-action" data-action="cancel" data-id="${u.id}">取消</button>`);
        }

        return `<div class="upload-item ${u.status}">
            <div class="upload-item-info">
                <span class="upload-item-name" title="${escapeAttr(u.name)}">${escapeHtml(u.name)}</span>
                <span class="upload-item-status">${statusIcon} ${u.progress}%</span>
            </div>
            <div class="upload-item-progress">
                <div class="upload-item-bar" style="width:${u.progress}%"></div>
            </div>
            <div class="upload-item-actions">${actions.join('')}</div>
        </div>`;
    }).join('');

    // 绑定操作按钮
    listEl.querySelectorAll('.upload-action').forEach(btn => {
        btn.addEventListener('click', (e) => {
            const action = btn.dataset.action;
            const id = btn.dataset.id;
            if (action === 'pause') pauseUpload(id);
            else if (action === 'resume') resumeUpload(id);
            else if (action === 'cancel') cancelUpload(id);
        });
    });
}

function escapeHtml(text) {
    const d = document.createElement('div');
    d.textContent = text;
    return d.innerHTML;
}

function escapeAttr(text) {
    return text.replace(/"/g, '&quot;');
}
