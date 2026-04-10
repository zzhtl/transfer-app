# transfer-app

`transfer-app` 是一个基于 Rust + Axum 的局域网文件传输与目录管理服务。启动后把一个本地目录映射成带 Web UI 的共享空间，同一网络内的手机、电脑或平板直接用浏览器访问即可，不需要额外客户端。

当前版本的前端资源会直接嵌入二进制，编译后可单文件运行。默认提供 HTTP，传入证书后可切换到 HTTPS。项目更适合受信任内网，或部署在已有反向代理和鉴权之后。

## 功能概览

- 浏览共享目录，支持面包屑导航、列表/网格切换、目录优先排序
- 浏览器上传文件和文件夹，支持拖拽上传
- 基于 tus 协议的断点续传，支持暂停、继续、刷新后恢复
- 服务重启后恢复未完成上传会话，并定期清理过期会话
- 单文件下载支持 `HTTP Range`、`ETag` 和断点续传
- 多文件或目录流式打包为 ZIP 下载，不预先落完整压缩包
- 在线预览图片、视频、音频、PDF、文本/代码和 Markdown
- 新建文件夹、重命名、批量删除
- 提供健康检查接口和请求日志
- 可选启用 Rustls TLS

## 适用场景

- 手机和电脑在同一局域网内快速互传文件
- 临时共享某个目录给同事或多台设备
- 浏览和下载文档、代码、媒体资源
- 在受信任网络内提供一个轻量文件工作台

## 运行要求

- Rust `1.82+`
- 现代浏览器
- 一个可读写的共享目录

## 快速开始

### 1. 编译

```bash
cargo build --release
```

### 2. 启动服务

```bash
cargo run --release -- --path /path/to/share
```

也可以用环境变量传入共享目录：

```bash
TRANSFER_PATH=/path/to/share cargo run --release
```

启动后终端会打印本机访问地址，例如：

```text
Local:   http://127.0.0.1:8080
Network: http://192.168.1.100:8080
```

浏览器访问输出的地址即可。

## HTTPS / TLS

仓库内自带一个生成自签名证书的脚本，适合开发和内网环境：

```bash
./tls/gen-cert.sh
cargo run --release -- \
  --path /path/to/share \
  --tls-cert tls/cert.pem \
  --tls-key tls/key.pem
```

说明：

- 只有同时提供 `--tls-cert` 和 `--tls-key` 时才会启用 HTTPS
- 自签名证书会触发浏览器告警，属于预期行为
- 如果要对公网提供服务，建议放到 Nginx、Caddy 等反向代理之后，并自行增加鉴权

## 配置项

| 参数 | 环境变量 | 默认值 | 说明 |
| --- | --- | --- | --- |
| `--path` | `TRANSFER_PATH` | 无 | 共享根目录，必填 |
| `--bind` | `TRANSFER_BIND` | `0.0.0.0` | 监听 IP |
| `--port` | `TRANSFER_PORT` | `8080` | 监听端口 |
| `--tls-cert` | `TRANSFER_TLS_CERT` | 无 | TLS 证书 PEM |
| `--tls-key` | `TRANSFER_TLS_KEY` | 无 | TLS 私钥 PEM |
| `--max-upload-size` | `TRANSFER_MAX_UPLOAD` | `0` | 单文件最大上传字节数，`0` 表示不限制 |
| `--max-concurrent-transfers` | 无 | `32` | 预留参数，当前版本尚未接入实际并发限流 |
| `--upload-expiration-secs` | 无 | `604800` | 上传会话过期时间，默认 7 天 |
| `--log-filter` | `RUST_LOG` | `info,transfer_app=debug` | `tracing` 日志过滤规则 |
| `--config` | `TRANSFER_CONFIG` | 无 | 预留 TOML 配置入口，当前仍建议优先使用 CLI 或环境变量 |

补充说明：

- 当前 `--config` 的 TOML 合并能力还比较基础，不能替代 `--path` 这样的核心启动参数
- `--path` 会在启动时做规范化和目录校验，若目标不是目录会直接报错退出

## Web 界面能力

- 面包屑导航，支持通过 URL hash 直接定位子目录
- 名称、大小、修改时间排序
- 列表视图和网格视图切换
- 当前目录关键字过滤
- 右键菜单支持打开、预览、下载、重命名、删除
- 上传面板支持文件上传、文件夹上传、拖拽上传、暂停、继续和进度显示
- 选中多个项目后可批量删除，或打包为 ZIP 下载
- 移动端提供浮动上传按钮

## 服务端接口概览

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `GET` | `/api/files?path=` | 列出目录内容 |
| `POST` | `/api/files/mkdir` | 创建目录 |
| `POST` | `/api/files/rename` | 重命名文件或目录 |
| `POST` | `/api/files/move` | 移动文件或目录 |
| `POST` | `/api/files/copy` | 复制文件或目录 |
| `POST` | `/api/files/delete` | 批量删除 |
| `GET` | `/api/files/search?q=&path=&limit=` | 服务端按名称搜索 |
| `OPTIONS`, `POST` | `/api/upload` | tus 能力发现、创建上传会话 |
| `HEAD`, `PATCH`, `DELETE` | `/api/upload/{file_id}` | 查询进度、续传、取消上传 |
| `GET` | `/api/download/{path}` | 单文件下载，支持 `Range` / `ETag` |
| `GET` | `/api/download-zip?paths=a,b,c` | 流式 ZIP 下载 |
| `GET` | `/api/preview/{path}` | 文件预览 |
| `GET` | `/api/healthz` | 存活检查 |
| `GET` | `/api/readyz` | 就绪检查 |

当前前端已经接入浏览、上传、重命名、删除、打包下载和预览。`move`、`copy`、`search` 这类接口也可以用于后续二次集成。

## 预览与下载细节

- 图片、视频、音频、PDF 由浏览器直接展示
- Markdown 由服务端渲染成 HTML
- 文本和代码文件最多读取前 `1 MiB` 用于预览
- 下载接口会根据参数决定 `inline` 或 `attachment`
- ZIP 下载采用流式写出，适合大文件和大目录

## 运行时约束

- 所有访问路径都会被限制在共享根目录内，防止目录穿越
- 程序会在共享目录下创建隐藏目录 `.transfer-tmp`，用于保存上传分片和会话元数据
- `.transfer-tmp` 不会出现在文件列表中
- 启动时会尝试恢复未完成的上传；后台任务会按小时扫描并清理过期上传
- 当前版本没有内置身份认证，同时 `CORS` 配置较宽松，只建议用于受信任网络
- 前端静态资源通过 `rust-embed` 嵌入二进制，编译后不依赖额外前端构建产物

## 关键依赖

- `axum`、`tower-http`：HTTP 服务、路由和中间件
- `tokio`：异步运行时
- `rustls`、`tokio-rustls`：TLS 支持
- `async_zip`：流式 ZIP 打包
- `rust-embed`：嵌入静态前端资源
- `tracing`、`tracing-subscriber`：日志与可观测性
- `tus-js-client`：浏览器端断点续传上传

## 测试与验证

查看 CLI 帮助：

```bash
cargo run -- --help
```

运行单元测试：

```bash
cargo test
```

当前仓库内已有测试主要覆盖：

- `HTTP Range` 解析
- 路径安全与目录穿越防护

接口集成测试和前端交互测试还可以继续补充。

## 已知限制

- 暂无内置登录、鉴权和权限隔离
- `--max-concurrent-transfers` 目前尚未真正生效
- `--config` 仍处于基础实现状态，不适合作为唯一配置来源
- Web UI 还没有把 `move`、`copy` 暴露成直接操作入口

## 许可

仓库当前未附带 `LICENSE` 文件；如果需要对外分发或开源发布，建议先补齐授权信息。
