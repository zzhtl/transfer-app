use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;

use crate::state::AppState;

#[derive(Serialize)]
pub struct HealthResponse {
    status: &'static str,
    version: &'static str,
}

pub async fn live() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

pub async fn ready(State(state): State<AppState>) -> Result<Json<HealthResponse>, StatusCode> {
    // 检查 root 目录可读
    if !state.root.exists() || !state.root.is_dir() {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    // 检查 tmp 目录可写
    let tmp = state.upload_manager.tmp_dir();
    if !tmp.exists() {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    Ok(Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    }))
}
