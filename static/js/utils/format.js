/**
 * 格式化工具函数
 */

import { icon } from './dom.js';

const SIZE_UNITS = ['B', 'KB', 'MB', 'GB', 'TB'];

/** 格式化文件大小 */
export function formatSize(bytes) {
    if (bytes == null || bytes < 0) return '-';
    if (bytes === 0) return '0 B';
    const i = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), SIZE_UNITS.length - 1);
    const val = bytes / Math.pow(1024, i);
    return `${val.toFixed(i > 0 ? 1 : 0)} ${SIZE_UNITS[i]}`;
}

/** 格式化时间戳（秒） */
export function formatTime(ts) {
    if (!ts) return '-';
    const d = new Date(ts * 1000);
    const now = new Date();
    const diff = (now - d) / 1000;

    if (diff < 60) return '刚刚';
    if (diff < 3600) return `${Math.floor(diff / 60)} 分钟前`;
    if (diff < 86400) return `${Math.floor(diff / 3600)} 小时前`;
    if (diff < 604800) return `${Math.floor(diff / 86400)} 天前`;

    const y = d.getFullYear();
    const m = String(d.getMonth() + 1).padStart(2, '0');
    const day = String(d.getDate()).padStart(2, '0');
    const h = String(d.getHours()).padStart(2, '0');
    const min = String(d.getMinutes()).padStart(2, '0');

    if (y === now.getFullYear()) return `${m}-${day} ${h}:${min}`;
    return `${y}-${m}-${day}`;
}

/** 完整时间（用于悬停提示），如 2026-09-24 08:22 */
export function formatDateTime(ts) {
    if (!ts) return '';
    const d = new Date(ts * 1000);
    const p = n => String(n).padStart(2, '0');
    return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

/** 剩余时间，如 3天 / 5小时 / 12分钟 */
export function formatRemaining(seconds) {
    if (seconds <= 0) return '已过期';
    if (seconds < 3600) return `${Math.max(1, Math.round(seconds / 60))} 分钟`;
    if (seconds < 86400) return `${Math.round(seconds / 3600)} 小时`;
    return `${Math.round(seconds / 86400)} 天`;
}

/** 文件图标：一份 SVG sprite + 按类别着色的 class */
export function fileIcon(file) {
    if (file.is_dir) return icon('folder', 'ic-folder');
    const category = fileCategory(file.name);
    return icon('file', category ? `ic-${category}` : '');
}

const CATEGORY = {};
for (const [cat, exts] of Object.entries({
    // 克制配色：仅少数大类保留低饱和度点缀色，其余一律中性灰
    code: 'js ts jsx tsx rs go py java c cpp h hpp html css json xml yml yaml toml sh rb php sql vue svelte kt swift',
    image: 'png jpg jpeg gif webp svg bmp ico heic avif',
    video: 'mp4 mkv avi webm mov flv m4v',
    audio: 'mp3 wav flac aac ogg m4a',
    pdf: 'pdf',
    archive: 'zip tar gz rar 7z bz2 xz tgz',
})) {
    for (const ext of exts.split(' ')) CATEGORY[ext] = cat;
}

/** 按扩展名归类：code / image / video / audio / pdf / archive，其余返回空串 */
export function fileCategory(name) {
    const dot = name.lastIndexOf('.');
    return dot > 0 ? CATEGORY[name.slice(dot + 1).toLowerCase()] || '' : '';
}
