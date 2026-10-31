//! macOS native window (WKWebView via wry).

use std::sync::Arc;

use crate::web::SharedGuiProtocolBridge;

pub struct GuiLaunch {
    pub webview_url: String,
    pub init_script: String,
}

#[cfg(target_os = "macos")]
const MENU_SHOW_WINDOWS_ID: &str = crate::gui_ipc::MENU_SHOW_WINDOWS_ID;

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
    use muda::{AboutMetadata, Menu, MenuItem, PredefinedMenuItem, Submenu};

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

    let window = Submenu::new("Window", true);
    let show_windows = MenuItem::with_id(MENU_SHOW_WINDOWS_ID, "Show Open Windows…", true, None);
    let _ = window.append_items(&[
        &show_windows,
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::minimize(None),
        &PredefinedMenuItem::maximize(None),
        &PredefinedMenuItem::bring_all_to_front(None),
    ]);

    let _ = menu.append_items(&[&app, &edit, &window]);
    menu.init_for_nsapp();
    menu
}

#[cfg(target_os = "macos")]
enum UserEvent {
    OpenWindow(String),
    ShowWindow(tao::window::WindowId),
    ToggleFullscreen(tao::window::WindowId),
    StartWindowDrag(tao::window::WindowId),
    DeliverWindowsList {
        requester: tao::window::WindowId,
    },
    FocusWindow {
        label: u32,
    },
    ShowOpenWindowsPicker,
    ShowOpenWindowsPickerFor {
        requester: tao::window::WindowId,
    },
    MacMenu(muda::MenuId),
}

#[cfg(target_os = "macos")]
struct OpenWindow {
    label: u32,
    title: String,
    window: tao::window::Window,
    webview: wry::WebView,
}

#[cfg(target_os = "macos")]
fn windows_list_json(
    webviews: &std::collections::HashMap<tao::window::WindowId, OpenWindow>,
    requester: tao::window::WindowId,
) -> String {
    let requester_label = webviews
        .get(&requester)
        .map(|entry| entry.label)
        .unwrap_or(0);
    let entries: Vec<crate::gui_ipc::WindowListEntry> = webviews
        .values()
        .map(|entry| crate::gui_ipc::WindowListEntry {
            label: entry.label,
            title: entry.title.clone(),
        })
        .collect();
    crate::gui_ipc::windows_list_json(&entries, requester_label)
}

#[cfg(target_os = "macos")]
fn deliver_windows_list(
    webviews: &std::collections::HashMap<tao::window::WindowId, OpenWindow>,
    requester: tao::window::WindowId,
) {
    let Some(entry) = webviews.get(&requester) else {
        return;
    };
    let json = windows_list_json(webviews, requester);
    let script = crate::gui_ipc::deliver_windows_list_script(&json);
    if let Err(e) = entry.webview.evaluate_script(&script) {
        tracing::warn!("Failed to deliver window list to webview: {e:#}");
    }
}

#[cfg(target_os = "macos")]
fn show_open_windows_picker(
    webviews: &std::collections::HashMap<tao::window::WindowId, OpenWindow>,
    preferred: Option<tao::window::WindowId>,
) {
    let target = preferred
        .and_then(|id| webviews.get(&id).map(|entry| (id, entry)))
        .or_else(|| {
            webviews
                .iter()
                .find(|(_, entry)| entry.window.is_focused())
                .map(|(id, entry)| (*id, entry))
        })
        .or_else(|| webviews.iter().next().map(|(id, entry)| (*id, entry)));
    let Some((window_id, entry)) = target else {
        return;
    };
    let json = windows_list_json(webviews, window_id);
    let script = crate::gui_ipc::show_windows_picker_script(&json);
    if let Err(e) = entry.webview.evaluate_script(&script) {
        tracing::warn!("Failed to open window picker in webview: {e:#}");
    }
}

#[cfg(target_os = "macos")]
fn create_window(
    launch: &GuiLaunch,
    window_label: u32,
    protocol_bridge: SharedGuiProtocolBridge,
    event_loop: &tao::event_loop::EventLoopWindowTarget<UserEvent>,
    proxy: tao::event_loop::EventLoopProxy<UserEvent>,
) -> anyhow::Result<OpenWindow> {
    use tao::platform::macos::WindowBuilderExtMacOS;
    use tao::window::WindowBuilder;
    use wry::{http::Request, PageLoadEvent, WebViewBuilder};

    let title = crate::gui_ipc::window_title_from_url(&launch.webview_url);
    let window = WindowBuilder::new()
        .with_title(title.clone())
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
        "window.__reaperWindowLabel = {window_label};\n\
         document.documentElement.classList.add('ij-native-titlebar');\n\
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
            // Show as soon as HTML/splash is parsing — don't wait for Monaco and other late assets.
            if matches!(event, PageLoadEvent::Started) {
                let _ = show_proxy.send_event(UserEvent::ShowWindow(window_id));
            }
        })
        .with_ipc_handler(move |req: Request<String>| {
            let Some(message) = crate::gui_ipc::parse_ipc_message(req.body()) else {
                return;
            };
            match message {
                crate::gui_ipc::IpcFromWeb::OpenWindow(next_url) => {
                    let _ = ipc_proxy.send_event(UserEvent::OpenWindow(next_url));
                }
                crate::gui_ipc::IpcFromWeb::ToggleFullscreen => {
                    let _ = ipc_proxy.send_event(UserEvent::ToggleFullscreen(window_id));
                }
                crate::gui_ipc::IpcFromWeb::StartWindowDrag => {
                    let _ = ipc_proxy.send_event(UserEvent::StartWindowDrag(window_id));
                }
                crate::gui_ipc::IpcFromWeb::ListWindows => {
                    let _ = ipc_proxy.send_event(UserEvent::DeliverWindowsList {
                        requester: window_id,
                    });
                }
                crate::gui_ipc::IpcFromWeb::FocusWindow(label) => {
                    let _ = ipc_proxy.send_event(UserEvent::FocusWindow { label });
                }
                crate::gui_ipc::IpcFromWeb::ShowOpenWindows => {
                    let _ = ipc_proxy.send_event(UserEvent::ShowOpenWindowsPickerFor {
                        requester: window_id,
                    });
                }
            }
        })
        .with_new_window_req_handler(move |target_url| {
            let _ = popup_proxy.send_event(UserEvent::OpenWindow(target_url));
            false
        })
        .build(&window)?;

    Ok(OpenWindow {
        label: window_label,
        title,
        window,
        webview,
    })
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
    let menu_proxy = proxy.clone();
    muda::MenuEvent::set_event_handler(Some(move |event: muda::MenuEvent| {
        let _ = menu_proxy.send_event(UserEvent::MacMenu(event.id().clone()));
    }));

    let mut webviews: HashMap<WindowId, OpenWindow> = HashMap::new();
    let mut next_window_label = 1u32;

    let protocol_bridge_loop = Arc::clone(&protocol_bridge);
    let child_init_script = launch.init_script.clone();

    let initial = create_window(
        launch,
        next_window_label,
        protocol_bridge,
        &event_loop,
        proxy.clone(),
    )?;
    next_window_label += 1;
    webviews.insert(initial.window.id(), initial);

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
                if let Some(entry) = webviews.get(&window_id) {
                    entry.window.set_visible(true);
                    entry.window.set_focus();
                }
            }
            Event::UserEvent(UserEvent::ToggleFullscreen(window_id)) => {
                if let Some(entry) = webviews.get(&window_id) {
                    use tao::window::Fullscreen;
                    let next = if entry.window.fullscreen().is_some() {
                        None
                    } else {
                        Some(Fullscreen::Borderless(None))
                    };
                    entry.window.set_fullscreen(next);
                }
            }
            Event::UserEvent(UserEvent::StartWindowDrag(window_id)) => {
                if let Some(entry) = webviews.get(&window_id) {
                    if let Err(e) = entry.window.drag_window() {
                        tracing::warn!("Window drag failed: {e:?}");
                    }
                }
            }
            Event::UserEvent(UserEvent::DeliverWindowsList { requester }) => {
                deliver_windows_list(&webviews, requester);
            }
            Event::UserEvent(UserEvent::FocusWindow { label }) => {
                if let Some((_, entry)) = webviews.iter().find(|(_, w)| w.label == label) {
                    entry.window.set_visible(true);
                    entry.window.set_focus();
                }
            }
            Event::UserEvent(UserEvent::ShowOpenWindowsPicker) => {
                show_open_windows_picker(&webviews, None);
            }
            Event::UserEvent(UserEvent::ShowOpenWindowsPickerFor { requester }) => {
                show_open_windows_picker(&webviews, Some(requester));
            }
            Event::UserEvent(UserEvent::MacMenu(id)) => {
                if id.0 == MENU_SHOW_WINDOWS_ID {
                    show_open_windows_picker(&webviews, None);
                }
            }
            Event::UserEvent(UserEvent::OpenWindow(next_url)) => {
                let child_launch = GuiLaunch {
                    webview_url: next_url,
                    init_script: child_init_script.clone(),
                };
                match create_window(
                    &child_launch,
                    next_window_label,
                    Arc::clone(&protocol_bridge_loop),
                    event_loop,
                    proxy.clone(),
                ) {
                    Ok(entry) => {
                        next_window_label += 1;
                        webviews.insert(entry.window.id(), entry);
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
