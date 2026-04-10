use std::net::SocketAddr;
use std::sync::Arc;

use crate::config::AppConfig;
use crate::routes;
use crate::state::{AppState, AppStateInner};
use crate::upload;
use crate::util::ip;

/// 构建并启动服务器
pub async fn run(config: AppConfig) -> anyhow::Result<()> {
    let addr = SocketAddr::from((config.bind, config.port));

    let state: AppState = Arc::new(AppStateInner::new(config.clone())?);

    // 恢复未完成的上传会话
    let recovered = state.upload_manager.boot_recover().await?;
    if recovered > 0 {
        tracing::info!(count = recovered, "recovered upload sessions");
    }

    // 启动后台清理任务
    upload::janitor::spawn(state.clone());

    let app = routes::build_router(state);

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
            let (stream, _peer) = listener.accept().await?;
            let acceptor = tls_acceptor.clone();
            let app = app.clone();

            tokio::spawn(async move {
                match acceptor.accept(stream).await {
                    Ok(tls_stream) => {
                        let io = hyper_util::rt::TokioIo::new(tls_stream);
                        let service = hyper_util::service::TowerToHyperService::new(
                            app.into_service(),
                        );
                        if let Err(e) = hyper_util::server::conn::auto::Builder::new(
                            hyper_util::rt::TokioExecutor::new(),
                        )
                        .serve_connection(io, service)
                        .await
                        {
                            tracing::debug!(error = %e, "connection error");
                        }
                    }
                    Err(e) => {
                        tracing::debug!(error = %e, "TLS handshake failed");
                    }
                }
            });
        }
    }

    // 非 TLS 启动
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "listening");

    axum::serve(listener, app).await?;

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
