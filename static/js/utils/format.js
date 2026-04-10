/**
 * 格式化工具函数
 */

const SIZE_UNITS = ['B', 'KB', 'MB', 'GB', 'TB'];

/** 格式化文件大小 */
export function formatSize(bytes) {
    if (bytes == null || bytes < 0) return '-';
    if (bytes === 0) return '0 B';
    const i = Math.floor(Math.log(bytes) / Math.log(1024));
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

/** 文件图标 SVG */
export function fileIcon(file) {
    if (file.is_dir) {
        return `<svg width="20" height="20" viewBox="0 0 24 24" fill="var(--accent)" stroke="none">
            <path d="M2 6a2 2 0 012-2h5l2 2h9a2 2 0 012 2v10a2 2 0 01-2 2H4a2 2 0 01-2-2V6z"/>
        </svg>`;
    }

    const ext = file.name.split('.').pop()?.toLowerCase() || '';
    const color = extColor(ext);

    return `<svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="${color}" stroke-width="1.5">
        <path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8l-6-6z"/>
        <polyline points="14 2 14 8 20 8"/>
    </svg>`;
}

function extColor(ext) {
    const colors = {
        js: '#f7df1e', ts: '#3178c6', rs: '#dea584', go: '#00add8',
        py: '#3776ab', java: '#ed8b00', c: '#555', cpp: '#659bd3',
        html: '#e34c26', css: '#1572b6', json: '#5b5b5b',
        md: '#083fa1', txt: 'var(--text-tertiary)',
        png: '#f472b6', jpg: '#f472b6', gif: '#f472b6', webp: '#f472b6', svg: '#f472b6',
        mp4: '#a855f7', mkv: '#a855f7', avi: '#a855f7', webm: '#a855f7',
        mp3: '#22c55e', wav: '#22c55e', flac: '#22c55e',
        pdf: '#ef4444', zip: '#eab308', tar: '#eab308', gz: '#eab308',
    };
    return colors[ext] || 'var(--text-tertiary)';
}
