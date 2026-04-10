use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::fs::{meta::FileMeta, operations, walker};
use crate::state::AppState;

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
        return Err(AppError::IsADirectory);
    }

    let mut entries = walker::list_directory(&abs).await?;
    // 填充相对路径
    let prefix = &state.root;
    for entry in &mut entries {
        let entry_abs = abs.join(&entry.name);
        entry.path = entry_abs
            .strip_prefix(prefix)
            .unwrap_or(&entry_abs)
            .to_string_lossy()
            .to_string();
    }
    let breadcrumbs = build_breadcrumbs(&abs, &state.root);

    let display_path = abs
        .strip_prefix(&state.root)
        .unwrap_or(&abs)
        .to_string_lossy()
        .to_string();

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

    let base_clone = base.clone();
    let results = tokio::task::spawn_blocking(move || {
        let mut found = Vec::new();
        for entry in walkdir::WalkDir::new(&base_clone)
            .min_depth(1)
            .max_depth(10)
            .into_iter()
            .filter_map(Result::ok)
        {
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
