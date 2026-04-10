pub mod download;
pub mod files;
pub mod health;
pub mod preview;
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
        // 健康检查
        .route("/healthz", axum::routing::get(health::live))
        .route("/readyz", axum::routing::get(health::ready));

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
                .layer(CatchPanicLayer::new()),
        )
}
