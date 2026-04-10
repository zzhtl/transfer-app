use axum::http::Request;
use tower_http::trace::MakeSpan;

/// 自定义 trace span
#[derive(Clone)]
pub struct CustomMakeSpan;

impl<B> MakeSpan<B> for CustomMakeSpan {
    fn make_span(&mut self, request: &Request<B>) -> tracing::Span {
        let req_id = request
            .headers()
            .get("x-request-id")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("-");

        tracing::info_span!(
            "http",
            method = %request.method(),
            uri = %request.uri(),
            req_id = %req_id,
        )
    }
}
