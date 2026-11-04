//! The Polyph desktop shell: one main window running the shared app
//! (`packages/ui`), a tray, the always-on-top status pill, and native
//! notifications (sent by the app through the notification plugin).
//! Closing the main window keeps Polyph in the tray; Quit leaves the daemon
//! running, as the CLI does.

mod daemon;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WindowEvent};

#[tauri::command]
async fn connection() -> Result<daemon::Conn, String> {
    // Starting the daemon can take a few seconds; keep it off the UI thread.
    tauri::async_runtime::spawn_blocking(daemon::ensure).await.map_err(|e| e.to_string())?
}

fn show_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

#[tauri::command]
fn show_main(app: AppHandle) {
    show_main_window(&app);
}

fn toggle_pill_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("pill") {
        if w.is_visible().unwrap_or(false) {
            let _ = w.hide();
        } else {
            let _ = w.show();
        }
    }
}

#[tauri::command]
fn toggle_pill(app: AppHandle) {
    toggle_pill_window(&app);
}

#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    // Only web links: never hand arbitrary strings to the OS opener.
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("not a web link".into());
    }
    #[cfg(target_os = "macos")]
    let mut c = std::process::Command::new("open");
    #[cfg(target_os = "windows")]
    let mut c = {
        let mut c = std::process::Command::new("rundll32");
        c.arg("url.dll,FileProtocolHandler");
        c
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut c = std::process::Command::new("xdg-open");
    c.arg(url).spawn().map(|_| ()).map_err(|e| e.to_string())
}

fn tray(app: &AppHandle) -> tauri::Result<()> {
    let l = polyph_i18n::Localizer::new(&polyph_i18n::detect(None));
    let show = MenuItem::with_id(app, "show", l.t("app-tray-show"), true, None::<&str>)?;
    let pill = MenuItem::with_id(app, "pill", l.t("app-tray-pill"), true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", l.t("app-tray-quit"), true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &pill, &PredefinedMenuItem::separator(app)?, &quit])?;
    let mut b = TrayIconBuilder::with_id("polyph").menu(&menu).tooltip(l.t("app-name"));
    if let Some(icon) = app.default_window_icon() {
        b = b.icon(icon.clone());
    }
    #[cfg(target_os = "macos")]
    {
        b = b.icon_as_template(true);
    }
    b.show_menu_on_left_click(false)
        .on_menu_event(|app, e| match e.id.as_ref() {
            "show" => show_main_window(app),
            "pill" => toggle_pill_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|t, e| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                show_main_window(t.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main_window(app)))
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(tauri::generate_handler![connection, show_main, toggle_pill, open_url])
        .setup(|app| {
            tray(app.handle())?;
            Ok(())
        })
        .on_window_event(|w, e| {
            // Closing the main window keeps Polyph in the tray.
            if let WindowEvent::CloseRequested { api, .. } = e
                && w.label() == "main"
            {
                api.prevent_close();
                let _ = w.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running the Polyph desktop app");
}
