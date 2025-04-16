use anyhow::Result;
use futures::future::FutureExt;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server, StatusCode};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

mod html;
mod upload;
mod download;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. 设置文件夹路径和服务器地址
    let dir_path = "/Users/zcl/r1/未命名文件夹/2/1";
    let canonical_dir = std::fs::canonicalize(dir_path)?; // 规范化为绝对路径
    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));

    // 2. 创建共享文件夹路径
    let shared_dir = Arc::new(canonical_dir);

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
    let server = Server::bind(&addr).serve(make_svc);
    println!("Server running on http://{}", addr);

    if let Some(local_ip) = get_local_ip() {
        println!("Access files at: http://{}:8080", local_ip);
    }

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
    let full_path = dir_path.join(&decoded_path[1..]);

    // 路径安全检查
    let canonical_path = match tokio::fs::canonicalize(&full_path).await {
        Ok(p) => p,
        Err(_) => return Ok(not_found_response()),
    };

    if !canonical_path.starts_with(&*dir_path) {
        return Ok(Response::builder()
            .status(StatusCode::FORBIDDEN)
            .body(Body::empty())?);
    }

    upload::handle_upload(req, canonical_path).await
}

// 新增删除处理函数
async fn handle_delete(req: Request<Body>, dir_path: Arc<PathBuf>) -> Result<Response<Body>> {
    let request_path = req.uri().path();
    let decoded_path = percent_encoding::percent_decode_str(request_path)
        .decode_utf8_lossy()
        .to_string();
    let full_path = dir_path.join(&decoded_path[1..]);

    // 安全验证
    let canonical_path = match tokio::fs::canonicalize(&full_path).await {
        Ok(p) => p,
        Err(_) => return Ok(not_found_response()),
    };

    if !canonical_path.starts_with(&*dir_path) {
        return Ok(Response::builder()
            .status(StatusCode::FORBIDDEN)
            .body(Body::empty())?);
    }

    // 执行删除操作
    let metadata = tokio::fs::metadata(&canonical_path).await?;
    let response = if metadata.is_dir() {
        tokio::fs::remove_dir_all(canonical_path).await?;
        "目录删除成功"
    } else {
        tokio::fs::remove_file(canonical_path).await?;
        "文件删除成功"
    };

    Ok(Response::builder()
        .status(StatusCode::OK)
        .body(Body::from(response))?)
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
