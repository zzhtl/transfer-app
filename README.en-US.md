# transfer-app

`transfer-app` is a local network file transfer and directory management service built with Rust + Axum. After startup, it maps a local directory into a shared space with a Web UI. Devices on the same network—phones, computers, or tablets—can access it directly through a browser without any extra client.

The current version embeds frontend resources directly into the binary, allowing single-file execution after compilation. HTTP is provided by default; HTTPS can be enabled by passing certificates. The project is better suited for trusted intranets, or deployments behind an existing reverse proxy and authentication layer.

## Feature Overview

- Browse shared directories with breadcrumb navigation, list/grid toggle, and directory-first sorting
- Upload files and folders via browser, including drag-and-drop support
- Resumable uploads based on the tus protocol, with pause, resume, and recovery after refresh
- Resume incomplete upload sessions after service restart, with periodic cleanup of expired sessions
- Single-file download supports `HTTP Range`, `ETag`, and resumable downloads
- Stream multiple files or directories into a ZIP download on the fly, without pre-building a full archive
- Online preview for images, video, audio, PDF, text/code, and Markdown
- Create folders, rename, and batch delete
- Provides health check endpoints and request logging
- Optional Rustls TLS support

## Use Cases

- Quickly transfer files between a phone and computer on the same LAN
- Temporarily share a directory with colleagues or multiple devices
- Browse and download documents, code, and media resources
- Provide a lightweight file workspace on a trusted network

## Requirements

- Rust `1.82+`
- Modern browser
- A readable and writable shared directory

## Quick Start

### 1. Build

```bash
cargo build --release
```

### 2. Start the Service

```bash
cargo run --release -- --path /path/to/share
```

You can also pass the shared directory via environment variable:

```bash
TRANSFER_PATH=/path/to/share cargo run --release
```

After startup, the terminal will print the local access address, for example:

```text
Local:   http://127.0.0.1:8080
Network: http://192.168.1.100:8080
```

Open the printed address in your browser.

## HTTPS / TLS

The repository includes a script for generating self-signed certificates, suitable for development and intranet environments:

```bash
./tls/gen-cert.sh
cargo run --release -- \
  --path /path/to/share \
  --tls-cert tls/cert.pem \
  --tls-key tls/key.pem
```

Notes:

- HTTPS is only enabled when both `--tls-cert` and `--tls-key` are provided
- Self-signed certificates will trigger browser warnings; this is expected behavior
- If serving to the public internet, it is recommended to place it behind Nginx, Caddy, or another reverse proxy, and add authentication yourself

## Configuration

| Parameter | Environment Variable | Default | Description |
| --- | --- | --- | --- |
| `--path` | `TRANSFER_PATH` | None | Shared root directory, required |
| `--bind` | `TRANSFER_BIND` | `0.0.0.0` | Listen IP |
| `--port` | `TRANSFER_PORT` | `8080` | Listen port |
| `--tls-cert` | `TRANSFER_TLS_CERT` | None | TLS certificate PEM |
| `--tls-key` | `TRANSFER_TLS_KEY` | None | TLS private key PEM |
| `--max-upload-size` | `TRANSFER_MAX_UPLOAD` | `0` | Max upload bytes per file, `0` means unlimited |
| `--max-concurrent-transfers` | None | `32` | Reserved parameter; not yet wired into actual concurrency limiting in the current version |
| `--upload-expiration-secs` | None | `604800` | Upload session expiration time, default 7 days |
| `--log-filter` | `RUST_LOG` | `info,transfer_app=debug` | `tracing` log filter rule |
| `--config` | `TRANSFER_CONFIG` | None | Reserved TOML config entry; CLI and environment variables are still recommended |

Additional notes:

- The current `--config` TOML merge capability is basic and cannot replace core startup parameters like `--path`
- `--path` is normalized and validated at startup; it will error out and exit if the target is not a directory

## Web UI Capabilities

- Breadcrumb navigation, with support for direct subdirectory positioning via URL hash
- Sort by name, size, or modification time
- Toggle between list view and grid view
- Keyword filtering for the current directory
- Right-click menu supports open, preview, download, rename, and delete
- Upload panel supports file upload, folder upload, drag-and-drop, pause, resume, and progress display
- Select multiple items for batch deletion or pack into a ZIP download
- Floating upload button provided on mobile

## Server API Overview

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/api/files?path=` | List directory contents |
| `POST` | `/api/files/mkdir` | Create a directory |
| `POST` | `/api/files/rename` | Rename a file or directory |
| `POST` | `/api/files/move` | Move a file or directory |
| `POST` | `/api/files/copy` | Copy a file or directory |
| `POST` | `/api/files/delete` | Batch delete |
| `GET` | `/api/files/search?q=&path=&limit=` | Server-side name search |
| `OPTIONS`, `POST` | `/api/upload` | tus capability discovery, create upload session |
| `HEAD`, `PATCH`, `DELETE` | `/api/upload/{file_id}` | Query progress, resume, or cancel upload |
| `GET` | `/api/download/{path}` | Single-file download, supports `Range` / `ETag` |
| `GET` | `/api/download-zip?paths=a,b,c` | Streaming ZIP download |
| `GET` | `/api/preview/{path}` | File preview |
| `GET` | `/api/healthz` | Liveness check |
| `GET` | `/api/readyz` | Readiness check |

The current frontend has integrated browsing, uploading, renaming, deleting, ZIP download, and preview. Interfaces like `move`, `copy`, and `search` are also available for future secondary integration.

## Preview and Download Details

- Images, video, audio, and PDF are rendered directly by the browser
- Markdown is rendered into HTML by the server
- Text and code files read up to `1 MiB` for preview
- Download interface decides between `inline` or `attachment` based on parameters
- ZIP downloads use streaming output, suitable for large files and large directories

## Runtime Constraints

- All access paths are restricted within the shared root directory to prevent directory traversal
- The program creates a hidden directory `.transfer-tmp` inside the shared directory to store upload chunks and session metadata
- `.transfer-tmp` does not appear in the file listing
- Incomplete uploads are attempted to be recovered at startup; a background task scans hourly and cleans up expired uploads
- The current version has no built-in identity authentication, and `CORS` is configured loosely; it is recommended for trusted networks only
- Frontend static resources are embedded into the binary via `rust-embed`; no extra frontend build artifacts are needed after compilation

## Key Dependencies

- `axum`, `tower-http`: HTTP service, routing, and middleware
- `tokio`: Async runtime
- `rustls`, `tokio-rustls`: TLS support
- `async_zip`: Streaming ZIP packaging
- `rust-embed`: Embed static frontend resources
- `tracing`, `tracing-subscriber`: Logging and observability
- `tus-js-client`: Browser-side resumable upload

## Testing and Verification

View CLI help:

```bash
cargo run -- --help
```

Run unit tests:

```bash
cargo test
```

Existing tests in the repository mainly cover:

- `HTTP Range` parsing
- Path security and directory traversal protection

Integration tests for APIs and frontend interaction tests can still be added.

## Known Limitations

- No built-in login, authentication, or permission isolation
- `--max-concurrent-transfers` is not yet actually effective
- `--config` is still at a basic implementation stage and is not suitable as the sole configuration source
- The Web UI has not yet exposed `move` and `copy` as direct operation entries

## License

The repository currently does not include a `LICENSE` file; if you need to distribute or open-source it, it is recommended to add license information first.
