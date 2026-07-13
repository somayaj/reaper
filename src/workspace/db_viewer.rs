//! SQL database connection, schema browser, and query runner.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use rusqlite::types::ValueRef;
use serde::{Deserialize, Serialize};

use super::exec::{run_command_with_env, run_shell_command};
use crate::git::GitOutput;
use crate::repos::metadata::{DbSshTunnelSettings, DbSslSettings};
use super::db_ssh_tunnel;
use super::safe_join;

const COMPOSE_FILE_NAMES: &[&str] = &[
    "docker-compose.yml",
    "docker-compose.yaml",
    "compose.yml",
    "compose.yaml",
];

const COMPOSE_FALLBACK_DIRS: &[&str] = &["docker", "deploy", "infra", "compose", ".docker"];

pub fn effective_database_url(ws: &Path, stored: Option<&str>) -> Option<String> {
    stored
        .filter(|s| !s.trim().is_empty())
        .map(str::trim)
        .map(str::to_string)
        .or_else(|| discover_database_url(ws))
}

/// Infer a DB URL from `DATABASE_URL` in `.env` or Docker Compose (postgres / mysql).
pub fn discover_database_url(ws: &Path) -> Option<String> {
    let mut env = load_dotenv_file(&ws.join(".env"));
    for (dir, _rel) in find_compose_files(ws) {
        env.extend(load_dotenv_file(&dir.join(".env")));
        if let Some(url) = env.get("DATABASE_URL").filter(|v| is_supported_url(v)).cloned() {
            return Some(normalize_discovered_url(url, &env));
        }
        let compose_path = COMPOSE_FILE_NAMES
            .iter()
            .map(|name| dir.join(name))
            .find(|path| path.is_file());
        if let Some(path) = compose_path {
            if let Ok(text) = std::fs::read_to_string(&path) {
                let rel_dir = compose_rel_path(ws, &dir).unwrap_or_default();
                if let Some((url, _exec)) = postgres_source_from_compose(&text, &env, &rel_dir) {
                    return Some(normalize_postgres_url(url, &env));
                }
                if let Some((url, _exec)) = mysql_source_from_compose(&text, &env, &rel_dir) {
                    return Some(normalize_mysql_url(url, &env));
                }
            }
        }
    }
    env.get("DATABASE_URL")
        .filter(|v| is_supported_url(v))
        .cloned()
        .map(|url| normalize_discovered_url(url, &env))
}

fn is_supported_url(s: &str) -> bool {
    is_postgres_url(s) || is_mysql_url(s)
}

fn normalize_discovered_url(url: String, env: &HashMap<String, String>) -> String {
    if is_mysql_url(&url) {
        normalize_mysql_url(url, env)
    } else {
        normalize_postgres_url(url, env)
    }
}

fn lookup_compose_postgres(ws: &Path) -> Option<(String, ComposePostgresExec)> {
    let mut env = load_dotenv_file(&ws.join(".env"));
    for (dir, _) in find_compose_files(ws) {
        env.extend(load_dotenv_file(&dir.join(".env")));
        let compose_path = COMPOSE_FILE_NAMES
            .iter()
            .map(|name| dir.join(name))
            .find(|path| path.is_file())?;
        let text = std::fs::read_to_string(&compose_path).ok()?;
        let rel_dir = compose_rel_path(ws, &dir).unwrap_or_default();
        if let Some(found) = postgres_source_from_compose(&text, &env, &rel_dir) {
            let (url, exec) = found;
            return Some((normalize_postgres_url(url, &env), exec));
        }
    }
    None
}

fn normalize_postgres_url(url: String, env: &HashMap<String, String>) -> String {
    let Ok(mut parsed) = url::Url::parse(&url) else {
        return url;
    };
    if !parsed.path().trim_matches('/').is_empty() {
        return url;
    }
    let database = env
        .get("POSTGRES_DB")
        .filter(|s| !s.trim().is_empty())
        .or_else(|| env.get("POSTGRES_USER").filter(|s| !s.trim().is_empty()))
        .cloned()
        .unwrap_or_else(|| "postgres".into());
    parsed.set_path(&format!("/{database}"));
    parsed.to_string()
}

fn compose_rel_path(ws: &Path, dir: &Path) -> Option<String> {
    super::gradle::rel_path_for(ws, dir).ok()
}

fn find_compose_files(ws: &Path) -> Vec<(PathBuf, String)> {
    let mut found = Vec::new();
    let mut seen = std::collections::HashSet::new();

    let mut push_dir = |dir: &Path| {
        if !dir.is_dir() {
            return;
        }
        for name in COMPOSE_FILE_NAMES {
            if dir.join(name).is_file() {
                let key = dir.join(name);
                if seen.insert(key) {
                    let rel = compose_rel_path(ws, dir).unwrap_or_default();
                    found.push((dir.to_path_buf(), rel));
                }
                return;
            }
        }
    };

    push_dir(ws);
    for sub in COMPOSE_FALLBACK_DIRS {
        push_dir(&ws.join(sub));
    }
    found
}

fn load_dotenv_file(path: &Path) -> HashMap<String, String> {
    let mut env = HashMap::new();
    let Ok(text) = std::fs::read_to_string(path) else {
        return env;
    };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        let value = strip_dotenv_quotes(value.trim());
        env.insert(key.to_string(), value);
    }
    env
}

fn strip_dotenv_quotes(value: &str) -> String {
    if (value.starts_with('"') && value.ends_with('"'))
        || (value.starts_with('\'') && value.ends_with('\''))
    {
        value[1..value.len().saturating_sub(1)].to_string()
    } else {
        value.to_string()
    }
}

fn resolve_env_template(raw: &str, env: &HashMap<String, String>) -> String {
    let mut out = String::new();
    let mut chars = raw.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '$' && chars.peek() == Some(&'{') {
            chars.next();
            let mut token = String::new();
            for inner in chars.by_ref() {
                if inner == '}' {
                    break;
                }
                token.push(inner);
            }
            if let Some((key, default)) = token.split_once(":-") {
                out.push_str(env.get(key).map(String::as_str).unwrap_or(default));
            } else if !token.is_empty() {
                out.push_str(env.get(&token).map(String::as_str).unwrap_or(""));
            }
            continue;
        }
        out.push(ch);
    }
    out
}

fn postgres_source_from_compose(
    text: &str,
    env: &HashMap<String, String>,
    compose_dir: &str,
) -> Option<(String, ComposePostgresExec)> {
    let value = serde_yaml::from_str::<serde_yaml::Value>(text).ok()?;
    let services = value.get("services")?.as_mapping()?;
    for (name, service) in services {
        let service_name = name.as_str().unwrap_or("");
        if !is_postgres_compose_service(service_name, service) {
            continue;
        }
        let mut merged = env.clone();
        if let Some(map) = service.get("environment").and_then(|e| e.as_mapping()) {
            for (k, v) in map {
                let Some(key) = k.as_str() else { continue };
                let val = match v {
                    serde_yaml::Value::String(s) => resolve_env_template(s, &merged),
                    serde_yaml::Value::Number(n) => n.to_string(),
                    serde_yaml::Value::Bool(b) => b.to_string(),
                    _ => continue,
                };
                merged.insert(key.to_string(), val);
            }
        } else if let Some(list) = service.get("environment").and_then(|e| e.as_sequence()) {
            for item in list {
                let Some(entry) = item.as_str() else { continue };
                if let Some((key, val)) = entry.split_once('=') {
                    merged.insert(
                        key.trim().to_string(),
                        resolve_env_template(val.trim(), &merged),
                    );
                }
            }
        }

        let user = merged
            .get("POSTGRES_USER")
            .filter(|s| !s.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| "postgres".into());
        let password = merged
            .get("POSTGRES_PASSWORD")
            .filter(|s| !s.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| "postgres".into());
        let database = merged
            .get("POSTGRES_DB")
            .filter(|s| !s.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| user.clone());
        let host = merged
            .get("POSTGRES_HOST")
            .filter(|s| !s.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| "localhost".into());
        let port = merged
            .get("POSTGRES_PORT")
            .filter(|s| !s.trim().is_empty())
            .cloned()
            .or_else(|| host_port_from_compose_ports(service, &merged))
            .unwrap_or_else(|| "5432".into());

        let url = build_postgres_url(&user, &password, &host, &port, &database);
        let exec = ComposePostgresExec {
            compose_dir: compose_dir.to_string(),
            service: service_name.to_string(),
            user,
            database,
        };
        return Some((url, exec));
    }
    None
}

fn is_postgres_compose_service(name: &str, service: &serde_yaml::Value) -> bool {
    let lower = name.to_ascii_lowercase();
    if matches!(lower.as_str(), "postgres" | "postgresql") {
        return true;
    }
    if matches!(lower.as_str(), "db" | "database") {
        return service
            .get("image")
            .and_then(|i| i.as_str())
            .is_some_and(|image| {
                let img = image.to_ascii_lowercase();
                img.contains("postgres") && !img.contains("mysql") && !img.contains("mariadb")
            });
    }
    service
        .get("image")
        .and_then(|i| i.as_str())
        .is_some_and(|image| image.to_ascii_lowercase().contains("postgres"))
}

fn host_port_from_compose_ports(
    service: &serde_yaml::Value,
    env: &HashMap<String, String>,
) -> Option<String> {
    let ports = service.get("ports")?.as_sequence()?;
    let first = ports.first()?;
    let raw = match first {
        serde_yaml::Value::String(s) => resolve_env_template(s, env),
        serde_yaml::Value::Number(n) => n.to_string(),
        serde_yaml::Value::Mapping(map) => map
            .get(serde_yaml::Value::String("published".into()))
            .or_else(|| map.get(serde_yaml::Value::String("target".into())))
            .and_then(|v| v.as_u64())
            .map(|n| n.to_string())?,
        _ => return None,
    };
    raw.split(':').next().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

fn build_postgres_url(user: &str, password: &str, host: &str, port: &str, database: &str) -> String {
    fn enc(value: &str) -> String {
        url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
    }
    format!(
        "postgresql://{}:{}@{}:{}/{}",
        enc(user),
        enc(password),
        host,
        port,
        enc(database.trim_start_matches('/'))
    )
}

fn lookup_compose_mysql(ws: &Path) -> Option<(String, ComposeMysqlExec)> {
    let mut env = load_dotenv_file(&ws.join(".env"));
    for (dir, _) in find_compose_files(ws) {
        env.extend(load_dotenv_file(&dir.join(".env")));
        let compose_path = COMPOSE_FILE_NAMES
            .iter()
            .map(|name| dir.join(name))
            .find(|path| path.is_file())?;
        let text = std::fs::read_to_string(&compose_path).ok()?;
        let rel_dir = compose_rel_path(ws, &dir).unwrap_or_default();
        if let Some(found) = mysql_source_from_compose(&text, &env, &rel_dir) {
            let (url, exec) = found;
            return Some((normalize_mysql_url(url, &env), exec));
        }
    }
    None
}

fn normalize_mysql_url(url: String, env: &HashMap<String, String>) -> String {
    let Ok(mut parsed) = url::Url::parse(&url) else {
        return url;
    };
    if !parsed.path().trim_matches('/').is_empty() {
        return url;
    }
    let database = env
        .get("MYSQL_DATABASE")
        .filter(|s| !s.trim().is_empty())
        .or_else(|| env.get("MYSQL_DB").filter(|s| !s.trim().is_empty()))
        .or_else(|| env.get("MARIADB_DATABASE").filter(|s| !s.trim().is_empty()))
        .cloned()
        .unwrap_or_else(|| "mysql".into());
    parsed.set_path(&format!("/{database}"));
    parsed.to_string()
}

fn mysql_source_from_compose(
    text: &str,
    env: &HashMap<String, String>,
    compose_dir: &str,
) -> Option<(String, ComposeMysqlExec)> {
    let doc: serde_yaml::Value = serde_yaml::from_str(text).ok()?;
    let services = doc.get("services")?.as_mapping()?;
    for (name, service) in services {
        let service_name = name.as_str()?;
        if !is_mysql_compose_service(service_name, service) {
            continue;
        }
        let service_env = compose_service_env(service, env);
        let mut merged = env.clone();
        merged.extend(service_env);
        let user = merged
            .get("MYSQL_USER")
            .filter(|s| !s.trim().is_empty())
            .or_else(|| merged.get("MARIADB_USER").filter(|s| !s.trim().is_empty()))
            .cloned()
            .unwrap_or_else(|| "root".into());
        let password = merged
            .get("MYSQL_PASSWORD")
            .filter(|s| !s.trim().is_empty())
            .or_else(|| merged.get("MYSQL_ROOT_PASSWORD").filter(|s| !s.trim().is_empty()))
            .or_else(|| merged.get("MARIADB_PASSWORD").filter(|s| !s.trim().is_empty()))
            .or_else(|| merged.get("MARIADB_ROOT_PASSWORD").filter(|s| !s.trim().is_empty()))
            .cloned()
            .unwrap_or_default();
        let database = merged
            .get("MYSQL_DATABASE")
            .filter(|s| !s.trim().is_empty())
            .or_else(|| merged.get("MARIADB_DATABASE").filter(|s| !s.trim().is_empty()))
            .cloned()
            .unwrap_or_else(|| "mysql".into());
        let host = merged
            .get("MYSQL_HOST")
            .filter(|s| !s.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| "localhost".into());
        let port = merged
            .get("MYSQL_PORT")
            .filter(|s| !s.trim().is_empty())
            .or_else(|| merged.get("MARIADB_PORT").filter(|s| !s.trim().is_empty()))
            .cloned()
            .or_else(|| host_port_from_compose_ports(service, &merged))
            .unwrap_or_else(|| "3306".into());
        let url = build_mysql_url(&user, &password, &host, &port, &database);
        let exec = ComposeMysqlExec {
            compose_dir: compose_dir.to_string(),
            service: service_name.to_string(),
            user,
            database,
        };
        return Some((url, exec));
    }
    None
}

fn is_mysql_compose_service(name: &str, service: &serde_yaml::Value) -> bool {
    let lower = name.to_ascii_lowercase();
    if matches!(lower.as_str(), "mysql" | "mariadb" | "mysqld") {
        return true;
    }
    // Generic "db"/"database" only when image is MySQL/MariaDB (not Postgres).
    if matches!(lower.as_str(), "db" | "database") {
        return service
            .get("image")
            .and_then(|v| v.as_str())
            .is_some_and(|image| {
                let img = image.to_ascii_lowercase();
                (img.contains("mysql") || img.contains("mariadb")) && !img.contains("postgres")
            });
    }
    service
        .get("image")
        .and_then(|v| v.as_str())
        .is_some_and(|image| {
            let img = image.to_ascii_lowercase();
            (img.contains("mysql") || img.contains("mariadb")) && !img.contains("postgres")
        })
}

fn build_mysql_url(user: &str, password: &str, host: &str, port: &str, database: &str) -> String {
    fn enc(value: &str) -> String {
        url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
    }
    format!(
        "mysql://{}:{}@{}:{}/{}",
        enc(user),
        enc(password),
        host,
        port,
        enc(database.trim_start_matches('/'))
    )
}

fn compose_service_env(
    service: &serde_yaml::Value,
    file_env: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Some(env_node) = service.get("environment") else {
        return out;
    };
    if let Some(map) = env_node.as_mapping() {
        for (k, v) in map {
            let Some(key) = k.as_str() else { continue };
            let raw = match v {
                serde_yaml::Value::String(s) => s.clone(),
                serde_yaml::Value::Number(n) => n.to_string(),
                serde_yaml::Value::Bool(b) => b.to_string(),
                _ => continue,
            };
            out.insert(key.to_string(), resolve_env_template(&raw, file_env));
        }
    } else if let Some(seq) = env_node.as_sequence() {
        for item in seq {
            let Some(s) = item.as_str() else { continue };
            let Some((key, value)) = s.split_once('=') else {
                continue;
            };
            out.insert(
                key.trim().to_string(),
                resolve_env_template(value.trim(), file_env),
            );
        }
    }
    out
}

#[derive(Debug, Clone, Serialize)]
pub struct DbConnectionView {
    pub database_url: Option<String>,
    /// `sqlite` | `postgres` | `mysql` | `none`
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_path: Option<String>,
    pub display: String,
    pub connected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssl: Option<DbSslSettings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh: Option<DbSshTunnelSettings>,
    /// Local port of the active SSH forward, when a tunnel is up.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_local_port: Option<u16>,
}

#[derive(Debug, Clone)]
struct ComposePostgresExec {
    compose_dir: String,
    service: String,
    user: String,
    database: String,
}

#[derive(Debug, Clone)]
struct ComposeMysqlExec {
    compose_dir: String,
    service: String,
    user: String,
    database: String,
}

#[derive(Debug, Clone)]
enum DbKind {
    Sqlite(PathBuf),
    Postgres {
        url: String,
        compose: Option<ComposePostgresExec>,
    },
    Mysql {
        url: String,
        compose: Option<ComposeMysqlExec>,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct DbColumn {
    pub name: String,
    pub type_name: String,
    pub nullable: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct DbIndex {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub columns: Vec<String>,
    #[serde(default)]
    pub unique: bool,
    #[serde(default)]
    pub primary: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DbTable {
    pub schema: String,
    pub name: String,
    #[serde(default = "default_object_kind")]
    pub kind: String,
    pub columns: Vec<DbColumn>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub indexes: Vec<DbIndex>,
}

fn default_object_kind() -> String {
    "table".into()
}

#[derive(Debug, Clone, Serialize)]
pub struct DbSchemaResponse {
    #[serde(flatten)]
    pub connection: DbConnectionView,
    pub tables: Vec<DbTable>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DbQueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub row_count: usize,
    pub truncated: bool,
    pub elapsed_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DbQueryRequest {
    pub sql: String,
    #[serde(default = "default_query_limit")]
    pub limit: u32,
}

#[derive(Debug, Deserialize)]
pub struct DbConnectionRequest {
    #[serde(default)]
    pub database_url: Option<String>,
    #[serde(default)]
    pub ssl: Option<DbSslSettings>,
    #[serde(default)]
    pub ssh: Option<DbSshTunnelSettings>,
}

fn default_query_limit() -> u32 {
    500
}

pub fn connection_view(
    ws: &Path,
    database_url: Option<&str>,
    ssl: Option<&DbSslSettings>,
    ssh: Option<&DbSshTunnelSettings>,
) -> DbConnectionView {
    let effective = effective_database_url(ws, database_url);
    let ssl_out = ssl.cloned().and_then(|s| s.clone().normalized());
    let ssh_out = ssh.cloned().and_then(|s| s.clone().normalized());
    if let Some(err) = ssl_out.as_ref().and_then(validate_ssl_files) {
        return DbConnectionView {
            database_url: effective,
            kind: "none".into(),
            resolved_path: None,
            display: "Not connected".into(),
            connected: false,
            error: Some(err),
            ssl: ssl_out,
            ssh: ssh_out,
            ssh_local_port: None,
        };
    }
    if ssh_out.as_ref().is_some_and(|s| s.enabled) && !ssh_out.as_ref().is_some_and(|s| s.is_enabled())
    {
        return DbConnectionView {
            database_url: effective,
            kind: "none".into(),
            resolved_path: None,
            display: "Not connected".into(),
            connected: false,
            error: Some("SSH tunnel enabled but bastion host is missing".into()),
            ssl: ssl_out,
            ssh: ssh_out,
            ssh_local_port: None,
        };
    }
    match resolve_db_kind_for_ops(ws, database_url, "", ssh_out.as_ref()) {
        Ok((kind, local_port)) => {
            connection_view_for_kind(effective.as_deref(), &kind, ssl_out, ssh_out, local_port)
        }
        Err(e) => DbConnectionView {
            database_url: effective,
            kind: "none".into(),
            resolved_path: None,
            display: "Not connected".into(),
            connected: false,
            error: Some(e.to_string()),
            ssl: ssl_out,
            ssh: ssh_out,
            ssh_local_port: None,
        },
    }
}

fn connection_view_for_kind(
    database_url: Option<&str>,
    kind: &DbKind,
    ssl: Option<DbSslSettings>,
    ssh: Option<DbSshTunnelSettings>,
    ssh_local_port: Option<u16>,
) -> DbConnectionView {
    let stored_url = database_url
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string);
    match kind {
        DbKind::Sqlite(path) => {
            let display = path.file_name().and_then(|n| n.to_str()).unwrap_or("sqlite").to_string();
            DbConnectionView {
                database_url: stored_url,
                kind: "sqlite".into(),
                resolved_path: Some(path.display().to_string()),
                display,
                connected: true,
                error: None,
                ssl: None,
                ssh: None,
                ssh_local_port: None,
            }
        }
        DbKind::Postgres { url, .. } => {
            let base = stored_url.clone().unwrap_or_else(|| url.clone());
            let mut display = postgres_display(stored_url.as_deref().unwrap_or(url));
            if let Some(port) = ssh_local_port {
                display = format!("{display} via SSH :{port}");
            }
            DbConnectionView {
                database_url: Some(base),
                kind: "postgres".into(),
                resolved_path: None,
                display,
                connected: true,
                error: None,
                ssl,
                ssh,
                ssh_local_port,
            }
        }
        DbKind::Mysql { url, .. } => {
            let base = stored_url.clone().unwrap_or_else(|| url.clone());
            let mut display = mysql_display(stored_url.as_deref().unwrap_or(url));
            if let Some(port) = ssh_local_port {
                display = format!("{display} via SSH :{port}");
            }
            DbConnectionView {
                database_url: Some(base),
                kind: "mysql".into(),
                resolved_path: None,
                display,
                connected: true,
                error: None,
                ssl,
                ssh,
                ssh_local_port,
            }
        }
    }
}

/// Resolve DB kind and optionally open an SSH tunnel (compose exec is skipped when tunneled).
fn resolve_db_kind_for_ops(
    ws: &Path,
    database_url: Option<&str>,
    rel_path: &str,
    ssh: Option<&DbSshTunnelSettings>,
) -> Result<(DbKind, Option<u16>)> {
    let kind = resolve_db_kind(ws, database_url, rel_path)?;
    let Some(ssh) = ssh.filter(|s| s.is_enabled()) else {
        return Ok((kind, None));
    };
    match kind {
        DbKind::Sqlite(path) => Ok((DbKind::Sqlite(path), None)),
        DbKind::Postgres { url, .. } => {
            let endpoint = db_ssh_tunnel::ensure_tunnel(ws, &url, ssh)?;
            let tunneled = db_ssh_tunnel::rewrite_url_through_tunnel(&url, endpoint.local_port)?;
            Ok((
                DbKind::Postgres {
                    url: tunneled,
                    compose: None,
                },
                Some(endpoint.local_port),
            ))
        }
        DbKind::Mysql { url, .. } => {
            let endpoint = db_ssh_tunnel::ensure_tunnel(ws, &url, ssh)?;
            let tunneled = db_ssh_tunnel::rewrite_url_through_tunnel(&url, endpoint.local_port)?;
            Ok((
                DbKind::Mysql {
                    url: tunneled,
                    compose: None,
                },
                Some(endpoint.local_port),
            ))
        }
    }
}

pub fn fetch_schema(
    ws: &Path,
    database_url: Option<&str>,
    ssl: Option<&DbSslSettings>,
    ssh: Option<&DbSshTunnelSettings>,
) -> DbSchemaResponse {
    let effective = effective_database_url(ws, database_url);
    let ssl_out = ssl.cloned().and_then(|s| s.clone().normalized());
    let ssh_out = ssh.cloned().and_then(|s| s.clone().normalized());
    if let Some(err) = ssl_out.as_ref().and_then(validate_ssl_files) {
        return DbSchemaResponse {
            connection: DbConnectionView {
                database_url: effective,
                kind: "none".into(),
                resolved_path: None,
                display: "Not connected".into(),
                connected: false,
                error: Some(err),
                ssl: ssl_out,
                ssh: ssh_out,
                ssh_local_port: None,
            },
            tables: Vec::new(),
        };
    }
    match resolve_db_kind_for_ops(ws, database_url, "", ssh_out.as_ref()) {
        Ok((kind, local_port)) => {
            let connection = connection_view_for_kind(
                effective.as_deref(),
                &kind,
                ssl_out.clone(),
                ssh_out.clone(),
                local_port,
            );
            let tables = match &kind {
                DbKind::Sqlite(path) => sqlite_schema(path),
                DbKind::Postgres { url, compose } => {
                    postgres_schema(ws, url, compose.as_ref(), ssl_out.as_ref())
                }
                DbKind::Mysql { url, compose } => {
                    mysql_schema(ws, url, compose.as_ref(), ssl_out.as_ref())
                }
            };
            match tables {
                Ok(tables) => DbSchemaResponse { connection, tables },
                Err(e) => DbSchemaResponse {
                    connection: DbConnectionView {
                        error: Some(e.to_string()),
                        connected: false,
                        ..connection
                    },
                    tables: Vec::new(),
                },
            }
        }
        Err(e) => DbSchemaResponse {
            connection: DbConnectionView {
                database_url: database_url.filter(|s| !s.trim().is_empty()).map(str::to_string),
                kind: "none".into(),
                resolved_path: None,
                display: "Not connected".into(),
                connected: false,
                error: Some(e.to_string()),
                ssl: ssl_out,
                ssh: ssh_out,
                ssh_local_port: None,
            },
            tables: Vec::new(),
        },
    }
}

pub fn run_query(
    ws: &Path,
    database_url: Option<&str>,
    ssl: Option<&DbSslSettings>,
    ssh: Option<&DbSshTunnelSettings>,
    sql: &str,
    limit: u32,
) -> DbQueryResult {
    let started = Instant::now();
    let trimmed = sql.trim();
    if trimmed.is_empty() {
        return DbQueryResult {
            columns: Vec::new(),
            rows: Vec::new(),
            row_count: 0,
            truncated: false,
            elapsed_ms: started.elapsed().as_millis() as u64,
            error: Some("SQL query is empty".into()),
        };
    }

    if let Some(err) = ssl.and_then(validate_ssl_files) {
        return DbQueryResult {
            columns: Vec::new(),
            rows: Vec::new(),
            row_count: 0,
            truncated: false,
            elapsed_ms: started.elapsed().as_millis() as u64,
            error: Some(err),
        };
    }

    match resolve_db_kind_for_ops(ws, database_url, "", ssh) {
        Ok((kind, _)) => match kind {
            DbKind::Sqlite(path) => sqlite_query(&path, trimmed, limit, started),
            DbKind::Postgres { url, compose } => {
                postgres_query(ws, &url, compose.as_ref(), ssl, trimmed, limit, started)
            }
            DbKind::Mysql { url, compose } => {
                mysql_query(ws, &url, compose.as_ref(), ssl, trimmed, limit, started)
            }
        },
        Err(e) => DbQueryResult {
            columns: Vec::new(),
            rows: Vec::new(),
            row_count: 0,
            truncated: false,
            elapsed_ms: started.elapsed().as_millis() as u64,
            error: Some(e.to_string()),
        },
    }
}

const SQL_RUN_OVERLAY: &str = ".reaper/sql-run/current.sql";

/// Write SQL (editor buffer or disk) to a stable overlay path, then build the run shell command.
pub fn prepare_sql_run_command(
    ws: &Path,
    rel_path: &str,
    content: Option<&str>,
    database_url: Option<&str>,
    ssl: Option<&DbSslSettings>,
    ssh: Option<&DbSshTunnelSettings>,
) -> Result<String> {
    let rel = super::normalize_workspace_source_path(rel_path);
    let text = match content {
        Some(c) => c.to_string(),
        None => super::read_file(ws, &rel).with_context(|| format!("read SQL file `{rel}`"))?,
    };
    if text.trim().is_empty() {
        bail!("SQL file is empty — add statements before running");
    }
    let overlay = ws.join(SQL_RUN_OVERLAY);
    if let Some(parent) = overlay.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&overlay, &text)?;
    sql_run_command(ws, SQL_RUN_OVERLAY, database_url, ssl, ssh)
}

pub fn sql_run_command(
    ws: &Path,
    rel_path: &str,
    database_url: Option<&str>,
    ssl: Option<&DbSslSettings>,
    ssh: Option<&DbSshTunnelSettings>,
) -> Result<String> {
    let (kind, _) = resolve_db_kind_for_ops(ws, database_url, rel_path, ssh)?;
    let rel = rel_path.replace('\\', "/");
    match kind {
        DbKind::Sqlite(path) => {
            let sqlite3 = crate::toolchain::resolve_program("sqlite3")
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|| "sqlite3".into());
            Ok(format!(
                "{} {} < {}",
                shell_quote(&sqlite3),
                shell_quote(&path.to_string_lossy()),
                shell_quote(&rel)
            ))
        }
        DbKind::Postgres { url, compose } => {
            if let Some(exec) = compose {
                return Ok(compose_psql_file_command(&exec, &rel));
            }
            let psql = crate::toolchain::resolve_program("psql")
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|| "psql".into());
            Ok(format!(
                "{}{} {} -v ON_ERROR_STOP=1 -f {}",
                ssl_env_shell_prefix(ssl),
                shell_quote(&psql),
                shell_quote(&url),
                shell_quote(&rel)
            ))
        }
        DbKind::Mysql { url, compose } => {
            if let Some(exec) = compose {
                return Ok(compose_mysql_file_command(&exec, &rel));
            }
            Ok(mysql_cli_file_command(&url, ssl, &rel)?)
        }
    }
}

fn resolve_db_kind(ws: &Path, url: Option<&str>, rel_path: &str) -> Result<DbKind> {
    let compose_pg = lookup_compose_postgres(ws);
    let compose_my = lookup_compose_mysql(ws);
    if let Some(raw) = effective_database_url(ws, url) {
        if is_postgres_url(&raw) {
            let mut env = load_dotenv_file(&ws.join(".env"));
            for (dir, _) in find_compose_files(ws) {
                env.extend(load_dotenv_file(&dir.join(".env")));
            }
            let normalized = normalize_postgres_url(raw, &env);
            return Ok(DbKind::Postgres {
                url: normalized,
                compose: compose_pg.map(|(_, exec)| exec),
            });
        }
        if is_mysql_url(&raw) {
            let mut env = load_dotenv_file(&ws.join(".env"));
            for (dir, _) in find_compose_files(ws) {
                env.extend(load_dotenv_file(&dir.join(".env")));
            }
            let normalized = normalize_mysql_url(raw, &env);
            return Ok(DbKind::Mysql {
                url: normalized,
                compose: compose_my.map(|(_, exec)| exec),
            });
        }
        return Ok(DbKind::Sqlite(resolve_sqlite_path(ws, &raw)?));
    }
    if let Some((compose_url, exec)) = compose_pg {
        return Ok(DbKind::Postgres {
            url: compose_url,
            compose: Some(exec),
        });
    }
    if let Some((compose_url, exec)) = compose_my {
        return Ok(DbKind::Mysql {
            url: compose_url,
            compose: Some(exec),
        });
    }
    if let Some(path) = find_sqlite_near(ws, rel_path)? {
        return Ok(DbKind::Sqlite(path));
    }
    bail!(
        "No database connection — set a PostgreSQL or MySQL URL in the Database panel, add DATABASE_URL to .env, or configure postgres/mysql in docker-compose.yml"
    );
}

fn is_postgres_url(s: &str) -> bool {
    let lower = s.to_lowercase();
    lower.starts_with("postgres://") || lower.starts_with("postgresql://")
}

fn is_mysql_url(s: &str) -> bool {
    let lower = s.to_lowercase();
    lower.starts_with("mysql://")
        || lower.starts_with("mysql2://")
        || lower.starts_with("mariadb://")
}

fn resolve_sqlite_path(ws: &Path, raw: &str) -> Result<PathBuf> {
    let path = Path::new(raw);
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        ws.join(raw)
    };
    if abs.is_file() {
        return Ok(abs);
    }
    bail!("SQLite database not found: {raw}");
}

fn find_sqlite_near(ws: &Path, rel_path: &str) -> Result<Option<PathBuf>> {
    let mut dir = if rel_path.is_empty() {
        ws.to_path_buf()
    } else {
        safe_join(ws, rel_path)?
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| ws.to_path_buf())
    };

    loop {
        if let Some(found) = scan_dir_for_sqlite(&dir)? {
            return Ok(Some(found));
        }
        if dir == ws {
            break;
        }
        let Some(parent) = dir.parent() else {
            break;
        };
        if !parent.starts_with(ws) && parent != ws {
            break;
        }
        dir = parent.to_path_buf();
    }
    Ok(None)
}

fn scan_dir_for_sqlite(dir: &Path) -> Result<Option<PathBuf>> {
    const PRIORITY: &[&str] = &[
        "development.sqlite3",
        "db.sqlite3",
        "database.sqlite",
        "database.sqlite3",
        "app.db",
        "database.db",
    ];
    for name in PRIORITY {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Ok(Some(candidate));
        }
    }

    let mut matches = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return Ok(None),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        if matches!(ext.to_lowercase().as_str(), "sqlite" | "sqlite3" | "db") {
            matches.push(path);
        }
    }
    matches.sort();
    Ok(matches.into_iter().next())
}

fn sqlite_schema(path: &Path) -> Result<Vec<DbTable>> {
    let conn = rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .with_context(|| format!("open SQLite database {}", path.display()))?;

    let mut stmt = conn.prepare(
        "SELECT name, type FROM sqlite_master \
         WHERE type IN ('table', 'view') AND name NOT LIKE 'sqlite_%' \
         ORDER BY type, name",
    )?;
    let objects: Vec<(String, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .filter_map(|row| row.ok())
        .collect();

    let mut tables = Vec::new();
    for (name, obj_type) in objects {
        let kind = if obj_type == "view" {
            "view".to_string()
        } else {
            "table".to_string()
        };
        let columns = sqlite_table_columns(&conn, &name)?;
        let indexes = if kind == "view" {
            Vec::new()
        } else {
            sqlite_table_indexes(&conn, &name)?
        };
        tables.push(DbTable {
            schema: "main".into(),
            name,
            kind,
            columns,
            indexes,
        });
    }
    Ok(tables)
}

fn sqlite_table_columns(conn: &rusqlite::Connection, name: &str) -> Result<Vec<DbColumn>> {
    let pragma = format!("PRAGMA table_info({})", quote_sqlite_ident(name));
    let mut col_stmt = conn.prepare(&pragma)?;
    let columns = col_stmt
        .query_map([], |row| {
            Ok(DbColumn {
                name: row.get::<_, String>(1)?,
                type_name: row.get::<_, String>(2)?,
                nullable: row.get::<_, i64>(3)? == 0,
            })
        })?
        .filter_map(|row| row.ok())
        .collect();
    Ok(columns)
}

fn sqlite_table_indexes(conn: &rusqlite::Connection, table: &str) -> Result<Vec<DbIndex>> {
    let pragma = format!("PRAGMA index_list({})", quote_sqlite_ident(table));
    let mut stmt = conn.prepare(&pragma)?;
    let rows: Vec<(String, i64, String)> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .filter_map(|row| row.ok())
        .collect();

    let mut indexes = Vec::new();
    for (idx_name, unique, origin) in rows {
        let columns = sqlite_index_columns(conn, &idx_name)?;
        indexes.push(DbIndex {
            name: idx_name,
            columns,
            unique: unique != 0,
            primary: origin == "pk",
        });
    }
    Ok(indexes)
}

fn sqlite_index_columns(conn: &rusqlite::Connection, index: &str) -> Result<Vec<String>> {
    let pragma = format!("PRAGMA index_info({})", quote_sqlite_ident(index));
    let mut stmt = conn.prepare(&pragma)?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(2))?
        .filter_map(|row| row.ok())
        .collect();
    Ok(columns)
}

fn sqlite_query(path: &Path, sql: &str, limit: u32, started: Instant) -> DbQueryResult {
    match rusqlite::Connection::open(path) {
        Ok(conn) => {
            let limited = maybe_limit_select(sql, limit);
            let mut stmt = match conn.prepare(&limited) {
                Ok(stmt) => stmt,
                Err(e) => {
                    return DbQueryResult {
                        columns: Vec::new(),
                        rows: Vec::new(),
                        row_count: 0,
                        truncated: false,
                        elapsed_ms: started.elapsed().as_millis() as u64,
                        error: Some(e.to_string()),
                    };
                }
            };
            let columns: Vec<String> = stmt
                .column_names()
                .into_iter()
                .map(str::to_string)
                .collect();
            let mut rows = Vec::new();
            let mut truncated = false;
            match stmt.query([]) {
                Ok(mut cursor) => loop {
                    match cursor.next() {
                        Ok(Some(row)) => {
                            if rows.len() as u32 >= limit {
                                truncated = true;
                                break;
                            }
                            let mut values = Vec::with_capacity(columns.len());
                            for idx in 0..columns.len() {
                                match row.get_ref(idx) {
                                    Ok(value) => values.push(sqlite_value_as_string(value)),
                                    Err(e) => {
                                        return DbQueryResult {
                                            row_count: rows.len(),
                                            columns,
                                            rows,
                                            truncated,
                                            elapsed_ms: started.elapsed().as_millis() as u64,
                                            error: Some(e.to_string()),
                                        };
                                    }
                                }
                            }
                            rows.push(values);
                        }
                        Ok(None) => break,
                        Err(e) => {
                            return DbQueryResult {
                                row_count: rows.len(),
                                columns,
                                rows,
                                truncated,
                                elapsed_ms: started.elapsed().as_millis() as u64,
                                error: Some(e.to_string()),
                            };
                        }
                    }
                },
                Err(e) => {
                    return DbQueryResult {
                        columns,
                        rows,
                        row_count: 0,
                        truncated: false,
                        elapsed_ms: started.elapsed().as_millis() as u64,
                        error: Some(e.to_string()),
                    };
                }
            }
            DbQueryResult {
                row_count: rows.len(),
                columns,
                rows,
                truncated,
                elapsed_ms: started.elapsed().as_millis() as u64,
                error: None,
            }
        }
        Err(e) => DbQueryResult {
            columns: Vec::new(),
            rows: Vec::new(),
            row_count: 0,
            truncated: false,
            elapsed_ms: started.elapsed().as_millis() as u64,
            error: Some(e.to_string()),
        },
    }
}

fn postgres_schema(
    ws: &Path,
    url: &str,
    compose: Option<&ComposePostgresExec>,
    ssl: Option<&DbSslSettings>,
) -> Result<Vec<DbTable>> {
    let sql = "SELECT c.table_schema, c.table_name, c.column_name, c.data_type, c.is_nullable, \
               COALESCE( \
                 CASE t.table_type \
                   WHEN 'BASE TABLE' THEN 'table' \
                   WHEN 'VIEW' THEN 'view' \
                 END, \
                 CASE WHEN mv.matviewname IS NOT NULL THEN 'materialized_view' END, \
                 'table' \
               ) \
               FROM information_schema.columns c \
               LEFT JOIN information_schema.tables t \
                 ON c.table_schema = t.table_schema \
                AND c.table_name = t.table_name \
                AND t.table_type IN ('BASE TABLE', 'VIEW') \
               LEFT JOIN pg_matviews mv \
                 ON c.table_schema = mv.schemaname \
                AND c.table_name = mv.matviewname \
               WHERE c.table_schema NOT IN ('pg_catalog', 'information_schema') \
               ORDER BY c.table_schema, c.table_name, c.ordinal_position";
    let out = run_postgres_psql(
        ws,
        url,
        compose,
        ssl,
        &["-At", "-F", "\x1f", "-v", "ON_ERROR_STOP=1", "-c", sql],
    )?;
    if out.exit_code != 0 {
        bail!("{}", format_psql_error(&out.stdout, &out.stderr));
    }

    let mut tables: Vec<DbTable> = Vec::new();
    for line in out.stdout.lines().filter(|l| !l.trim().is_empty()) {
        let parts: Vec<&str> = line.split('\x1f').collect();
        if parts.len() < 6 {
            continue;
        }
        let schema = parts[0].to_string();
        let name = parts[1].to_string();
        let column = DbColumn {
            name: parts[2].to_string(),
            type_name: parts[3].to_string(),
            nullable: parts[4].eq_ignore_ascii_case("YES"),
        };
        let kind = parts[5].to_string();
        if let Some(table) = tables
            .iter_mut()
            .find(|t| t.schema == schema && t.name == name)
        {
            table.columns.push(column);
        } else {
            tables.push(DbTable {
                schema,
                name,
                kind,
                columns: vec![column],
                indexes: Vec::new(),
            });
        }
    }

    attach_postgres_indexes(ws, url, compose, ssl, &mut tables)?;
    Ok(tables)
}

fn attach_postgres_indexes(
    ws: &Path,
    url: &str,
    compose: Option<&ComposePostgresExec>,
    ssl: Option<&DbSslSettings>,
    tables: &mut [DbTable],
) -> Result<()> {
    let sql = "SELECT n.nspname, t.relname, i.relname, ix.indisunique, ix.indisprimary, \
               COALESCE(( \
                 SELECT string_agg(a.attname, '|' ORDER BY u.ord) \
                 FROM unnest(ix.indkey) WITH ORDINALITY AS u(attnum, ord) \
                 JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = u.attnum AND u.attnum > 0 \
               ), '') \
               FROM pg_class t \
               JOIN pg_namespace n ON n.oid = t.relnamespace \
               JOIN pg_index ix ON ix.indrelid = t.oid \
               JOIN pg_class i ON i.oid = ix.indexrelid \
               WHERE n.nspname NOT IN ('pg_catalog', 'information_schema') \
                 AND t.relkind IN ('r', 'm') \
               ORDER BY n.nspname, t.relname, i.relname";
    let out = run_postgres_psql(
        ws,
        url,
        compose,
        ssl,
        &["-At", "-F", "\x1f", "-v", "ON_ERROR_STOP=1", "-c", sql],
    )?;
    if out.exit_code != 0 {
        bail!("{}", format_psql_error(&out.stdout, &out.stderr));
    }

    for line in out.stdout.lines().filter(|l| !l.trim().is_empty()) {
        let parts: Vec<&str> = line.split('\x1f').collect();
        if parts.len() < 6 {
            continue;
        }
        let schema = parts[0];
        let table_name = parts[1];
        let index = DbIndex {
            name: parts[2].to_string(),
            columns: parts[5]
                .split('|')
                .filter(|c| !c.is_empty())
                .map(str::to_string)
                .collect(),
            unique: parts[3] == "t",
            primary: parts[4] == "t",
        };
        if let Some(table) = tables
            .iter_mut()
            .find(|t| t.schema == schema && t.name == table_name)
        {
            table.indexes.push(index);
        }
    }
    Ok(())
}

fn postgres_query(
    ws: &Path,
    url: &str,
    compose: Option<&ComposePostgresExec>,
    ssl: Option<&DbSslSettings>,
    sql: &str,
    limit: u32,
    started: Instant,
) -> DbQueryResult {
    let limited = maybe_limit_select(sql, limit);
    let out = match run_postgres_psql(
        ws,
        url,
        compose,
        ssl,
        &["--csv", "-v", "ON_ERROR_STOP=1", "-c", &limited],
    ) {
        Ok(out) => out,
        Err(e) => {
            return DbQueryResult {
                columns: Vec::new(),
                rows: Vec::new(),
                row_count: 0,
                truncated: false,
                elapsed_ms: started.elapsed().as_millis() as u64,
                error: Some(e.to_string()),
            };
        }
    };
    if out.exit_code != 0 {
        return DbQueryResult {
            columns: Vec::new(),
            rows: Vec::new(),
            row_count: 0,
            truncated: false,
            elapsed_ms: started.elapsed().as_millis() as u64,
            error: Some(format_psql_error(&out.stdout, &out.stderr)),
        };
    }

    let parsed = parse_csv_rows(&out.stdout);
    let Some((columns, mut rows)) = parsed else {
        return DbQueryResult {
            columns: Vec::new(),
            rows: Vec::new(),
            row_count: 0,
            truncated: false,
            elapsed_ms: started.elapsed().as_millis() as u64,
            error: None,
        };
    };
    let truncated = rows.len() as u32 > limit;
    if truncated {
        rows.truncate(limit as usize);
    }
    DbQueryResult {
        row_count: rows.len(),
        columns,
        rows,
        truncated,
        elapsed_ms: started.elapsed().as_millis() as u64,
        error: None,
    }
}

fn mysql_schema(
    ws: &Path,
    url: &str,
    compose: Option<&ComposeMysqlExec>,
    ssl: Option<&DbSslSettings>,
) -> Result<Vec<DbTable>> {
    let sql = "SELECT c.TABLE_SCHEMA, c.TABLE_NAME, c.COLUMN_NAME, c.COLUMN_TYPE, c.IS_NULLABLE, \
               CASE t.TABLE_TYPE \
                 WHEN 'BASE TABLE' THEN 'table' \
                 WHEN 'VIEW' THEN 'view' \
                 ELSE 'table' \
               END \
               FROM information_schema.COLUMNS c \
               JOIN information_schema.TABLES t \
                 ON c.TABLE_SCHEMA = t.TABLE_SCHEMA AND c.TABLE_NAME = t.TABLE_NAME \
               WHERE c.TABLE_SCHEMA NOT IN ('information_schema', 'mysql', 'performance_schema', 'sys') \
               ORDER BY c.TABLE_SCHEMA, c.TABLE_NAME, c.ORDINAL_POSITION";
    let out = run_mysql_cli(ws, url, compose, ssl, &["--batch", "--raw", "--skip-column-names", "-e", sql])?;
    if out.exit_code != 0 {
        bail!("{}", format_mysql_error(&out.stdout, &out.stderr));
    }

    let mut tables: Vec<DbTable> = Vec::new();
    for line in out.stdout.lines().filter(|l| !l.trim().is_empty()) {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 6 {
            continue;
        }
        let schema = parts[0].to_string();
        let name = parts[1].to_string();
        let column = DbColumn {
            name: parts[2].to_string(),
            type_name: parts[3].to_string(),
            nullable: parts[4].eq_ignore_ascii_case("YES"),
        };
        let kind = parts[5].to_string();
        if let Some(table) = tables
            .iter_mut()
            .find(|t| t.schema == schema && t.name == name)
        {
            table.columns.push(column);
        } else {
            tables.push(DbTable {
                schema,
                name,
                kind,
                columns: vec![column],
                indexes: Vec::new(),
            });
        }
    }

    attach_mysql_indexes(ws, url, compose, ssl, &mut tables)?;
    Ok(tables)
}

fn attach_mysql_indexes(
    ws: &Path,
    url: &str,
    compose: Option<&ComposeMysqlExec>,
    ssl: Option<&DbSslSettings>,
    tables: &mut [DbTable],
) -> Result<()> {
    let sql = "SELECT TABLE_SCHEMA, TABLE_NAME, INDEX_NAME, NON_UNIQUE, \
               GROUP_CONCAT(COLUMN_NAME ORDER BY SEQ_IN_INDEX SEPARATOR '|') \
               FROM information_schema.STATISTICS \
               WHERE TABLE_SCHEMA NOT IN ('information_schema', 'mysql', 'performance_schema', 'sys') \
               GROUP BY TABLE_SCHEMA, TABLE_NAME, INDEX_NAME, NON_UNIQUE \
               ORDER BY TABLE_SCHEMA, TABLE_NAME, INDEX_NAME";
    let out = run_mysql_cli(ws, url, compose, ssl, &["--batch", "--raw", "--skip-column-names", "-e", sql])?;
    if out.exit_code != 0 {
        // Indexes are best-effort — schema columns still useful.
        tracing::warn!("mysql index lookup failed: {}", format_mysql_error(&out.stdout, &out.stderr));
        return Ok(());
    }

    for line in out.stdout.lines().filter(|l| !l.trim().is_empty()) {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 5 {
            continue;
        }
        let schema = parts[0];
        let table_name = parts[1];
        let index_name = parts[2];
        let non_unique = parts[3];
        let cols = parts[4];
        let index = DbIndex {
            name: index_name.to_string(),
            columns: cols
                .split('|')
                .filter(|c| !c.is_empty())
                .map(str::to_string)
                .collect(),
            unique: non_unique == "0",
            primary: index_name.eq_ignore_ascii_case("PRIMARY"),
        };
        if let Some(table) = tables
            .iter_mut()
            .find(|t| t.schema == schema && t.name == table_name)
        {
            table.indexes.push(index);
        }
    }
    Ok(())
}

fn mysql_query(
    ws: &Path,
    url: &str,
    compose: Option<&ComposeMysqlExec>,
    ssl: Option<&DbSslSettings>,
    sql: &str,
    limit: u32,
    started: Instant,
) -> DbQueryResult {
    let limited = maybe_limit_select(sql, limit);
    let out = match run_mysql_cli(
        ws,
        url,
        compose,
        ssl,
        &["--batch", "--raw", "-e", &limited],
    ) {
        Ok(out) => out,
        Err(e) => {
            return DbQueryResult {
                columns: Vec::new(),
                rows: Vec::new(),
                row_count: 0,
                truncated: false,
                elapsed_ms: started.elapsed().as_millis() as u64,
                error: Some(e.to_string()),
            };
        }
    };
    if out.exit_code != 0 {
        return DbQueryResult {
            columns: Vec::new(),
            rows: Vec::new(),
            row_count: 0,
            truncated: false,
            elapsed_ms: started.elapsed().as_millis() as u64,
            error: Some(format_mysql_error(&out.stdout, &out.stderr)),
        };
    }

    let parsed = parse_tsv_rows(&out.stdout);
    let Some((columns, mut rows)) = parsed else {
        return DbQueryResult {
            columns: Vec::new(),
            rows: Vec::new(),
            row_count: 0,
            truncated: false,
            elapsed_ms: started.elapsed().as_millis() as u64,
            error: None,
        };
    };
    let truncated = rows.len() as u32 > limit;
    if truncated {
        rows.truncate(limit as usize);
    }
    DbQueryResult {
        row_count: rows.len(),
        columns,
        rows,
        truncated,
        elapsed_ms: started.elapsed().as_millis() as u64,
        error: None,
    }
}

fn maybe_limit_select(sql: &str, limit: u32) -> String {
    let trimmed = sql.trim().trim_end_matches(';').trim();
    let lower = trimmed.to_lowercase();
    if lower.contains(" limit ") || lower.ends_with(" limit") {
        return trimmed.to_string();
    }
    if lower.starts_with("select") || lower.starts_with("with") {
        format!("{trimmed} LIMIT {limit}")
    } else {
        trimmed.to_string()
    }
}

fn sqlite_value_as_string(value: ValueRef<'_>) -> String {
    match value {
        ValueRef::Null => String::new(),
        ValueRef::Integer(v) => v.to_string(),
        ValueRef::Real(v) => v.to_string(),
        ValueRef::Text(v) => String::from_utf8_lossy(v).into_owned(),
        ValueRef::Blob(v) => format!("<blob {} bytes>", v.len()),
    }
}

fn quote_sqlite_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn postgres_display(url: &str) -> String {
    if let Ok(parsed) = url::Url::parse(url) {
        let host = parsed.host_str().unwrap_or("localhost");
        let db = parsed.path().trim_start_matches('/');
        if db.is_empty() {
            return host.to_string();
        }
        return format!("{host}/{db}");
    }
    "PostgreSQL".to_string()
}

fn mysql_display(url: &str) -> String {
    if let Ok(parsed) = url::Url::parse(url) {
        let host = parsed.host_str().unwrap_or("localhost");
        let db = parsed.path().trim_start_matches('/');
        if db.is_empty() {
            return format!("mysql://{host}");
        }
        return format!("{host}/{db}");
    }
    "MySQL".to_string()
}

#[derive(Debug, Clone)]
struct MysqlCliTarget {
    host: String,
    port: String,
    user: String,
    password: Option<String>,
    database: Option<String>,
}

fn parse_mysql_url(url: &str) -> Result<MysqlCliTarget> {
    let parsed = url::Url::parse(url).with_context(|| format!("invalid MySQL URL: {url}"))?;
    let host = parsed
        .host_str()
        .filter(|h| !h.is_empty())
        .unwrap_or("localhost")
        .to_string();
    let port = parsed
        .port()
        .map(|p| p.to_string())
        .unwrap_or_else(|| "3306".into());
    let user = if parsed.username().is_empty() {
        "root".into()
    } else {
        urlencoding_decode(parsed.username())
    };
    let password = parsed.password().map(urlencoding_decode);
    let database = {
        let path = parsed.path().trim_start_matches('/');
        if path.is_empty() {
            None
        } else {
            Some(path.split('/').next().unwrap_or(path).to_string())
        }
    };
    Ok(MysqlCliTarget {
        host,
        port,
        user,
        password,
        database,
    })
}

fn urlencoding_decode(value: &str) -> String {
    percent_decode(value).unwrap_or_else(|| value.to_string())
}

fn percent_decode(value: &str) -> Option<String> {
    let mut out = Vec::new();
    let bytes = value.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

fn mysql_ssl_mode(ssl: Option<&DbSslSettings>) -> Option<&'static str> {
    let mode = ssl
        .and_then(|s| s.ssl_mode.as_deref())
        .map(str::trim)
        .filter(|m| !m.is_empty())?;
    Some(match mode.to_ascii_lowercase().as_str() {
        "disable" | "disabled" => "DISABLED",
        "allow" | "prefer" | "preferred" => "PREFERRED",
        "require" | "required" => "REQUIRED",
        "verify-ca" => "VERIFY_CA",
        "verify-full" | "verify_identity" => "VERIFY_IDENTITY",
        _ => "PREFERRED",
    })
}

fn mysql_ssl_cli_args(ssl: Option<&DbSslSettings>) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(mode) = mysql_ssl_mode(ssl) {
        args.push(format!("--ssl-mode={mode}"));
    }
    let Some(ssl) = ssl else {
        return args;
    };
    if let Some(path) = ssl.ssl_root_cert.as_deref().filter(|p| !p.is_empty()) {
        args.push(format!("--ssl-ca={path}"));
    }
    if let Some(path) = ssl.ssl_cert.as_deref().filter(|p| !p.is_empty()) {
        args.push(format!("--ssl-cert={path}"));
    }
    if let Some(path) = ssl.ssl_key.as_deref().filter(|p| !p.is_empty()) {
        args.push(format!("--ssl-key={path}"));
    }
    args
}

fn mysql_base_cli_args(target: &MysqlCliTarget, ssl: Option<&DbSslSettings>) -> Vec<String> {
    let mut args = vec![
        "-h".into(),
        target.host.clone(),
        "-P".into(),
        target.port.clone(),
        "-u".into(),
        target.user.clone(),
    ];
    if let Some(db) = &target.database {
        args.push("-D".into());
        args.push(db.clone());
    }
    args.extend(mysql_ssl_cli_args(ssl));
    args
}

fn mysql_cli_env(target: &MysqlCliTarget) -> Vec<(String, String)> {
    let mut env = Vec::new();
    if let Some(password) = &target.password {
        env.push(("MYSQL_PWD".into(), password.clone()));
    }
    env
}

fn mysql_program() -> String {
    crate::toolchain::resolve_program("mysql")
        .map(|p| p.to_string_lossy().into_owned())
        .or_else(|| {
            crate::toolchain::resolve_program("mariadb")
                .map(|p| p.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "mysql".into())
}

fn run_mysql_cli(
    ws: &Path,
    url: &str,
    compose: Option<&ComposeMysqlExec>,
    ssl: Option<&DbSslSettings>,
    extra_args: &[&str],
) -> Result<GitOutput> {
    if let Some(exec) = compose {
        let mut parts = vec![
            "exec".to_string(),
            "-T".to_string(),
            shell_quote(&exec.service),
            "mysql".to_string(),
            "-u".to_string(),
            shell_quote(&exec.user),
            "-D".to_string(),
            shell_quote(&exec.database),
        ];
        // SSL inside compose container is rare; still pass through when set.
        for arg in mysql_ssl_cli_args(ssl) {
            parts.push(shell_quote(&arg));
        }
        parts.extend(extra_args.iter().map(|s| shell_quote(s)));
        let dir = if exec.compose_dir.is_empty() {
            ".".into()
        } else {
            exec.compose_dir.clone()
        };
        let command = format!(
            "cd {} && {}",
            shell_quote(&dir),
            docker_compose_cmd(&parts.join(" "))
        );
        return run_shell_command(ws, &command);
    }

    let target = parse_mysql_url(url)?;
    let mut owned = mysql_base_cli_args(&target, ssl);
    owned.extend(extra_args.iter().map(|s| (*s).to_string()));
    let refs: Vec<&str> = owned.iter().map(String::as_str).collect();
    let env_vec = mysql_cli_env(&target);
    let env_refs: Vec<(&str, &str)> = env_vec
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let program = mysql_program();
    run_command_with_env(ws, &program, &refs, &env_refs)
}

fn mysql_cli_file_command(url: &str, ssl: Option<&DbSslSettings>, rel_sql_file: &str) -> Result<String> {
    let target = parse_mysql_url(url)?;
    let program = mysql_program();
    let mut parts = vec![shell_quote(&program)];
    for arg in mysql_base_cli_args(&target, ssl) {
        parts.push(shell_quote(&arg));
    }
    let pwd_prefix = target
        .password
        .as_ref()
        .map(|p| format!("MYSQL_PWD={} ", shell_quote(p)))
        .unwrap_or_default();
    Ok(format!(
        "{}{} < {}",
        pwd_prefix,
        parts.join(" "),
        shell_quote(rel_sql_file)
    ))
}

fn compose_mysql_file_command(exec: &ComposeMysqlExec, rel_sql_file: &str) -> String {
    let dir = if exec.compose_dir.is_empty() {
        ".".into()
    } else {
        exec.compose_dir.clone()
    };
    let inner = format!(
        "exec -T {} mysql -u {} -D {}",
        shell_quote(&exec.service),
        shell_quote(&exec.user),
        shell_quote(&exec.database)
    );
    let compose = docker_compose_cmd(&inner);
    if dir == "." {
        format!("{} < {}", compose, shell_quote(rel_sql_file))
    } else {
        format!(
            "(cd {} && {}) < {}",
            shell_quote(&dir),
            compose,
            shell_quote(rel_sql_file)
        )
    }
}

fn format_mysql_error(stdout: &str, stderr: &str) -> String {
    let combined = format!("{stderr}\n{stdout}").trim().to_string();
    if combined.is_empty() {
        "mysql command failed — install the mysql or mariadb client (Settings → Compiler)".into()
    } else {
        combined
    }
}

fn parse_tsv_rows(raw: &str) -> Option<(Vec<String>, Vec<Vec<String>>)> {
    let mut lines = raw.lines().filter(|l| !l.is_empty());
    let header = lines.next()?;
    let columns = header.split('\t').map(str::to_string).collect::<Vec<_>>();
    let rows = lines
        .map(|line| line.split('\t').map(str::to_string).collect())
        .collect();
    Some((columns, rows))
}

fn docker_compose_cmd(args: &str) -> String {
    format!(
        "if docker compose version >/dev/null 2>&1; then docker compose {args}; \
         elif command -v docker-compose >/dev/null 2>&1; then docker-compose {args}; \
         else docker compose {args}; fi"
    )
}

fn compose_workdir(exec: &ComposePostgresExec) -> String {
    if exec.compose_dir.is_empty() {
        ".".into()
    } else {
        exec.compose_dir.clone()
    }
}

fn compose_psql_exec_args(exec: &ComposePostgresExec, psql_args: &[&str]) -> String {
    let mut parts = vec![
        "exec".to_string(),
        "-T".to_string(),
        shell_quote(&exec.service),
        "psql".to_string(),
        "-U".to_string(),
        shell_quote(&exec.user),
        "-d".to_string(),
        shell_quote(&exec.database),
    ];
    parts.extend(psql_args.iter().map(|s| shell_quote(s)));
    parts.join(" ")
}

fn compose_psql_file_command(exec: &ComposePostgresExec, rel_sql_file: &str) -> String {
    let dir = compose_workdir(exec);
    let inner = compose_psql_exec_args(exec, &["-v", "ON_ERROR_STOP=1"]);
    let compose = docker_compose_cmd(&inner);
    if dir == "." {
        format!("{} < {}", compose, shell_quote(rel_sql_file))
    } else {
        format!(
            "(cd {} && {}) < {}",
            shell_quote(&dir),
            compose,
            shell_quote(rel_sql_file)
        )
    }
}

fn compose_psql_shell_command(
    exec: &ComposePostgresExec,
    psql_args: &[&str],
) -> String {
    let dir = compose_workdir(exec);
    format!(
        "cd {} && {}",
        shell_quote(&dir),
        docker_compose_cmd(&compose_psql_exec_args(exec, psql_args))
    )
}

fn run_postgres_psql(
    ws: &Path,
    url: &str,
    compose: Option<&ComposePostgresExec>,
    ssl: Option<&DbSslSettings>,
    extra_args: &[&str],
) -> Result<GitOutput> {
    if let Some(exec) = compose {
        let command = compose_psql_shell_command(exec, extra_args);
        return run_shell_command(ws, &command);
    }
    let mut owned = vec![url.to_string()];
    owned.extend(extra_args.iter().map(|s| (*s).to_string()));
    let refs: Vec<&str> = owned.iter().map(String::as_str).collect();
    let env_vec = ssl_env_pairs(ssl);
    let env_refs: Vec<(&str, &str)> = env_vec
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    if let Some(psql) = crate::toolchain::resolve_program("psql") {
        return run_command_with_env(ws, psql.to_string_lossy().as_ref(), &refs, &env_refs);
    }
    run_command_with_env(
        ws,
        crate::toolchain::resolve_program_or("psql")?.to_string_lossy().as_ref(),
        &refs,
        &env_refs,
    )
}

fn validate_ssl_files(ssl: &DbSslSettings) -> Option<String> {
    for (label, path) in [
        ("CA certificate", ssl.ssl_root_cert.as_deref()),
        ("Client certificate", ssl.ssl_cert.as_deref()),
        ("Client private key", ssl.ssl_key.as_deref()),
    ] {
        let Some(path) = path.filter(|p| !p.trim().is_empty()) else {
            continue;
        };
        if !Path::new(path).is_file() {
            return Some(format!("{label} not found: {path}"));
        }
    }
    None
}

fn ssl_env_pairs(ssl: Option<&DbSslSettings>) -> Vec<(String, String)> {
    let Some(ssl) = ssl.filter(|s| !s.is_empty()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if let Some(mode) = ssl
        .ssl_mode
        .as_deref()
        .filter(|m| !m.is_empty() && !m.eq_ignore_ascii_case("disable"))
    {
        out.push(("PGSSLMODE".into(), mode.to_string()));
    }
    if let Some(path) = ssl.ssl_root_cert.as_deref().filter(|p| !p.is_empty()) {
        out.push(("PGSSLROOTCERT".into(), path.to_string()));
    }
    if let Some(path) = ssl.ssl_cert.as_deref().filter(|p| !p.is_empty()) {
        out.push(("PGSSLCERT".into(), path.to_string()));
    }
    if let Some(path) = ssl.ssl_key.as_deref().filter(|p| !p.is_empty()) {
        out.push(("PGSSLKEY".into(), path.to_string()));
    }
    out
}

fn ssl_env_shell_prefix(ssl: Option<&DbSslSettings>) -> String {
    let prefix = ssl_env_pairs(ssl)
        .into_iter()
        .map(|(key, value)| format!("{key}={}", shell_quote(&value)))
        .collect::<Vec<_>>()
        .join(" ");
    if prefix.is_empty() {
        prefix
    } else {
        format!("{prefix} ")
    }
}

fn format_psql_error(stdout: &str, stderr: &str) -> String {
    let combined = format!("{stderr}\n{stdout}").trim().to_string();
    if combined.is_empty() {
        "psql command failed".into()
    } else {
        combined
    }
}

fn parse_csv_rows(raw: &str) -> Option<(Vec<String>, Vec<Vec<String>>)> {
    let mut lines = raw.lines().filter(|l| !l.is_empty());
    let header = lines.next()?;
    let columns = parse_csv_line(header);
    let rows = lines.map(parse_csv_line).collect();
    Some((columns, rows))
}

fn parse_csv_line(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '"' if in_quotes => {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    current.push('"');
                } else {
                    in_quotes = false;
                }
            }
            '"' => in_quotes = true,
            ',' if !in_quotes => {
                out.push(current.clone());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    out.push(current);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_csv() {
        let (cols, rows) = parse_csv_rows("a,b\n1,2").unwrap();
        assert_eq!(cols, vec!["a", "b"]);
        assert_eq!(rows, vec![vec!["1", "2"]]);
    }

    #[test]
    fn prepare_sql_run_rejects_empty_content() {
        let tmp = std::env::temp_dir().join(format!("reaper-db-empty-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(
            tmp.join("docker-compose.yml"),
            "services:\n  postgres:\n    image: postgres:16-alpine\n",
        )
        .unwrap();
        let err = prepare_sql_run_command(&tmp, "query.sql", Some("  \n  "), None, None, None)
            .unwrap_err()
            .to_string();
        assert!(err.contains("empty"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn prepare_sql_run_writes_overlay() {
        let tmp = std::env::temp_dir().join(format!("reaper-db-overlay-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(
            tmp.join("docker-compose.yml"),
            "services:\n  postgres:\n    image: postgres:16-alpine\n    environment:\n      POSTGRES_USER: sqlproj\n      POSTGRES_DB: sqlproj\n",
        )
        .unwrap();
        let cmd = prepare_sql_run_command(&tmp, "query.sql", Some("SELECT 1;\n"), None, None, None).unwrap();
        let overlay = tmp.join(SQL_RUN_OVERLAY);
        assert!(overlay.is_file());
        assert_eq!(std::fs::read_to_string(&overlay).unwrap(), "SELECT 1;\n");
        assert!(cmd.contains("< '.reaper/sql-run/current.sql'"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn discovers_postgres_url_from_compose_and_env() {
        let tmp = std::env::temp_dir().join(format!("reaper-db-compose-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(
            tmp.join("docker-compose.yml"),
            "services:\n  postgres:\n    image: postgres:16-alpine\n    ports:\n      - \"${POSTGRES_PORT:-5431}:5432\"\n    environment:\n      POSTGRES_USER: ${POSTGRES_USER:-sqlproj}\n      POSTGRES_PASSWORD: ${POSTGRES_PASSWORD:-sqlproj}\n      POSTGRES_DB: ${POSTGRES_DB:-sqlproj}\n",
        )
        .unwrap();
        let url = discover_database_url(&tmp).expect("discovered url");
        assert!(url.starts_with("postgresql://"));
        assert!(url.contains("sqlproj"));
        assert!(url.contains(":5431/"));
        let cmd = sql_run_command(&tmp, "sql/queries/examples.sql", None, None, None).expect("command");
        assert!(
            cmd.contains("compose exec") && cmd.contains("exec -T") && cmd.contains("psql"),
            "unexpected sql run command: {cmd}"
        );
        assert!(cmd.contains("'postgres'"));
        assert!(cmd.contains("< 'sql/queries/examples.sql'"));
        assert!(cmd.contains("ON_ERROR_STOP=1"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn normalizes_database_url_missing_db_name() {
        let mut env = HashMap::new();
        env.insert("POSTGRES_DB".into(), "sqlproj".into());
        let url = normalize_postgres_url("postgresql://sqlproj:sqlproj@localhost:5433/".into(), &env);
        assert!(url.ends_with("/sqlproj"));
    }

    #[test]
    fn compose_psql_exec_args_shell_quotes_sql() {
        let exec = ComposePostgresExec {
            compose_dir: ".".into(),
            service: "postgres".into(),
            user: "sqlproj".into(),
            database: "sqlproj".into(),
        };
        let sql = "SELECT id,\nemail\nFROM users\nORDER BY id";
        let inner = compose_psql_exec_args(&exec, &["--csv", "-c", sql]);
        assert!(inner.contains("'SELECT id,\nemail\nFROM users\nORDER BY id'"));
        assert!(inner.contains("'postgres'"));
    }

    #[test]
    fn sqlite_schema_roundtrip() {
        let db = std::env::temp_dir().join(format!("reaper-db-test-{}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&db);
        {
            let conn = rusqlite::Connection::open(&db).unwrap();
            conn.execute_batch(
                "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL); \
                 CREATE INDEX idx_users_name ON users(name);",
            )
            .unwrap();
        }
        let tables = sqlite_schema(&db).unwrap();
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].name, "users");
        assert_eq!(tables[0].kind, "table");
        assert_eq!(tables[0].columns.len(), 2);
        assert!(!tables[0].indexes.is_empty());
        let _ = std::fs::remove_file(db);
    }

    #[test]
    fn detects_mysql_urls() {
        assert!(is_mysql_url("mysql://root@localhost:3306/app"));
        assert!(is_mysql_url("mysql2://user:pass@db.example/app"));
        assert!(is_mysql_url("mariadb://root@127.0.0.1/app"));
        assert!(!is_mysql_url("postgresql://localhost/app"));
        assert!(!is_mysql_url("sqlite:memory:"));
    }

    #[test]
    fn parses_mysql_url_with_encoded_password() {
        let target =
            parse_mysql_url("mysql://app%40user:p%40ss%2Fw@db.example.com:3307/orders").unwrap();
        assert_eq!(target.host, "db.example.com");
        assert_eq!(target.port, "3307");
        assert_eq!(target.user, "app@user");
        assert_eq!(target.password.as_deref(), Some("p@ss/w"));
        assert_eq!(target.database.as_deref(), Some("orders"));
    }

    #[test]
    fn normalizes_mysql_url_missing_db_name() {
        let mut env = HashMap::new();
        env.insert("MYSQL_DATABASE".into(), "orders".into());
        let url = normalize_mysql_url("mysql://root:secret@localhost:3306/".into(), &env);
        assert!(url.ends_with("/orders"));
    }

    #[test]
    fn mysql_ssl_cli_args_include_ca_cert_and_private_key() {
        let ssl = DbSslSettings {
            ssl_mode: Some("verify-full".into()),
            ssl_root_cert: Some("/certs/ca.pem".into()),
            ssl_cert: Some("/certs/client-cert.pem".into()),
            ssl_key: Some("/certs/client-key.pem".into()),
        };
        let args = mysql_ssl_cli_args(Some(&ssl));
        assert!(args.iter().any(|a| a == "--ssl-mode=VERIFY_IDENTITY"));
        assert!(args.iter().any(|a| a == "--ssl-ca=/certs/ca.pem"));
        assert!(args.iter().any(|a| a == "--ssl-cert=/certs/client-cert.pem"));
        assert!(args.iter().any(|a| a == "--ssl-key=/certs/client-key.pem"));
    }

    #[test]
    fn mysql_sql_run_command_applies_ssl_and_client_key() {
        let tmp = std::env::temp_dir().join(format!("reaper-db-mysql-ssl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let ssl = DbSslSettings {
            ssl_mode: Some("require".into()),
            ssl_root_cert: Some("/tmp/ca.pem".into()),
            ssl_cert: Some("/tmp/client.crt".into()),
            ssl_key: Some("/tmp/client.key".into()),
        };
        let cmd = sql_run_command(
            &tmp,
            "queries/seed.sql",
            Some("mysql://app:secret@db.example:3306/app"),
            Some(&ssl),
            None,
        )
        .unwrap();
        assert!(cmd.contains("--ssl-mode=REQUIRED"), "cmd={cmd}");
        assert!(cmd.contains("--ssl-ca=/tmp/ca.pem"), "cmd={cmd}");
        assert!(cmd.contains("--ssl-cert=/tmp/client.crt"), "cmd={cmd}");
        assert!(cmd.contains("--ssl-key=/tmp/client.key"), "cmd={cmd}");
        assert!(cmd.contains("MYSQL_PWD="), "cmd={cmd}");
        assert!(cmd.contains("< 'queries/seed.sql'"), "cmd={cmd}");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn discovers_mysql_url_from_compose() {
        let tmp = std::env::temp_dir().join(format!("reaper-db-mysql-compose-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(
            tmp.join("docker-compose.yml"),
            "services:\n  mysql:\n    image: mysql:8.4\n    ports:\n      - \"3307:3306\"\n    environment:\n      MYSQL_USER: app\n      MYSQL_PASSWORD: secret\n      MYSQL_DATABASE: app\n      MYSQL_ROOT_PASSWORD: root\n",
        )
        .unwrap();
        let url = discover_database_url(&tmp).expect("discovered mysql url");
        assert!(
            is_mysql_url(&url),
            "expected mysql url, got {url}"
        );
        assert!(url.contains("app"));
        let cmd = sql_run_command(&tmp, "sql/seed.sql", None, None, None).expect("command");
        assert!(
            cmd.contains("compose exec") && cmd.contains("mysql"),
            "unexpected sql run command: {cmd}"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn parses_mysql_tsv_rows() {
        let (cols, rows) = parse_tsv_rows("id\tname\n1\talice\n2\tbob").unwrap();
        assert_eq!(cols, vec!["id", "name"]);
        assert_eq!(rows, vec![vec!["1", "alice"], vec!["2", "bob"]]);
    }

    #[test]
    fn sql_run_command_rewrites_through_ssh_tunnel_settings_without_starting_ssh() {
        // Disabled tunnel settings must not rewrite; enabled without host is rejected at connection_view.
        let tmp = std::env::temp_dir().join(format!("reaper-db-ssh-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let ssh = DbSshTunnelSettings {
            enabled: false,
            host: Some("bastion.example".into()),
            user: Some("ubuntu".into()),
            ..Default::default()
        };
        let cmd = sql_run_command(
            &tmp,
            "q.sql",
            Some("postgresql://app:x@db.internal:5432/app"),
            None,
            Some(&ssh),
        )
        .unwrap();
        assert!(cmd.contains("db.internal:5432"), "cmd={cmd}");
        assert!(!cmd.contains("127.0.0.1"), "cmd={cmd}");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
