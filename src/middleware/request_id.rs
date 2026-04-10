use tower_http::request_id::{MakeRequestId, RequestId};

/// 为每个请求生成 UUID request id
#[derive(Clone)]
pub struct MakeRequestUuid;

impl MakeRequestId for MakeRequestUuid {
    fn make_request_id<B>(
        &mut self,
        _request: &axum::http::Request<B>,
    ) -> Option<RequestId> {
        let id = uuid::Uuid::new_v4().to_string();
        Some(RequestId::new(id.parse().unwrap()))
    }
}
