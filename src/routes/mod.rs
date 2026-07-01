pub mod auth;
pub mod download;
pub mod files;
pub mod health;
pub mod preview;
pub mod share;
pub mod static_assets;
pub mod upload;
pub mod zipdl;

use axum::Router;
use tower::ServiceBuilder;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::normalize_path::NormalizePathLayer;
use tower_http::request_id::SetRequestIdLayer;
use tower_http::trace::TraceLayer;

use crate::middleware::request_id::MakeRequestUuid;
use crate::middleware::trace::CustomMakeSpan;
use crate::state::AppState;

/// 构建完整的路由树
pub fn build_router(state: AppState) -> Router {
    let api = Router::new()
        // 文件 CRUD
        .route("/files", axum::routing::get(files::list))
        .route("/files/mkdir", axum::routing::post(files::mkdir))
        .route("/files/rename", axum::routing::post(files::rename))
        .route("/files/move", axum::routing::post(files::r#move))
        .route("/files/copy", axum::routing::post(files::copy))
        .route("/files/delete", axum::routing::post(files::batch_delete))
        .route("/files/search", axum::routing::get(files::search))
        .route("/files/content", axum::routing::get(files::content))
        .route("/files/save", axum::routing::post(files::save))
        // tus 上传
        .route(
            "/upload",
            axum::routing::options(upload::options).post(upload::create),
        )
        .route(
            "/upload/{file_id}",
            axum::routing::head(upload::head)
                .patch(upload::patch)
                .delete(upload::cancel),
        )
        // 下载
        .route("/download/{*path}", axum::routing::get(download::get))
        .route("/download-zip", axum::routing::get(zipdl::get))
        // 预览
        .route("/preview/{*path}", axum::routing::get(preview::get))
        .route("/preview/markdown", axum::routing::post(preview::render_md))
        // 分享管理（鉴权开启时需登录）
        .route(
            "/share",
            axum::routing::post(share::create).get(share::list),
        )
        .route("/share/{id}", axum::routing::delete(share::revoke))
        // 分享公开访问（中间件白名单 /api/s/，免登录）
        .route("/s/{token}", axum::routing::get(share::meta))
        .route("/s/{token}/download", axum::routing::get(share::download))
        .route("/s/{token}/zip", axum::routing::get(share::zip))
        .route("/s/{token}/list", axum::routing::get(share::list_dir))
        // 鉴权（在中间件白名单内，始终可匿名访问）
        .route("/auth/login", axum::routing::post(auth::login))
        .route("/auth/logout", axum::routing::post(auth::logout))
        .route("/auth/status", axum::routing::get(auth::status))
        // 健康检查
        .route("/healthz", axum::routing::get(health::live))
        .route("/readyz", axum::routing::get(health::ready));

    // 鉴权中间件需要一份 state（挂在 CORS 内侧）
    let auth_state = state.clone();

    Router::new()
        .nest("/api", api)
        // 静态资源
        .route("/", axum::routing::get(static_assets::index))
        .route("/static/{*path}", axum::routing::get(static_assets::serve))
        .fallback(static_assets::index)
        .with_state(state)
        .layer(
            ServiceBuilder::new()
                .layer(NormalizePathLayer::trim_trailing_slash())
                .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
                .layer(
                    TraceLayer::new_for_http().make_span_with(CustomMakeSpan),
                )
                .layer(
                    CompressionLayer::new()
                        .br(true)
                        .gzip(true)
                        .no_br()  // 只用 gzip，br 对动态内容收益不大
                )
                .layer(CorsLayer::very_permissive())
                // 鉴权：CORS 内侧（OPTIONS 已被 CORS 短路，tus 预检不受影响）、CatchPanic 外侧
                .layer(axum::middleware::from_fn_with_state(
                    auth_state,
                    crate::auth::middleware::require_auth,
                ))
                .layer(CatchPanicLayer::new()),
        )
}
