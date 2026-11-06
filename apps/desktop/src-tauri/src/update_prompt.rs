use crate::updates::{require_main, ReleaseChannel};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};

#[derive(Clone, Deserialize, Serialize)]
pub struct UpdateAction {
    revision: Option<u64>,
    #[serde(flatten)]
    kind: UpdateActionKind,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum UpdateActionKind {
    Ready,
    Cancel,
    Skip,
    Check,
    Download,
    Install,
    ReviewDowngrade,
    ConfirmDowngrade,
    Back,
    OpenDownloads,
    SetChannel { channel: ReleaseChannel },
}

#[tauri::command]
pub async fn sync_update_prompt(
    window: WebviewWindow,
    state: serde_json::Value,
    show: bool,
) -> Result<bool, String> {
    require_main(&window)?;
    let prompt = if let Some(prompt) = window.get_webview_window("update-prompt") {
        prompt
    } else {
        if !show {
            return Ok(false);
        }
        let script = format!("window.__SCOREWING_UPDATE_STATE__ = {};", state);
        let prompt = WebviewWindowBuilder::new(
            window.app_handle(),
            "update-prompt",
            WebviewUrl::App("index.html?update-prompt".into()),
        )
        .title("Scorewing updates")
        .inner_size(620.0, 328.0)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .skip_taskbar(true)
        .visible(false)
        .center()
        .parent(&window)
        .map_err(|error| error.to_string())?
        .initialization_script(script)
        .build()
        .map_err(|error| error.to_string())?;
        let handle = prompt.clone();
        prompt.on_window_event(move |event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = handle.emit_to(
                    "main",
                    "update-prompt-action",
                    UpdateAction {
                        revision: None,
                        kind: UpdateActionKind::Cancel,
                    },
                );
            }
        });
        prompt
    };
    prompt
        .emit("update-prompt-state", state)
        .map_err(|error| error.to_string())?;
    if show {
        prompt
            .show()
            .and_then(|_| prompt.set_focus())
            .map_err(|error| error.to_string())?;
    }
    Ok(true)
}

#[tauri::command]
pub fn hide_update_prompt(window: WebviewWindow) -> Result<(), String> {
    require_main(&window)?;
    if let Some(prompt) = window.get_webview_window("update-prompt") {
        prompt.hide().map_err(|error| error.to_string())?;
    }
    window.set_focus().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn update_prompt_action(window: WebviewWindow, action: UpdateAction) -> Result<(), String> {
    if window.label() != "update-prompt" {
        return Err("Only the update window can send update actions.".into());
    }
    window
        .emit_to("main", "update-prompt-action", action)
        .map_err(|error| error.to_string())
}
