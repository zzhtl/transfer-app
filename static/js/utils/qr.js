/**
 * 二维码：按需动态加载 vendored qrcode-generator，只在真正要显示时才下载那 50KB。
 */

let loader = null;

/**
 * 把 text 渲染成二维码 SVG 填进 el。
 * 码本身固定白底黑码：暗色主题下反色的码很多扫码器识别不了。
 */
export async function renderQr(el, text) {
    loader ??= import('/static/vendor/qrcode.mjs').catch((e) => {
        loader = null;
        throw e;
    });
    const { default: qrcode } = await loader;
    const qr = qrcode(0, 'M');
    qr.addData(text);
    qr.make();
    el.innerHTML = qr.createSvgTag({ cellSize: 4, margin: 16, scalable: true });
}
