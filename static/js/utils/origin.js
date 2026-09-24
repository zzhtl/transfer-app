/**
 * 给其他设备用的访问地址
 */

import { serverInfo } from '../api.js';

const LOOPBACK = /^(localhost|127(?:\.\d{1,3}){3}|\[::1\])$/i;
let cached = null;

/** 当前是否通过本机回环地址访问 */
export function isLoopback() {
    return LOOPBACK.test(location.hostname);
}

/**
 * 在本机用 localhost / 127.0.0.1 打开时，location.origin 生成的分享链接和二维码
 * 在别的设备上打不开，换成服务端探测到的局域网地址；拿不到就退回 location.origin。
 */
export function publicOrigin() {
    if (!isLoopback()) return Promise.resolve(location.origin);
    cached ??= serverInfo()
        .then(info => info.lan_origin || location.origin)
        .catch(() => {
            cached = null;
            return location.origin;
        });
    return cached;
}
