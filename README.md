# transfer-app

`transfer-app` 是一个基于 Rust + Axum 的局域网文件传输与目录管理服务。启动后把一个本地目录映射成带 Web UI 的共享空间，同一网络内的手机、电脑或平板直接用浏览器访问即可，不需要额外客户端。

前端资源直接嵌入二进制，单文件即可运行，无运行时依赖。默认提供 HTTP，传入证书后可切换到 HTTPS；通过 `--auth-password` 可启用站点密码鉴权。项目更适合受信任内网，或部署在已有反向代理之后。

## 功能概览

- 浏览共享目录，支持面包屑导航、列表/网格切换、目录优先排序
- 浏览器上传文件和文件夹，支持拖拽上传
- 基于 tus 协议的断点续传，支持暂停、继续、刷新后恢复
- 服务重启后恢复未完成上传会话，并定期清理过期会话
- 单文件下载支持 `HTTP Range`、`ETag` 和断点续传
- 多文件或目录流式打包为 ZIP 下载，不预先落完整压缩包
- 在线预览图片、视频、音频、PDF、文本/代码和 Markdown
- 新建文件夹、重命名、移动、复制、批量删除
- 文本文件在线编辑，原子保存
- 可选站点密码鉴权（`--auth-password`），未设置时保持匿名开放
- 对文件或目录生成带有效期的公开分享链接，免登录访问，支持撤销
- 提供健康检查接口和请求日志
- 可选启用 Rustls TLS

## 适用场景

- 手机和电脑在同一局域网内快速互传文件
- 临时共享某个目录给同事或多台设备
- 浏览和下载文档、代码、媒体资源
- 在受信任网络内提供一个轻量文件工作台

## 安装

### 方式一：一键安装脚本（Linux / macOS）

```bash
curl -fsSL https://raw.githubusercontent.com/zzhtl/transfer-app/main/install.sh | sh
```

脚本会自动识别系统和架构，从 [GitHub Releases](https://github.com/zzhtl/transfer-app/releases) 下载对应二进制，校验 `SHA256` 后安装到 `/usr/local/bin`（目录不可写时自动尝试 `sudo`）。

可用环境变量控制安装行为：

| 环境变量 | 默认值 | 说明 |
| --- | --- | --- |
| `TRANSFER_VERSION` | `latest` | 安装指定版本，如 `v0.3.0` |
| `TRANSFER_INSTALL_DIR` | `/usr/local/bin` | 安装目录 |

示例（安装到用户目录，免 `sudo`）：

```bash
TRANSFER_INSTALL_DIR=$HOME/.local/bin \
  curl -fsSL https://raw.githubusercontent.com/zzhtl/transfer-app/main/install.sh | sh
```

### 方式二：手动下载

从 [Releases](https://github.com/zzhtl/transfer-app/releases) 页面下载对应平台的压缩包，每个产物都附带 `.sha256` 校验文件：

| 平台 | 产物 |
| --- | --- |
| Linux x86_64 | `transfer-app-x86_64-unknown-linux-musl.tar.gz` |
| Linux arm64 | `transfer-app-aarch64-unknown-linux-musl.tar.gz` |
| macOS Intel | `transfer-app-x86_64-apple-darwin.tar.gz` |
| macOS Apple Silicon | `transfer-app-aarch64-apple-darwin.tar.gz` |

Linux 产物为 musl 静态链接，不依赖 glibc，任意发行版可直接运行。macOS 二进制未签名，浏览器下载后若被 Gatekeeper 拦截，执行 `xattr -d com.apple.quarantine transfer-app` 解除。

### 方式三：源码编译

需要 Rust `1.82+`：

```bash
cargo build --release
# 产物在 target/release/transfer-app
```

## 快速开始

```bash
transfer-app --path /path/to/share
```

也可以用环境变量传入共享目录：

```bash
TRANSFER_PATH=/path/to/share transfer-app
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
transfer-app \
  --path /path/to/share \
  --tls-cert tls/cert.pem \
  --tls-key tls/key.pem
```

说明：

- 只有同时提供 `--tls-cert` 和 `--tls-key` 时才会启用 HTTPS
- 自签名证书会触发浏览器告警，属于预期行为
- 如果要对公网提供服务，建议放到 Nginx、Caddy 等反向代理之后，并自行增加鉴权

## 部署为常驻服务（systemd）

```ini
# /etc/systemd/system/transfer-app.service
[Unit]
Description=transfer-app LAN file transfer server
After=network.target

[Service]
ExecStart=/usr/local/bin/transfer-app --path /srv/share --port 8080
# 启用鉴权时不要把密码写在 ExecStart 里（会出现在进程列表中），
# 用 EnvironmentFile 传入，文件权限设为 600：
# EnvironmentFile=/etc/transfer-app.env    # 内容: TRANSFER_AUTH_PASSWORD=...
# 按需改成对共享目录有读写权限的用户
User=www-data
Restart=on-failure

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now transfer-app
```

## 配置项

| 参数 | 环境变量 | 默认值 | 说明 |
| --- | --- | --- | --- |
| `--path` | `TRANSFER_PATH` | 无 | 共享根目录，必填 |
| `--bind` | `TRANSFER_BIND` | `0.0.0.0` | 监听 IP |
| `--port` | `TRANSFER_PORT` | `8080` | 监听端口 |
| `--tls-cert` | `TRANSFER_TLS_CERT` | 无 | TLS 证书 PEM |
| `--tls-key` | `TRANSFER_TLS_KEY` | 无 | TLS 私钥 PEM |
| `--max-upload-size` | `TRANSFER_MAX_UPLOAD` | `0` | 单文件最大上传字节数，`0` 表示不限制 |
| `--max-concurrent-transfers` | 无 | `32` | 并发上传传输上限（tus `PATCH` 数据传输期间占用许可） |
| `--upload-expiration-secs` | 无 | `604800` | 上传会话过期时间，默认 7 天 |
| `--auth-password` | `TRANSFER_AUTH_PASSWORD` | 无 | 站点访问密码，设置后启用鉴权，不设则匿名开放 |
| `--session-ttl-secs` | 无 | `604800` | 登录会话有效期，默认 7 天 |
| `--share-expiration-default-secs` | 无 | `604800` | 分享链接默认有效期，默认 7 天 |
| `--share-max-expiration-secs` | 无 | `2592000` | 分享链接最大有效期，默认 30 天 |
| `--log-filter` | `RUST_LOG` | `info,transfer_app=debug` | `tracing` 日志过滤规则 |
| `--config` | `TRANSFER_CONFIG` | 无 | 预留 TOML 配置入口，当前仍建议优先使用 CLI 或环境变量 |

补充说明：

- 当前 `--config` 的 TOML 合并能力还比较基础，不能替代 `--path` 这样的核心启动参数
- `--path` 会在启动时做规范化和目录校验，若目标不是目录会直接报错退出

## Web 界面能力

- 面包屑导航，支持通过 URL hash 直接定位子目录
- 名称、大小、修改时间排序（名称按自然顺序，「第2集」在「第10集」前面）
- 列表视图和网格视图切换；大目录只渲染可见部分，上万个文件也能流畅滚动和点选
- 当前目录关键字过滤；输入两个字以上时递归搜索子文件夹
- 单击选中、双击打开，Shift / Ctrl（⌘）多选；方向键、Enter、Delete、F2、Ctrl+A、Esc、`/` 等快捷键
- 右键菜单或每行末尾的 ⋯ 支持打开、预览、下载、打包下载、分享、移动、复制、重命名、删除；触屏上单击直接打开
- 上传面板支持文件、文件夹、拖入整个文件夹（保留目录结构），暂停、继续、失败重试，显示总进度
- 预览支持 ←/→ 切换同目录文件；在线编辑支持 Ctrl+S 保存，未保存时关闭会先确认
- 「我的分享」查看、复制、二维码、撤销；「手机访问」显示本机局域网地址的二维码
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
| `GET` | `/api/files/content?path=` | 读取文本文件内容（供在线编辑，限 `1 MiB`） |
| `POST` | `/api/files/save` | 保存文本文件（原子写） |
| `OPTIONS`, `POST` | `/api/upload` | tus 能力发现、创建上传会话 |
| `HEAD`, `PATCH`, `DELETE` | `/api/upload/{file_id}` | 查询进度、续传、取消上传 |
| `GET` | `/api/download/{path}` | 单文件下载，支持 `Range` / `ETag` |
| `GET` | `/api/download-zip?paths=a&paths=b[&name=x.zip]` | 流式 ZIP 下载（`paths` 可重复，每个值是一个完整路径） |
| `GET` | `/api/preview/{path}` | 文件预览 |
| `POST`, `GET` | `/api/share` | 创建分享链接、列出全部分享 |
| `DELETE` | `/api/share/{id}` | 撤销分享 |
| `GET` | `/api/s/{token}` | 分享元信息（免登录） |
| `GET` | `/api/s/{token}/download` | 分享文件下载（免登录） |
| `GET` | `/api/s/{token}/zip` | 分享目录打包下载（免登录） |
| `GET` | `/api/s/{token}/list` | 分享目录列表（免登录） |
| `POST` | `/api/auth/login` | 登录 |
| `POST` | `/api/auth/logout` | 退出登录 |
| `GET` | `/api/auth/status` | 鉴权状态查询 |
| `GET` | `/api/server-info` | 版本与局域网访问地址（分享链接、二维码用） |
| `GET` | `/api/healthz` | 存活检查 |
| `GET` | `/api/readyz` | 就绪检查 |

当前前端已经接入浏览、上传、移动、复制、重命名、删除、打包下载、预览、在线编辑、分享和登录。

## 预览与下载细节

- 图片、视频、音频、PDF 由浏览器直接展示
- Markdown 由服务端渲染成 HTML
- 文本和代码文件最多读取前 `1 MiB` 用于预览
- 下载接口会根据参数决定 `inline` 或 `attachment`
- ZIP 下载采用流式写出，适合大文件和大目录

## 运行时约束

- 所有访问路径都会被限制在共享根目录内，防止目录穿越
- 程序会在共享目录下创建隐藏目录 `.transfer-tmp`，用于保存上传分片、会话元数据和分享记录
- `.transfer-tmp` 不会出现在文件列表和搜索结果中，也不能通过接口读写
- 启动时会尝试恢复未完成的上传；后台任务会按小时扫描并清理过期上传
- 未设置 `--auth-password` 时匿名开放；设置后除登录接口和分享公开链接（`/api/s/`）外均需登录。会话用每进程随机的 HMAC key 签名，服务重启后需要重新登录
- 不开启 `CORS`，只接受同源访问：其他网页无法跨域读写这里的文件
- 以 inline 方式打开用户上传的 html、svg、xml 时附带 `Content-Security-Policy: sandbox`，其中的脚本无法以本站身份执行
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

- `HTTP Range` 解析、`Content-Disposition`、`ETag` / `If-Range`
- 路径安全与目录穿越防护（含上传 `relativePath`、符号链接、内部目录）
- 接口端到端测试（`src/routes/api_tests.rs`）：上传、打包下载、下载响应头与压缩、静态资源缓存、列目录、搜索、移动 / 复制、登录、CORS
- Markdown 预览的 HTML 转义与链接过滤

前端交互测试还可以继续补充。

## 已知限制

- 鉴权是单密码的站点级方案，没有多用户和细粒度权限隔离
- `--max-concurrent-transfers` 目前只限制上传传输，下载和打包暂不设并发上限
- `--config` 仍处于基础实现状态，不适合作为唯一配置来源

## 发布流程

发布由 GitHub Actions 完成（`.github/workflows/release.yml`），推送 `v*` 标签自动触发：

```bash
# 1. 更新 Cargo.toml 中的 version（CI 会校验 tag 与版本号一致）
# 2. 打标签并推送
git tag v0.3.0
git push origin v0.3.0
```

流水线内容：

1. 运行 `cargo test`，并校验 tag 与 `Cargo.toml` 版本一致
2. 四个目标平台并行构建：Linux musl 在原生 x86_64 / arm64 runner 上编译，macOS 双架构在 Apple Silicon runner 上编译
3. 每个产物打成 `tar.gz` 并生成 `.sha256` 校验文件
4. 自动创建 GitHub Release，附上全部产物和自动生成的 Release Notes

`install.sh` 依赖产物命名 `transfer-app-<target>.tar.gz`（不带版本号），配合 `releases/latest/download/` 直链下载，避免调用 GitHub API 受速率限制影响。

## 许可

仓库当前未附带 `LICENSE` 文件；如果需要对外分发或开源发布，建议先补齐授权信息。
