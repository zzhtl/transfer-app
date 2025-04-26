# Transfer-App: 高性能文件传输服务

基于Rust和Hyper构建的高性能跨设备文件传输服务，专为局域网中的快速文件共享优化。支持手机、电脑等任何带浏览器的设备，无需安装客户端，即开即用。

## 🚀 核心功能

- **文件浏览与下载**: 通过Web界面浏览服务器文件目录，支持各种类型文件的下载
- **文件上传**: 支持从任何设备向服务器上传文件
- **文件删除**: 支持删除服务器上的文件和目录
- **高效传输**: 针对不同大小的文件采用最优的传输策略

## 💪 技术特性

| 特性                | 说明                                                                 |
|---------------------|----------------------------------------------------------------------|
| **零配置传输**      | 无需安装客户端，浏览器直接访问即可使用                                 |
| **大文件支持**      | 采用流式传输，内存占用恒定，支持任意大小文件传输                        |
| **断点续传**        | 支持HTTP Range请求，实现断点续传功能                                   |
| **高性能下载**      | 多种优化策略：小文件内存映射、大文件分块传输、GZIP压缩等               |
| **内存映射上传**    | 使用mmap技术高效处理文件上传，减少内存消耗                             |
| **局域网极速**      | 优化的传输策略，充分利用局域网带宽                                    |
| **安全传输**        | 内置路径安全校验，防止目录遍历攻击                                   |

## 🔧 技术实现

### 高效下载

```rust
// 针对不同文件类型的优化策略
// 小文件使用内存映射
if file_size < SMALL_FILE_THRESHOLD && !supports_gzip {
    return handle_mmap_download(canonical_path, file_size, response_builder).await;
}

// 大文件使用优化的分块传输
let stream = FramedRead::with_capacity(file, BytesCodec::new(), CHUNK_SIZE);
let body = Body::wrap_stream(stream);
```

### 断点续传

```rust
// 支持HTTP Range请求
if let Some(range) = range_header {
    return handle_range_request(range, canonical_path, file_size, response_builder, mime_type.as_ref()).await;
}
```

### 内存映射上传

```rust
// 使用内存映射技术高效处理文件上传
let mut mmap = tokio::task::spawn_blocking({
    let file = file.try_clone()?;
    move || {
        file.set_len(new_cursor)?;
        unsafe { MmapMut::map_mut(&file) }
    }
}).await??;

// 将数据块直接写入内存映射区域
(&mut mmap[cursor as usize..new_cursor as usize]).copy_from_slice(&chunk);
mmap.flush()?;
```

## 📦 性能优化关键点

- **大缓冲区**: 使用1MB缓冲区和256KB块大小，提高IO效率
- **内存映射技术**: 对小文件(<32MB)使用mmap技术，减少系统调用和内存拷贝
- **异步IO**: 全面采用tokio异步IO，高效处理并发请求
- **高效压缩**: 使用优化的压缩级别，平衡压缩率和速度
- **非阻塞操作**: 将潜在的阻塞操作放入阻塞线程池执行，避免阻塞主线程

## 🖥️ 使用方法

### 安装

确保已安装Rust开发环境，然后克隆并编译项目：

```bash
git clone https://github.com/zzhtl/transfer-app.git
cd transfer-app
cargo build --release
```

### 配置与运行

运行服务：

```bash
cargo run --release -- path /Volumns/ZCL
```

服务启动后，终端会显示访问地址，例如：
```
Server running on http://0.0.0.0:8080
Access files at: http://192.168.1.100:8080
```

通过上述地址，局域网中的任何设备都可以通过浏览器访问该服务。

## 📱 使用场景

- 在手机和电脑之间快速传输照片、视频和文档
- 快速分享大型文件给同一网络中的其他设备
- 临时文件共享服务器
- 在不同操作系统设备间无障碍传输文件

## 🛠️ 依赖

- hyper: 高性能HTTP服务器/客户端
- tokio: 异步运行时
- async-compression: 异步压缩/解压
- memmap2: 内存映射文件访问
- multer: 处理multipart/form-data请求
- 其他：详见Cargo.toml

## 🧪 测试

项目包含单元测试，可通过以下命令运行：

```bash
cargo test
```

## 📄 许可

MIT

## 🔜 未来计划

- 添加简单身份验证
- 支持WebSocket实时传输状态更新
- 文件夹压缩下载功能
- 支持移动端直接拍照上传
- 支持WebRTC P2P传输
