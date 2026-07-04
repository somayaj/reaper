//! Localhost TLS for HTTP/2 (self-signed cert in the data directory).

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use anyhow::Context;
use rcgen::{CertificateParams, DnType, KeyPair, SanType};
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};

static RUSTLS_PROVIDER: OnceLock<()> = OnceLock::new();

fn ensure_rustls_provider() {
    RUSTLS_PROVIDER.get_or_init(|| {
        let _ = tokio_rustls::rustls::crypto::ring::default_provider().install_default();
    });
}

pub struct LocalTls {
    pub acceptor: TlsAcceptor,
    pub cert_pem_path: PathBuf,
}

pub fn ensure_local_tls(data_dir: &Path, host: &str) -> anyhow::Result<LocalTls> {
    ensure_rustls_provider();
    let cert_path = data_dir.join("reaper-local.crt.pem");
    let key_path = data_dir.join("reaper-local.key.pem");

    if !cert_path.is_file() || !key_path.is_file() {
        generate_self_signed(&cert_path, &key_path, host)?;
    }

    let acceptor = TlsAcceptor::from(rustls_server_config(&cert_path, &key_path)?);
    Ok(LocalTls {
        acceptor,
        cert_pem_path: cert_path,
    })
}

fn generate_self_signed(cert_path: &Path, key_path: &Path, host: &str) -> anyhow::Result<()> {
    let mut params = CertificateParams::new(vec!["localhost".into()])
        .context("certificate params")?;
    params
        .distinguished_name
        .push(DnType::CommonName, "Reaper Local");
    params
        .subject_alt_names
        .push(SanType::IpAddress("127.0.0.1".parse().expect("127.0.0.1")));
    if host != "localhost" {
        if let Ok(ip) = host.parse::<std::net::IpAddr>() {
            params.subject_alt_names.push(SanType::IpAddress(ip));
        } else if let Ok(dns) = host.to_string().try_into() {
            params.subject_alt_names.push(SanType::DnsName(dns));
        }
    }

    let key_pair = KeyPair::generate().context("generate TLS key")?;
    let cert = params
        .self_signed(&key_pair)
        .context("sign localhost certificate")?;

    std::fs::write(cert_path, cert.pem()).with_context(|| cert_path.display().to_string())?;
    std::fs::write(key_path, key_pair.serialize_pem())
        .with_context(|| key_path.display().to_string())?;
    tracing::info!(
        "Generated local TLS certificate at {}",
        cert_path.display()
    );
    Ok(())
}

fn rustls_server_config(cert_path: &Path, key_path: &Path) -> anyhow::Result<Arc<ServerConfig>> {
    let key = PrivateKeyDer::from_pem_file(key_path)
        .with_context(|| format!("read TLS key {}", key_path.display()))?;
    let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_file_iter(cert_path)
        .with_context(|| format!("read TLS cert {}", cert_path.display()))?
        .map(|c| c.map(CertificateDer::from))
        .collect::<Result<_, _>>()?;

    let mut config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .context("rustls server config")?;

    // HTTP/2 via ALPN; fall back to HTTP/1.1 for git/curl without h2.
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

    Ok(Arc::new(config))
}
