//! macOS native window (WKWebView via wry).

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
pub fn run(url: &str) -> anyhow::Result<()> {
    use tao::platform::run_return::EventLoopExtRunReturn;
    use tao::{
        event::{Event, WindowEvent},
        event_loop::{ControlFlow, EventLoopBuilder},
        window::WindowBuilder,
    };
    use wry::WebViewBuilder;

    let _menu = install_macos_menu();

    let mut event_loop = EventLoopBuilder::new().build();
    let window = WindowBuilder::new()
        .with_title("Reaper")
        .with_inner_size(tao::dpi::LogicalSize::new(1280.0, 840.0))
        .build(&event_loop)?;

    let _webview = WebViewBuilder::new()
        .with_url(url)
        .build(&window)?;

    event_loop.run_return(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            *control_flow = ControlFlow::Exit;
        }
    });

    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn run(_url: &str) -> anyhow::Result<()> {
    anyhow::bail!("GUI mode is only supported on macOS")
}
