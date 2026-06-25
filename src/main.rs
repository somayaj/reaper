mod config;
mod cursor;
mod git;
mod agent;
mod auth;
mod repos;
mod settings;
mod state;
mod web;
mod workspace;

use std::net::SocketAddr;

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use config::Config;
use settings::SettingsStore;
use state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "reaper=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

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

    let state = AppState::new(config.clone(), settings);

    let addr = SocketAddr::from((config.host.parse::<std::net::IpAddr>()?, config.port));
    let listener = tokio::net::TcpListener::bind(addr).await?;

    tracing::info!("Reaper listening on http://{addr}");
    tracing::info!("Repositories stored in {}", config.repos_dir.display());

    axum::serve(listener, web::router(state)).await?;

    Ok(())
}
