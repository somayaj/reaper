//! WebView ↔ native GUI IPC parsing and open-window list payloads.

pub const MENU_SHOW_WINDOWS_ID: &str = "reaper.windows.show";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IpcFromWeb {
    OpenWindow(String),
    ToggleFullscreen,
    StartWindowDrag,
    ListWindows,
    FocusWindow(u32),
    ShowOpenWindows,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowListEntry {
    pub label: u32,
    pub title: String,
}

pub fn parse_ipc_message(body: &str) -> Option<IpcFromWeb> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    match value.get("type")?.as_str()? {
        "open-repo-window" => {
            let url = value.get("url")?.as_str()?.trim();
            if url.is_empty() {
                None
            } else {
                Some(IpcFromWeb::OpenWindow(url.to_string()))
            }
        }
        "toggle-fullscreen" => Some(IpcFromWeb::ToggleFullscreen),
        "start-window-drag" => Some(IpcFromWeb::StartWindowDrag),
        "list-windows" => Some(IpcFromWeb::ListWindows),
        "focus-window" => {
            let id = value.get("id").and_then(|v| v.as_u64()).map(|n| n as u32)?;
            Some(IpcFromWeb::FocusWindow(id))
        }
        "show-open-windows" => Some(IpcFromWeb::ShowOpenWindows),
        _ => None,
    }
}

pub fn window_title_from_url(url: &str) -> String {
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

pub fn windows_list_json(entries: &[WindowListEntry], requester_label: u32) -> String {
    let mut rows: Vec<_> = entries
        .iter()
        .map(|entry| {
            serde_json::json!({
                "id": entry.label,
                "title": entry.title,
                "current": entry.label == requester_label,
            })
        })
        .collect();
    rows.sort_by_key(|row| row.get("id").and_then(|v| v.as_u64()).unwrap_or(0));
    serde_json::to_string(&rows).unwrap_or_else(|_| "[]".into())
}

pub fn deliver_windows_list_script(json: &str) -> String {
    format!("window.__reaperDeliverWindowsList?.({json})")
}

pub fn show_windows_picker_script(json: &str) -> String {
    format!("window.__reaperShowWindowsPicker?.({json})")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_start_window_drag_ipc() {
        assert_eq!(
            parse_ipc_message(r#"{"type":"start-window-drag"}"#),
            Some(IpcFromWeb::StartWindowDrag)
        );
    }

    #[test]
    fn parse_list_and_focus_windows_ipc() {
        assert_eq!(
            parse_ipc_message(r#"{"type":"list-windows"}"#),
            Some(IpcFromWeb::ListWindows)
        );
        assert_eq!(
            parse_ipc_message(r#"{"type":"focus-window","id":2}"#),
            Some(IpcFromWeb::FocusWindow(2))
        );
        assert_eq!(
            parse_ipc_message(r#"{"type":"show-open-windows"}"#),
            Some(IpcFromWeb::ShowOpenWindows)
        );
    }

    #[test]
    fn parse_open_repo_window_requires_url() {
        assert_eq!(
            parse_ipc_message(r#"{"type":"open-repo-window","url":"https://x/?repo=a"}"#),
            Some(IpcFromWeb::OpenWindow("https://x/?repo=a".into()))
        );
        assert_eq!(
            parse_ipc_message(r#"{"type":"open-repo-window","url":"  "#),
            None
        );
    }

    #[test]
    fn window_title_from_repo_query() {
        assert_eq!(
            window_title_from_url("https://127.0.0.1:9443/?repo=my-crate"),
            "Reaper — my-crate"
        );
        assert_eq!(window_title_from_url("https://127.0.0.1:9443/"), "Reaper");
    }

    #[test]
    fn windows_list_json_marks_requester_and_sorts() {
        let json = windows_list_json(
            &[
                WindowListEntry {
                    label: 3,
                    title: "Reaper — c".into(),
                },
                WindowListEntry {
                    label: 1,
                    title: "Reaper — a".into(),
                },
                WindowListEntry {
                    label: 2,
                    title: "Reaper — b".into(),
                },
            ],
            2,
        );
        let rows: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0]["id"], 1);
        assert_eq!(rows[1]["id"], 2);
        assert_eq!(rows[2]["id"], 3);
        assert_eq!(rows[0]["current"], false);
        assert_eq!(rows[1]["current"], true);
        assert_eq!(rows[2]["current"], false);
    }

    #[test]
    fn evaluate_scripts_call_frontend_hooks() {
        let json = r#"[{"id":1,"title":"Reaper","current":true}]"#;
        assert_eq!(
            deliver_windows_list_script(json),
            "window.__reaperDeliverWindowsList?.([{\"id\":1,\"title\":\"Reaper\",\"current\":true}])"
        );
        assert_eq!(
            show_windows_picker_script(json),
            "window.__reaperShowWindowsPicker?.([{\"id\":1,\"title\":\"Reaper\",\"current\":true}])"
        );
    }
}
