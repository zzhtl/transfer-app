/**
 * 上传面板：tus 断点续传，拖拽上传（含文件夹），暂停 / 继续 / 重试
 *
 * 条目的 DOM 在入队时只建一次，进度更新先记下，再在 requestAnimationFrame 里统一改
 * 进度条和文字。之前每个 onProgress 都替换整个上传数组、重建所有条目并重新绑定事件，
 * 一次拖进几百个文件时面板和页面都会明显卡。
 */

import { state, subscribe } from '../store.js';
import { refresh } from '../actions.js';
import { showToast } from './toast.js';
import { formatSize, fileIcon } from '../utils/format.js';
import { escapeHtml, loadScript, icon } from '../utils/dom.js';

const MAX_CONCURRENT = 3;
// 服务端每个 PATCH 结束会落盘一次，分块大小与之配套，保持不变
const CHUNK_SIZE = 8 * 1024 * 1024;
const SPEED_WINDOW_MS = 500;
const SPEED_ALPHA = 0.3;
const STATUS_TEXT = { pending: '等待中', paused: '已暂停', done: '已完成' };

/** 按入队顺序保存的上传条目 */
const entries = new Map();
let nextId = 0;

let panelEl;
let listEl;
let summaryEl;
let clearBtn;

const dirty = new Set();
let flushRaf = 0;

/** 自上次汇总提示以来完成 / 失败的文件 */
let finished = { done: [], failed: [] };
let refreshTimer = null;
let refreshDeadline = 0;
let unloadGuard = false;

export function initUploadPanel() {
    panelEl = document.getElementById('upload-panel');
    if (!panelEl) return;
    listEl = panelEl.querySelector('.upload-list');
    summaryEl = panelEl.querySelector('.upload-summary');
    clearBtn = panelEl.querySelector('.upload-clear');

    panelEl.querySelector('.upload-panel-close')?.addEventListener('click', () => {
        state.uploadPanelOpen = false;
    });
    panelEl.querySelector('.btn-select-files')?.addEventListener('click', () => pickFiles(false));
    panelEl.querySelector('.btn-select-folder')?.addEventListener('click', () => pickFiles(true));
    clearBtn?.addEventListener('click', clearFinished);

    // 条目上的按钮统一委托
    listEl.addEventListener('click', (e) => {
        const btn = e.target.closest('[data-action]');
        const entry = btn && entries.get(btn.closest('.upload-item')?.dataset.id);
        if (!entry) return;
        ({ pause, resume, retry: resume, cancel })[btn.dataset.action]?.(entry);
    });

    initDragDrop();

    subscribe('uploadPanelOpen', () => {
        panelEl.classList.toggle('open', state.uploadPanelOpen);
        // 手机上面板是底部抽屉，浮动按钮会透过半透明背景露出来
        document.body.classList.toggle('upload-panel-open', state.uploadPanelOpen);
    });
    subscribe('uploadSummary', renderIndicators);
    renderEmpty();
}

function pickFiles(folder) {
    const input = document.createElement('input');
    input.type = 'file';
    if (folder) input.webkitdirectory = true;
    else input.multiple = true;
    input.addEventListener('change', () => {
        addFiles([...input.files].map(file => ({ file, relativePath: file.webkitRelativePath || '' })));
    });
    input.click();
}

// ===== 拖拽 =====

function initDragDrop() {
    const overlay = document.getElementById('drop-overlay');
    let depth = 0;
    // 只响应从系统拖进来的文件；页面里拖动文字、图片不该弹出上传遮罩
    const hasFiles = e => [...(e.dataTransfer?.types || [])].includes('Files');

    document.addEventListener('dragenter', (e) => {
        if (!hasFiles(e)) return;
        e.preventDefault();
        depth++;
        overlay?.classList.add('visible');
    });
    document.addEventListener('dragleave', (e) => {
        if (!hasFiles(e)) return;
        depth--;
        if (depth <= 0) {
            depth = 0;
            overlay?.classList.remove('visible');
        }
    });
    document.addEventListener('dragover', (e) => {
        if (!hasFiles(e)) return;
        e.preventDefault();
        e.dataTransfer.dropEffect = 'copy';
    });
    document.addEventListener('drop', (e) => {
        if (!hasFiles(e)) return;
        e.preventDefault();
        depth = 0;
        overlay?.classList.remove('visible');
        if (state.authRequired) return;
        collectDropped(e.dataTransfer)
            .then(addFiles)
            .catch(() => showToast('读取拖入的文件失败', 'error'));
    });
}

/** 拖入的条目：文件夹要递归展开，并保留目录结构（relativePath） */
function collectDropped(dataTransfer) {
    // webkitGetAsEntry 只能在 drop 事件的同步阶段调用，之后 DataTransferItem 就失效了
    const roots = [...dataTransfer.items]
        .filter(item => item.kind === 'file')
        .map(item => item.webkitGetAsEntry?.())
        .filter(Boolean);
    if (!roots.length) {
        return Promise.resolve([...dataTransfer.files].map(file => ({ file, relativePath: '' })));
    }
    return (async () => {
        const out = [];
        for (const root of roots) await walkEntry(root, out);
        return out;
    })();
}

async function walkEntry(entry, out) {
    if (entry.isFile) {
        const file = await new Promise((resolve, reject) => entry.file(resolve, reject));
        const relative = entry.fullPath.replace(/^\/+/, '');
        out.push({ file, relativePath: relative.includes('/') ? relative : '' });
    } else if (entry.isDirectory) {
        const reader = entry.createReader();
        // readEntries 每次只返回一批（Chrome 一次最多 100 个），要读到空为止
        for (;;) {
            const batch = await new Promise((resolve, reject) => reader.readEntries(resolve, reject));
            if (!batch.length) break;
            for (const child of batch) await walkEntry(child, out);
        }
    }
}

// ===== 队列 =====

/**
 * 加入上传队列
 * @param {{file: File, relativePath: string}[]} items
 */
async function addFiles(items) {
    if (!items.length) return;
    // 入队时就定下目标目录：排队中的文件不能因为用户切换了目录而传到别处
    const targetDir = state.currentPath;
    const fragment = document.createDocumentFragment();
    for (const { file, relativePath } of items) {
        const id = `u${++nextId}`;
        const entry = {
            id, file, targetDir, relativePath,
            name: file.name,
            size: file.size,
            uploaded: 0,
            status: 'pending',
            speed: 0,
            sampleTime: 0,
            sampleBytes: 0,
            error: '',
            upload: null,
            ready: null,
            token: null,
            renderedStatus: null,
        };
        entry.destDir = [targetDir, ...relativePath.split('/').slice(0, -1)].filter(Boolean).join('/');
        entries.set(id, entry);
        fragment.appendChild(createItem(entry));
    }
    listEl.querySelector('.upload-empty')?.remove();
    listEl.appendChild(fragment);
    state.uploadPanelOpen = true;
    for (const entry of entries.values()) if (entry.status === 'pending') markDirty(entry);

    try {
        await loadScript('/static/vendor/tus.min.js');
    } catch {
        for (const entry of entries.values()) {
            if (entry.status === 'pending') fail(entry, '上传组件加载失败，请刷新页面重试');
        }
        return;
    }
    pump();
}

/** 并发调度：同时最多 MAX_CONCURRENT 个，其余排队 */
function pump() {
    let running = 0;
    for (const entry of entries.values()) if (entry.status === 'uploading') running++;
    for (const entry of entries.values()) {
        if (running >= MAX_CONCURRENT) break;
        if (entry.status !== 'pending') continue;
        running++;
        activate(entry);
    }
    updateUnloadGuard();
}

/**
 * 开始（或继续）传一个条目。每次激活换一个 token：暂停后又很快继续时，
 * 之前那次还没等到 findPreviousUploads 的回调就不会再多调一次 start()。
 */
function activate(entry) {
    entry.status = 'uploading';
    entry.sampleTime = performance.now();
    entry.sampleBytes = entry.uploaded;
    markDirty(entry);
    if (!entry.upload) createUpload(entry);
    const token = {};
    entry.token = token;
    entry.ready.then(() => {
        if (entry.token === token && entry.status === 'uploading') entry.upload.start();
    });
}

function createUpload(entry) {
    const metadata = {
        filename: entry.name,
        filetype: entry.file.type,
        targetDir: entry.targetDir,
    };
    if (entry.relativePath) metadata.relativePath = entry.relativePath;

    const upload = new tus.Upload(entry.file, {
        endpoint: '/api/upload',
        retryDelays: [0, 1000, 3000, 5000, 10000, 20000],
        chunkSize: CHUNK_SIZE,
        metadata,
        storeFingerprintForResuming: true,
        removeFingerprintOnSuccess: true,
        // 默认指纹不含目标目录：同一个文件先往 A 传到一半、再往 B 传，会续到 A 的会话里落进 A
        fingerprint: file => Promise.resolve([
            'tus-ft', entry.targetDir, entry.relativePath || file.name, file.size, file.lastModified,
        ].join('|')),
        onProgress: (sent) => onProgress(entry, sent),
        onSuccess: () => complete(entry),
        onError: (error) => fail(entry, uploadErrorText(error)),
    });
    entry.upload = upload;

    // 尝试恢复之前中断的上传（刷新页面后重新选同一个文件即可续传）
    entry.ready = upload.findPreviousUploads()
        .then((previous) => {
            if (previous.length) upload.resumeFromPreviousUpload(previous[0]);
        })
        .catch(() => {});
}

function uploadErrorText(error) {
    const status = error?.originalResponse?.getStatus?.();
    if (status === 413) return '文件超过服务端允许的大小';
    if (status === 401) return '登录已失效';
    if (status === 400 || status === 403) return '服务端拒绝了这个文件';
    if (status === 0 || status == null) return '网络中断';
    return `上传失败（${status}）`;
}

function onProgress(entry, sent) {
    entry.uploaded = sent;
    const now = performance.now();
    const elapsed = now - entry.sampleTime;
    if (elapsed >= SPEED_WINDOW_MS) {
        // 至少 0.5s 采一次样再做指数平滑，速度和剩余时间才不会上下乱跳
        const instant = Math.max(0, (sent - entry.sampleBytes) / (elapsed / 1000));
        entry.speed = entry.speed ? SPEED_ALPHA * instant + (1 - SPEED_ALPHA) * entry.speed : instant;
        entry.sampleTime = now;
        entry.sampleBytes = sent;
    }
    markDirty(entry);
}

function complete(entry) {
    entry.status = 'done';
    entry.uploaded = entry.size;
    entry.speed = 0;
    markDirty(entry);
    scheduleRefresh(entry);
    finished.done.push(entry.name);
    afterSettled();
}

function fail(entry, message) {
    entry.status = 'error';
    entry.error = message;
    entry.speed = 0;
    markDirty(entry);
    finished.failed.push(entry.name);
    afterSettled();
}

/** 一个条目结束后：继续调度；整个队列空闲下来时只弹一条汇总 */
function afterSettled() {
    pump();
    if (isBusy()) return;
    const { done, failed } = finished;
    finished = { done: [], failed: [] };
    if (failed.length) {
        const names = failed.slice(0, 3).join('、') + (failed.length > 3 ? ' 等' : '');
        showToast(`上传完成 ${done.length} 个，失败 ${failed.length} 个：${names}`, 'error');
    } else if (done.length === 1) {
        showToast(`「${done[0]}」上传完成`, 'success');
    } else if (done.length) {
        showToast(`${done.length} 个文件上传完成`, 'success');
    }
}

function isBusy() {
    for (const entry of entries.values()) {
        if (entry.status === 'uploading' || entry.status === 'pending') return true;
    }
    return false;
}

/**
 * 上传落到当前目录（或当前目录就是它新建的子目录）时才刷新列表，并做防抖：
 * 之前每完成一个文件都整目录刷新一次，一次传几百个文件就是几百次列表请求。
 */
function scheduleRefresh(entry) {
    const current = state.currentPath;
    if (current !== entry.targetDir && current !== entry.destDir) return;
    const now = Date.now();
    if (!refreshTimer) refreshDeadline = now + 3000;
    clearTimeout(refreshTimer);
    refreshTimer = setTimeout(() => {
        refreshTimer = null;
        refresh();
    }, Math.max(0, Math.min(800, refreshDeadline - now)));
}

function pause(entry) {
    if (entry.status !== 'uploading') return;
    entry.token = null;
    entry.upload?.abort();
    entry.status = 'paused';
    entry.speed = 0;
    markDirty(entry);
    pump();
}

/** 继续暂停的、重试失败的：都回到队列里排队，同一个 tus 对象会从服务端记录的进度续传 */
function resume(entry) {
    if (entry.status !== 'paused' && entry.status !== 'error') return;
    entry.status = 'pending';
    entry.error = '';
    markDirty(entry);
    pump();
}

function cancel(entry) {
    entry.token = null;
    // abort(true) 同时让服务端删掉未完成的分片
    if (entry.upload && entry.status !== 'done') entry.upload.abort(true).catch(() => {});
    removeEntry(entry);
    pump();
}

function clearFinished() {
    for (const entry of [...entries.values()]) {
        if (entry.status === 'done') removeEntry(entry);
    }
}

function removeEntry(entry) {
    entries.delete(entry.id);
    dirty.delete(entry);
    listEl.querySelector(`[data-id="${entry.id}"]`)?.remove();
    renderEmpty();
    scheduleFlush();
}

function updateUnloadGuard() {
    const busy = isBusy();
    if (busy === unloadGuard) return;
    unloadGuard = busy;
    if (busy) window.addEventListener('beforeunload', onBeforeUnload);
    else window.removeEventListener('beforeunload', onBeforeUnload);
}

function onBeforeUnload(e) {
    // 关页面会中断正在进行的上传
    e.preventDefault();
    e.returnValue = '';
}

// ===== 渲染 =====

function createItem(entry) {
    const el = document.createElement('div');
    el.className = 'upload-item';
    el.dataset.id = entry.id;
    const shownName = entry.relativePath || entry.name;
    el.innerHTML = `
        <div class="upload-item-row">
            <span class="upload-item-icon">${fileIcon({ name: entry.name, is_dir: false })}</span>
            <span class="upload-item-name" title="${escapeHtml(shownName)}">${escapeHtml(shownName)}</span>
            <span class="upload-item-actions"></span>
        </div>
        <div class="upload-item-progress"><div class="upload-item-bar"></div></div>
        <div class="upload-item-meta">
            <span class="upload-item-status"></span>
            <span class="upload-item-size"></span>
        </div>`;
    entry.el = {
        root: el,
        bar: el.querySelector('.upload-item-bar'),
        status: el.querySelector('.upload-item-status'),
        size: el.querySelector('.upload-item-size'),
        actions: el.querySelector('.upload-item-actions'),
    };
    return el;
}

function markDirty(entry) {
    dirty.add(entry);
    scheduleFlush();
}

function scheduleFlush() {
    if (!flushRaf) flushRaf = requestAnimationFrame(flush);
}

function flush() {
    flushRaf = 0;
    for (const entry of dirty) renderItem(entry);
    dirty.clear();
    renderSummary();
}

function actionButton(action, label, iconName) {
    return `<button type="button" class="upload-action" data-action="${action}" title="${label}" aria-label="${label}">${icon(iconName)}</button>`;
}

function renderItem(entry) {
    const el = entry.el;
    const pct = entry.size ? Math.min(100, Math.floor((entry.uploaded / entry.size) * 100)) : (entry.status === 'done' ? 100 : 0);
    el.bar.style.width = `${pct}%`;

    if (entry.status === 'uploading') {
        let text = `${pct}%`;
        if (entry.speed > 0) {
            text += ` · ${formatSize(entry.speed)}/s`;
            const eta = (entry.size - entry.uploaded) / entry.speed;
            if (Number.isFinite(eta)) text += ` · 剩余 ${formatEta(eta)}`;
        }
        el.status.textContent = text;
    } else if (entry.status === 'error') {
        el.status.textContent = entry.error;
    } else {
        el.status.textContent = entry.status === 'paused' ? `${STATUS_TEXT.paused} · ${pct}%` : STATUS_TEXT[entry.status];
    }
    el.size.textContent = entry.status === 'done'
        ? formatSize(entry.size)
        : `${formatSize(entry.uploaded)} / ${formatSize(entry.size)}`;

    if (entry.renderedStatus !== entry.status) {
        entry.renderedStatus = entry.status;
        el.root.dataset.status = entry.status;
        const buttons = [];
        if (entry.status === 'uploading') buttons.push(actionButton('pause', '暂停', 'pause'));
        if (entry.status === 'paused') buttons.push(actionButton('resume', '继续', 'play'));
        if (entry.status === 'error') buttons.push(actionButton('retry', '重试', 'refresh'));
        if (entry.status !== 'done') buttons.push(actionButton('cancel', '取消', 'close'));
        el.actions.innerHTML = buttons.join('');
    }
}

function renderEmpty() {
    if (entries.size || listEl.querySelector('.upload-empty')) return;
    listEl.innerHTML = '<div class="upload-empty">把文件或文件夹拖到页面任意位置，<br>或点上面的按钮选择</div>';
}

function renderSummary() {
    let totalBytes = 0;
    let sentBytes = 0;
    let speed = 0;
    let busy = 0;
    let done = 0;
    let failed = 0;
    for (const entry of entries.values()) {
        if (entry.status === 'error') {
            failed++;
            continue;
        }
        totalBytes += entry.size;
        sentBytes += entry.status === 'done' ? entry.size : entry.uploaded;
        if (entry.status === 'done') done++;
        if (entry.status === 'uploading' || entry.status === 'pending') busy++;
        if (entry.status === 'uploading') speed += entry.speed;
    }
    const count = entries.size;
    const percent = totalBytes ? Math.floor((sentBytes / totalBytes) * 100) : 0;

    if (!count) summaryEl.textContent = '';
    else if (busy) summaryEl.textContent = `${done}/${count} · ${percent}%${speed ? ` · ${formatSize(speed)}/s` : ''}`;
    else summaryEl.textContent = failed ? `完成 ${done} 个，失败 ${failed} 个` : `全部完成，共 ${done} 个`;
    if (clearBtn) clearBtn.hidden = done === 0;

    state.uploadSummary = busy ? { percent, busy, done, total: count } : null;
}

/** 工具栏上传按钮与移动端浮动按钮：面板收起时也能看到总进度 */
function renderIndicators() {
    const summary = state.uploadSummary;
    for (const btn of document.querySelectorAll('.upload-trigger')) {
        btn.classList.toggle('is-uploading', !!summary);
        btn.style.setProperty('--upload-progress', summary ? summary.percent : 0);
        const label = btn.querySelector('.upload-trigger-label');
        if (label) label.textContent = summary ? `${summary.percent}%` : '上传';
        btn.title = summary ? `上传中 ${summary.done}/${summary.total}，总进度 ${summary.percent}%` : '上传';
    }
}

function formatEta(seconds) {
    if (seconds < 60) return `${Math.max(1, Math.round(seconds))} 秒`;
    const m = Math.floor(seconds / 60);
    if (m < 60) return `${m} 分 ${Math.round(seconds % 60)} 秒`;
    return `${Math.floor(m / 60)} 小时 ${m % 60} 分`;
}
