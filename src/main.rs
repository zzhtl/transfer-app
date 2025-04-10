use anyhow::Result;
use futures::future::FutureExt;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server, StatusCode};
use mime_guess::from_path;
use percent_encoding::{NON_ALPHANUMERIC, percent_encode};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs::File;
use tokio_util::codec::{BytesCodec, FramedRead};

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
    if req.method() != hyper::Method::GET {
        return Ok(Response::builder()
            .status(StatusCode::METHOD_NOT_ALLOWED)
            .body(Body::empty())?);
    }

    let request_path = req.uri().path();
    let decoded_path = percent_encoding::percent_decode_str(request_path)
        .decode_utf8_lossy()
        .to_string();
    let full_path = dir_path.join(&decoded_path[1..]); // 去掉前导斜杠

    // 安全验证：确保路径在允许的目录内
    let canonical_path = match tokio::fs::canonicalize(&full_path).await {
        Ok(p) => p,
        Err(_) => return Ok(not_found_response()),
    };

    if !canonical_path.starts_with(&*dir_path) {
        return Ok(Response::builder()
            .status(StatusCode::FORBIDDEN)
            .body(Body::empty())?);
    }

    // 处理目录请求
    if tokio::fs::metadata(&canonical_path).await?.is_dir() {
        return handle_directory(&canonical_path, request_path).await;
    }

    // 处理文件请求
    match File::open(&canonical_path).await {
        Ok(file) => {
            let stream = FramedRead::new(file, BytesCodec::new());
            let body = Body::wrap_stream(stream);

            let mime_type = from_path(&canonical_path).first_or_octet_stream();
            let file_name = canonical_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("file");

            // 对文件名进行RFC 5987编码
            let encoded_filename = percent_encode(file_name.as_bytes(), NON_ALPHANUMERIC);

            // 构造Content-Disposition头
            let content_disposition = format!(
                "attachment; filename=\"{}\"; filename*=UTF-8''{}",
                file_name.replace("\"", "\\\""), // 转义双引号
                encoded_filename
            );

            Ok(Response::builder()
                .header("Content-Type", mime_type.as_ref())
                .header("Content-Disposition", content_disposition)
                .body(body)?)
        }
        Err(_) => Ok(not_found_response()),
    }
}

async fn handle_directory(path: &Path, request_path: &str) -> Result<Response<Body>> {
    let mut dir_entries = tokio::fs::read_dir(path).await?;
    let mut html = String::from("<html><body><h1>Directory Listing</h1><ul>");

    while let Some(entry) = dir_entries.next_entry().await? {
        let file_name = entry.file_name();
        let file_name_str = file_name.to_string_lossy();
        let escaped_name = html_escape::encode_text(&file_name_str);

        let path = if request_path.ends_with('/') {
            format!("{}{}", request_path, file_name_str)
        } else {
            format!("{}/{}", request_path, file_name_str)
        };
        let escaped_path = html_escape::encode_text(&path);

        html.push_str(&format!(
            "<li><a href='{}'>{}</a></li>",
            escaped_path, escaped_name
        ));
    }

    html.push_str("</ul></body></html>");
    Ok(Response::new(Body::from(html)))
}

fn not_found_response() -> Response<Body> {
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
