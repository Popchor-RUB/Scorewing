use serde::{Deserialize, Serialize};
use tauri::{Listener, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UpdateChoice {
    Cancel,
    Skip,
    Install,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateDetails {
    current_version: String,
    version: String,
    can_install: bool,
}

#[tauri::command]
pub async fn show_update_prompt(
    window: WebviewWindow,
    current_version: String,
    version: String,
    can_install: bool,
) -> Result<UpdateChoice, String> {
    if window.label() != "main" {
        return Err("Only the main window can check for updates.".into());
    }
    let details = UpdateDetails {
        current_version,
        version,
        can_install,
    };
    let script = format!(
        "window.__SCOREWING_UPDATE_PROMPT__ = {};",
        serde_json::to_string(&details).map_err(|error| error.to_string())?
    );
    let prompt = WebviewWindowBuilder::new(
        window.app_handle(),
        "update-prompt",
        WebviewUrl::App("index.html?update-prompt".into()),
    )
    .title("Scorewing update available")
    .inner_size(620.0, 248.0)
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

    let (sender, mut receiver) = tauri::async_runtime::channel(1);
    let closed = sender.clone();
    prompt.on_window_event(move |event| {
        if matches!(
            event,
            WindowEvent::CloseRequested { .. } | WindowEvent::Destroyed
        ) {
            let _ = closed.try_send(UpdateChoice::Cancel);
        }
    });
    let listener = prompt.listen("update-prompt-choice", move |event| {
        if let Ok(choice) = serde_json::from_str::<UpdateChoice>(event.payload()) {
            if !matches!(choice, UpdateChoice::Install) || can_install {
                let _ = sender.try_send(choice);
            }
        }
    });
    let shown = window
        .set_enabled(false)
        .and_then(|_| prompt.show())
        .and_then(|_| prompt.set_focus());
    let result = match shown {
        Ok(()) => receiver
            .recv()
            .await
            .ok_or_else(|| "The update dialog closed unexpectedly.".into()),
        Err(error) => Err(error.to_string()),
    };
    prompt.unlisten(listener);
    // Closing the title bar may have already destroyed the window.
    let _ = prompt.destroy();
    window
        .set_enabled(true)
        .map_err(|error| error.to_string())?;
    let _ = window.set_focus();
    result
}
