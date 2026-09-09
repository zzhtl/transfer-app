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
use tower::{Layer as _, ServiceBuilder};
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::normalize_path::{NormalizePath, NormalizePathLayer};
use tower_http::request_id::SetRequestIdLayer;
use tower_http::trace::TraceLayer;

use crate::middleware::request_id::MakeRequestUuid;
use crate::middleware::trace::CustomMakeSpan;
use crate::state::AppState;

/// 对外服务的完整栈：路由树 + 路径归一化。
///
/// `NormalizePathLayer` **必须包在 Router 外面**，不能挂进 `Router::layer` 的
/// ServiceBuilder 里：后者在路由**之后**执行，路径匹配已经定了再去修 URI 就晚了。
/// 之前挂在里面时 `/api/healthz/` 落到 `.fallback(static_assets::index)` 上，
/// 返回的是 200 + 前端 HTML 而不是 JSON——因为有 SPA 兜底，这个失效一直看不出来。
pub fn build_service(state: AppState) -> NormalizePath<Router> {
    NormalizePathLayer::trim_trailing_slash().layer(build_router(state))
}

/// `/api` 下没有匹配到任何路由。
///
/// 取 `OriginalUri` 而不是 `Uri`：`nest()` 会把前缀剥掉再交给内层，用 `Uri`
/// 报出来的是 `/nope` 而不是用户真正请求的 `/api/nope`。
async fn api_not_found(
    axum::extract::OriginalUri(uri): axum::extract::OriginalUri,
) -> crate::error::AppError {
    crate::error::AppError::NotFound(format!("接口 {} 不存在", uri.path()))
}

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
        // `/api` 下的未知路径必须是 JSON 404。没有这一条它会落到下面的
        // `.fallback(static_assets::index)` 上，客户端拿到 200 + 一段前端 HTML
        // ——一个拼错的接口路径看起来像「调通了」，是最难查的一类问题。
        .nest("/api", api.fallback(api_not_found))
        // 静态资源
        .route("/", axum::routing::get(static_assets::index))
        .route("/static/{*path}", axum::routing::get(static_assets::serve))
        .fallback(static_assets::index)
        .with_state(state)
        .layer(
            ServiceBuilder::new()
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::http::{Request, StatusCode};
    use clap::Parser as _;
    use tower::ServiceExt as _;

    fn service(dir: &std::path::Path) -> NormalizePath<Router> {
        let config = crate::config::AppConfig::parse_from([
            "transfer-app",
            "-p",
            dir.to_str().expect("临时目录路径"),
        ]);
        let state: AppState =
            std::sync::Arc::new(crate::state::AppStateInner::new(config).expect("建 state"));
        build_service(state)
    }

    /// `NormalizePathLayer` 挂进 `Router::layer` 的 ServiceBuilder 里是**无效的**：
    /// 它在路由之后才执行，路径匹配已经定了。之前就是这么挂的，`/api/healthz/`
    /// 会落到 `.fallback(static_assets::index)` 上，返回 200 + 前端 HTML；
    /// 因为有 SPA 兜底，状态码还是 200，这个失效一直没被发现。
    #[tokio::test]
    async fn trailing_slash_reaches_the_api_handler_not_the_spa_fallback() {
        let dir = tempfile::tempdir().expect("临时目录");

        for uri in ["/api/healthz", "/api/healthz/"] {
            let response = service(dir.path())
                .oneshot(
                    Request::builder()
                        .uri(uri)
                        .body(Body::empty())
                        .expect("请求"),
                )
                .await
                .expect("响应");
            assert_eq!(response.status(), StatusCode::OK, "{uri}");

            let body = to_bytes(response.into_body(), 1024 * 1024)
                .await
                .expect("body");
            let text = String::from_utf8_lossy(&body);
            assert!(
                text.starts_with('{'),
                "{uri} 应当返回 JSON，实际落到了 SPA 兜底上：{}",
                &text[..text.len().min(80)]
            );
        }
    }

    /// 拼错的接口路径必须是 JSON 404，不能落到 SPA 兜底上。
    ///
    /// 落到兜底上的话客户端拿到的是 200 + 一段 HTML，看起来像「调通了」——
    /// 而实际上那个接口根本不存在。
    #[tokio::test]
    async fn an_unknown_api_path_is_a_json_404_not_the_spa() {
        let dir = tempfile::tempdir().expect("临时目录");
        for uri in ["/api/nope", "/api/files/typo", "/api/s"] {
            let response = service(dir.path())
                .oneshot(
                    Request::builder()
                        .uri(uri)
                        .body(Body::empty())
                        .expect("请求"),
                )
                .await
                .expect("响应");
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{uri}");

            let body = to_bytes(response.into_body(), 1024 * 1024)
                .await
                .expect("body");
            let text = String::from_utf8_lossy(&body);
            assert!(
                text.starts_with('{'),
                "{uri} 应当返回 JSON，实际是：{}",
                &text[..text.len().min(80)]
            );
        }
    }

    /// 归一化不能把前端路由也一起吃掉。
    #[tokio::test]
    async fn frontend_routes_still_fall_back_to_the_spa() {
        let dir = tempfile::tempdir().expect("临时目录");
        let response = service(dir.path())
            .oneshot(
                Request::builder()
                    .uri("/some/spa/route")
                    .body(Body::empty())
                    .expect("请求"),
            )
            .await
            .expect("响应");
        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("body");
        assert!(String::from_utf8_lossy(&body).contains("<!DOCTYPE html>"));
    }
}
