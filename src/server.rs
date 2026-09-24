use std::net::SocketAddr;
use std::sync::Arc;
#[cfg(feature = "tls")]
use std::time::Duration;

use axum::serve::ListenerExt as _;

use crate::config::AppConfig;
use crate::routes;
use crate::state::{AppState, AppStateInner};
use crate::upload;
use crate::util::ip;

#[cfg(feature = "tls")]
const TLS_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// 构建并启动服务器
pub async fn run(config: AppConfig) -> anyhow::Result<()> {
    let addr = SocketAddr::from((config.bind, config.port));

    let state: AppState = Arc::new(AppStateInner::new(config.clone())?);

    // 恢复未完成的上传会话
    let recovered = state.upload_manager.boot_recover().await?;
    if recovered > 0 {
        tracing::info!(count = recovered, "recovered upload sessions");
    }

    // 恢复未过期的分享
    let recovered_shares = state.share_manager.boot_recover().await?;
    if recovered_shares > 0 {
        tracing::info!(count = recovered_shares, "recovered shares");
    }

    // 启动后台清理任务
    upload::janitor::spawn(state.clone());

    let app = routes::build_service(state);

    // 打印启动信息
    print_banner(&config, addr);

    // TLS 启动
    #[cfg(feature = "tls")]
    if let (Some(cert), Some(key)) = (&config.tls_cert, &config.tls_key) {
        let tls_config = crate::tls::load_rustls_config(cert, key)?;
        tracing::info!("TLS enabled");

        let listener = tokio::net::TcpListener::bind(addr).await?;
        let tls_acceptor = tokio_rustls::TlsAcceptor::from(tls_config);

        loop {
            // accept 失败（比如 EMFILE 句柄耗尽）通常是暂时的。之前用 `?` 直接从 run()
            // 返回，整个服务就退出了；这里和 axum::serve 一样记日志、稍等后继续。
            let (stream, _peer) = match listener.accept().await {
                Ok(conn) => conn,
                Err(e) => {
                    tracing::warn!(error = %e, "accept failed");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }
            };
            if let Err(e) = stream.set_nodelay(true) {
                tracing::trace!(error = %e, "set_nodelay failed");
            }
            let acceptor = tls_acceptor.clone();
            let app = app.clone();

            tokio::spawn(async move {
                // 握手要有时限：只建 TCP 不握手的连接不能一直挂着
                let tls_stream = match tokio::time::timeout(
                    TLS_HANDSHAKE_TIMEOUT,
                    acceptor.accept(stream),
                )
                .await
                {
                    Ok(Ok(tls_stream)) => tls_stream,
                    Ok(Err(e)) => {
                        tracing::debug!(error = %e, "TLS handshake failed");
                        return;
                    }
                    Err(_) => {
                        tracing::debug!("TLS handshake timed out");
                        return;
                    }
                };
                let io = hyper_util::rt::TokioIo::new(tls_stream);
                // NormalizePath<Router> 自身就是 Service<Request<_>>，
                // 不需要再走 Router::into_service()
                let service = hyper_util::service::TowerToHyperService::new(app);
                if let Err(e) = hyper_util::server::conn::auto::Builder::new(
                    hyper_util::rt::TokioExecutor::new(),
                )
                .serve_connection(io, service)
                .await
                {
                    tracing::debug!(error = %e, "connection error");
                }
            });
        }
    }

    // 非 TLS 启动
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "listening");

    // 路径归一化包在路由外面，所以这里要把它转成 MakeService 再交给 axum::serve
    axum::serve(
        listener.tap_io(|tcp| {
            if let Err(e) = tcp.set_nodelay(true) {
                tracing::trace!(error = %e, "set_nodelay failed");
            }
        }),
        axum::ServiceExt::<axum::extract::Request>::into_make_service(app),
    )
    .await?;

    Ok(())
}

fn print_banner(config: &AppConfig, addr: SocketAddr) {
    let protocol = if config.tls_cert.is_some() {
        "https"
    } else {
        "http"
    };

    let local_ip = ip::get_local_ip().unwrap_or_else(|| "unknown".to_string());

    println!();
    println!("  ╔══════════════════════════════════════════════════╗");
    println!(
        "  ║          FileTransfer Server v{}               ║",
        env!("CARGO_PKG_VERSION")
    );
    println!("  ╠══════════════════════════════════════════════════╣");
    println!(
        "  ║  Local:   {}://127.0.0.1:{:<21} ║",
        protocol, addr.port()
    );
    println!(
        "  ║  Network: {}://{}:{:<15} ║",
        protocol, local_ip, addr.port()
    );
    println!("  ╚══════════════════════════════════════════════════╝");
    println!();
    println!("  共享目录: {}", config.path.display());
    println!("  按 Ctrl+C 停止服务器");
    println!();
}
