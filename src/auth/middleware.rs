//! 鉴权中间件。挂在 CatchPanicLayer 外侧。
//! 不特判 OPTIONS：服务只接受同源请求（不挂 CORS），浏览器不会发预检；
//! tus 的 `OPTIONS /api/upload` 能力发现在启用鉴权时同样需要登录。
//! 中间件只读 header/URI 后原样透传 Request，绝不触碰 body（保住流式 PATCH）。

use axum::extract::{Request, State};
use axum::http::{header::COOKIE, HeaderMap};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::auth::{now_secs, token};
use crate::error::AppError;
use crate::state::AppState;

/// 会话校验：未启用鉴权→透传；非 /api/→透传；公开白名单→透传；其余需有效会话 Cookie。
pub async fn require_auth(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let Some(auth) = state.auth.as_ref() else {
        return next.run(req).await;
    };

    let path = req.uri().path();
    if !path.starts_with("/api/") || is_public_api(path) {
        return next.run(req).await;
    }

    let authed = session_cookie(req.headers())
        .and_then(|t| token::verify(&auth.key, t, now_secs()))
        .is_some();

    if authed {
        next.run(req).await
    } else {
        AppError::Unauthorized("login required").into_response()
    }
}

/// 免鉴权的公开 API：登录相关、健康检查、以及分享公开访问前缀。
fn is_public_api(path: &str) -> bool {
    matches!(
        path,
        "/api/auth/login" | "/api/auth/logout" | "/api/auth/status" | "/api/healthz" | "/api/readyz"
    ) || path.starts_with("/api/s/")
}

/// 从 Cookie 头解析 `session=<token>`
pub fn session_cookie(headers: &HeaderMap) -> Option<&str> {
    let cookie = headers.get(COOKIE)?.to_str().ok()?;
    for part in cookie.split(';') {
        if let Some(val) = part.trim().strip_prefix("session=") {
            return Some(val);
        }
    }
    None
}
