//! Tauri commands: the IPC surface used by the Svelte front end.

use crate::error::{AppError, AppResult};
use crate::models::*;
use crate::stats::Stats;
use crate::{cards, db, decks, dictionary, pleco, stats, study, AppState};
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::MutexGuard;
use tauri::{AppHandle, Manager, State};

fn lock<'a>(state: &'a State<AppState>) -> MutexGuard<'a, Connection> {
    // A panic while holding the lock leaves the connection usable; recover it.
    state.db.lock().unwrap_or_else(|e| e.into_inner())
}

/// Run blocking work (file IO, network, big imports) off the main thread.
async fn blocking<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&mut Connection) -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let mut conn = state.db.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut conn)
    })
    .await
    .map_err(|e| AppError::msg(e.to_string()))?
}

// ---------- settings ----------

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> AppResult<HashMap<String, String>> {
    let conn = lock(&state);
    let mut map: HashMap<String, String> = [
        ("language", "en".to_string()),
        ("theme", "system".to_string()),
        ("desired_retention", db::DEFAULT_RETENTION.to_string()),
        ("new_per_day", db::DEFAULT_NEW_PER_DAY.to_string()),
        ("tone_colors", "true".to_string()),
        ("speech_rate", "0.9".to_string()),
        ("autoplay_audio", "false".to_string()),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect();
    let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
    for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (k, v) = row?;
        map.insert(k, v);
    }
    Ok(map)
}

#[tauri::command]
pub fn set_setting(state: State<AppState>, key: String, value: String) -> AppResult<()> {
    Ok(db::set_setting(&lock(&state), &key, &value)?)
}

// ---------- decks ----------

#[tauri::command]
pub fn list_decks(state: State<AppState>) -> AppResult<Vec<Deck>> {
    decks::list_decks(&lock(&state))
}

#[tauri::command]
pub fn create_deck(
    state: State<AppState>,
    name: String,
    description: String,
    parent_id: Option<i64>,
) -> AppResult<i64> {
    decks::create_deck(&lock(&state), &name, &description, parent_id)
}

#[tauri::command]
pub fn update_deck(
    state: State<AppState>,
    id: i64,
    name: String,
    description: String,
    parent_id: Option<i64>,
) -> AppResult<()> {
    decks::update_deck(&lock(&state), id, &name, &description, parent_id)
}

#[tauri::command]
pub fn delete_deck(state: State<AppState>, id: i64) -> AppResult<()> {
    decks::delete_deck(&mut lock(&state), id)
}

// ---------- cards ----------

#[tauri::command]
pub fn list_cards(state: State<AppState>, deck_id: Option<i64>, search: Option<String>) -> AppResult<Vec<CardRow>> {
    cards::list_cards(&lock(&state), deck_id, search.as_deref().unwrap_or(""))
}

#[tauri::command]
pub fn create_card(state: State<AppState>, input: CardInput) -> AppResult<Card> {
    cards::create_card(&lock(&state), &input)
}

#[tauri::command]
pub fn update_card(state: State<AppState>, id: i64, input: CardInput) -> AppResult<Card> {
    cards::update_card(&lock(&state), id, &input)
}

#[tauri::command]
pub fn delete_cards(state: State<AppState>, ids: Vec<i64>) -> AppResult<()> {
    cards::delete_cards(&mut lock(&state), &ids)
}

#[tauri::command]
pub fn move_cards(state: State<AppState>, ids: Vec<i64>, deck_id: i64) -> AppResult<()> {
    cards::move_cards(&mut lock(&state), &ids, deck_id)
}

#[tauri::command]
pub fn list_tags(state: State<AppState>) -> AppResult<Vec<String>> {
    cards::list_tags(&lock(&state))
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardWithDeck {
    pub card: Card,
    pub deck_name: String,
}

#[tauri::command]
pub fn cards_by_hanzi(state: State<AppState>, hanzi: String) -> AppResult<Vec<CardWithDeck>> {
    Ok(cards::cards_by_hanzi(&lock(&state), &hanzi)?
        .into_iter()
        .map(|(card, deck_name)| CardWithDeck { card, deck_name })
        .collect())
}

#[tauri::command]
pub async fn import_preview(app: AppHandle, deck_id: i64, path: String) -> AppResult<ImportPreview> {
    blocking(app, move |conn| {
        let text = cards::read_text(&PathBuf::from(path))?;
        cards::preview_csv(conn, deck_id, &text)
    })
    .await
}

#[tauri::command]
pub async fn import_cards(
    app: AppHandle,
    deck_id: i64,
    path: String,
    include_other_decks: Option<bool>,
) -> AppResult<ImportResult> {
    blocking(app, move |conn| {
        let text = cards::read_text(&PathBuf::from(path))?;
        cards::import_text(conn, deck_id, &text, include_other_decks.unwrap_or(false))
    })
    .await
}

#[tauri::command]
pub async fn pleco_preview(app: AppHandle, path: String, default_deck: Option<i64>) -> AppResult<PlecoPreview> {
    blocking(app, move |conn| {
        let cards = pleco::read_file(&PathBuf::from(path))?;
        pleco::preview(conn, &cards, default_deck)
    })
    .await
}

#[tauri::command]
pub async fn import_pleco(
    app: AppHandle,
    path: String,
    default_deck: Option<i64>,
    use_dictionary: bool,
    include_other_decks: Option<bool>,
) -> AppResult<PlecoImportResult> {
    blocking(app, move |conn| {
        let cards = pleco::read_file(&PathBuf::from(path))?;
        let opts = pleco::ImportOptions {
            default_deck,
            use_dictionary,
            include_other_decks: include_other_decks.unwrap_or(false),
        };
        pleco::import(conn, &cards, &opts)
    })
    .await
}

#[tauri::command]
pub async fn export_pleco(app: AppHandle, deck_id: Option<i64>, path: String) -> AppResult<i64> {
    blocking(app, move |conn| pleco::export(conn, deck_id, &PathBuf::from(path))).await
}

#[tauri::command]
pub async fn export_cards(app: AppHandle, deck_id: Option<i64>, path: String) -> AppResult<i64> {
    blocking(app, move |conn| cards::export_file(conn, deck_id, &PathBuf::from(path))).await
}

// ---------- study ----------

#[tauri::command]
pub fn build_session(state: State<AppState>, filter: StudyFilter) -> AppResult<SessionInfo> {
    study::build_session(&lock(&state), &filter)
}

#[tauri::command]
pub fn preview_intervals(state: State<AppState>, card_id: i64, direction: String) -> AppResult<Vec<IntervalPreview>> {
    study::preview(&lock(&state), card_id, &direction)
}

#[tauri::command]
pub fn answer_card(
    state: State<AppState>,
    card_id: i64,
    direction: String,
    rating: u8,
    duration_ms: i64,
) -> AppResult<AnswerResult> {
    study::answer(&mut lock(&state), card_id, &direction, rating, duration_ms)
}

#[tauri::command]
pub fn get_stats(state: State<AppState>, days: i64) -> AppResult<Stats> {
    stats::get_stats(&lock(&state), days)
}

// ---------- dictionary ----------

#[tauri::command]
pub fn dict_status(state: State<AppState>) -> AppResult<DictStatus> {
    dictionary::status(&lock(&state))
}

#[tauri::command]
pub fn dict_search(state: State<AppState>, query: String, limit: Option<i64>) -> AppResult<Vec<DictEntry>> {
    dictionary::search(&lock(&state), &query, limit.unwrap_or(60))
}

#[tauri::command]
pub fn dict_lookup(state: State<AppState>, hanzi: String) -> AppResult<Vec<DictEntry>> {
    dictionary::lookup(&lock(&state), &hanzi)
}

#[tauri::command]
pub fn dict_candidates(state: State<AppState>, query: String, limit: Option<usize>) -> AppResult<Vec<Candidate>> {
    dictionary::candidates(&lock(&state), &query, limit.unwrap_or(9))
}

#[tauri::command]
pub fn dict_segment(state: State<AppState>, text: String, prefer: Option<Vec<i64>>) -> AppResult<Vec<Segment>> {
    dictionary::segment(&lock(&state), &text, &prefer.unwrap_or_default())
}

#[tauri::command]
pub async fn dict_import_file(app: AppHandle, path: String) -> AppResult<i64> {
    // Read and decompress before taking the database lock.
    let text = tauri::async_runtime::spawn_blocking(move || dictionary::read_file(&PathBuf::from(path)))
        .await
        .map_err(|e| AppError::msg(e.to_string()))??;
    blocking(app, move |conn| dictionary::import(conn, &text)).await
}

#[tauri::command]
pub async fn dict_download(app: AppHandle) -> AppResult<i64> {
    let text = tauri::async_runtime::spawn_blocking(dictionary::download)
        .await
        .map_err(|e| AppError::msg(e.to_string()))??;
    blocking(app, move |conn| dictionary::import(conn, &text)).await
}
