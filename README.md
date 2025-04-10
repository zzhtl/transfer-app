# Mac ↔ Android 文件传输工具

基于Rust构建的高性能本地文件传输服务，专为Mac和Android设备间大文件传输优化。

## 🚀 核心优势

| 特性                | 说明                                                                 |
|---------------------|----------------------------------------------------------------------|
| **零配置传输**      | 无需安装客户端，手机浏览器访问即用                                   |
| **大文件支持**      | 采用`FramedRead`流式传输，内存占用恒定，实测支持100GB+文件传输       |
| **全格式兼容**      | 智能识别文件类型，完美处理APK/PDF/MP4等任意格式                      |
| **局域网极速**      | 内网直连速度可达50-120MB/s（取决于路由器性能）                      |
| **安全传输**        | 内置路径安全校验，防止目录遍历攻击                                   |

## 📦 技术亮点

```rust
// 流式传输核心代码（内存效率优化关键）
let file = File::open(&path).await?;
let stream = FramedRead::new(file, BytesCodec::new()); // 分块读取（默认8KB/块）
let body = Body::wrap_stream(stream); // 异步流式传输
```

## 初始化步骤

修改文件夹路径为自己本机电脑路径
```bash
cd mac-android-transfer
cargo run --release
```
