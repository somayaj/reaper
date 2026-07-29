//! Native desktop window (WKWebView on macOS, WebView2 on Windows via wry).

#[cfg(any(target_os = "macos", target_os = "windows"))]
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

#[cfg(target_os = "windows")]
pub fn show_error(message: &str) {
    let _ = rfd::MessageDialog::new()
        .set_title("Reaper")
        .set_description(message)
        .set_level(rfd::MessageLevel::Error)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
    eprintln!("{message}");
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
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

#[cfg(any(target_os = "macos", target_os = "windows"))]
enum UserEvent {
    OpenWindow(String),
    ShowWindow(tao::window::WindowId),
    ToggleFullscreen(tao::window::WindowId),
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
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

#[cfg(any(target_os = "macos", target_os = "windows"))]
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

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn parse_ipc_toggle_fullscreen(body: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("type")
                .and_then(|t| t.as_str())
                .map(|t| t == "toggle-fullscreen")
        })
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

    sync_webview_bounds(&window, &webview);
    Ok((window, webview))
}

/// Fit and center the window inside the Windows work area (screen minus taskbar/menu bar)
/// so the full IDE chrome is visible and not cropped.
#[cfg(target_os = "windows")]
fn place_window_in_work_area(window: &tao::window::Window) {
    #[repr(C)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    #[link(name = "user32")]
    extern "system" {
        fn SystemParametersInfoW(
            ui_action: u32,
            ui_param: u32,
            pv_param: *mut core::ffi::c_void,
            f_win_ini: u32,
        ) -> i32;
    }

    const SPI_GETWORKAREA: u32 = 0x0030;
    let mut work = Rect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            &mut work as *mut Rect as *mut core::ffi::c_void,
            0,
        )
    };
    if ok == 0 {
        return;
    }

    let work_w = (work.right - work.left).max(1);
    let work_h = (work.bottom - work.top).max(1);
    let margin = 16i32;

    let outer = window.outer_size();
    let inner = window.inner_size();
    let chrome_w = outer.width.saturating_sub(inner.width) as i32;
    let chrome_h = outer.height.saturating_sub(inner.height) as i32;

    let max_outer_w = (work_w - margin).max(400);
    let max_outer_h = (work_h - margin).max(300);
    let mut outer_w = outer.width as i32;
    let mut outer_h = outer.height as i32;

    if outer_w > max_outer_w || outer_h > max_outer_h {
        outer_w = outer_w.min(max_outer_w);
        outer_h = outer_h.min(max_outer_h);
        let inner_w = (outer_w - chrome_w).max(400) as u32;
        let inner_h = (outer_h - chrome_h).max(300) as u32;
        window.set_inner_size(tao::dpi::PhysicalSize::new(inner_w, inner_h));
        let outer = window.outer_size();
        outer_w = outer.width as i32;
        outer_h = outer.height as i32;
    }

    let x = work.left + ((work_w - outer_w) / 2).max(0);
    // Prefer sitting fully above the taskbar/menu bar (work.bottom).
    let y = work.top + ((work_h - outer_h) / 2).max(0);
    let y = y.min(work.bottom - outer_h).max(work.top);
    let x = x.min(work.right - outer_w).max(work.left);
    window.set_outer_position(tao::dpi::PhysicalPosition::new(x, y));
}

/// Keep the WebView2 surface matched to the native window (required on resize/maximize).
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn sync_webview_bounds(window: &tao::window::Window, webview: &wry::WebView) {
    use tao::dpi::{LogicalPosition, LogicalSize};

    let scale = window.scale_factor();
    let size = window.inner_size().to_logical::<u32>(scale);
    let _ = webview.set_bounds(wry::Rect {
        position: LogicalPosition::new(0, 0).into(),
        size: LogicalSize::new(size.width.max(1), size.height.max(1)).into(),
    });
    let _ = webview.evaluate_script(
        "window.dispatchEvent(new Event('resize'));\
         if(typeof window.__reaperSyncLayout==='function')window.__reaperSyncLayout();",
    );
}

#[cfg(target_os = "windows")]
fn create_window(
    launch: &GuiLaunch,
    protocol_bridge: SharedGuiProtocolBridge,
    event_loop: &tao::event_loop::EventLoopWindowTarget<UserEvent>,
    proxy: tao::event_loop::EventLoopProxy<UserEvent>,
) -> anyhow::Result<(tao::window::Window, wry::WebView)> {
    use tao::window::WindowBuilder;
    use wry::WebViewBuilderExtWindows;
    use wry::{http::Request, PageLoadEvent, WebViewBuilder};

    let title = window_title_from_url(&launch.webview_url);
    // Windows: normal OS title bar (no macOS transparent chrome).
    let mut window_builder = WindowBuilder::new()
        .with_title(title)
        .with_inner_size(tao::dpi::LogicalSize::new(1280.0, 840.0))
        .with_visible(false);
    if let Some(icon) = crate::platform::app_window_icon() {
        window_builder = window_builder.with_window_icon(Some(icon));
    }
    let window = window_builder.build(event_loop)?;
    place_window_in_work_area(&window);

    let window_id = window.id();
    let show_proxy = proxy.clone();
    let ipc_proxy = proxy.clone();
    let popup_proxy = proxy.clone();
    // WebView2: static splash + logo (skip animated harvest layers via CSS).
    let init_script = format!(
        r#"document.documentElement.classList.add('ij-platform-windows');
window.__reaperWindowsSplash=true;
window.__reaperSkipSplash=false;
window.__reaperSplashAt=Date.now();
{}"#,
        launch.init_script
    );
    let webview = WebViewBuilder::new()
        .with_url(&launch.webview_url)
        .with_initialization_script(&init_script)
        .with_background_color((0x2B, 0x2B, 0x2B, 255))
        .with_additional_browser_args(
            "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection \
             --disable-gpu --disable-gpu-compositing --disable-smooth-scrolling",
        )
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

    sync_webview_bounds(&window, &webview);
    Ok((window, webview))
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub fn run(launch: &GuiLaunch, protocol_bridge: SharedGuiProtocolBridge) -> anyhow::Result<()> {
    use std::collections::HashMap;

    use tao::{
        event::{Event, WindowEvent},
        event_loop::{ControlFlow, EventLoopBuilder},
        window::WindowId,
    };

    #[cfg(target_os = "macos")]
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
            Event::WindowEvent { event, window_id, .. } => match event {
                WindowEvent::CloseRequested => {
                    webviews.remove(&window_id);
                    if webviews.is_empty() {
                        *control_flow = ControlFlow::Exit;
                    }
                }
                WindowEvent::Resized(_) => {
                    if let Some((window, webview)) = webviews.get(&window_id) {
                        sync_webview_bounds(window, webview);
                    }
                }
                _ => {}
            }
            Event::UserEvent(UserEvent::ShowWindow(window_id)) => {
                if let Some((window, webview)) = webviews.get(&window_id) {
                    #[cfg(target_os = "windows")]
                    place_window_in_work_area(window);
                    window.set_visible(true);
                    window.set_focus();
                    sync_webview_bounds(window, webview);
                }
            }
            Event::UserEvent(UserEvent::ToggleFullscreen(window_id)) => {
                if let Some((window, webview)) = webviews.get(&window_id) {
                    use tao::window::Fullscreen;
                    let next = if window.fullscreen().is_some() {
                        None
                    } else {
                        Some(Fullscreen::Borderless(None))
                    };
                    window.set_fullscreen(next);
                    sync_webview_bounds(window, webview);
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

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn run(_launch: &GuiLaunch, _protocol_bridge: SharedGuiProtocolBridge) -> anyhow::Result<()> {
    anyhow::bail!("GUI mode is only supported on macOS and Windows")
}
