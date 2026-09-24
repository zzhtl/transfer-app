//! 端到端接口测试：直接驱动完整的服务栈（路由 + 全部中间件），断言状态码、响应头和响应体。

use axum::body::{to_bytes, Body, Bytes};
use axum::http::{header, HeaderMap, Request, StatusCode};
use base64::Engine as _;
use clap::Parser as _;
use tower::ServiceExt as _;

use super::*;

struct TestApp {
    dir: tempfile::TempDir,
    service: NormalizePath<Router>,
}

impl TestApp {
    fn new() -> Self {
        Self::with_args(&[])
    }

    fn with_args(extra: &[&str]) -> Self {
        let dir = tempfile::tempdir().expect("临时目录");
        let root = dunce::canonicalize(dir.path()).expect("规范化临时目录");
        let mut args = vec!["transfer-app", "-p", root.to_str().expect("临时目录路径")];
        args.extend_from_slice(extra);
        let config = crate::config::AppConfig::parse_from(args);
        let state: AppState =
            std::sync::Arc::new(crate::state::AppStateInner::new(config).expect("建 state"));
        Self {
            dir,
            service: build_service(state),
        }
    }

    fn root(&self) -> std::path::PathBuf {
        dunce::canonicalize(self.dir.path()).expect("规范化临时目录")
    }

    fn write(&self, rel: &str, content: &[u8]) {
        let path = self.root().join(rel);
        std::fs::create_dir_all(path.parent().expect("父目录")).expect("建父目录");
        std::fs::write(path, content).expect("写文件");
    }

    async fn send(&self, request: Request<Body>) -> (StatusCode, HeaderMap, Bytes) {
        let response = self.service.clone().oneshot(request).await.expect("响应");
        let status = response.status();
        let headers = response.headers().clone();
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        (status, headers, body)
    }

    async fn get(&self, uri: &str) -> (StatusCode, HeaderMap, Bytes) {
        self.send(Request::get(uri).body(Body::empty()).expect("请求"))
            .await
    }
}

fn tus_metadata(pairs: &[(&str, &str)]) -> String {
    let b64 = base64::engine::general_purpose::STANDARD;
    pairs
        .iter()
        .map(|(k, v)| format!("{k} {}", b64.encode(v)))
        .collect::<Vec<_>>()
        .join(",")
}

async fn tus_upload(app: &TestApp, meta: &[(&str, &str)], data: &[u8]) -> StatusCode {
    let (status, headers, _) = app
        .send(
            Request::post("/api/upload")
                .header("Tus-Resumable", "1.0.0")
                .header("Upload-Length", data.len().to_string())
                .header("Upload-Metadata", tus_metadata(meta))
                .body(Body::empty())
                .expect("请求"),
        )
        .await;
    if status != StatusCode::CREATED {
        return status;
    }
    let location = headers[header::LOCATION]
        .to_str()
        .expect("location")
        .to_string();
    let (status, _, _) = app
        .send(
            Request::patch(location)
                .header("Tus-Resumable", "1.0.0")
                .header("Upload-Offset", "0")
                .header(header::CONTENT_TYPE, "application/offset+octet-stream")
                .body(Body::from(data.to_vec()))
                .expect("请求"),
        )
        .await;
    status
}

/// relativePath 之前原样拼进 `target_dir.join(..)`：`../x` 或绝对路径就能在共享根
/// 之外建目录、写文件，匿名模式下局域网里任何人都能做到。
#[tokio::test]
async fn upload_relative_path_cannot_escape_the_root() {
    let app = TestApp::new();
    let outside = app.root().parent().expect("上级目录").to_path_buf();
    let marker = format!("escaped-{}", uuid::Uuid::new_v4().simple());

    for rel in [
        format!("../{marker}/x.txt"),
        format!("a/../../{marker}/x.txt"),
        format!("{}/x.txt", outside.join(&marker).display()),
    ] {
        let status = tus_upload(
            &app,
            &[("filename", "x.txt"), ("relativePath", &rel)],
            b"hello",
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{rel}");
    }
    assert!(!outside.join(&marker).exists());
}

#[tokio::test]
async fn upload_with_folder_structure_still_lands_inside_the_target() {
    let app = TestApp::new();
    std::fs::create_dir_all(app.root().join("dest")).expect("建目录");
    let status = tus_upload(
        &app,
        &[
            ("filename", "f.txt"),
            ("relativePath", "album/2024/f.txt"),
            ("targetDir", "dest"),
        ],
        b"hello",
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let written = app.root().join("dest/album/2024/f.txt");
    assert_eq!(std::fs::read(written).expect("落盘文件"), b"hello");
}

/// 内部目录能被 API 写入的话，就能伪造一个 target_dir 指向外部的 `.meta`，
/// 重启恢复后由 finalize 写到共享根之外。
#[tokio::test]
async fn reserved_dir_cannot_be_written_through_the_api() {
    let app = TestApp::new();
    let (status, _, _) = app
        .send(
            Request::post("/api/files/save")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"path":".transfer-tmp/evil.meta","content":"{}"}"#,
                ))
                .expect("请求"),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(!app.root().join(".transfer-tmp/evil.meta").exists());

    let status = tus_upload(
        &app,
        &[("filename", "evil.meta"), ("targetDir", ".transfer-tmp")],
        b"{}",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

async fn post_json(app: &TestApp, uri: &str, json: serde_json::Value) -> (StatusCode, Bytes) {
    let (status, _, body) = app
        .send(
            Request::post(uri)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json.to_string()))
                .expect("请求"),
        )
        .await;
    (status, body)
}

#[tokio::test]
async fn moving_or_copying_a_folder_into_itself_is_rejected() {
    let app = TestApp::new();
    app.write("a/b/keep.txt", b"x");

    for uri in ["/api/files/move", "/api/files/copy"] {
        let (status, _) = post_json(
            &app,
            uri,
            serde_json::json!({"source": "a", "destination": "a/b"}),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}");
    }
    // 源目录原封不动，也没有被复制出一层层嵌套
    assert!(app.root().join("a/b/keep.txt").exists());
    assert!(!app.root().join("a/b/a").exists());
}

#[tokio::test]
async fn existing_target_is_a_conflict_without_absolute_paths() {
    let app = TestApp::new();
    app.write("x.txt", b"1");
    app.write("y.txt", b"2");
    let (status, body) = post_json(
        &app,
        "/api/files/rename",
        serde_json::json!({"path": "x.txt", "new_name": "y.txt"}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let text = String::from_utf8_lossy(&body);
    assert!(text.contains("already_exists"), "{text}");
    assert!(!text.contains(&app.root().display().to_string()), "{text}");
}

