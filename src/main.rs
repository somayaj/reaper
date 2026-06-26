mod config;
mod cursor;
mod git;
mod agent;
mod auth;
mod gui;
mod jdk;
mod repos;
mod settings;
mod state;
mod system;
mod web;
mod workspace;

use std::net::SocketAddr;

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use config::{Config, running_in_app_bundle};
use settings::SettingsStore;
use state::AppState;

fn main() -> anyhow::Result<()> {
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
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "reaper=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
}

async fn prepare_state() -> anyhow::Result<AppState> {
    let config = Config::from_env();
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

async fn bind_listener(host: &str, start_port: u16) -> anyhow::Result<(tokio::net::TcpListener, SocketAddr)> {
    let ip: std::net::IpAddr = host.parse()?;
    let end = start_port.saturating_add(20);
    for port in start_port..=end {
        let addr = SocketAddr::from((ip, port));
        match tokio::net::TcpListener::bind(addr).await {
            Ok(listener) => {
                if port != start_port {
                    tracing::warn!(
                        "Port {start_port} in use; listening on {port} instead (stop other Reaper instances or set REAPER_PORT)"
                    );
                }
                return Ok((listener, addr));
            }
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => continue,
            Err(e) => return Err(e.into()),
        }
    }
    anyhow::bail!(
        "Could not bind to {host} on ports {start_port}–{end}. Quit other Reaper instances or set REAPER_PORT."
    );
}

async fn run_server_mode() -> anyhow::Result<()> {
    init_tracing();

    let state = prepare_state().await?;
    let config = state.config.clone();

    let (listener, addr) = bind_listener(&config.host, config.port).await?;

    tracing::info!("Reaper listening on http://{addr}");
    tracing::info!("Repositories stored in {}", config.repos_dir.display());
    tracing::info!("Static assets from {}", config.static_dir.display());

    axum::serve(listener, web::router(state)).await?;

    Ok(())
}

fn run_gui_mode() -> anyhow::Result<()> {
    init_tracing();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    let gui_result = rt.block_on(async {
        let state = prepare_state().await?;
        let config = state.config.clone();

        let (listener, addr) = bind_listener(&config.host, config.port).await?;
        let url = format!("http://{addr}");

        tracing::info!("Reaper listening on {url}");
        tracing::info!("Repositories stored in {}", config.repos_dir.display());
        tracing::info!("Static assets from {}", config.static_dir.display());

        tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, web::router(state)).await {
                tracing::error!("Server stopped: {e:#}");
            }
        });

        Ok::<_, anyhow::Error>(url)
    });

    match gui_result {
        Ok(url) => gui::run(&url),
        Err(e) => {
            gui::show_error(&format!("Reaper failed to start:\n\n{e:#}"));
            Err(e)
        }
    }
}
