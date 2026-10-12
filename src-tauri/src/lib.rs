//! Database Manager desktop Menzies: Tauri entry, catalog, and drivers.

mod catalog;
mod commands;
mod drivers;
mod error;
mod models;
mod pack;
mod paths;
mod secrets;

use commands::AppState;
use std::sync::Arc;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            if let Err(err) = paths::ensure_app_dirs() {
                log::warn!("could not create Menzies app dirs: {err}");
            }

            let catalog =
                tauri::async_runtime::block_on(catalog::Catalog::open()).map_err(|err| {
                    log::error!("catalog open failed: {err}");
                    std::io::Error::other(err.to_string())
                })?;
            let state = Arc::new(AppState {
                catalog: Arc::new(tokio::sync::RwLock::new(catalog)),
                pool: Arc::new(tokio::sync::RwLock::new(drivers::SessionPool::new())),
            });
            app.manage(state);

            #[cfg(desktop)]
            {
                app.handle()
                    .plugin(tauri_plugin_updater::Builder::new().build())?;
            }

            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            paths::app_paths,
            commands::app_paths_cmd,
            commands::list_connections,
            commands::save_connection,
            commands::delete_connection,
            commands::get_connection,
            commands::test_connection,
            commands::open_session,
            commands::close_session,
            commands::list_tree,
            commands::describe_table,
            commands::run_query,
            commands::table_page,
            commands::apply_edits,
            commands::get_erd,
            commands::generate_ddl,
            commands::compare_schemas,
            commands::list_history,
            commands::list_bookmarks,
            commands::add_bookmark,
            commands::delete_bookmark,
            commands::get_settings,
            commands::save_settings,
            commands::export_results,
            commands::export_connection_pack,
            commands::import_connection_pack,
            commands::export_backup,
            commands::result_diff,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|err| {
            eprintln!("error while running tauri application: {err}");
            std::process::exit(1);
        });
}
