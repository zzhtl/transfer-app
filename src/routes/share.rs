//! 分享端点。
//! 受保护：POST/GET /api/share、DELETE /api/share/{id}（鉴权开启时需登录）。
//! 公开（中间件白名单 /api/s/）：meta / download / zip / list。

use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, Response, StatusCode};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::auth::now_secs;
use crate::error::AppError;
use crate::fs::walker;
use crate::routes::{download::serve_file, zipdl::zip_response};
use crate::share::record::{sha256_hex, ShareRecord};
use crate::state::AppState;

// ===== 受保护：管理 =====

#[derive(Deserialize)]
pub struct CreateShareReq {
    pub path: String,
    pub expires_in_secs: Option<u64>,
    pub code: Option<String>,
    #[serde(default)]
    pub download_only: bool,
}

#[derive(Serialize)]
pub struct ShareInfo {
    pub id: String,
    pub token: String,
    pub url: String,
    pub target: String,
    pub name: String,
    pub is_dir: bool,
    pub created_at: u64,
    pub expires_at: u64,
    pub has_code: bool,
    pub download_only: bool,
}

fn to_info(rec: &ShareRecord) -> ShareInfo {
    let name = std::path::Path::new(&rec.target)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| rec.target.clone());
    ShareInfo {
        id: rec.id.clone(),
        token: rec.token.clone(),
        url: format!("/s/{}", rec.token),
        target: rec.target.clone(),
        name,
        is_dir: rec.is_dir,
        created_at: rec.created_at,
        expires_at: rec.expires_at,
        has_code: rec.code_hash.is_some(),
        download_only: rec.download_only,
    }
}

/// POST /api/share — 创建分享
pub async fn create(
    State(state): State<AppState>,
    Json(req): Json<CreateShareReq>,
) -> Result<Json<ShareInfo>, AppError> {
    let abs = state.path_safety.resolve(&req.path)?;
    let meta = tokio::fs::metadata(&abs)
        .await
        .map_err(|_| AppError::NotFound(req.path.clone()))?;

    // 规范化为 root 相对路径存储
    let target = abs
        .strip_prefix(&state.root)
        .unwrap_or(&abs)
        .to_string_lossy()
        .to_string();
    if target.is_empty() {
        return Err(AppError::BadRequest("cannot share root".into()));
    }

    let now = now_secs();
    let default_ttl = state.config.share_expiration_default_secs;
    let max_ttl = state.config.share_max_expiration_secs;
    let ttl = req.expires_in_secs.unwrap_or(default_ttl).clamp(60, max_ttl);

    let code_hash = req
        .code
        .as_ref()
        .filter(|c| !c.is_empty())
        .map(|c| sha256_hex(c.as_bytes()));

    let rec = ShareRecord {
        id: uuid::Uuid::new_v4().to_string().replace('-', ""),
        token: uuid::Uuid::new_v4().to_string().replace('-', ""),
        target,
        is_dir: meta.is_dir(),
        created_at: now,
        expires_at: now.saturating_add(ttl),
        code_hash,
        download_only: req.download_only,
    };

    let arc = state
        .share_manager
        .create(rec)
        .await
        .map_err(AppError::Internal)?;
    Ok(Json(to_info(&arc)))
}

/// GET /api/share — 列出分享
pub async fn list(State(state): State<AppState>) -> Json<Vec<ShareInfo>> {
    let mut infos: Vec<ShareInfo> = state.share_manager.list().iter().map(|r| to_info(r)).collect();
    infos.sort_by_key(|s| std::cmp::Reverse(s.created_at));
    Json(infos)
}

/// DELETE /api/share/{id} — 吊销分享
pub async fn revoke(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    if state.share_manager.revoke(&id).await {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound(id))
    }
}

// ===== 公开访问 =====

#[derive(Deserialize, Default)]
pub struct AccessParams {
    pub code: Option<String>,
    #[serde(default)]
    pub path: String,
}

/// 取有效分享（未知/过期都返回 404，避免有效性 oracle）
fn get_valid(state: &AppState, token: &str) -> Result<Arc<ShareRecord>, AppError> {
    let rec = state
        .share_manager
        .get(token)
        .ok_or_else(|| AppError::NotFound("share".into()))?;
    if rec.is_expired(now_secs()) {
        return Err(AppError::NotFound("share".into()));
    }
    Ok(rec)
}

/// 在分享子树内解析子路径，并做包含性复检（防 root 内符号链接越权）
fn resolve_within_share(
    state: &AppState,
    rec: &ShareRecord,
    subpath: &str,
) -> Result<PathBuf, AppError> {
    let base = state.path_safety.resolve(&rec.target)?;
    if subpath.is_empty() {
        return Ok(base);
    }
    // PathSafety 会过滤 `..`，天然阻止逃逸；再显式校验一次 canonical 包含关系
    let combined = format!("{}/{}", rec.target, subpath);
    let abs = state.path_safety.resolve(&combined)?;
    if !abs.starts_with(&base) {
        return Err(AppError::Forbidden("outside share"));
    }
    Ok(abs)
}

/// GET /api/s/{token} — 分享元信息（不校验提取码、不列目录）
pub async fn meta(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let rec = get_valid(&state, &token)?;
    let abs = state.path_safety.resolve(&rec.target)?;
    let m = tokio::fs::metadata(&abs)
        .await
        .map_err(|_| AppError::NotFound("share".into()))?;
    let name = abs
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    Ok(Json(serde_json::json!({
        "name": name,
        "size": if rec.is_dir { serde_json::Value::Null } else { serde_json::json!(m.len()) },
        "is_dir": rec.is_dir,
        "download_only": rec.download_only,
        "requires_code": rec.requires_code(),
        "expires_at": rec.expires_at,
    })))
}

/// GET /api/s/{token}/download?code=&path= — 下载（文件分享整体；目录分享的子文件）
pub async fn download(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Query(params): Query<AccessParams>,
    headers: HeaderMap,
) -> Result<Response<Body>, AppError> {
    let rec = get_valid(&state, &token)?;
    if !rec.verify_code(params.code.as_deref()) {
        return Err(AppError::Forbidden("invalid code"));
    }

    let abs = if rec.is_dir {
        if params.path.is_empty() {
            return Err(AppError::BadRequest("use zip for a directory".into()));
        }
        if rec.download_only {
            return Err(AppError::Forbidden("browsing disabled"));
        }
        resolve_within_share(&state, &rec, &params.path)?
    } else {
        state.path_safety.resolve(&rec.target)?
    };

    serve_file(&abs, true, &headers).await
}

/// GET /api/s/{token}/zip?code= — 目录分享打包下载
pub async fn zip(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Query(params): Query<AccessParams>,
) -> Result<Response<Body>, AppError> {
    let rec = get_valid(&state, &token)?;
    if !rec.verify_code(params.code.as_deref()) {
        return Err(AppError::Forbidden("invalid code"));
    }
    if !rec.is_dir {
        return Err(AppError::BadRequest("not a directory".into()));
    }
    let abs = state.path_safety.resolve(&rec.target)?;
    let name = abs
        .file_name()
        .map(|n| format!("{}.zip", n.to_string_lossy()))
        .unwrap_or_else(|| "share.zip".to_string());
    Ok(zip_response(vec![abs], name))
}

/// GET /api/s/{token}/list?code=&path= — 目录分享浏览（非 download_only）
pub async fn list_dir(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Query(params): Query<AccessParams>,
) -> Result<Json<serde_json::Value>, AppError> {
    let rec = get_valid(&state, &token)?;
    if !rec.verify_code(params.code.as_deref()) {
        return Err(AppError::Forbidden("invalid code"));
    }
    if !rec.is_dir {
        return Err(AppError::BadRequest("not a directory".into()));
    }
    if rec.download_only {
        return Err(AppError::Forbidden("browsing disabled"));
    }

    let base = state.path_safety.resolve(&rec.target)?;
    let abs = resolve_within_share(&state, &rec, &params.path)?;
    if !abs.is_dir() {
        return Err(AppError::NotADirectory);
    }

    // path 填为相对分享 base 的路径，供前端作为子路径二次访问
    let prefix = abs
        .strip_prefix(&base)
        .unwrap_or(&abs)
        .to_string_lossy()
        .to_string();
    let entries = walker::list_directory(&abs, &prefix).await?;

    Ok(Json(serde_json::json!({
        "path": params.path,
        "entries": entries,
    })))
}
