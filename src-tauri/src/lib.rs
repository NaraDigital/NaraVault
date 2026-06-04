mod bridge;
mod commands;
mod crypto;
mod db;
mod error;
mod model;
mod s3;
mod state;
mod totp;

use tauri::{Emitter, Manager};

use crate::state::AppState;

/// Show the launcher if hidden, hide it if already visible (Alt+N toggle).
#[cfg(desktop)]
fn toggle_launcher<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(win) = app.get_webview_window("launcher") {
        if matches!(win.is_visible(), Ok(true)) {
            let _ = win.hide();
        } else {
            let _ = win.show();
            let _ = win.set_focus();
            // Tell the launcher UI to refresh its list + reset to a clean state.
            let _ = win.emit("naravault://launcher-shown", ());
        }
    }
}

/// Bring the main vault window to the foreground (show + unminimize + focus).
#[cfg(desktop)]
fn show_main<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_shell::init());

    #[cfg(desktop)]
    {
        // Single-instance guard MUST be the first plugin: a second launch (e.g. the
        // user reopening NaraVault from Windows search) runs this callback in the
        // ORIGINAL process and then exits, instead of spawning another tray icon.
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main(app);
        }));

        use tauri_plugin_global_shortcut::ShortcutState;
        builder = builder.plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        toggle_launcher(app);
                    }
                })
                .build(),
        );
    }

    builder
        .setup(|app| {
            // Resolve a per-user app data directory and open (or create) the vault DB.
            let dir = app
                .path()
                .app_data_dir()
                .map_err(|e| format!("failed to resolve app data dir: {e}"))?;
            std::fs::create_dir_all(&dir)
                .map_err(|e| format!("failed to create app data dir: {e}"))?;
            let db_path = dir.join("naravault.db");
            let conn = db::open(&db_path)
                .map_err(|e| format!("failed to open vault database: {e}"))?;
            app.manage(AppState::new(conn));

            // Start the loopback autofill bridge for the browser extension. It is
            // best-effort: it reads the live DEK from AppState and serves the
            // native-messaging host on 127.0.0.1 only. The handshake file tells
            // the host which port + token to use.
            #[cfg(desktop)]
            {
                let handshake_path = dir.join("bridge.json");
                bridge::start(app.handle().clone(), handshake_path);
            }

            // Register the global Alt+N hotkey for the quick launcher.
            // Non-fatal: another app may already hold this shortcut.
            #[cfg(desktop)]
            {
                use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};
                let alt_n = Shortcut::new(Some(Modifiers::ALT), Code::KeyN);
                if let Err(e) = app.global_shortcut().register(alt_n) {
                    eprintln!("[naravault] Alt+N shortcut registration failed (non-fatal): {e}");
                }
            }

            // System tray: keeps the app alive in the background so the vault
            // stays unlocked for browser autofill even when the main window is
            // hidden. Left-click or "Open" shows the window; "Quit" fully exits.
            #[cfg(desktop)]
            {
                use tauri::menu::{Menu, MenuItem};
                use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

                let open_item =
                    MenuItem::with_id(app, "open", "Open NaraVault", true, None::<&str>)?;
                let quit_item =
                    MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
                let tray_menu = Menu::with_items(app, &[&open_item, &quit_item])?;

                let icon = app
                    .default_window_icon()
                    .cloned()
                    .ok_or_else(|| "tray: no default window icon configured")?;

                TrayIconBuilder::new()
                    .icon(icon)
                    .tooltip("NaraVault")
                    .menu(&tray_menu)
                    .menu_on_left_click(false)
                    .on_menu_event(|app, event| match event.id.as_ref() {
                        "open" => show_main(app),
                        "quit" => app.exit(0),
                        _ => {}
                    })
                    .on_tray_icon_event(|tray, event| {
                        if let TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } = event
                        {
                            show_main(tray.app_handle());
                        }
                    })
                    .build(app)?;
            }

            Ok(())
        })
        .on_window_event(|window, event| match event {
            // Close button → hide to tray so the vault stays unlocked in the
            // background for browser autofill. The DEK is NOT cleared on hide.
            tauri::WindowEvent::CloseRequested { api, .. } => {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
            // Zeroize the in-memory key only when the process actually exits
            // (triggered by app.exit(0) from the tray "Quit" menu). The launcher
            // hides/shows constantly — it must never trigger a lock.
            tauri::WindowEvent::Destroyed => {
                if window.label() == "main" {
                    if let Some(state) = window.try_state::<AppState>() {
                        state.clear_dek();
                    }
                }
            }
            // Spotlight-style: the launcher dismisses itself when it loses focus.
            tauri::WindowEvent::Focused(false) => {
                if window.label() == "launcher" {
                    let _ = window.hide();
                }
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            commands::vault_status,
            commands::create_vault,
            commands::unlock,
            commands::lock,
            commands::verify_master,
            commands::change_master,
            commands::list_items,
            commands::list_item_meta,
            commands::save_item,
            commands::delete_item,
            commands::reset_vault,
            commands::launcher_open_item,
            commands::autofill_consent_reply,
            commands::ping,
            commands::save_s3_config,
            commands::load_s3_config,
            commands::test_s3_connection,
            commands::export_to_s3,
            commands::list_s3_backups,
            commands::import_from_s3,
            commands::delete_items,
        ])
        .run(tauri::generate_context!())
        .expect("error while running NaraVault");
}
