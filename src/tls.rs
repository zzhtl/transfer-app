use std::path::Path;
use std::sync::Arc;

/// 从 PEM 文件加载 rustls ServerConfig
pub fn load_rustls_config(
    cert_path: &Path,
    key_path: &Path,
) -> anyhow::Result<Arc<rustls::ServerConfig>> {
    use std::fs::File;
    use std::io::BufReader;

    let cert_file = File::open(cert_path)?;
    let key_file = File::open(key_path)?;

    let certs: Vec<_> = rustls_pemfile::certs(&mut BufReader::new(cert_file))
        .collect::<Result<_, _>>()?;

    let key = rustls_pemfile::private_key(&mut BufReader::new(key_file))?
        .ok_or_else(|| anyhow::anyhow!("no private key found in {}", key_path.display()))?;

    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)?;

    Ok(Arc::new(config))
}
