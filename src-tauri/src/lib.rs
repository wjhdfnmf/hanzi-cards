mod ai;
mod billing;
mod calibration;
mod cards;
mod commands;
mod data;
mod db;
mod decks;
mod dictionary;
mod error;
mod fonts;
mod gpu;
mod hsk;
mod input;
mod models;
mod net;
mod pinyin;
mod pleco;
mod scheduler;
mod speech;
mod stats;
mod stories;
mod study;
mod tone_check;
mod tones;
mod tts;
mod tutor;
mod whisper;
mod whisper_net;

use std::sync::Mutex;
use tauri::Manager;

/// Shared application state: SQLite connections guarded by mutexes. Stories (Reading) are in
/// a database of their own, separate from the cards.
pub struct AppState {
    pub db: Mutex<rusqlite::Connection>,
    pub stories: Mutex<rusqlite::Connection>,
    /// Set when a database couldn't be opened at startup (it's damaged): the app then shows
    /// what happened and offers the backups, instead of the usual pages. Until one is put in
    /// place, that database's connection is an empty stand-in.
    pub startup_problem: Mutex<Option<StartupProblem>>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupProblem {
    /// "cards" or "reading": the database that couldn't be opened.
    pub kind: String,
    /// "damaged" (its backups are offered), "in_use" (another program has it: try again) or
    /// "error" (anything else, such as no permission to read the file).
    pub reason: String,
    pub message: String,
}

impl StartupProblem {
    pub fn new(kind: &str, e: &error::AppError) -> Self {
        let reason = match e {
            error::AppError::Damaged(_) => "damaged",
            error::AppError::InUse(_) => "in_use",
            _ => "error",
        };
        StartupProblem { kind: kind.into(), reason: reason.into(), message: e.to_string() }
    }
}

/// Linux's webview (WebKitGTK) turns the microphone off and refuses every request for it
/// unless the app says otherwise. Spoken answers need it, so allow it for the app's own page.
#[cfg(target_os = "linux")]
fn allow_microphone(app: &tauri::App) {
    use webkit2gtk::{PermissionRequestExt, SettingsExt, UserMediaPermissionRequest, WebViewExt};
    use webkit2gtk::glib::prelude::Cast;
    let Some(window) = app.get_webview_window("main") else { return };
    let _ = window.with_webview(|wv| {
        let view = wv.inner();
        if let Some(settings) = WebViewExt::settings(&view) {
            settings.set_enable_media_stream(true);
        }
        view.connect_permission_request(|_, request| {
            if request.downcast_ref::<UserMediaPermissionRequest>().is_some() {
                request.allow();
                return true;
            }
            false // anything else: WebKitGTK's default (deny)
        });
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // First, so a second copy stops before it opens anything: starting the app again
        // brings the running window forward instead.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            // A database that can't be opened (damaged) doesn't stop the app from starting:
            // it opens on a page that says so and offers the backups (commands.rs).
            let mut problem: Option<StartupProblem> = None;
            // backs the database up first when a new version starts for the first time
            let conn = match data::open_for_version(&dir, env!("CARGO_PKG_VERSION")) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("cards database: {e}");
                    problem = Some(StartupProblem::new("cards", &e));
                    rusqlite::Connection::open_in_memory()?
                }
            };
            gpu::init(&dir); // where downloaded GPU libraries live
            whisper::set_device_preference(
                &db::get_setting(&conn, "whisper_device").ok().flatten().unwrap_or_else(|| "auto".into()),
            );
            let stories = match data::open_reading(&dir) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("reading database: {e}");
                    problem.get_or_insert(StartupProblem::new("reading", &e));
                    rusqlite::Connection::open_in_memory()?
                }
            };
            let healthy = problem.is_none();
            app.manage(AppState {
                db: Mutex::new(conn),
                stories: Mutex::new(stories),
                startup_problem: Mutex::new(problem),
            });
            // the dictionary ships with the app: import it on first start, in the background
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if !healthy {
                    return;
                }
                if let Err(e) = commands::import_bundled_dictionary(handle).await {
                    eprintln!("dictionary import: {e}");
                }
            });
            #[cfg(target_os = "linux")]
            allow_microphone(app);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::startup_problem,
            commands::startup_retry,
            commands::get_settings,
            commands::set_setting,
            commands::data_info,
            commands::data_backup,
            commands::data_delete_backup,
            commands::data_save_copy,
            commands::data_inspect,
            commands::data_use_file,
            commands::data_start_fresh,
            commands::data_show_in_folder,
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
            commands::remove_tags,
            commands::hsk_tag_all,
            commands::cards_by_hanzi,
            commands::import_preview,
            commands::import_cards,
            commands::export_cards,
            commands::pleco_preview,
            commands::import_pleco,
            commands::export_pleco,
            commands::build_session,
            commands::session_counts,
            commands::preview_intervals,
            commands::answer_card,
            commands::speech_status,
            commands::speech_listen,
            commands::speech_cancel,
            commands::speech_check_audio,
            commands::tts_available,
            commands::tts_wav,
            commands::pitch_profile_info,
            commands::pitch_analyze,
            commands::pitch_calibration_save,
            commands::pitch_profile_reset,
            commands::pitch_save_recording,
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
            commands::list_story_folders,
            commands::create_story_folder,
            commands::update_story_folder,
            commands::delete_story_folder,
            commands::list_stories,
            commands::get_story,
            commands::create_story,
            commands::update_story,
            commands::delete_story,
            commands::import_stories,
            commands::import_story_folder,
            commands::export_story,
            commands::export_story_folder,
            commands::story_segments,
            commands::story_total,
            commands::fonts_list,
            commands::font_download,
            commands::font_delete,
            commands::font_data,
        ])
        .build(tauri::generate_context!())
        .expect("error while running Hanzi Cards")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                settle_databases(app);
            }
        });
}

/// On the way out, merge the dictionary's write-ahead log into its file (the cards and
/// reading databases have no log: every change is already in their main files).
fn settle_databases(app: &tauri::AppHandle) {
    let Some(state) = app.try_state::<AppState>() else { return };
    // never wait here: a command still running at exit keeps its lock
    let held = state.db.try_lock();
    if let Ok(conn) = &held {
        let _ = conn.execute_batch("PRAGMA dict.wal_checkpoint(TRUNCATE)");
    }
    drop(held);
}
