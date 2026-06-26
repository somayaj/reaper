use anyhow::{Context, Result, bail};
use url::Url;

use crate::settings::SettingsStore;

pub fn derive_repo_name_from_url(raw: &str) -> Result<String> {
    let clean = normalize_remote_url(raw)?;
    let url = parse_git_url(&clean)?;
    let mut path = url.path().trim_start_matches('/').to_string();
    if let Some(stripped) = path.strip_suffix(".git") {
        path = stripped.to_string();
    }
    if path.is_empty() || !crate::config::Config::is_valid_repo_name(&path) {
        bail!("could not derive repository name from URL; enter a name manually");
    }
    Ok(path)
}

pub fn host_from_url(raw: &str) -> Result<String> {
    let url = parse_git_url(raw)?;
    url.host_str()
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("URL has no host"))
}

pub fn normalize_remote_url(raw: &str) -> Result<String> {
    let mut url = parse_git_url(raw)?;

    if url.scheme() == "ssh" || url.scheme() == "git" {
        bail!("SSH remotes are not supported for PAT auth; use an HTTPS URL");
    }

    let _ = url.set_username("");
    let _ = url.set_password(None);
    url.set_fragment(None);
    url.set_query(None);

    Ok(url.to_string())
}

pub fn authenticate_url(clean_url: &str, token: &str) -> Result<String> {
    let mut url = parse_git_url(clean_url)?;
    let host = url.host_str().unwrap_or("").to_lowercase();
    let (username, password) = credential_pair(&host, token);

    if !username.is_empty() {
        url.set_username(&username)
            .map_err(|_| anyhow::anyhow!("failed to set username for remote URL"))?;
    }
    url.set_password(Some(&password))
        .map_err(|_| anyhow::anyhow!("failed to set token for remote URL"))?;

    Ok(url.to_string())
}

pub fn authenticated_url(clean_url: &str, settings: &SettingsStore) -> Result<String> {
    let host = host_from_url(clean_url)?;
    let token = settings.token_for_host(&host).ok_or_else(|| {
        anyhow::anyhow!(
            "no PAT configured for {host}; add a token in Settings or set REAPER_PAT"
        )
    })?;
    authenticate_url(clean_url, &token)
}

fn credential_pair(host: &str, token: &str) -> (String, String) {
    if host == "github.com" || host.ends_with(".github.com") {
        ("x-access-token".into(), token.into())
    } else if host.contains("gitlab") {
        ("oauth2".into(), token.into())
    } else if host == "bitbucket.org" || host.ends_with(".bitbucket.org") {
        ("x-token-auth".into(), token.into())
    } else if host.contains("dev.azure.com") || host.contains("visualstudio.com") {
        (String::new(), token.into())
    } else {
        let username =
            std::env::var("REAPER_GIT_USERNAME").unwrap_or_else(|_| "git".to_string());
        (username, token.into())
    }
}

fn parse_git_url(raw: &str) -> Result<Url> {
    if raw.starts_with("git@") {
        bail!("SSH remotes are not supported for PAT auth; use an HTTPS URL");
    }

    let candidate = if raw.contains("://") {
        raw.to_string()
    } else {
        format!("https://{raw}")
    };

    Url::parse(&candidate).with_context(|| format!("invalid remote URL: {raw}"))
}
