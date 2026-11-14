//! Tauri commands: the IPC surface used by the Svelte front end.

use crate::error::{AppError, AppResult};
use crate::models::*;
use crate::stats::Stats;
use crate::{ai, billing, fonts, calibration, cards, data, db, decks, dictionary, pinyin, pleco, speech, stats, stories, study, tone_check, tones, tutor, whisper, AppState};
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

// ---------- startup ----------

/// Why the app couldn't open one of its databases at startup, if it couldn't (the page then
/// shows this and the backups instead of the usual pages).
#[tauri::command]
pub fn startup_problem(state: State<AppState>) -> Option<crate::StartupProblem> {
    state.startup_problem.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Try again to open the database that couldn't be opened at startup (another program had
/// it, say). True when it's open now; the page then loads the app as usual.
#[tauri::command]
pub fn startup_retry(app: AppHandle, state: State<AppState>) -> AppResult<bool> {
    let Some(problem) = startup_problem(state.clone()) else { return Ok(true) };
    let kind = data::Kind::parse(Some(&problem.kind))?;
    let dir = app_data(&app)?;
    let opened = match kind {
        data::Kind::Cards => data::open_for_version(&dir, env!("CARGO_PKG_VERSION")),
        data::Kind::Reading => data::open_reading(&dir),
    };
    match opened {
        Ok(conn) => {
            with_db(&state, kind, |c| {
                *c = conn;
                Ok(())
            })?;
            clear_startup_problem(&app, &state, kind);
            if state.startup_problem.lock().unwrap_or_else(|e| e.into_inner()).is_some() {
                return Ok(false); // the other database's turn
            }
            if kind == data::Kind::Cards {
                whisper::set_device_preference(&db::get_setting(&lock(&state), "whisper_device")?.unwrap_or_else(|| "auto".into()));
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = import_bundled_dictionary(app).await {
                        eprintln!("dictionary import: {e}");
                    }
                });
            }
            Ok(true)
        }
        Err(e) => {
            *state.startup_problem.lock().unwrap_or_else(|e| e.into_inner()) = Some(crate::StartupProblem::new(&problem.kind, &e));
            Ok(false)
        }
    }
}

/// Whether the database of `kind` is the one that couldn't be opened at startup.
fn has_startup_problem(state: &AppState, kind: data::Kind) -> bool {
    let problem = state.startup_problem.lock().unwrap_or_else(|e| e.into_inner());
    problem.as_ref().is_some_and(|p| data::Kind::parse(Some(&p.kind)).ok() == Some(kind))
}

/// The database of `kind` is in use again (a backup was put in place, or a new one started).
fn clear_startup_problem(app: &AppHandle, state: &AppState, kind: data::Kind) {
    if !has_startup_problem(state, kind) {
        return;
    }
    *state.startup_problem.lock().unwrap_or_else(|e| e.into_inner()) = None;
    // Only the first problem is shown. If the reading database couldn't be opened either, it
    // is still a stand-in: open it now, or show its problem next.
    let Ok(dir) = app_data(app) else { return };
    let mut stories = state.stories.lock().unwrap_or_else(|e| e.into_inner());
    if stories.path().is_none_or(str::is_empty) {
        match data::open_reading(&dir) {
            Ok(conn) => *stories = conn,
            Err(e) => {
                *state.startup_problem.lock().unwrap_or_else(|e| e.into_inner()) = Some(crate::StartupProblem::new("reading", &e));
            }
        }
    }
}

// ---------- settings ----------

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> AppResult<HashMap<String, String>> {
    let conn = lock(&state);
    let mut map: HashMap<String, String> = [
        ("language", "en".to_string()),
        ("theme", "light".to_string()), // fresh installs start light
        ("desired_retention", db::DEFAULT_RETENTION.to_string()),
        ("new_per_day", db::DEFAULT_NEW_PER_DAY.to_string()),
        ("tone_colors", "true".to_string()),
        ("hsk_autotag", "true".to_string()),
        ("speech_rate", "0.9".to_string()),
        ("autoplay_audio", "false".to_string()),
        ("speech_engine", "auto".to_string()),
        ("whisper_model", "base".to_string()),
        ("whisper_device", "auto".to_string()),
        ("pitch_tones", "false".to_string()),
        // Settings › About; development builds are in debug mode regardless (`debug_mode`)
        ("debug_mode", "false".to_string()),
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
    crate::input::check_setting(&key, &value)?;
    if key == "whisper_device" {
        whisper::set_device_preference(&value);
    }
    Ok(db::set_setting(&lock(&state), &key, &value)?)
}

// ---------- your data (the two database files: cards & decks, and reading) ----------

/// Runs `f` with the connection of the database `kind` names ("cards" or "reading").
fn with_db<T>(state: &State<AppState>, kind: data::Kind, f: impl FnOnce(&mut Connection) -> AppResult<T>) -> AppResult<T> {
    match kind {
        data::Kind::Cards => f(&mut lock(state)),
        data::Kind::Reading => f(&mut lock_stories(state)),
    }
}

#[tauri::command]
pub fn data_info(app: AppHandle, state: State<AppState>, kind: Option<String>) -> AppResult<data::DataInfo> {
    let kind = data::Kind::parse(kind.as_deref())?;
    let dir = app_data(&app)?;
    with_db(&state, kind, |c| data::info(c, &dir, kind))
}

/// Back up a database now; returns the backup's file name.
#[tauri::command]
pub fn data_backup(app: AppHandle, state: State<AppState>, kind: Option<String>) -> AppResult<String> {
    let kind = data::Kind::parse(kind.as_deref())?;
    let dir = app_data(&app)?;
    let path = with_db(&state, kind, |c| data::backup(c, &dir, kind, data::Made::ByHand))?;
    Ok(path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
}

/// Delete one backup file the learner chose in Settings.
#[tauri::command]
pub fn data_delete_backup(app: AppHandle, kind: Option<String>, path: String) -> AppResult<()> {
    let kind = data::Kind::parse(kind.as_deref())?;
    data::delete_backup(&app_data(&app)?, kind, std::path::Path::new(&path))
}

/// Save a copy of a database to a file the user chose.
#[tauri::command]
pub fn data_save_copy(app: AppHandle, state: State<AppState>, kind: Option<String>, path: String) -> AppResult<()> {
    let kind = data::Kind::parse(kind.as_deref())?;
    let dir = app_data(&app)?;
    with_db(&state, kind, |c| data::save_copy(c, &dir, kind, std::path::Path::new(&path)))
}

/// What a database file holds, before switching to it.
#[tauri::command]
pub fn data_inspect(kind: Option<String>, path: String) -> AppResult<data::FileInfo> {
    data::inspect(std::path::Path::new(&path), data::Kind::parse(kind.as_deref())?)
}

/// Switch to another database file (a backup, or one from another computer). The current
/// one is backed up first; returns that backup's file name.
#[tauri::command]
pub fn data_use_file(app: AppHandle, state: State<AppState>, kind: Option<String>, path: String) -> AppResult<String> {
    let kind = data::Kind::parse(kind.as_deref())?;
    let dir = app_data(&app)?;
    let _no_import = if kind == data::Kind::Cards { Some(dictionary_idle()?) } else { None };
    let recovering = has_startup_problem(&state, kind);
    let saved = with_db(&state, kind, |c| data::use_file(c, &dir, kind, std::path::Path::new(&path)))?;
    clear_startup_problem(&app, &state, kind);
    if kind == data::Kind::Cards && recovering {
        // the dictionary import was skipped at startup; an old backup may not have one
        let handle = app.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(e) = import_bundled_dictionary(handle).await {
                eprintln!("dictionary import: {e}");
            }
        });
    }
    if kind == data::Kind::Cards {
        whisper::set_device_preference(&db::get_setting(&lock(&state), "whisper_device")?.unwrap_or_else(|| "auto".into()));
    }
    Ok(saved.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
}

/// Only when a database couldn't be opened at startup: set its damaged files aside (in a
/// "damaged-<time>" folder, untouched) and start a new, empty one. Returns that folder's name.
#[tauri::command]
pub fn data_start_fresh(app: AppHandle, state: State<AppState>, kind: Option<String>) -> AppResult<String> {
    let kind = data::Kind::parse(kind.as_deref())?;
    let dir = app_data(&app)?;
    if !has_startup_problem(&state, kind) {
        return Err(AppError::msg("That database is in use; there's nothing to replace."));
    }
    let aside = with_db(&state, kind, |c| data::start_fresh(c, &dir, kind))?;
    clear_startup_problem(&app, &state, kind);
    if kind == data::Kind::Cards {
        // the new database has no dictionary yet
        tauri::async_runtime::spawn(async move {
            if let Err(e) = import_bundled_dictionary(app).await {
                eprintln!("dictionary import: {e}");
            }
        });
    }
    Ok(aside.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
}

/// Open the app-data folder, where both databases and their backups are.
#[tauri::command]
pub fn data_show_in_folder(app: AppHandle) -> AppResult<()> {
    data::open_folder(&app_data(&app)?)
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
pub async fn list_cards(
    app: AppHandle,
    deck_id: Option<i64>,
    search: Option<String>,
    merge: Option<bool>,
) -> AppResult<Vec<CardRow>> {
    // off the main thread: a big deck takes a moment to load, and the window shouldn't freeze meanwhile
    blocking(app, move |conn| {
        let search = search.as_deref().unwrap_or("");
        // `merge`: one row for a word that several decks hold with the same pinyin and meaning
        if merge.unwrap_or(false) {
            cards::list_words(conn, deck_id, search)
        } else {
            cards::list_cards(conn, deck_id, search)
        }
    })
    .await
}

#[tauri::command]
pub fn create_card(state: State<AppState>, input: CardInput) -> AppResult<Card> {
    cards::create_card(&lock(&state), &input)
}

#[tauri::command]
pub fn update_card(state: State<AppState>, id: i64, input: CardInput, copies: Option<Vec<i64>>) -> AppResult<Card> {
    // `copies`: the cards a folded row stood for; they get the same characters, pinyin and meaning
    cards::update_card_and_copies(&mut lock(&state), id, &input, copies.as_deref().unwrap_or(&[]))
}

#[tauri::command]
pub fn delete_cards(state: State<AppState>, ids: Vec<i64>) -> AppResult<()> {
    cards::delete_cards(&mut lock(&state), &ids)
}

#[tauri::command]
pub fn move_cards(state: State<AppState>, ids: Vec<i64>, deck_id: i64) -> AppResult<()> {
    cards::move_cards(&mut lock(&state), &ids, deck_id)
}

/// Add HSK level tags to every card that lacks one; returns how many cards changed.
#[tauri::command]
pub async fn hsk_tag_all(app: AppHandle) -> AppResult<usize> {
    blocking(app, |conn| crate::hsk::tag_all(conn)).await
}

#[tauri::command]
pub fn list_tags(state: State<AppState>) -> AppResult<Vec<String>> {
    cards::list_tags(&lock(&state))
}

/// Remove these tags from every card; returns how many cards changed.
#[tauri::command]
pub fn remove_tags(state: State<AppState>, tags: Vec<String>) -> AppResult<usize> {
    cards::remove_tags(&mut lock(&state), &tags)
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
pub async fn build_session(app: AppHandle, filter: StudyFilter) -> AppResult<SessionInfo> {
    // off the main thread: it reads every card of the chosen decks
    blocking(app, move |conn| study::build_session(conn, &filter)).await
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCounts {
    /// Cards a session with this selection would have.
    pub matching: usize,
    pub due_count: i64,
    pub new_remaining_today: i64,
}

/// What `build_session` would give, as numbers only: the Study page recounts on every change
/// of the selection, and a big selection is thousands of cards it has no use for.
#[tauri::command]
pub async fn session_counts(app: AppHandle, filter: StudyFilter) -> AppResult<SessionCounts> {
    blocking(app, move |conn| {
        let info = study::build_session(conn, &filter)?;
        Ok(SessionCounts { matching: info.items.len(), due_count: info.due_count, new_remaining_today: info.new_remaining_today })
    })
    .await
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

/// Take back the latest answer (the Study page's Back button).
#[tauri::command]
pub fn undo_answer(state: State<AppState>, card_id: i64, direction: String) -> AppResult<()> {
    study::undo_answer(&mut lock(&state), card_id, &direction)
}

#[tauri::command]
pub async fn get_stats(app: AppHandle, filter: Option<stats::StatsFilter>) -> AppResult<Stats> {
    blocking(app, move |conn| stats::get_stats(conn, &filter.unwrap_or_default())).await
}

/// The Reading and AI Tutor pages report the time spent on them, a little at a time.
#[tauri::command]
pub fn log_study_time(state: State<AppState>, kind: String, seconds: i64) -> AppResult<()> {
    stats::log_time(&lock(&state), &kind, seconds)
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
    crate::input::check_pcm(&pcm)?;
    let dir = app_data(&app)?;
    let (plan, status, pitch) = blocking(app.clone(), {
        let dir = dir.clone();
        let others = others.clone();
        move |conn| {
            let plan = speech::plan(conn, card_id, &others)?;
            // "Use pitch to tell tones apart": the words it may choose between, and the
            // learner's pitch range so far
            let pitch = if db::get_setting(conn, "pitch_tones")?.as_deref() == Some("true") {
                let cands = tone_check::candidates(conn, &plan.hanzi, &plan.pinyin, tone_check::MAX_VARIANTS)?;
                let profile = tones::PitchProfile::parse(&db::get_setting(conn, "pitch_profile")?.unwrap_or_default());
                Some((cands, profile))
            } else {
                None
            };
            Ok((plan, speech::engine_status(conn, &dir), pitch))
        }
    })
    .await?;
    let model = status.whisper_model.ok_or_else(|| AppError::msg("No Whisper model is downloaded."))?;
    // Hint Whisper with the words being studied (the answer among them, not first).
    let mut words: Vec<String> = others.iter().take(30).cloned().collect();
    let answer = plan.accepted.first().cloned().unwrap_or_else(|| plan.hanzi.clone());
    words.retain(|w| *w != answer);
    words.insert(card_id.unsigned_abs() as usize % (words.len() + 1), answer);
    let prompt = format!("{}。", words.join("，"));
    let text = match pitch {
        Some((cands, profile)) => {
            let text = run_whisper(&app, dir.clone(), model.clone(), pcm.clone(), prompt, true).await?;
            pick_by_pitch(&app, dir, model, pcm, text, cands, profile).await?
        }
        None => run_whisper(&app, dir, model, pcm, prompt, true).await?,
    };
    blocking(app, move |conn| speech::judge_text(conn, &plan, &text)).await
}

/// With the "pitch_tones" setting: when Whisper wrote the answer or a word with the same
/// syllables in another tone (妈 / 马), choose among those words by Whisper's score plus the
/// pitch of the recording (see tone_check.rs). Every recording also extends the learner's
/// pitch range; choosing starts once it has `PROFILE_MIN_RECORDINGS` of them.
async fn pick_by_pitch(
    app: &AppHandle,
    dir: PathBuf,
    model: String,
    pcm: Vec<f32>,
    text: String,
    cands: Vec<tone_check::Candidate>,
    mut profile: tones::PitchProfile,
) -> AppResult<String> {
    let (heard, cands) = blocking(app.clone(), {
        let text = text.clone();
        move |conn| {
            let heard: String = speech::to_simplified(conn, &text)?.chars().filter(|c| pinyin::is_hanzi(*c)).collect();
            let mut cands = cands;
            tone_check::add_heard(conn, &mut cands, &heard)?;
            Ok((heard, cands))
        }
    })
    .await?;
    let (chosen, profile) = tauri::async_runtime::spawn_blocking(move || -> AppResult<(Option<String>, tones::PitchProfile)> {
        let trim = if whisper::using_gpu() { None } else { Some(SPEECH_WINDOW) };
        let chosen = tone_check::select(&dir, &model, &pcm, &heard, &cands, &mut profile, trim)?;
        Ok((chosen, profile))
    })
    .await
    .map_err(|e| AppError::msg(e.to_string()))??;
    blocking(app.clone(), move |conn| Ok(db::set_setting(conn, "pitch_profile", &profile.serialize())?)).await?;
    Ok(chosen.unwrap_or(text))
}

// ---------- speech output to a chosen device (Settings › Audio) ----------

/// Whether the app can make speech itself (and so send it to a chosen output device).
#[tauri::command]
pub fn tts_available() -> bool {
    crate::tts::available()
}

/// Speech for `text` as a WAV file (raw bytes, not JSON).
#[tauri::command]
pub async fn tts_wav(app: AppHandle, text: String, voice: String, rate: f64, engine: Option<String>) -> AppResult<tauri::ipc::Response> {
    let text = crate::input::clip(&text, 2000);
    let dir = app_data(&app)?;
    let wav = tauri::async_runtime::spawn_blocking(move || match engine.as_deref() {
        // the built-in voice (Settings › Audio), else the system's
        Some("kokoro") => crate::kokoro::synthesize(&dir, &text, rate),
        _ => crate::tts::synthesize(&text, &voice, rate),
    })
        .await
        .map_err(|e| AppError::msg(e.to_string()))?
        .map_err(AppError::msg)?;
    Ok(tauri::ipc::Response::new(wav))
}

// ---------- the built-in voice (Settings › Audio) ----------

#[tauri::command]
pub fn kokoro_status(app: AppHandle) -> AppResult<crate::kokoro::Status> {
    Ok(crate::kokoro::status(&app_data(&app)?))
}

#[tauri::command]
pub async fn kokoro_download(app: AppHandle) -> AppResult<()> {
    let dir = app_data(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut last = std::time::Instant::now();
        crate::kokoro::download(&dir, |received, total| {
            if last.elapsed().as_millis() >= 200 || received == total {
                last = std::time::Instant::now();
                let _ = app.emit("voice-download", serde_json::json!({ "received": received, "total": total }));
            }
        })
    })
    .await
    .map_err(|e| AppError::msg(e.to_string()))?
}

#[tauri::command]
pub fn kokoro_remove(app: AppHandle) -> AppResult<()> {
    crate::kokoro::remove(&app_data(&app)?)
}

// ---------- tone calibration (Settings) ----------

fn saved_profile(conn: &Connection) -> AppResult<tones::PitchProfile> {
    Ok(tones::PitchProfile::parse(&db::get_setting(conn, "pitch_profile")?.unwrap_or_default()))
}

/// The learner's pitch range so far and the tone detector's settings.
#[tauri::command]
pub async fn pitch_profile_info(app: AppHandle) -> AppResult<calibration::ProfileInfo> {
    blocking(app, |conn| Ok(calibration::info(&saved_profile(conn)?))).await
}

/// Analyse one calibration recording; `earlier` are the pitch tracks of the calibration's
/// earlier recordings. Nothing is saved.
#[tauri::command]
pub async fn pitch_analyze(app: AppHandle, pcm: Vec<f32>, tone: Option<u8>, earlier: Vec<calibration::Earlier>) -> AppResult<calibration::Analysis> {
    crate::input::check_pcm(&pcm)?;
    let saved_typical = blocking(app, |conn| Ok(saved_profile(conn)?.typical_length())).await?;
    tauri::async_runtime::spawn_blocking(move || calibration::analyze(&pcm, tone, &earlier, saved_typical))
        .await
        .map_err(|e| AppError::msg(e.to_string()))
}

/// Replace the pitch profile with the one a finished calibration's recordings make.
#[tauri::command]
pub async fn pitch_calibration_save(app: AppHandle, recordings: Vec<calibration::Earlier>) -> AppResult<calibration::ProfileInfo> {
    blocking(app, move |conn| {
        let profile = calibration::build(&recordings);
        db::set_setting(conn, "pitch_profile", &profile.serialize())?;
        Ok(calibration::info(&profile))
    })
    .await
}

/// Where calibration recordings are kept (for checking the pitch tracker on real voices).
const CALIBRATION_RECORDINGS: &str = "calibration-recordings";
/// How many recordings are kept there (the oldest are removed).
const KEEP_CALIBRATION_RECORDINGS: usize = 40;

/// Debug mode: a development build, or Settings › About › Debug mode turned on. Only then are
/// calibration recordings kept on disk.
fn debug_mode(conn: &Connection) -> AppResult<bool> {
    Ok(cfg!(debug_assertions) || db::get_setting(conn, "debug_mode")?.as_deref() == Some("true"))
}

/// In debug mode, save a calibration recording (16 kHz mono) as a WAV file in the app-data
/// folder, keeping the latest `KEEP_CALIBRATION_RECORDINGS`, and return its path. Otherwise
/// nothing is written and it returns None: a learner's voice isn't kept unless they ask.
#[tauri::command]
pub async fn pitch_save_recording(app: AppHandle, pcm: Vec<f32>, label: String) -> AppResult<Option<String>> {
    crate::input::check_pcm(&pcm)?;
    if !blocking(app.clone(), |conn| debug_mode(conn)).await? {
        return Ok(None);
    }
    let dir = app_data(&app)?.join(CALIBRATION_RECORDINGS);
    tauri::async_runtime::spawn_blocking(move || -> AppResult<Option<String>> {
        std::fs::create_dir_all(&dir)?;
        let safe: String = label.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
        let path = dir.join(format!("{}-{safe}.wav", chrono::Local::now().format("%Y%m%d-%H%M%S%.3f")));
        speech::write_wav(&path, &pcm)?;
        let mut all: Vec<PathBuf> = std::fs::read_dir(&dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "wav"))
            .collect();
        all.sort(); // the names start with the time
        for old in all.iter().take(all.len().saturating_sub(KEEP_CALIBRATION_RECORDINGS)) {
            let _ = std::fs::remove_file(old);
        }
        Ok(Some(path.display().to_string()))
    })
    .await
    .map_err(|e| AppError::msg(e.to_string()))?
}

/// Forget the pitch profile (it builds up again from spoken answers, or a new calibration).
#[tauri::command]
pub async fn pitch_profile_reset(app: AppHandle) -> AppResult<calibration::ProfileInfo> {
    blocking(app, |conn| {
        db::set_setting(conn, "pitch_profile", "")?;
        Ok(calibration::info(&tones::PitchProfile::parse("")))
    })
    .await
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

/// Judge a recorded English answer ("say the meaning") with Whisper.
#[tauri::command]
pub async fn speech_check_english(app: AppHandle, card_id: i64, pcm: Vec<f32>) -> AppResult<speech::SpokenAnswer> {
    crate::input::check_pcm(&pcm)?;
    let dir = app_data(&app)?;
    let (card, status, keep_recording) = blocking(app.clone(), {
        let dir = dir.clone();
        move |conn| Ok((cards::get_card(conn, card_id)?, speech::engine_status(conn, &dir), debug_mode(conn)?))
    })
    .await?;
    let model = status.whisper_model.ok_or_else(|| AppError::msg("No Whisper model is downloaded."))?;
    let text = tauri::async_runtime::spawn_blocking(move || {
        if keep_recording {
            let _ = speech::write_wav(&dir.join(LAST_RECORDING), &pcm);
        }
        let opts = whisper::Options {
            trim: if whisper::using_gpu() { None } else { Some(SPEECH_WINDOW) },
            ..Default::default()
        };
        whisper::transcribe(&dir, &model, &pcm, "en", &opts)
    })
    .await
    .map_err(|e| AppError::msg(e.to_string()))??;
    Ok(speech::judge_meaning(&card.meaning, &text))
}

/// In debug mode the last recording is kept in the app's data folder (overwritten each time),
/// so recognition problems can be checked. Otherwise a learner's voice isn't written to disk.
const LAST_RECORDING: &str = "last-recording.wav";

/// Transcribe a learner speaking Mandarin with Whisper (raw text), favouring their card
/// words.
/// `chinese_only`: block Latin letters and digits (drill answers); otherwise English is kept,
/// and a recording that's clearly English is written in English (the tutor).
async fn run_whisper(
    app: &AppHandle,
    dir: PathBuf,
    model: String,
    pcm: Vec<f32>,
    prompt: String,
    chinese_only: bool,
) -> AppResult<String> {
    let (bias_words, keep_recording) = blocking(app.clone(), |conn| Ok((card_words(conn)?, debug_mode(conn)?))).await?;
    // English only with a model that handles it; smaller models garble it into Chinese
    let chinese_only = chinese_only || !whisper::keeps_english(&model);
    tauri::async_runtime::spawn_blocking(move || {
        if keep_recording {
            let _ = speech::write_wav(&dir.join(LAST_RECORDING), &pcm);
        }
        let opts = whisper::Options {
            prompt,
            bias_words,
            bias: CARD_WORD_BIAS,
            chinese_only,
            // On the CPU, encode the speech plus 2 s (at least 10 s) instead of a padded 30 s
            // window: about 3x faster with Whisper small and nearly as accurate. On the GPU
            // the full window costs ~0.2 s and is the most accurate (whisper_bench, 2026-11-09).
            trim: if whisper::using_gpu() { None } else { Some(SPEECH_WINDOW) },
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
    crate::input::check_pcm(&pcm)?;
    let dir = app_data(&app)?;
    let status = blocking(app.clone(), {
        let dir = dir.clone();
        move |conn| Ok(speech::engine_status(conn, &dir))
    })
    .await?;
    let model = status.whisper_model.ok_or_else(|| AppError::msg("No Whisper model is downloaded."))?;
    // free speech (the tutor, the microphone test): English is kept
    let text = run_whisper(&app, dir, model, pcm, prompt.unwrap_or_default(), false).await?;
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

/// Where Whisper runs (GPU or CPU) and why, for Settings. The first call checks the GPU.
#[tauri::command]
pub async fn whisper_device_info(app: AppHandle) -> AppResult<whisper::DeviceInfo> {
    let dir = app_data(&app)?;
    tauri::async_runtime::spawn_blocking(move || whisper::device_info(&dir)).await.map_err(|e| AppError::msg(e.to_string()))
}

/// Download NVIDIA's GPU libraries (progress events "gpu-download": {received, total}).
#[tauri::command]
pub async fn gpu_libs_download(app: AppHandle) -> AppResult<()> {
    let dir = app_data(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut last = std::time::Instant::now();
        crate::gpu::download(&dir, |received, total| {
            if last.elapsed().as_millis() >= 200 || received == total {
                last = std::time::Instant::now();
                let _ = app.emit("gpu-download", serde_json::json!({ "received": received, "total": total }));
            }
        })
    })
    .await
    .map_err(|e| AppError::msg(e.to_string()))?
}

#[tauri::command]
pub fn gpu_libs_remove(app: AppHandle) -> AppResult<()> {
    crate::gpu::remove(&app_data(&app)?)
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

/// First start: import the CC-CEDICT copy that ships with the app when the dictionary is
/// empty (it can still be updated from the Dictionary page). Emits "dictionary-ready".
pub async fn import_bundled_dictionary(app: AppHandle) -> AppResult<()> {
    let empty = blocking(app.clone(), |conn| Ok(dictionary::status(conn)?.entries == 0)).await?;
    if !empty {
        return Ok(());
    }
    let path = app
        .path()
        .resolve("resources/cedict_1_0_ts_utf-8_mdbg.txt.gz", tauri::path::BaseDirectory::Resource)
        .map_err(|e| AppError::msg(e.to_string()))?;
    let text = tauri::async_runtime::spawn_blocking(move || dictionary::read_file(&path))
        .await
        .map_err(|e| AppError::msg(e.to_string()))??;
    let n = import_dictionary(&app, text).await?;
    let _ = app.emit("dictionary-ready", n);
    Ok(())
}

/// Held while a dictionary import writes: one import at a time, and the cards database
/// isn't switched for another underneath it.
static DICTIONARY_IMPORT: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Fails while a dictionary import is running; otherwise holds imports off until dropped.
fn dictionary_idle() -> AppResult<MutexGuard<'static, ()>> {
    match DICTIONARY_IMPORT.try_lock() {
        Ok(guard) => Ok(guard),
        Err(std::sync::TryLockError::Poisoned(e)) => Ok(e.into_inner()),
        Err(std::sync::TryLockError::WouldBlock) => {
            Err(AppError::msg("The dictionary is being updated. Try again when it has finished."))
        }
    }
}

/// Replace the dictionary (its own file, dictionary.db), through a connection of its own. The
/// import is one write that takes a few seconds; on the app's connection every other
/// command, and with them the window, would wait for it (most noticeably at the very first
/// start). This way reading goes on meanwhile (the previous dictionary shows until the new
/// one is complete), and the cards database isn't touched at all.
async fn import_dictionary(app: &AppHandle, text: String) -> AppResult<i64> {
    let path = app_data(app)?.join(dictionary::DB_FILE);
    tauri::async_runtime::spawn_blocking(move || {
        let _one = DICTIONARY_IMPORT.lock().unwrap_or_else(|e| e.into_inner());
        let mut conn = dictionary::open(&path)?;
        let n = dictionary::import(&mut conn, &text)?;
        // into the file itself now, rather than whenever the log next fills up
        let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)");
        Ok(n)
    })
    .await
    .map_err(|e| AppError::msg(e.to_string()))?
}

#[tauri::command]
pub fn dict_status(state: State<AppState>) -> AppResult<DictStatus> {
    dictionary::status(&lock(&state))
}

// Dictionary lookups run off the main thread: a search reads the whole dictionary, and they
// are made as the learner types. (The page ignores an answer that arrives after a newer one.)

#[tauri::command]
pub async fn dict_search(app: AppHandle, query: String, limit: Option<i64>) -> AppResult<Vec<DictEntry>> {
    blocking(app, move |conn| dictionary::search(conn, &query, limit.unwrap_or(60))).await
}

#[tauri::command]
pub async fn dict_lookup(app: AppHandle, hanzi: String) -> AppResult<Vec<DictEntry>> {
    blocking(app, move |conn| dictionary::lookup(conn, &hanzi)).await
}

#[tauri::command]
pub async fn dict_candidates(app: AppHandle, query: String, limit: Option<usize>) -> AppResult<Vec<Candidate>> {
    blocking(app, move |conn| dictionary::candidates(conn, &query, limit.unwrap_or(9))).await
}

#[tauri::command]
pub async fn dict_segment(app: AppHandle, text: String, prefer: Option<Vec<i64>>) -> AppResult<Vec<Segment>> {
    blocking(app, move |conn| dictionary::segment(conn, &text, &prefer.unwrap_or_default())).await
}

#[tauri::command]
pub async fn dict_import_file(app: AppHandle, path: String) -> AppResult<i64> {
    // Read and decompress before taking the database lock.
    let text = tauri::async_runtime::spawn_blocking(move || dictionary::read_file(&PathBuf::from(path)))
        .await
        .map_err(|e| AppError::msg(e.to_string()))??;
    import_dictionary(&app, text).await
}

#[tauri::command]
pub async fn dict_download(app: AppHandle) -> AppResult<i64> {
    let text = tauri::async_runtime::spawn_blocking(dictionary::download)
        .await
        .map_err(|e| AppError::msg(e.to_string()))??;
    import_dictionary(&app, text).await
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
        let scenario = crate::input::clip(&crate::input::clean(&scenario, false), crate::input::MAX_MESSAGE);
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
    if let Some(t) = &text {
        crate::input::fits("That message", t, crate::input::MAX_MESSAGE)?;
    }
    let text = text.map(|t| crate::input::clean(&t, true));
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
    crate::input::fits("That text", &text, crate::input::MAX_STORY)?;
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

// ---------- reading (stories.db, separate from the cards database) ----------

fn lock_stories<'a>(state: &'a State<AppState>) -> MutexGuard<'a, Connection> {
    state.stories.lock().unwrap_or_else(|e| e.into_inner())
}

/// Like `blocking`, with the stories database.
async fn blocking_stories<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&mut Connection) -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let mut conn = state.stories.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut conn)
    })
    .await
    .map_err(|e| AppError::msg(e.to_string()))?
}

#[tauri::command]
pub fn list_story_folders(state: State<AppState>) -> AppResult<Vec<stories::StoryFolder>> {
    stories::list_folders(&lock_stories(&state))
}

#[tauri::command]
pub fn create_story_folder(
    state: State<AppState>,
    name: String,
    description: String,
    parent_id: Option<i64>,
) -> AppResult<i64> {
    stories::create_folder(&lock_stories(&state), &name, &description, parent_id)
}

#[tauri::command]
pub fn update_story_folder(
    state: State<AppState>,
    id: i64,
    name: String,
    description: String,
    parent_id: Option<i64>,
) -> AppResult<()> {
    stories::update_folder(&lock_stories(&state), id, &name, &description, parent_id)
}

#[tauri::command]
pub fn delete_story_folder(state: State<AppState>, id: i64) -> AppResult<()> {
    stories::delete_folder(&mut lock_stories(&state), id)
}

#[tauri::command]
pub fn list_stories(state: State<AppState>, folder_id: Option<i64>) -> AppResult<Vec<stories::StoryInfo>> {
    stories::list_stories(&lock_stories(&state), folder_id)
}

#[tauri::command]
pub fn bookmark_story(state: State<AppState>, id: i64, on: bool) -> AppResult<()> {
    stories::set_bookmark(&lock_stories(&state), id, on)
}

#[tauri::command]
pub fn story_total(state: State<AppState>) -> AppResult<i64> {
    stories::total(&lock_stories(&state))
}

#[tauri::command]
pub fn get_story(state: State<AppState>, id: i64) -> AppResult<stories::Story> {
    stories::get_story(&lock_stories(&state), id)
}

#[tauri::command]
pub fn create_story(state: State<AppState>, folder_id: Option<i64>, title: String, body: String) -> AppResult<i64> {
    stories::create_story(&lock_stories(&state), folder_id, &title, &body)
}

#[tauri::command]
pub fn update_story(
    state: State<AppState>,
    id: i64,
    folder_id: Option<i64>,
    title: String,
    body: String,
) -> AppResult<()> {
    stories::update_story(&lock_stories(&state), id, folder_id, &title, &body)
}

#[tauri::command]
pub fn delete_story(state: State<AppState>, id: i64) -> AppResult<()> {
    stories::delete_story(&lock_stories(&state), id)
}

#[tauri::command]
pub async fn import_stories(app: AppHandle, folder_id: Option<i64>, paths: Vec<String>) -> AppResult<usize> {
    // read the files before taking the database lock
    let files = tauri::async_runtime::spawn_blocking(move || {
        paths.iter().map(|p| stories::read_file(&PathBuf::from(p))).collect::<AppResult<Vec<_>>>()
    })
    .await
    .map_err(|e| AppError::msg(e.to_string()))??;
    blocking_stories(app, move |conn| stories::import_files(conn, folder_id, &files)).await
}

/// Import a folder of story files as a collection (subfolders become subcollections).
#[tauri::command]
pub async fn import_story_folder(app: AppHandle, folder_id: Option<i64>, path: String) -> AppResult<stories::ImportSummary> {
    // read the files before taking the database lock
    let tree = tauri::async_runtime::spawn_blocking(move || stories::read_folder(&PathBuf::from(path)))
        .await
        .map_err(|e| AppError::msg(e.to_string()))??;
    blocking_stories(app, move |conn| stories::import_folder(conn, folder_id, &tree)).await
}

/// Write one story to a text file the learner chose (the format the import reads).
#[tauri::command]
pub async fn export_story(app: AppHandle, id: i64, path: String) -> AppResult<stories::ExportSummary> {
    blocking_stories(app, move |conn| stories::export_story(conn, id, &PathBuf::from(path))).await
}

/// Write a collection (none = every story) as a new folder of story files inside `path`.
#[tauri::command]
pub async fn export_story_folder(app: AppHandle, folder_id: Option<i64>, path: String) -> AppResult<stories::ExportSummary> {
    blocking_stories(app, move |conn| stories::export_folder(conn, folder_id, &PathBuf::from(path))).await
}

/// The story's text split into dictionary words (the dictionary is in the cards database).
#[tauri::command]
pub async fn story_segments(app: AppHandle, id: i64) -> AppResult<Vec<Segment>> {
    let body = blocking_stories(app.clone(), move |conn| Ok(stories::get_story(conn, id)?.body)).await?;
    // off the main thread: a long story is many dictionary lookups
    blocking(app, move |conn| dictionary::segment(conn, &body, &[])).await
}

// ---------- Reading fonts (downloaded into app data) ----------

#[tauri::command]
pub fn fonts_list(app: AppHandle) -> AppResult<Vec<fonts::FontFile>> {
    Ok(fonts::list(&app_data(&app)?))
}

/// Download a font, emitting "font-download" progress events.
#[tauri::command]
pub async fn font_download(app: AppHandle, key: String) -> AppResult<()> {
    let dir = app_data(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut last = std::time::Instant::now();
        fonts::download(&dir, &key, |received, total| {
            if last.elapsed().as_millis() >= 150 || received == total {
                last = std::time::Instant::now();
                let _ = app.emit("font-download", DownloadProgress { name: key.clone(), received, total });
            }
        })
    })
    .await
    .map_err(|e| AppError::msg(e.to_string()))?
}

#[tauri::command]
pub fn font_delete(app: AppHandle, key: String) -> AppResult<()> {
    fonts::delete(&app_data(&app)?, &key)
}

/// A downloaded font's bytes, for the page to load with FontFace.
#[tauri::command]
pub async fn font_data(app: AppHandle, key: String) -> AppResult<tauri::ipc::Response> {
    let dir = app_data(&app)?;
    let bytes = tauri::async_runtime::spawn_blocking(move || fonts::read(&dir, &key))
        .await
        .map_err(|e| AppError::msg(e.to_string()))??;
    Ok(tauri::ipc::Response::new(bytes))
}
