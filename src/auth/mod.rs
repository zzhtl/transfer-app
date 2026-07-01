//! 可选的密码鉴权：单站点密码 + 无状态 HMAC 会话 token。
//! 未配置 `--auth-password` 时不构造 `AuthContext`，中间件直接放行，行为与匿名模式一致。

pub mod middleware;
pub mod token;

use std::time::Duration;

/// 鉴权运行时上下文（仅在启用鉴权时存在于 AppState）
pub struct AuthContext {
    /// HMAC 签名 key（每进程随机）
    pub key: [u8; 32],
    /// 会话有效期
    pub ttl: Duration,
    /// 是否给 Cookie 加 Secure（TLS 已启用时）
    pub cookie_secure: bool,
}

/// 当前 unix 秒
pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
