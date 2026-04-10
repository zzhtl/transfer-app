use std::net::IpAddr;
use std::path::PathBuf;

use clap::Parser;
use serde::Deserialize;

#[derive(Debug, Clone, Parser, Deserialize)]
#[command(name = "transfer-app", version, about = "High-performance LAN file transfer server")]
pub struct AppConfig {
    /// 共享根目录
    #[arg(short = 'p', long, env = "TRANSFER_PATH")]
    pub path: PathBuf,

    /// 监听地址
    #[arg(short = 'b', long, default_value = "0.0.0.0", env = "TRANSFER_BIND")]
    pub bind: IpAddr,

    /// 端口
    #[arg(short = 'P', long, default_value_t = 8080, env = "TRANSFER_PORT")]
    pub port: u16,

    /// TLS 证书 (PEM)
    #[arg(long, env = "TRANSFER_TLS_CERT")]
    pub tls_cert: Option<PathBuf>,

    /// TLS 私钥 (PEM)
    #[arg(long, env = "TRANSFER_TLS_KEY")]
    pub tls_key: Option<PathBuf>,

    /// 单文件最大上传 (字节, 0 = 无限制)
    #[arg(long, default_value_t = 0, env = "TRANSFER_MAX_UPLOAD")]
    pub max_upload_size: u64,

    /// 全局并发传输上限
    #[arg(long, default_value_t = 32)]
    pub max_concurrent_transfers: usize,

    /// 上传会话过期 (秒, 默认 7 天)
    #[arg(long, default_value_t = 7 * 24 * 3600)]
    pub upload_expiration_secs: u64,

    /// 配置文件 (TOML)
    #[arg(short = 'c', long, env = "TRANSFER_CONFIG")]
    pub config: Option<PathBuf>,

    /// 日志级别
    #[arg(long, default_value = "info,transfer_app=debug", env = "RUST_LOG")]
    pub log_filter: String,
}

impl AppConfig {
    pub fn load() -> anyhow::Result<Self> {
        let mut cli = Self::parse();

        // 如果指定了配置文件，合并 TOML 配置
        if let Some(ref cfg_path) = cli.config {
            if cfg_path.exists() {
                let content = std::fs::read_to_string(cfg_path)?;
                let file_cfg: toml::Value = toml::from_str(&content)?;

                // TOML 配置作为默认值，CLI 参数优先
                if let Some(path) = file_cfg.get("path").and_then(|v| v.as_str()) {
                    if cli.path.as_os_str().is_empty() {
                        cli.path = PathBuf::from(path);
                    }
                }
            }
        }

        // 规范化路径
        cli.path = dunce::canonicalize(&cli.path)?;

        if !cli.path.is_dir() {
            anyhow::bail!("path '{}' is not a directory", cli.path.display());
        }

        Ok(cli)
    }
}
