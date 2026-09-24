use std::net::IpAddr;

use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::state::AppState;

#[derive(Serialize)]
pub struct ServerInfo {
    version: &'static str,
    /// 局域网里其他设备能访问到的地址；只绑定回环地址时为 null
    lan_origin: Option<String>,
}

/// GET /api/server-info
///
/// 在本机用 127.0.0.1 打开页面时，`location.origin` 生成的分享链接和二维码
/// 在别的设备上打不开，前端用这里的 `lan_origin` 替换。
pub async fn get(State(state): State<AppState>) -> Json<ServerInfo> {
    let config = &state.config;
    let tls = cfg!(feature = "tls") && config.tls_cert.is_some() && config.tls_key.is_some();
    let scheme = if tls { "https" } else { "http" };

    let host = match config.bind {
        ip if ip.is_loopback() => None,
        // 每次请求现探测：DHCP 换了地址、切换了网络也能跟上
        ip if ip.is_unspecified() => crate::util::ip::get_local_ip(),
        IpAddr::V6(v6) => Some(format!("[{v6}]")),
        IpAddr::V4(v4) => Some(v4.to_string()),
    };

    Json(ServerInfo {
        version: env!("CARGO_PKG_VERSION"),
        lan_origin: host.map(|host| format!("{scheme}://{host}:{}", config.port)),
    })
}
