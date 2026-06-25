use std::path::Path;

use anyhow::{Context, Result, bail};
use axum::body::Bytes;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

pub enum GitService {
    UploadPack,
    ReceivePack,
}

impl GitService {
    pub fn from_query(service: &str) -> Option<Self> {
        match service {
            "git-upload-pack" => Some(Self::UploadPack),
            "git-receive-pack" => Some(Self::ReceivePack),
            _ => None,
        }
    }

    fn binary_name(&self) -> &'static str {
        match self {
            Self::UploadPack => "git-upload-pack",
            Self::ReceivePack => "git-receive-pack",
        }
    }

    fn advertise_content_type(&self) -> &'static str {
        match self {
            Self::UploadPack => "application/x-git-upload-pack-advertisement",
            Self::ReceivePack => "application/x-git-receive-pack-advertisement",
        }
    }

    fn rpc_content_type(&self) -> &'static str {
        match self {
            Self::UploadPack => "application/x-git-upload-pack-result",
            Self::ReceivePack => "application/x-git-receive-pack-result",
        }
    }
}

pub async fn advertise_refs(repo: &Path, service: GitService) -> Result<Response> {
    let repo_str = repo
        .canonicalize()
        .with_context(|| format!("repo not found: {}", repo.display()))?
        .to_string_lossy()
        .into_owned();

    let binary = service.binary_name();
    let output = Command::new(binary)
        .arg("--stateless-rpc")
        .arg("--advertise-refs")
        .arg(&repo_str)
        .output()
        .await
        .with_context(|| format!("failed to spawn {binary}"))?;

    if !output.status.success() {
        bail!(
            "{} failed: {}",
            binary,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let service_name = match service {
        GitService::UploadPack => "git-upload-pack",
        GitService::ReceivePack => "git-receive-pack",
    };
    let prefix = format!("# service={service_name}\n");
    let mut body = prefix.into_bytes();
    body.push(b'\x00');
    body.extend_from_slice(&output.stdout);

    Ok(git_response(body, service.advertise_content_type()))
}

pub async fn rpc(repo: &Path, service: GitService, body: Bytes) -> Result<Response> {
    let repo_str = repo
        .canonicalize()
        .with_context(|| format!("repo not found: {}", repo.display()))?
        .to_string_lossy()
        .into_owned();

    let binary = service.binary_name();
    let mut child = Command::new(binary)
        .arg("--stateless-rpc")
        .arg(&repo_str)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to spawn {binary}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(&body).await?;
    }

    let output = child.wait_with_output().await?;
    if !output.status.success() {
        bail!(
            "{} failed: {}",
            binary,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(git_response(output.stdout, service.rpc_content_type()))
}

fn git_response(body: Vec<u8>, content_type: &str) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(
        "Content-Type",
        HeaderValue::from_str(content_type)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    headers.insert("Cache-Control", HeaderValue::from_static("no-cache"));
    (StatusCode::OK, headers, body).into_response()
}
