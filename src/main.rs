mod config;
mod cursor;
mod port;
mod git;
mod agent;
mod auth;
mod gradle;
mod gui;
mod jdk;
mod repos;
mod settings;
mod state;
mod system;
mod toolchain;
mod web;
mod workspace;

use std::net::SocketAddr;
use std::sync::Mutex;

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use config::{Config, running_in_app_bundle};
use settings::SettingsStore;
use state::AppState;

fn main() -> anyhow::Result<()> {
    workspace::ensure_developer_path();
    if wants_gui() {
        run_gui_mode()
    } else {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?
            .block_on(run_server_mode())
    }
}

fn wants_gui() -> bool {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--server" || a == "--no-gui") {
        return false;
    }
    if args.iter().any(|a| a == "--gui") {
        return true;
    }
    #[cfg(target_os = "macos")]
    {
        return running_in_app_bundle();
    }
    #[cfg(not(target_os = "macos"))]
    false
}

fn init_tracing() {
    let data_dir = Config::resolve_data_dir();
    let log_path = Config::resolve_log_path();
    if let Err(e) = std::fs::create_dir_all(&data_dir) {
        eprintln!(
            "reaper: could not create data directory {}: {e}",
            data_dir.display()
        );
    }

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "reaper=debug,tower_http=debug".into());

    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    {
        Ok(log_file) => {
            let file_writer = Mutex::new(log_file);
            tracing_subscriber::registry()
                .with(filter)
                .with(
                    tracing_subscriber::fmt::layer()
                        .with_writer(file_writer)
                        .with_ansi(false)
                        .with_target(true),
                )
                .with(tracing_subscriber::fmt::layer())
                .init();
        }
        Err(e) => {
            eprintln!(
                "reaper: could not open log file {}: {e}",
                log_path.display()
            );
            tracing_subscriber::registry()
                .with(filter)
                .with(tracing_subscriber::fmt::layer())
                .init();
        }
    }
}

async fn prepare_state(bound_port: u16) -> anyhow::Result<AppState> {
    let mut config = Config::from_env();
    config.port = bound_port;
    config.ensure_dirs()?;

    let settings = SettingsStore::load(&config.settings_path)?;

    cursor::reclaim_bridge_port().await;
    match cursor::ensure_bridge_running().await {
        Ok(()) => tracing::info!("Cursor bridge connected at {}", cursor::bridge_url()),
        Err(e) => tracing::warn!(
            "Cursor bridge unavailable: {e:#} (agent chat disabled until bridge starts)"
        ),
    }

    Ok(AppState::new(config, settings))
}

async fn bind_listener(host: &str, preferred: u16) -> anyhow::Result<(tokio::net::TcpListener, SocketAddr)> {
    use std::net::IpAddr;

    let ip: IpAddr = host.parse()?;

    if preferred != port::AUTO_PORT {
        let addr = SocketAddr::from((ip, preferred));
        match tokio::net::TcpListener::bind(addr).await {
            Ok(listener) => {
                let bound = listener.local_addr()?;
                return Ok((listener, bound));
            }
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                tracing::warn!(
                    "Port {preferred} in use; choosing a random available port (set REAPER_PORT to pin a port)"
                );
            }
            Err(e) => return Err(e.into()),
        }
    }

    if let Ok(listener) = tokio::net::TcpListener::bind(SocketAddr::from((ip, 0))).await {
        let bound = listener.local_addr()?;
        tracing::info!("Bound to random port {bound} (set REAPER_PORT to use a fixed port)");
        return Ok((listener, bound));
    }

    for _ in 0..48 {
        let port = port::random_port_candidate();
        if port::is_avoided_port(port) {
            continue;
        }
        let addr = SocketAddr::from((ip, port));
        if let Ok(listener) = tokio::net::TcpListener::bind(addr).await {
            let bound = listener.local_addr()?;
            tracing::info!("Bound to random port {bound}");
            return Ok((listener, bound));
        }
    }

    anyhow::bail!("Could not bind to {host} on a random available port")
}

fn persist_server_port(data_dir: &std::path::Path, port: u16) {
    let path = data_dir.join("reaper.port");
    if let Err(e) = std::fs::write(&path, port.to_string()) {
        tracing::warn!("Could not write {}: {e}", path.display());
    }
}

async fn run_server_mode() -> anyhow::Result<()> {
    init_tracing();

    let config = Config::from_env();
    config.ensure_dirs()?;

    let (listener, addr) = bind_listener(&config.host, config.port).await?;
    persist_server_port(&config.data_dir, addr.port());

    let state = prepare_state(addr.port()).await?;
    let config = state.config.clone();

    tracing::info!("Reaper listening on http://{addr}");
    tracing::info!("Data directory: {}", config.data_dir.display());
    tracing::info!("Log file: {}", Config::resolve_log_path().display());
    tracing::info!("Repositories stored in {}", config.repos_dir.display());
    tracing::info!("Static assets from {}", config.static_dir.display());

    axum::serve(listener, web::router(state)).await?;

    Ok(())
}

fn run_gui_mode() -> anyhow::Result<()> {
    init_tracing();

    let (tx, rx) = std::sync::mpsc::sync_channel::<Result<String, String>>(1);
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");
        let result = rt.block_on(async {
            let config = Config::from_env();
            config.ensure_dirs()?;

            let (listener, addr) = bind_listener(&config.host, config.port).await?;
            persist_server_port(&config.data_dir, addr.port());

            let state = prepare_state(addr.port()).await?;
            let config = state.config.clone();
            let url = format!("http://{addr}");

            tracing::info!("Reaper listening on {url}");
            tracing::info!("Data directory: {}", config.data_dir.display());
            tracing::info!("Log file: {}", Config::resolve_log_path().display());
            tracing::info!("Repositories stored in {}", config.repos_dir.display());
            tracing::info!("Static assets from {}", config.static_dir.display());

            tx.send(Ok(url.clone()))
                .map_err(|_| anyhow::anyhow!("GUI exited before server started"))?;

            axum::serve(listener, web::router(state)).await?;
            Ok::<_, anyhow::Error>(())
        });
        if let Err(e) = result {
            let _ = tx.send(Err(format!("{e:#}")));
        }
    });

    let url = match rx.recv() {
        Ok(Ok(url)) => url,
        Ok(Err(msg)) => {
            gui::show_error(&format!("Reaper failed to start:\n\n{msg}"));
            anyhow::bail!(msg);
        }
        Err(_) => {
            gui::show_error("Reaper failed to start:\n\nserver thread exited unexpectedly");
            anyhow::bail!("server thread exited unexpectedly");
        }
    };

    gui::run(&url)
}
