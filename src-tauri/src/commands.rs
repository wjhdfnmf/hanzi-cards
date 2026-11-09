//! Tauri commands: the IPC surface used by the Svelte front end.

use crate::error::{AppError, AppResult};
use crate::models::*;
use crate::stats::Stats;
use crate::{ai, billing, cards, db, decks, dictionary, pleco, speech, stats, study, tutor, whisper, AppState};
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::MutexGuard;
use tauri::{AppHandle, Emitter, Manager, State};

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
        ("speech_engine", "auto".to_string()),
        ("whisper_model", "base".to_string()),
        ("tutor_model", ai::DEFAULT_MODEL.to_string()),
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

// ---------- speaking ----------

fn app_data(app: &AppHandle) -> AppResult<PathBuf> {
    app.path().app_data_dir().map_err(|e| AppError::msg(e.to_string()))
}

#[tauri::command]
pub async fn speech_status(app: AppHandle) -> AppResult<speech::SpeechStatus> {
    let dir = app_data(&app)?;
    blocking(app, move |conn| Ok(speech::engine_status(conn, &dir))).await
}

/// Judge a recorded answer (16 kHz mono samples) with Whisper. The database is not
/// locked while transcribing.
#[tauri::command]
pub async fn speech_check_audio(
    app: AppHandle,
    card_id: i64,
    others: Vec<String>,
    pcm: Vec<f32>,
) -> AppResult<speech::SpokenAnswer> {
    let dir = app_data(&app)?;
    let (plan, status) = blocking(app.clone(), {
        let dir = dir.clone();
        let others = others.clone();
        move |conn| Ok((speech::plan(conn, card_id, &others)?, speech::engine_status(conn, &dir)))
    })
    .await?;
    let model = status.whisper_model.ok_or_else(|| AppError::msg("No Whisper model is downloaded."))?;
    // Hint Whisper with the words being studied (the answer among them, not first).
    let mut words: Vec<String> = others.iter().take(30).cloned().collect();
    let answer = plan.accepted.first().cloned().unwrap_or_else(|| plan.hanzi.clone());
    words.retain(|w| *w != answer);
    words.insert(card_id.unsigned_abs() as usize % (words.len() + 1), answer);
    let prompt = format!("{}。", words.join("，"));
    let text = run_whisper(&app, dir, model, pcm, prompt).await?;
    blocking(app, move |conn| speech::judge_text(conn, &plan, &text)).await
}

/// How strongly Whisper is pulled toward words on the learner's cards (added to a token's
/// log-probability). Chosen with the `whisper_bench` test.
/// 1.0 was best on synthetic clips: card words right went from 17 to 29 of 40 (clean) and
/// 10 to 22 (noisy) together with the drill's word-list prompt; 2.0+ caused loops on noise.
const CARD_WORD_BIAS: f32 = 1.0;

/// (margin after the speech, minimum window) in seconds, for `whisper::Options::trim`.
pub const SPEECH_WINDOW: (f32, f32) = (2.0, 10.0);

/// Every word on the learner's cards, in the forms an answer may take.
fn card_words(conn: &Connection) -> AppResult<Vec<String>> {
    let mut stmt = conn.prepare("SELECT DISTINCT hanzi FROM cards")?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    let mut out = Vec::new();
    for h in rows {
        out.extend(speech::answer_forms(&h?));
    }
    out.sort();
    out.dedup();
    Ok(out)
}

/// Transcribe a learner speaking Mandarin with Whisper (raw text), favouring their card
/// words. Keeps the last recording in the app's data folder (`last-recording.wav`,
/// overwritten each time) so recognition problems can be checked.
async fn run_whisper(app: &AppHandle, dir: PathBuf, model: String, pcm: Vec<f32>, prompt: String) -> AppResult<String> {
    let bias_words = blocking(app.clone(), |conn| card_words(conn)).await?;
    tauri::async_runtime::spawn_blocking(move || {
        let _ = speech::write_wav(&dir.join("last-recording.wav"), &pcm);
        let opts = whisper::Options {
            prompt,
            bias_words,
            bias: CARD_WORD_BIAS,
            chinese_only: true,
            // Encode the speech plus 2 s (at least 10 s) instead of a padded 30 s window: about
            // 3x faster with Whisper small and as accurate (whisper_bench, 2026-11-09).
            trim: Some(SPEECH_WINDOW),
            ..Default::default()
        };
        whisper::transcribe(&dir, &model, &pcm, "zh", &opts)
    })
    .await
    .map_err(|e| AppError::msg(e.to_string()))?
}

/// Transcribe recorded Mandarin (16 kHz mono samples) with Whisper.
#[tauri::command]
pub async fn speech_transcribe(app: AppHandle, pcm: Vec<f32>, prompt: Option<String>) -> AppResult<String> {
    let dir = app_data(&app)?;
    let status = blocking(app.clone(), {
        let dir = dir.clone();
        move |conn| Ok(speech::engine_status(conn, &dir))
    })
    .await?;
    let model = status.whisper_model.ok_or_else(|| AppError::msg("No Whisper model is downloaded."))?;
    let text = run_whisper(&app, dir, model, pcm, prompt.unwrap_or_default()).await?;
    blocking(app, move |conn| speech::to_simplified(conn, &text)).await
}

/// Load the Whisper model in the background ahead of the first spoken answer.
#[tauri::command]
pub async fn speech_warmup(app: AppHandle) -> AppResult<()> {
    let dir = app_data(&app)?;
    let status = blocking(app, {
        let dir = dir.clone();
        move |conn| Ok(speech::engine_status(conn, &dir))
    })
    .await?;
    let Some(model) = status.whisper_model else { return Ok(()) };
    tauri::async_runtime::spawn_blocking(move || whisper::warm_up(&dir, &model))
        .await
        .map_err(|e| AppError::msg(e.to_string()))?
}

#[tauri::command]
pub fn whisper_models(app: AppHandle) -> AppResult<Vec<whisper::WhisperModel>> {
    Ok(whisper::list(&app_data(&app)?))
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadProgress {
    name: String,
    received: u64,
    total: u64,
}

/// Download a Whisper model, emitting "whisper-download" progress events.
#[tauri::command]
pub async fn whisper_download(app: AppHandle, name: String) -> AppResult<()> {
    let dir = app_data(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut last = std::time::Instant::now();
        whisper::download(&dir, &name, |received, total| {
            if last.elapsed().as_millis() >= 200 || received == total {
                last = std::time::Instant::now();
                let _ = app.emit("whisper-download", DownloadProgress { name: name.clone(), received, total });
            }
        })
    })
    .await
    .map_err(|e| AppError::msg(e.to_string()))?
}

#[tauri::command]
pub fn whisper_delete(app: AppHandle, name: String) -> AppResult<()> {
    whisper::delete(&app_data(&app)?, &name)
}

/// Listen for the learner saying a card's answer and judge it. `others` are other words
/// in the session, offered to the recognizer as distractors. The database is not locked
/// while listening.
#[tauri::command]
pub async fn speech_listen(app: AppHandle, card_id: i64, others: Vec<String>) -> AppResult<speech::SpokenAnswer> {
    let plan = blocking(app.clone(), move |conn| speech::plan(conn, card_id, &others)).await?;
    let phrases = plan.phrases.clone();
    let heard = tauri::async_runtime::spawn_blocking(move || speech::listen(&phrases))
        .await
        .map_err(|e| AppError::msg(e.to_string()))?
        .map_err(AppError::msg)?;
    blocking(app, move |conn| speech::judge(conn, &plan, heard)).await
}

#[tauri::command]
pub fn speech_cancel() {
    speech::cancel();
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

// ---------- AI tutor ----------

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyStatus {
    pub set: bool,
    /// Last four characters, so the learner can tell which key it is.
    pub hint: Option<String>,
}

#[tauri::command]
pub async fn ai_key_status() -> AppResult<KeyStatus> {
    let key = tauri::async_runtime::spawn_blocking(ai::get_key).await.map_err(|e| AppError::msg(e.to_string()))??;
    Ok(KeyStatus {
        set: key.is_some(),
        hint: key.map(|k| k.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect()),
    })
}

/// Check the key with a free API call, then store it in the system keychain.
#[tauri::command]
pub async fn ai_key_set(key: String) -> AppResult<()> {
    let key = key.trim().to_string();
    if key.is_empty() {
        return Err(AppError::msg("Paste an API key first."));
    }
    tauri::async_runtime::spawn_blocking(move || {
        ai::check_key(&key)?;
        ai::set_key(&key)
    })
    .await
    .map_err(|e| AppError::msg(e.to_string()))?
}

#[tauri::command]
pub async fn ai_key_clear() -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(ai::clear_key).await.map_err(|e| AppError::msg(e.to_string()))?
}

fn tutor_model(conn: &Connection) -> String {
    conn.query_row("SELECT value FROM settings WHERE key = 'tutor_model'", [], |r| r.get(0))
        .unwrap_or_else(|_| ai::DEFAULT_MODEL.to_string())
}

#[tauri::command]
pub async fn tutor_start(
    app: AppHandle,
    deck_ids: Vec<i64>,
    scenario: String,
    options: Option<tutor::TutorOptions>,
) -> AppResult<tutor::Started> {
    blocking(app, move |conn| {
        let model = tutor_model(conn);
        tutor::start(conn, &model, &deck_ids, &scenario, &options.unwrap_or_default())
    })
    .await
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct TutorEventPayload {
    session_id: u64,
    event: tutor::TurnEvent,
}

/// One turn: the learner's message (none = the tutor opens), streamed as "tutor-event"s.
#[tauri::command]
pub async fn tutor_send(app: AppHandle, session_id: u64, text: Option<String>) -> AppResult<tutor::TurnResult> {
    let billing_app = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let known = |hanzi: &str| {
            let state = app.state::<AppState>();
            let conn = state.db.lock().unwrap_or_else(|e| e.into_inner());
            conn.query_row("SELECT EXISTS(SELECT 1 FROM cards WHERE hanzi = ?1)", [hanzi], |r| r.get::<_, bool>(0))
                .unwrap_or(false)
        };
        tutor::turn(
            session_id,
            text.as_deref(),
            |event| {
                let _ = app.emit("tutor-event", TutorEventPayload { session_id, event });
            },
            known,
        )
    })
    .await
    .map_err(|e| AppError::msg(e.to_string()))?;
    // Record what this turn cost, whether or not it succeeded.
    if let Some((model, usage)) = tutor::take_unbilled(session_id) {
        blocking(billing_app, move |conn| billing::record(conn, "tutor", &model, Some(session_id), &usage)).await?;
    }
    result
}

/// Transcribe the learner's reply, hinting Whisper with their words and the tutor's last line.
#[tauri::command]
pub async fn tutor_transcribe(app: AppHandle, session_id: u64, pcm: Vec<f32>) -> AppResult<String> {
    speech_transcribe(app, pcm, Some(tutor::hint(session_id))).await
}

#[tauri::command]
pub fn tutor_cancel(session_id: u64) {
    tutor::cancel(session_id);
}

#[tauri::command]
pub fn tutor_end(session_id: u64) -> AppResult<tutor::Summary> {
    tutor::end(session_id)
}

#[tauri::command]
pub async fn tutor_translate(app: AppHandle, text: String, session_id: Option<u64>) -> AppResult<String> {
    let model = blocking(app.clone(), |conn| Ok(tutor_model(conn))).await?;
    let (english, model, usage) = tauri::async_runtime::spawn_blocking(move || tutor::translate(&model, &text))
        .await
        .map_err(|e| AppError::msg(e.to_string()))??;
    blocking(app, move |conn| billing::record(conn, "translate", &model, session_id, &usage)).await?;
    if english.is_empty() {
        return Err(AppError::msg("Claude declined to translate this."));
    }
    Ok(english)
}

#[tauri::command]
pub async fn ai_spending(app: AppHandle) -> AppResult<billing::Spending> {
    blocking(app, |conn| billing::summary(conn, 30)).await
}

#[tauri::command]
pub async fn ai_spending_export(app: AppHandle, path: String) -> AppResult<()> {
    let csv = blocking(app, |conn| billing::export_csv(conn)).await?;
    std::fs::write(path, csv)?;
    Ok(())
}
