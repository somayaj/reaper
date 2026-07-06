//! macOS native window (WKWebView via wry).

use std::sync::Arc;

use crate::web::SharedGuiProtocolBridge;

pub struct GuiLaunch {
    pub webview_url: String,
    pub init_script: String,
}

#[cfg(target_os = "macos")]
pub fn show_error(message: &str) {
    let escaped = message.replace('\\', "\\\\").replace('"', "\\\"");
    let _ = std::process::Command::new("osascript")
        .arg("-e")
        .arg(format!(
            "display dialog \"{escaped}\" with title \"Reaper\" buttons {{\"OK\"}} default button \"OK\" with icon stop"
        ))
        .status();
    eprintln!("{message}");
}

#[cfg(not(target_os = "macos"))]
pub fn show_error(message: &str) {
    eprintln!("{message}");
}

#[cfg(target_os = "macos")]
fn install_macos_menu() -> muda::Menu {
    use muda::{AboutMetadata, Menu, PredefinedMenuItem, Submenu};

    let menu = Menu::new();

    let app = Submenu::new("Reaper", true);
    let _ = app.append_items(&[
        &PredefinedMenuItem::about(
            None,
            Some(AboutMetadata {
                name: Some("Reaper".into()),
                version: Some(env!("CARGO_PKG_VERSION").into()),
                copyright: Some("Copyright (c) 2026 Asha Somayajula".into()),
                ..Default::default()
            }),
        ),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::hide(None),
        &PredefinedMenuItem::hide_others(None),
        &PredefinedMenuItem::show_all(None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::quit(Some("Quit Reaper")),
    ]);

    let edit = Submenu::new("Edit", true);
    let _ = edit.append_items(&[
        &PredefinedMenuItem::undo(None),
        &PredefinedMenuItem::redo(None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::cut(None),
        &PredefinedMenuItem::copy(None),
        &PredefinedMenuItem::paste(None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::select_all(None),
    ]);

    let _ = menu.append_items(&[&app, &edit]);
    menu.init_for_nsapp();
    menu
}

#[cfg(target_os = "macos")]
enum UserEvent {
    OpenWindow(String),
    ShowWindow(tao::window::WindowId),
    ToggleFullscreen(tao::window::WindowId),
}

#[cfg(target_os = "macos")]
fn window_title_from_url(url: &str) -> String {
    if let Ok(parsed) = url::Url::parse(url) {
        if let Some(repo) = parsed
            .query_pairs()
            .find(|(key, _)| key == "repo")
            .map(|(_, value)| value.into_owned())
            .filter(|name| !name.is_empty())
        {
            return format!("Reaper — {repo}");
        }
    }
    "Reaper".to_string()
}

#[cfg(target_os = "macos")]
fn parse_ipc_open_url(body: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    if value.get("type")?.as_str()? != "open-repo-window" {
        return None;
    }
    let url = value.get("url")?.as_str()?.trim();
    if url.is_empty() {
        None
    } else {
        Some(url.to_string())
    }
}

fn parse_ipc_toggle_fullscreen(body: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| value.get("type").and_then(|t| t.as_str()).map(|t| t == "toggle-fullscreen"))
        .unwrap_or(false)
}

#[cfg(target_os = "macos")]
fn create_window(
    launch: &GuiLaunch,
    protocol_bridge: SharedGuiProtocolBridge,
    event_loop: &tao::event_loop::EventLoopWindowTarget<UserEvent>,
    proxy: tao::event_loop::EventLoopProxy<UserEvent>,
) -> anyhow::Result<(tao::window::Window, wry::WebView)> {
    use tao::platform::macos::WindowBuilderExtMacOS;
    use tao::window::WindowBuilder;
    use wry::{http::Request, PageLoadEvent, WebViewBuilder};

    let title = window_title_from_url(&launch.webview_url);
    let window = WindowBuilder::new()
        .with_title(title)
        .with_inner_size(tao::dpi::LogicalSize::new(1280.0, 840.0))
        .with_visible(false)
        .with_background_color((0, 0, 0, 0))
        .with_titlebar_transparent(true)
        .with_title_hidden(true)
        .with_fullsize_content_view(true)
        .build(event_loop)?;

    let window_id = window.id();
    let show_proxy = proxy.clone();
    let ipc_proxy = proxy.clone();
    let popup_proxy = proxy.clone();
    let init_script = format!(
        "document.documentElement.classList.add('ij-native-titlebar');\n\
         document.documentElement.style.backgroundColor='transparent';\n\
         document.body.style.backgroundColor='transparent';\n{}",
        launch.init_script
    );
    let webview = WebViewBuilder::new()
        .with_url(&launch.webview_url)
        .with_transparent(true)
        .with_background_color((0, 0, 0, 0))
        .with_initialization_script(&init_script)
        .with_asynchronous_custom_protocol(
            crate::web::SCHEME.into(),
            move |_webview_id, request, responder| {
                let bridge = Arc::clone(&protocol_bridge);
                std::thread::spawn(move || {
                    let response = bridge.dispatch_sync(request);
                    responder.respond(response);
                });
            },
        )
        .with_on_page_load_handler(move |event, _loaded_url| {
            if matches!(event, PageLoadEvent::Finished) {
                let _ = show_proxy.send_event(UserEvent::ShowWindow(window_id));
            }
        })
        .with_ipc_handler(move |req: Request<String>| {
            if let Some(next_url) = parse_ipc_open_url(req.body()) {
                let _ = ipc_proxy.send_event(UserEvent::OpenWindow(next_url));
            } else if parse_ipc_toggle_fullscreen(req.body()) {
                let _ = ipc_proxy.send_event(UserEvent::ToggleFullscreen(window_id));
            }
        })
        .with_new_window_req_handler(move |target_url| {
            let _ = popup_proxy.send_event(UserEvent::OpenWindow(target_url));
            false
        })
        .build(&window)?;

    Ok((window, webview))
}

#[cfg(target_os = "macos")]
pub fn run(launch: &GuiLaunch, protocol_bridge: SharedGuiProtocolBridge) -> anyhow::Result<()> {
    use std::collections::HashMap;

    use tao::{
        event::{Event, WindowEvent},
        event_loop::{ControlFlow, EventLoopBuilder},
        window::WindowId,
    };

    let _menu = install_macos_menu();

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let mut webviews: HashMap<WindowId, (tao::window::Window, wry::WebView)> = HashMap::new();

    let protocol_bridge_loop = Arc::clone(&protocol_bridge);
    let child_init_script = launch.init_script.clone();

    let (window, webview) = create_window(launch, protocol_bridge, &event_loop, proxy.clone())?;
    webviews.insert(window.id(), (window, webview));

    event_loop.run(move |event, event_loop, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                window_id,
                ..
            } => {
                webviews.remove(&window_id);
                if webviews.is_empty() {
                    *control_flow = ControlFlow::Exit;
                }
            }
            Event::UserEvent(UserEvent::ShowWindow(window_id)) => {
                if let Some((window, _)) = webviews.get(&window_id) {
                    window.set_visible(true);
                }
            }
            Event::UserEvent(UserEvent::ToggleFullscreen(window_id)) => {
                if let Some((window, _)) = webviews.get(&window_id) {
                    use tao::window::Fullscreen;
                    let next = if window.fullscreen().is_some() {
                        None
                    } else {
                        Some(Fullscreen::Borderless(None))
                    };
                    window.set_fullscreen(next);
                }
            }
            Event::UserEvent(UserEvent::OpenWindow(next_url)) => {
                let child_launch = GuiLaunch {
                    webview_url: next_url,
                    init_script: child_init_script.clone(),
                };
                match create_window(
                    &child_launch,
                    Arc::clone(&protocol_bridge_loop),
                    event_loop,
                    proxy.clone(),
                ) {
                    Ok((window, webview)) => {
                        webviews.insert(window.id(), (window, webview));
                    }
                    Err(e) => tracing::error!("Failed to open Reaper window: {e:#}"),
                }
            }
            _ => {}
        }
    });

    #[allow(unreachable_code)]
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn run(_launch: &GuiLaunch, _protocol_bridge: SharedGuiProtocolBridge) -> anyhow::Result<()> {
    anyhow::bail!("GUI mode is only supported on macOS")
}
