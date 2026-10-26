use std::sync::Arc;
use tauri::{Emitter, Manager};

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let resources = if cfg!(debug_assertions) {
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .unwrap()
                    .join("resources")
            } else {
                app.path().resource_dir()?
            };
            let handle = app.handle().clone();
            let service = service::Service::new(
                resources,
                app.path().app_local_data_dir()?,
                app.path().download_dir()?,
                move |name, payload| {
                    let _ = handle.emit_to("main", name, payload);
                },
            )?;
            service.start();
            updater::spawn(service.clone());
            app.manage(service);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::snapshot,
            commands::preview_output,
            commands::save_settings,
            commands::control_job,
            commands::reorder_jobs,
            commands::enqueue,
            commands::discover,
            commands::cancel_discovery,
            commands::inspect_media,
            commands::thumbnail,
            commands::open_youtube,
            commands::open_output,
            commands::preview,
            commands::check_update,
            commands::shutdown
        ])
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    if let Some(service) = window.try_state::<Arc<service::Service>>() {
                        let active = service
                            .queue
                            .lock()
                            .unwrap()
                            .jobs
                            .iter()
                            .any(|j| j.active() || j.state == "queued");
                        if active {
                            api.prevent_close();
                            let _ = window.emit("close-request", ());
                        }
                    }
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("Impossibile avviare Frame");
}
pub mod commands;
pub mod files;
pub mod model;
pub mod process;
pub mod queue;
pub mod service;
pub mod store;
pub mod updater;
pub mod urls;
