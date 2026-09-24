//! 鉴权端点：登录签发会话 Cookie、登出清除、状态查询。
//! 这三个端点在中间件白名单里，始终可匿名访问。

use axum::extract::State;
use axum::http::{header::SET_COOKIE, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;

use crate::auth::middleware::session_cookie;
use crate::auth::{now_secs, token};
use crate::error::AppError;
use crate::share::record::sha256_hex;
use crate::state::AppState;

const LOGIN_FAILURE_DELAY: std::time::Duration = std::time::Duration::from_secs(1);

#[derive(Deserialize)]
pub struct LoginReq {
    pub password: String,
}

/// POST /api/auth/login — 校验站点密码，签发 HttpOnly 会话 Cookie
pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginReq>,
) -> Result<Response, AppError> {
    let Some(auth) = state.auth.as_ref() else {
        // 未启用鉴权：无需登录，直接视为已认证
        return Ok(Json(serde_json::json!({ "authenticated": true })).into_response());
    };

    // 先各自取 SHA-256 再比较：ct_eq 遇到长度不同会立即返回，直接比原文会泄露密码长度
    let expected = state.config.auth_password.as_deref().unwrap_or_default();
    if !token::ct_eq(
        sha256_hex(req.password.as_bytes()).as_bytes(),
        sha256_hex(expected.as_bytes()).as_bytes(),
    ) {
        // 失败固定慢一拍，抬高在局域网里逐个猜密码的成本
        tokio::time::sleep(LOGIN_FAILURE_DELAY).await;
        return Err(AppError::Unauthorized("invalid password"));
    }

    let tok = token::mint(&auth.key, auth.ttl, now_secs());
    let cookie = build_cookie(&format!("session={tok}"), auth.ttl.as_secs(), auth.cookie_secure);

    let mut resp = Json(serde_json::json!({ "authenticated": true })).into_response();
    resp.headers_mut().insert(SET_COOKIE, cookie.parse().unwrap());
    Ok(resp)
}

/// POST /api/auth/logout — 清除会话 Cookie
pub async fn logout(State(state): State<AppState>) -> Response {
    let secure = state.auth.as_ref().map(|a| a.cookie_secure).unwrap_or(false);
    let cookie = build_cookie("session=", 0, secure);

    let mut resp = StatusCode::OK.into_response();
    resp.headers_mut().insert(SET_COOKIE, cookie.parse().unwrap());
    resp
}

/// GET /api/auth/status — 供 SPA 决定是否弹登录
pub async fn status(State(state): State<AppState>, headers: HeaderMap) -> Json<serde_json::Value> {
    let authenticated = match state.auth.as_ref() {
        Some(auth) => session_cookie(&headers)
            .and_then(|t| token::verify(&auth.key, t, now_secs()))
            .is_some(),
        None => true,
    };
    Json(serde_json::json!({
        "auth_required": state.auth.is_some(),
        "authenticated": authenticated,
    }))
}

fn build_cookie(name_value: &str, max_age: u64, secure: bool) -> String {
    let mut c = format!("{name_value}; HttpOnly; Path=/; SameSite=Strict; Max-Age={max_age}");
    if secure {
        c.push_str("; Secure");
    }
    c
}
