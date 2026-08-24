#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

use easy_qemu_core::Manager;
use tauri::{AppHandle, Emitter, Manager as _};

const STATUSES_EVENT: &str = "vm:statuses";

/// Refresh VM statuses in the background and emit the full snapshot.
fn emit_statuses(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<Manager>();
        match state.refresh().await {
            Ok(updates) => {
                let _ = app.emit(STATUSES_EVENT, updates);
            }
            Err(e) => eprintln!("status refresh failed: {e:#}"),
        }
    });
}

async fn poll_loop(app: AppHandle) {
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        emit_statuses(&app);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .on_window_event(|window, event| {
            // Clean up the websockify proxy when a console window is closed.
            if matches!(event, tauri::WindowEvent::Destroyed) {
                let label = window.label();
                if let Some(id) = label.strip_prefix("console_") {
                    let id = id.to_string();
                    let h = window.app_handle().clone();
                    tauri::async_runtime::spawn(async move {
                        use tauri::Manager as _;
                        h.state::<Manager>().close_console(&id).await.ok();
                    });
                }
            }
        })
        .setup(|app| {
            let base = easy_qemu_core::config::default_base();
            let manager = Manager::new(base).map_err(|e| format!("{e:#}"))?;
            app.manage(manager);

            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(poll_loop(app_handle));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_vms,
            commands::get_statuses,
            commands::create_vm,
            commands::update_vm,
            commands::delete_vm,
            commands::start_vm,
            commands::vm_action,
            commands::force_stop,
            commands::open_console,
            commands::close_console,
            commands::read_log,
            commands::snapshot_list,
            commands::snapshot_create,
            commands::snapshot_apply,
            commands::snapshot_delete,
            commands::get_config,
            commands::set_config,
            commands::get_config_warnings,
            commands::pick_path,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Easy QEMU");
}
