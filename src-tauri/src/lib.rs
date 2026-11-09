mod cards;
mod commands;
mod db;
mod dictionary;
mod error;
mod models;
mod pinyin;
mod scheduler;
mod stats;
mod study;

use std::sync::Mutex;
use tauri::Manager;

/// Shared application state: one SQLite connection guarded by a mutex.
pub struct AppState {
    pub db: Mutex<rusqlite::Connection>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let conn = db::open(&dir.join("hanzi-cards.db"))?;
            app.manage(AppState {
                db: Mutex::new(conn),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::set_setting,
            commands::list_decks,
            commands::create_deck,
            commands::update_deck,
            commands::delete_deck,
            commands::list_cards,
            commands::create_card,
            commands::update_card,
            commands::delete_cards,
            commands::move_cards,
            commands::list_tags,
            commands::cards_by_hanzi,
            commands::import_cards,
            commands::export_cards,
            commands::build_session,
            commands::preview_intervals,
            commands::answer_card,
            commands::get_stats,
            commands::dict_status,
            commands::dict_search,
            commands::dict_lookup,
            commands::dict_import_file,
            commands::dict_download,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Hanzi Cards");
}
