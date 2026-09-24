use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::header::CONTENT_TYPE;
use axum::http::StatusCode;
use axum::response::Response;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::fs::{meta::FileMeta, operations, walker};
use crate::state::AppState;

/// 在线编辑的文件大小上限（1 MiB，兼顾 axum 默认 2MB body 限制）
const MAX_EDIT_SIZE: u64 = 1024 * 1024;

#[derive(Deserialize)]
pub struct ListParams {
    #[serde(default)]
    pub path: String,
}

#[derive(Serialize)]
pub struct ListResponse {
    pub path: String,
    pub entries: Vec<FileMeta>,
    pub breadcrumbs: Vec<Breadcrumb>,
}

#[derive(Serialize)]
pub struct Breadcrumb {
    pub name: String,
    pub path: String,
}

/// GET /api/files?path=xxx
pub async fn list(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<Json<ListResponse>, AppError> {
    let abs = if params.path.is_empty() {
        state.root.clone()
    } else {
        state.path_safety.resolve(&params.path)?
    };

    if !abs.is_dir() {
        return Err(AppError::NotADirectory);
    }

    let display_path = abs
        .strip_prefix(&state.root)
        .unwrap_or(&abs)
        .to_string_lossy()
        .to_string();
    let entries = walker::list_directory(&abs, &display_path).await?;
    let breadcrumbs = build_breadcrumbs(&abs, &state.root);

    Ok(Json(ListResponse {
        path: display_path,
        entries,
        breadcrumbs,
    }))
}

fn build_breadcrumbs(
    current: &std::path::Path,
    root: &std::path::Path,
) -> Vec<Breadcrumb> {
    let mut crumbs = vec![Breadcrumb {
        name: "Home".to_string(),
        path: String::new(),
    }];

    if let Ok(relative) = current.strip_prefix(root) {
        let mut accumulated = String::new();
        for component in relative.components() {
            let name = component.as_os_str().to_string_lossy().to_string();
            if !accumulated.is_empty() {
                accumulated.push('/');
            }
            accumulated.push_str(&name);
            crumbs.push(Breadcrumb {
                name,
                path: accumulated.clone(),
            });
        }
    }

    crumbs
}

#[derive(Deserialize)]
pub struct MkdirRequest {
    pub path: String,
    pub name: String,
}

/// POST /api/files/mkdir
pub async fn mkdir(
    State(state): State<AppState>,
    Json(req): Json<MkdirRequest>,
) -> Result<StatusCode, AppError> {
    let parent = if req.path.is_empty() {
        state.root.clone()
    } else {
        state.path_safety.resolve(&req.path)?
    };
    let name = sanitize_filename::sanitize(&req.name);
    let target = parent.join(&name);
    operations::mkdir(&target).await?;
    Ok(StatusCode::CREATED)
}

#[derive(Deserialize)]
pub struct RenameRequest {
    pub path: String,
    pub new_name: String,
}

/// POST /api/files/rename
pub async fn rename(
    State(state): State<AppState>,
    Json(req): Json<RenameRequest>,
) -> Result<StatusCode, AppError> {
    let from = state.path_safety.resolve(&req.path)?;
    let new_name = sanitize_filename::sanitize(&req.new_name);
    let to = from
        .parent()
        .ok_or(AppError::BadRequest("no parent".into()))?
        .join(&new_name);
    operations::rename(&from, &to).await?;
    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
pub struct MoveRequest {
    pub source: String,
    pub destination: String,
}

/// POST /api/files/move
pub async fn r#move(
    State(state): State<AppState>,
    Json(req): Json<MoveRequest>,
) -> Result<StatusCode, AppError> {
    let from = state.path_safety.resolve(&req.source)?;
    let dest_dir = state.path_safety.resolve(&req.destination)?;
    let name = from
        .file_name()
        .ok_or(AppError::BadRequest("no filename".into()))?;
    let to = dest_dir.join(name);
    operations::move_entry(&from, &to).await?;
    Ok(StatusCode::OK)
}

/// POST /api/files/copy
pub async fn copy(
    State(state): State<AppState>,
    Json(req): Json<MoveRequest>,
) -> Result<StatusCode, AppError> {
    let from = state.path_safety.resolve(&req.source)?;
    let dest_dir = state.path_safety.resolve(&req.destination)?;
    let name = from
        .file_name()
        .ok_or(AppError::BadRequest("no filename".into()))?;
    let to = dest_dir.join(name);
    operations::copy_file(&from, &to).await?;
    Ok(StatusCode::CREATED)
}

#[derive(Deserialize)]
pub struct BatchDeleteRequest {
    pub paths: Vec<String>,
}

/// POST /api/files/delete
pub async fn batch_delete(
    State(state): State<AppState>,
    Json(req): Json<BatchDeleteRequest>,
) -> Result<StatusCode, AppError> {
    for path_str in &req.paths {
        let path = state.path_safety.resolve(path_str)?;
        // 不允许删除根目录
        if path == state.root {
            return Err(AppError::Forbidden("cannot delete root directory"));
        }
        operations::delete(&path).await?;
    }
    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
pub struct SearchParams {
    pub q: String,
    #[serde(default)]
    pub path: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    50
}

struct CancelOnDrop(Arc<AtomicBool>);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// GET /api/files/search?q=xxx&path=xxx
pub async fn search(
    State(state): State<AppState>,
    Query(params): Query<SearchParams>,
) -> Result<Json<Vec<FileMeta>>, AppError> {
    let base = if params.path.is_empty() {
        state.root.clone()
    } else {
        state.path_safety.resolve(&params.path)?
    };

    let query = params.q.to_lowercase();
    let limit = params.limit.min(200);

    // 客户端中止请求（比如继续输入发起了新搜索）时 handler future 被 drop，
    // 借 guard 通知后台遍历停下，免得连续输入时堆起一串全盘遍历
    let cancelled = Arc::new(AtomicBool::new(false));
    let _cancel_on_drop = CancelOnDrop(cancelled.clone());

    let base_clone = base.clone();
    let results = tokio::task::spawn_blocking(move || {
        let mut found = Vec::new();
        for entry in walkdir::WalkDir::new(&base_clone)
            .min_depth(1)
            .max_depth(10)
            .into_iter()
            .filter_entry(|e| e.file_name() != ".transfer-tmp")
            .filter_map(Result::ok)
        {
            if cancelled.load(Ordering::Relaxed) {
                break;
            }
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if name.contains(&query) {
                found.push(entry.into_path());
            }
            if found.len() >= limit {
                break;
            }
        }
        found
    })
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("search: {}", e)))?;

    let mut metas = Vec::with_capacity(results.len());
    for path in results {
        if let Ok(mut meta) = FileMeta::from_path(&path).await {
            meta.path = path
                .strip_prefix(&state.root)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            metas.push(meta);
        }
    }

    Ok(Json(metas))
}

/// GET /api/files/content?path= — 读取文本文件完整内容（供在线编辑，限 1MiB）
pub async fn content(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<Response<Body>, AppError> {
    let abs = state.path_safety.resolve(&params.path)?;
    if abs.is_dir() {
        return Err(AppError::IsADirectory);
    }
    let meta = tokio::fs::metadata(&abs).await?;
    if meta.len() > MAX_EDIT_SIZE {
        return Err(AppError::BadRequest("file too large to edit".into()));
    }
    let data = tokio::fs::read(&abs).await?;
    if !data.is_empty() && !content_inspector::inspect(&data).is_text() {
        return Err(AppError::BadRequest("not a text file".into()));
    }
    let text = String::from_utf8_lossy(&data).to_string();
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Body::from(text))
        .unwrap())
}

#[derive(Deserialize)]
pub struct SaveRequest {
    pub path: String,
    pub content: String,
}

/// POST /api/files/save — 保存文本文件（原子写；新建文件传空内容即可）
pub async fn save(
    State(state): State<AppState>,
    Json(req): Json<SaveRequest>,
) -> Result<StatusCode, AppError> {
    if req.content.len() as u64 > MAX_EDIT_SIZE {
        return Err(AppError::PayloadTooLarge);
    }
    let abs = state.path_safety.resolve(&req.path)?;
    if abs.is_dir() {
        return Err(AppError::IsADirectory);
    }
    let parent = abs
        .parent()
        .ok_or_else(|| AppError::BadRequest("no parent directory".into()))?;

    // 原子写：先写临时文件，再 rename（仿 upload finalize）
    let tmp = parent.join(format!(".{}.savetmp", uuid::Uuid::new_v4().simple()));
    tokio::fs::write(&tmp, req.content.as_bytes()).await?;
    tokio::fs::rename(&tmp, &abs).await?;
    Ok(StatusCode::OK)
}
