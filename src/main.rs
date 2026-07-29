#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod platform;
mod config;
mod cursor;
mod port;
mod process_registry;
mod git;
mod agent;
mod auth;
mod gradle;
mod maven;
mod gui;
mod jdk;
mod local_https;
mod repos;
mod settings;
mod state;
mod system;
mod toolchain;
mod ui_preferences;
mod web;
mod workspace;

use std::net::SocketAddr;
use std::sync::Mutex;

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use config::{Config, running_in_app_bundle};
use settings::SettingsStore;
use state::AppState;
use ui_preferences::UiPreferencesStore;

fn main() -> anyhow::Result<()> {
    #[cfg(target_os = "windows")]
    windows_maybe_attach_console();

    workspace::ensure_developer_path();
    if wants_gui() {
        #[cfg(target_os = "windows")]
        crate::platform::free_console();
        run_gui_mode()
    } else {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?
            .block_on(run_server_mode())
    }
}

/// GUI builds use the Windows subsystem (no black cmd window on double-click).
/// `--server` still gets a console so the bind URL is visible.
#[cfg(target_os = "windows")]
fn windows_maybe_attach_console() {
    let args: Vec<String> = std::env::args().collect();
    let wants_console = args
        .iter()
        .any(|a| a == "--server" || a == "--no-gui" || a == "--help" || a == "-h");
    if !wants_console {
        return;
    }
    // ATTACH_PARENT_PROCESS = (DWORD)-1
    const ATTACH_PARENT_PROCESS: u32 = 0xFFFF_FFFF;
    #[link(name = "kernel32")]
    extern "system" {
        fn AttachConsole(dw_process_id: u32) -> i32;
        fn AllocConsole() -> i32;
    }
    unsafe {
        if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
            let _ = AllocConsole();
        }
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
    // Windows: default to native WebView2 window; use --server for browser-only.
    #[cfg(target_os = "windows")]
    {
        return true;
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
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
        .unwrap_or_else(|_| "reaper=warn,tower_http=warn".into());

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
    let ui_preferences = UiPreferencesStore::load(&config.ui_preferences_path)?;

    cursor::reclaim_bridge_port().await;
    match cursor::ensure_bridge_running().await {
        Ok(()) => tracing::info!("Cursor bridge connected at {}", cursor::bridge_url()),
        Err(e) => {
            let msg = format!("{e:#}");
            tracing::warn!(
                "Cursor bridge unavailable: {msg} (agent chat disabled until bridge starts)"
            );
            // Always print to the console so Windows users see this isn't fatal.
            eprintln!(
                "Note: Cursor agent bridge unavailable ({msg}). The IDE still runs without it."
            );
        }
    }

    Ok(AppState::new(config, settings, ui_preferences))
}

fn prefetch_startup_index(state: &AppState) {
    if let Ok(name) = std::env::var("REAPER_STARTUP_REPO") {
        let name = name.trim();
        if !name.is_empty() {
            state
                .project_index_jobs
                .prefetch_startup(&state.config, name);
            return;
        }
    }
    if let Some(name) = state.settings.prefetch_repo() {
        state
            .project_index_jobs
            .prefetch_startup(&state.config, &name);
    }
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
    if let Err(e) = std::fs::write(&path, format!("{port}\n")) {
        tracing::warn!("Could not write {}: {e}", path.display());
    }
}

fn persist_server_url(data_dir: &std::path::Path, url: &str) {
    let path = data_dir.join("reaper.url");
    if let Err(e) = std::fs::write(&path, format!("{url}\n")) {
        tracing::warn!("Could not write {}: {e}", path.display());
    }
}

async fn serve_app(
    listener: tokio::net::TcpListener,
    app: axum::Router,
    tls: local_https::LocalTls,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    web::serve::serve_tls(listener, app, tls.acceptor, shutdown).await
}

async fn run_server_mode() -> anyhow::Result<()> {
    init_tracing();

    let config = Config::from_env();
    config.ensure_dirs()?;

    let (listener, addr) = bind_listener(&config.host, config.port).await?;
    persist_server_port(&config.data_dir, addr.port());

    let tls = local_https::ensure_local_tls(&config.data_dir, &config.host)?;

    let state = prepare_state(addr.port()).await?;
    prefetch_startup_index(&state);
    let config = state.config.clone();
    let url = config.base_url();
    persist_server_url(&config.data_dir, &url);

    // Always print to stderr so Windows/.exe users see the URL (default log level is warn).
    eprintln!();
    eprintln!("Reaper is running.");
    eprintln!("Open in your browser:  {url}");
    eprintln!("(If the browser warns about the certificate, choose Advanced → Continue.)");
    eprintln!("Data directory: {}", config.data_dir.display());
    eprintln!();

    tracing::info!("Reaper listening on {url} (HTTP/2 over TLS)");
    tracing::info!("Data directory: {}", config.data_dir.display());
    tracing::info!("Log file: {}", Config::resolve_log_path().display());
    tracing::info!("Repositories stored in {}", config.repos_dir.display());
    tracing::info!("Static assets from {}", config.static_dir.display());

    tokio::spawn(process_registry::shutdown_watchdog());

    serve_app(
        listener,
        web::router(state),
        tls,
        process_registry::wait_for_shutdown_signal(),
    )
    .await?;

    cursor::stop_bridge().await;
    process_registry::shutdown_all();
    Ok(())
}

fn run_gui_mode() -> anyhow::Result<()> {
    init_tracing();

    use std::sync::Arc;

    use gui::GuiLaunch;
    use web::{loopback_ws_base, webview_init_script, GuiProtocolBridge};
    #[cfg(not(target_os = "windows"))]
    use web::WEBVIEW_ENTRY;

    let (tx, rx) = std::sync::mpsc::sync_channel::<Result<GuiLaunch, String>>(1);
    let (bridge_tx, bridge_rx) =
        std::sync::mpsc::sync_channel::<Arc<GuiProtocolBridge>>(1);
    let shutdown_notify = std::sync::Arc::new(tokio::sync::Notify::new());
    let server_shutdown = std::sync::Arc::clone(&shutdown_notify);
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
            prefetch_startup_index(&state);
            let config = state.config.clone();
            let loopback_url = config.base_url();
            persist_server_url(&config.data_dir, &loopback_url);

            let handle = tokio::runtime::Handle::current();
            let app = web::router(state);
            let bridge = Arc::new(GuiProtocolBridge::new(app.clone(), handle));
            let loopback_ws = loopback_ws_base(&config.host, addr.port());

            bridge_tx
                .send(bridge)
                .map_err(|_| anyhow::anyhow!("GUI exited before protocol bridge ready"))?;

            // Windows WebView2: load over loopback HTTP. Custom `reaper://` + CDN CSS often
            // paints a broken shell in VMs; HTTP uses the same axum router already bound.
            #[cfg(target_os = "windows")]
            let webview_url = {
                let base = loopback_url.trim_end_matches('/');
                format!("{base}/")
            };
            #[cfg(not(target_os = "windows"))]
            let webview_url = WEBVIEW_ENTRY.to_string();

            tracing::info!("Reaper loopback on {loopback_url} (terminal WS, git CLI)");
            tracing::info!("Reaper WebView on {webview_url}");
            tracing::info!("Data directory: {}", config.data_dir.display());
            tracing::info!("Log file: {}", Config::resolve_log_path().display());
            tracing::info!("Repositories stored in {}", config.repos_dir.display());
            tracing::info!("Static assets from {}", config.static_dir.display());

            tx.send(Ok(GuiLaunch {
                webview_url,
                init_script: webview_init_script(&loopback_ws),
            }))
            .map_err(|_| anyhow::anyhow!("GUI exited before server started"))?;

            tokio::spawn(process_registry::shutdown_watchdog());

            let gui_shutdown = async move {
                server_shutdown.notified().await;
                tracing::info!("GUI closed; shutting down server");
                process_registry::initiate_shutdown();
            };

            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    tokio::select! {
                        () = process_registry::wait_for_shutdown_signal() => {}
                        () = gui_shutdown => {}
                    }
                })
                .await?;
            cursor::stop_bridge().await;
            Ok::<_, anyhow::Error>(())
        });
        process_registry::shutdown_all();
        if let Err(e) = result {
            let _ = tx.send(Err(format!("{e:#}")));
        }
    });

    let launch = match rx.recv() {
        Ok(Ok(launch)) => launch,
        Ok(Err(msg)) => {
            gui::show_error(&format!("Reaper failed to start:\n\n{msg}"));
            anyhow::bail!(msg);
        }
        Err(_) => {
            gui::show_error("Reaper failed to start:\n\nserver thread exited unexpectedly");
            anyhow::bail!("server thread exited unexpectedly");
        }
    };

    let protocol_bridge = bridge_rx.recv().map_err(|_| {
        gui::show_error("Reaper failed to start:\n\nprotocol bridge not ready");
        anyhow::anyhow!("protocol bridge not ready")
    })?;

    let gui_result = gui::run(&launch, protocol_bridge);
    if let Err(ref e) = gui_result {
        #[cfg(target_os = "windows")]
        {
            gui::show_error(&format!(
                "Native window failed:\n\n{e:#}\n\nCommon fix: install WebView2 Runtime, then re-run reaper.exe\nhttps://go.microsoft.com/fwlink/p/?LinkId=2124703"
            ));
        }
        #[cfg(not(target_os = "windows"))]
        {
            eprintln!();
            eprintln!("Native window failed: {e:#}");
            gui::show_error(&format!("Reaper window failed:\n\n{e:#}"));
        }
    }
    shutdown_notify.notify_one();
    process_registry::initiate_shutdown();
    gui_result
}
