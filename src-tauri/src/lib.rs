mod ai;
mod billing;
mod cards;
mod commands;
mod db;
mod decks;
mod dictionary;
mod error;
mod gpu;
mod hsk;
mod models;
mod pinyin;
mod pleco;
mod scheduler;
mod speech;
mod stats;
mod study;
mod tone_check;
mod tones;
mod tutor;
mod whisper;
mod whisper_net;

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
            gpu::init(&dir); // where downloaded GPU libraries live
            whisper::set_device_preference(
                &db::get_setting(&conn, "whisper_device").ok().flatten().unwrap_or_else(|| "auto".into()),
            );
            app.manage(AppState {
                db: Mutex::new(conn),
            });
            // the dictionary ships with the app: import it on first start, in the background
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = commands::import_bundled_dictionary(handle).await {
                    eprintln!("dictionary import: {e}");
                }
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
            commands::hsk_tag_all,
            commands::cards_by_hanzi,
            commands::import_preview,
            commands::import_cards,
            commands::export_cards,
            commands::pleco_preview,
            commands::import_pleco,
            commands::export_pleco,
            commands::build_session,
            commands::preview_intervals,
            commands::answer_card,
            commands::speech_status,
            commands::speech_listen,
            commands::speech_cancel,
            commands::speech_check_audio,
            commands::speech_check_english,
            commands::speech_transcribe,
            commands::speech_warmup,
            commands::whisper_models,
            commands::whisper_device_info,
            commands::gpu_libs_download,
            commands::gpu_libs_remove,
            commands::whisper_download,
            commands::whisper_delete,
            commands::ai_key_status,
            commands::ai_key_set,
            commands::ai_key_clear,
            commands::tutor_start,
            commands::tutor_send,
            commands::tutor_transcribe,
            commands::tutor_cancel,
            commands::tutor_end,
            commands::tutor_translate,
            commands::ai_spending,
            commands::ai_spending_export,
            commands::get_stats,
            commands::dict_status,
            commands::dict_search,
            commands::dict_lookup,
            commands::dict_candidates,
            commands::dict_segment,
            commands::dict_import_file,
            commands::dict_download,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Hanzi Cards");
}
