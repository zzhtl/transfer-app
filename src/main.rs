use anyhow::Result;
use futures::future::FutureExt;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server, StatusCode};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

mod download;
mod html;
mod upload;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. 读取 --path 参数设置文件夹路径
    let args: Vec<String> = std::env::args().collect();
    let dir_path = if let Some(pos) = args.iter().position(|s| s == "path") {
        args.get(pos + 1).unwrap_or_else(|| {
            eprintln!("Usage: {} -- path <DIR_PATH>", args[0]);
            std::process::exit(1);
        })
    } else {
        eprintln!("Usage: {} -- path <DIR_PATH>", args[0]);
        std::process::exit(1);
    };
    let canonical_dir = std::fs::canonicalize(dir_path)?;
    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));

    // 2. 创建共享文件夹路径
    let shared_dir = Arc::new(canonical_dir.clone());

    // 清理可能残留的临时文件
    let _ = upload::cleanup_temp_files(&canonical_dir).await;

    // 3. 创建服务
    let make_svc = make_service_fn(move |_conn| {
        let dir = shared_dir.clone();
        async move {
            Ok::<_, hyper::Error>(service_fn(move |req| {
                handle_request(req, dir.clone()).boxed()
            }))
        }
    });

    // 4. 启动服务器
    let server = Server::bind(&addr)
        .http1_keepalive(true)
        .serve(make_svc);

    println!("╔══════════════════════════════════════════════════╗");
    println!("║          FileTransfer Server v0.2.0              ║");
    println!("╠══════════════════════════════════════════════════╣");
    println!("║  Local:   http://127.0.0.1:8080                  ║");
    if let Some(local_ip) = get_local_ip() {
        println!("║  Network: http://{}:8080{}", local_ip, " ".repeat(21 - local_ip.len()));
        println!("╚══════════════════════════════════════════════════╝");
    } else {
        println!("╚══════════════════════════════════════════════════╝");
    }
    println!("\n共享目录: {}", canonical_dir.display());
    println!("按 Ctrl+C 停止服务器\n");

    server.await?;
    Ok(())
}

async fn handle_request(req: Request<Body>, dir_path: Arc<PathBuf>) -> Result<Response<Body>> {
    match req.method() {
        &hyper::Method::GET => download::handle_get(req, dir_path).await,
        &hyper::Method::POST => handle_post(req, dir_path).await,
        &hyper::Method::DELETE => handle_delete(req, dir_path).await, // 新增
        _ => Ok(Response::builder()
            .status(StatusCode::METHOD_NOT_ALLOWED)
            .body(Body::empty())?),
    }
}

async fn handle_post(req: Request<Body>, dir_path: Arc<PathBuf>) -> Result<Response<Body>> {
    let request_path = req.uri().path();
    let decoded_path = percent_encoding::percent_decode_str(request_path)
        .decode_utf8_lossy()
        .to_string();

    // 如果路径为空或只有 "/"，使用根目录
    let relative_path = decoded_path.trim_start_matches('/');
    let full_path = if relative_path.is_empty() {
        dir_path.as_ref().clone()
    } else {
        dir_path.join(relative_path)
    };

    // 路径安全检查 - 对于目录，先确保它存在
    let canonical_path = if full_path.exists() {
        match tokio::fs::canonicalize(&full_path).await {
            Ok(p) => p,
            Err(_) => return Ok(not_found_response()),
        }
    } else {
        // 如果路径不存在，使用根目录
        dir_path.as_ref().clone()
    };

    if !canonical_path.starts_with(&*dir_path) {
        return Ok(Response::builder()
            .status(StatusCode::FORBIDDEN)
            .header("Content-Type", "application/json")
            .body(Body::from(r#"{"success":false,"message":"禁止访问"}"#))?);
    }

    // 确保目标是目录
    let target_dir = if canonical_path.is_dir() {
        canonical_path
    } else {
        canonical_path.parent().unwrap_or(&dir_path).to_path_buf()
    };

    match upload::handle_upload(req, target_dir).await {
        Ok(response) => Ok(response),
        Err(e) => {
            eprintln!("上传错误: {}", e);
            Ok(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header("Content-Type", "application/json")
                .body(Body::from(format!(
                    r#"{{"success":false,"message":"上传失败: {}"}}"#,
                    e.to_string().replace('"', "'")
                )))?)
        }
    }
}

/// 处理删除请求
async fn handle_delete(req: Request<Body>, dir_path: Arc<PathBuf>) -> Result<Response<Body>> {
    let request_path = req.uri().path();
    let decoded_path = percent_encoding::percent_decode_str(request_path)
        .decode_utf8_lossy()
        .to_string();
    let full_path = dir_path.join(&decoded_path[1..]);

    // 安全验证
    let canonical_path = match tokio::fs::canonicalize(&full_path).await {
        Ok(p) => p,
        Err(_) => {
            return Ok(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"success":false,"message":"文件或目录不存在"}"#))?)
        }
    };

    // 防止删除根目录
    if canonical_path == *dir_path {
        return Ok(Response::builder()
            .status(StatusCode::FORBIDDEN)
            .header("Content-Type", "application/json")
            .body(Body::from(r#"{"success":false,"message":"不能删除根目录"}"#))?);
    }

    if !canonical_path.starts_with(&*dir_path) {
        return Ok(Response::builder()
            .status(StatusCode::FORBIDDEN)
            .header("Content-Type", "application/json")
            .body(Body::from(r#"{"success":false,"message":"禁止访问"}"#))?);
    }

    // 执行删除操作
    let result = async {
        let metadata = tokio::fs::metadata(&canonical_path).await?;
        if metadata.is_dir() {
            tokio::fs::remove_dir_all(&canonical_path).await?;
            Ok::<_, std::io::Error>("目录")
        } else {
            tokio::fs::remove_file(&canonical_path).await?;
            Ok("文件")
        }
    }
    .await;

    match result {
        Ok(file_type) => Ok(Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/json")
            .body(Body::from(format!(
                r#"{{"success":true,"message":"{}删除成功"}}"#,
                file_type
            )))?),
        Err(e) => {
            eprintln!("删除错误: {}", e);
            Ok(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header("Content-Type", "application/json")
                .body(Body::from(format!(
                    r#"{{"success":false,"message":"删除失败: {}"}}"#,
                    e
                )))?)
        }
    }
}

pub fn not_found_response() -> Response<Body> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::empty())
        .unwrap()
}

// 获取本地IP地址
fn get_local_ip() -> Option<String> {
    use std::net::{IpAddr, Ipv4Addr};

    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?; // 80主要是防止防火墙拦截

    if let Ok(addr) = socket.local_addr() {
        match addr.ip() {
            IpAddr::V4(ipv4) if !ipv4.is_loopback() && ipv4 != Ipv4Addr::UNSPECIFIED => {
                return Some(ipv4.to_string());
            }
            _ => None,
        }
    } else {
        None
    }
}
