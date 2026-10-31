use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
    Emitter, Manager, State,
};

pub struct NativeMenu {
    open: MenuItem<tauri::Wry>,
    close: MenuItem<tauri::Wry>,
    updates: MenuItem<tauri::Wry>,
}

pub fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let menu = Menu::default(app.handle())?;
    let open = MenuItem::with_id(app, "open-score", "Open score…", false, Some("CmdOrCtrl+O"))?;
    let close = MenuItem::with_id(
        app,
        "close-score",
        "Close score",
        false,
        Some("CmdOrCtrl+Shift+W"),
    )?;
    let updates = MenuItem::with_id(
        app,
        "check-updates",
        "Check for updates…",
        false,
        None::<&str>,
    )?;
    let separator = PredefinedMenuItem::separator(app)?;
    let file = menu
        .items()?
        .into_iter()
        .filter_map(|item| item.as_submenu().cloned())
        .find(|submenu| submenu.text().is_ok_and(|text| text == "File"));
    if let Some(file) = file {
        file.insert_items(&[&open, &close, &separator], 0)?;
    } else {
        menu.insert(
            &Submenu::with_items(
                app,
                "File",
                true,
                &[
                    &open,
                    &close,
                    &separator,
                    &PredefinedMenuItem::quit(app, None)?,
                ],
            )?,
            0,
        )?;
    }

    #[cfg(target_os = "macos")]
    {
        let items = menu.items()?;
        let application = items
            .first()
            .and_then(|item| item.as_submenu())
            .ok_or("The native application menu is unavailable.")?;
        application.insert_items(&[&updates, &PredefinedMenuItem::separator(app)?], 2)?;
    }
    #[cfg(not(target_os = "macos"))]
    {
        let help = menu
            .get(tauri::menu::HELP_SUBMENU_ID)
            .ok_or("The native Help menu is unavailable.")?;
        help.as_submenu()
            .ok_or("The native Help menu is unavailable.")?
            .insert_items(&[&updates, &PredefinedMenuItem::separator(app)?], 0)?;
    }

    app.set_menu(menu)?;
    app.manage(NativeMenu {
        open,
        close,
        updates,
    });
    app.on_menu_event(|app, event| {
        let action = event.id().as_ref();
        if matches!(action, "open-score" | "close-score" | "check-updates") {
            if let Some(window) = app.get_webview_window("main") {
                if let Err(error) = window.emit("menu-action", action) {
                    eprintln!("Could not dispatch menu action: {error}");
                }
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub fn set_file_menu_state(
    menu: State<'_, NativeMenu>,
    open_enabled: bool,
    close_enabled: bool,
) -> Result<(), String> {
    menu.open
        .set_enabled(open_enabled)
        .map_err(|error| error.to_string())?;
    menu.close
        .set_enabled(close_enabled)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn set_update_menu_state(
    menu: State<'_, NativeMenu>,
    text: String,
    enabled: bool,
) -> Result<(), String> {
    menu.updates
        .set_text(text)
        .map_err(|error| error.to_string())?;
    menu.updates
        .set_enabled(enabled)
        .map_err(|error| error.to_string())
}
