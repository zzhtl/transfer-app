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

fn zip_names(body: &[u8]) -> Vec<String> {
    // 用 async_zip 的同步读不方便，这里直接扫中央目录：签名 PK\x01\x02，
    // 文件名长度在偏移 28，文件名从偏移 46 开始
    let mut names = Vec::new();
    let mut i = 0;
    while i + 46 <= body.len() {
        if &body[i..i + 4] == b"PK\x01\x02" {
            let len = u16::from_le_bytes([body[i + 28], body[i + 29]]) as usize;
            names.push(String::from_utf8_lossy(&body[i + 46..i + 46 + len]).into_owned());
            i += 46 + len;
        } else {
            i += 1;
        }
    }
    names.sort();
    names
}

/// 前端一直发重复的 `paths` 参数；之前后端按单个字符串解析，第二个 `paths`
/// 触发 serde 的 duplicate field，多选打包必定 400。
#[tokio::test]
async fn zip_accepts_repeated_paths_and_keeps_commas_and_spaces() {
    let app = TestApp::new();
    app.write("docs/a,b.txt", b"comma");
    app.write("docs/ lead.txt", b"space");
    app.write("docs/sub/deep.txt", b"deep");

    let uri = "/api/download-zip?paths=docs%2Fa%2Cb.txt&paths=docs%2F%20lead.txt&paths=docs%2Fsub";
    let (status, headers, body) = app.get(uri).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CONTENT_TYPE], "application/zip");
    // 条目相对于各自的父目录，不再多套一层 docs/
    assert_eq!(
        zip_names(&body),
        vec![" lead.txt", "a,b.txt", "sub/deep.txt"]
    );
}

#[tokio::test]
async fn zip_without_paths_is_a_bad_request() {
    let app = TestApp::new();
    let (status, _, _) = app.get("/api/download-zip?name=x.zip").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

/// 默认压缩谓词会把视频、zip、octet-stream 的下载也实时 gzip：吞吐被压到 gzip 的速度，
/// 还丢掉了 Content-Length，浏览器看不到下载进度。
#[tokio::test]
async fn file_downloads_are_not_gzipped_but_api_json_is() {
    let app = TestApp::new();
    // 全 0 数据极易压缩：如果还被压，这里一定能看出来
    app.write("big.bin", &vec![0u8; 64 * 1024]);
    app.write("notes.txt", &vec![b'a'; 64 * 1024]);
    for i in 0..200 {
        app.write(&format!("many/file-{i:03}.txt"), b"x");
    }

    for uri in [
        "/api/download/big.bin?download=1",
        "/api/download/notes.txt",
    ] {
        let (status, headers, body) = app
            .send(
                Request::get(uri)
                    .header(header::ACCEPT_ENCODING, "gzip")
                    .body(Body::empty())
                    .expect("请求"),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{uri}");
        assert!(
            headers.get(header::CONTENT_ENCODING).is_none(),
            "{uri} 不该被压缩"
        );
        assert_eq!(headers[header::CONTENT_LENGTH], "65536", "{uri}");
        assert_eq!(body.len(), 64 * 1024);
    }

    let (_, headers, _) = app
        .send(
            Request::get("/api/files?path=many")
                .header(header::ACCEPT_ENCODING, "gzip")
                .body(Body::empty())
                .expect("请求"),
        )
        .await;
    assert_eq!(headers[header::CONTENT_ENCODING], "gzip");
}

#[tokio::test]
async fn download_headers_are_valid_for_awkward_names() {
    let app = TestApp::new();
    app.write("dir/报告 \"v2\".txt", b"hello");
    let (status, headers, _) = app
        .get("/api/download/dir/%E6%8A%A5%E5%91%8A%20%22v2%22.txt?download=1")
        .await;
    assert_eq!(status, StatusCode::OK);
    let disposition = headers[header::CONTENT_DISPOSITION]
        .to_str()
        .expect("ASCII");
    assert!(
        disposition.starts_with("attachment; filename=\""),
        "{disposition}"
    );
    assert!(
        disposition.ends_with("filename*=UTF-8''%E6%8A%A5%E5%91%8A%20%22v2%22.txt"),
        "{disposition}"
    );
    let last_modified = headers[header::LAST_MODIFIED].to_str().expect("ASCII");
    assert!(
        httpdate::parse_http_date(last_modified).is_ok(),
        "{last_modified}"
    );
    assert_eq!(headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
}

#[cfg(unix)]
#[tokio::test]
async fn control_characters_in_file_names_do_not_break_the_response() {
    let app = TestApp::new();
    app.write("line\nbreak.txt", b"hi");
    let (status, headers, body) = app.get("/api/download/line%0Abreak.txt").await;
    assert_eq!(status, StatusCode::OK);
    assert!(headers.contains_key(header::CONTENT_DISPOSITION));
    assert_eq!(&body[..], b"hi");
}

/// 文件在两次请求之间变了，续传必须回完整的 200，不能把新旧内容拼在一起
#[tokio::test]
async fn range_is_ignored_when_if_range_no_longer_matches() {
    let app = TestApp::new();
    app.write("f.txt", b"0123456789");
    let (_, headers, _) = app.get("/api/download/f.txt").await;
    let etag = headers[header::ETAG].to_str().expect("etag").to_string();
    let last_modified = headers[header::LAST_MODIFIED]
        .to_str()
        .expect("date")
        .to_string();

    let ranged = |if_range: &str| {
        Request::get("/api/download/f.txt")
            .header(header::RANGE, "bytes=5-")
            .header(header::IF_RANGE, if_range)
            .body(Body::empty())
            .expect("请求")
    };

    let (status, _, body) = app.send(ranged(&etag)).await;
    assert_eq!(
        (status, &body[..]),
        (StatusCode::PARTIAL_CONTENT, &b"56789"[..])
    );
    let (status, _, body) = app.send(ranged(&last_modified)).await;
    assert_eq!(
        (status, &body[..]),
        (StatusCode::PARTIAL_CONTENT, &b"56789"[..])
    );

    let (status, _, body) = app.send(ranged("\"stale-etag\"")).await;
    assert_eq!((status, &body[..]), (StatusCode::OK, &b"0123456789"[..]));
    let (status, _, _) = app.send(ranged("Thu, 01 Jan 2004 00:00:00 GMT")).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn inline_active_content_is_sandboxed() {
    let app = TestApp::new();
    app.write("page.html", b"<script>alert(1)</script>");
    app.write("logo.svg", b"<svg xmlns='http://www.w3.org/2000/svg'/>");
    app.write("doc.pdf", b"%PDF-1.4");

    for uri in ["/api/download/page.html", "/api/download/logo.svg"] {
        let (_, headers, _) = app.get(uri).await;
        assert_eq!(headers[header::CONTENT_SECURITY_POLICY], "sandbox", "{uri}");
    }
    // PDF 要能在浏览器里内嵌显示，不能加 sandbox
    let (_, headers, _) = app.get("/api/download/doc.pdf").await;
    assert!(headers.get(header::CONTENT_SECURITY_POLICY).is_none());
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

