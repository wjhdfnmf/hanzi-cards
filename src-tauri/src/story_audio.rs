//! Stories read aloud in a made voice: MiniMax's speech API (the learner's own key), asked
//! once for each sentence. What comes back is kept as a sound file in the app's data folder,
//! so a story costs its characters once and plays from the files ever after.
//!
//! A sentence's file is named after what made it (the model, the voice and the sentence's
//! text), so the same sentence in two stories is one file, a story whose text changes needs
//! only its new sentences made, and another voice keeps its own files beside the first.
//!
//! Sentences, not whole stories, because the Reading page reads and marks a story sentence by
//! sentence and starts from any of them. How fast the voice speaks is chosen before making
//! (`Making::speed`: MiniMax says it slower itself, which sounds right); the player can also
//! stretch what was made (`playbackRate`), which is free but sounds worse the further it goes.
//!
//! Computers only: the App Store build is made without this file (build.rs, `appstore`).

use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const KEYRING_SERVICE: &str = "com.hanzicards.app";
const KEYRING_USER: &str = "minimax-api-key";
/// The folder in the app's data folder.
const FOLDER: &str = "story-audio";
pub const DEFAULT_MODEL: &str = "speech-2.8-hd";
pub const DEFAULT_VOICE: &str = "Chinese (Mandarin)_News_Anchor";
/// MiniMax's own limit is 10,000 characters a request; a sentence is far below it.
const MAX_TEXT: usize = 5_000;

/// MiniMax runs two services with separate accounts and keys: the international one and the
/// one for mainland China.
fn host(region: &str) -> &'static str {
    if region == "cn" {
        "https://api.minimaxi.com"
    } else {
        "https://api.minimax.io"
    }
}

// ---------- the key (in the system keychain, like the tutor's) ----------

fn entry() -> AppResult<keyring::Entry> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).map_err(|e| AppError::msg(format!("Keychain error: {e}")))
}

pub fn get_key() -> AppResult<Option<String>> {
    match entry()?.get_password() {
        Ok(k) => Ok(Some(k)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(AppError::msg(format!("Keychain error: {e}"))),
    }
}

pub fn set_key(key: &str) -> AppResult<()> {
    entry()?.set_password(key.trim()).map_err(|e| AppError::msg(format!("Keychain error: {e}")))
}

pub fn clear_key() -> AppResult<()> {
    match entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(AppError::msg(format!("Keychain error: {e}"))),
    }
}

pub fn require_key() -> AppResult<String> {
    get_key()?.ok_or_else(|| AppError::msg("Add your MiniMax API key in Settings › Audio first."))
}

// ---------- the service ----------

/// One request. MiniMax answers 200 with its own status inside (`base_resp`), so both are read.
fn post(key: &str, region: &str, path: &str, body: &Value, timeout: Duration) -> AppResult<Value> {
    let answer = ureq::post(&format!("{}{path}", host(region)))
        .set("Authorization", &format!("Bearer {}", key.trim()))
        .set("Content-Type", "application/json")
        .timeout(timeout)
        .send_string(&body.to_string());
    let value: Value = match answer {
        Ok(r) => serde_json::from_reader(r.into_reader()).map_err(|e| AppError::msg(format!("MiniMax's answer couldn't be read: {e}")))?,
        Err(ureq::Error::Status(401, _)) | Err(ureq::Error::Status(403, _)) => return Err(refused()),
        Err(ureq::Error::Status(code, _)) => return Err(AppError::msg(format!("MiniMax answered with an error ({code})."))),
        Err(e) => return Err(AppError::msg(format!("Couldn't reach MiniMax: {e}"))),
    };
    let code = value["base_resp"]["status_code"].as_i64().unwrap_or(0);
    if code == 0 {
        return Ok(value);
    }
    let said = value["base_resp"]["status_msg"].as_str().unwrap_or("").to_string();
    Err(match code {
        1004 | 2049 => refused(),
        1002 | 1039 => AppError::msg(BUSY),
        1008 => AppError::msg("The MiniMax account has no balance left."),
        _ => AppError::msg(format!("MiniMax refused the request ({code}): {said}")),
    })
}

const BUSY: &str = "MiniMax is asking for a pause (too many requests).";

fn refused() -> AppError {
    AppError::msg("That MiniMax API key was not accepted. Check the key, and that the region in Settings › Audio matches the account it is from.")
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Voice {
    pub id: String,
    pub name: String,
}

/// The voices the account can use, Mandarin ones first. Also how a key is checked: it costs
/// nothing.
pub fn voices(key: &str, region: &str) -> AppResult<Vec<Voice>> {
    let value = post(key, region, "/v1/get_voice", &json!({ "voice_type": "all" }), Duration::from_secs(30))?;
    let mut out = Vec::new();
    for list in ["voice_cloning", "voice_generation", "system_voice"] {
        for v in value[list].as_array().into_iter().flatten() {
            let Some(id) = v["voice_id"].as_str().filter(|s| !s.is_empty()) else { continue };
            let name = v["voice_name"].as_str().filter(|s| !s.is_empty()).unwrap_or(id);
            out.push(Voice { id: id.to_string(), name: name.to_string() });
        }
    }
    // (a stable sort: the learner's own voices stay first within each half)
    out.sort_by_key(|v| !is_mandarin(&v.id));
    Ok(out)
}

fn is_mandarin(id: &str) -> bool {
    id.starts_with("Chinese (Mandarin)")
}

/// One sentence as an MP3, and the characters MiniMax counted for it.
struct Spoken {
    audio: Vec<u8>,
    /// The characters MiniMax counted.
    characters: u64,
    /// When each word is said (MiniMax's subtitle file, as it came), if it gave one.
    timing: Option<String>,
}

/// One sentence as an MP3, with when each of its words is said.
fn synthesize(key: &str, making: &Making, text: &str) -> AppResult<Spoken> {
    let (region, model, voice) = (making.region.as_str(), making.model.as_str(), making.voice.as_str());
    let mut body = json!({
        "model": model,
        "text": text,
        "stream": false,
        "language_boost": "Chinese",
        "output_format": "hex",
        "subtitle_enable": true,
        "subtitle_type": "word",
        "voice_setting": { "voice_id": voice, "speed": making.speed(), "vol": 1, "pitch": 0 },
        "audio_setting": { "sample_rate": 32000, "bitrate": 128000, "format": "mp3", "channel": 1 },
    });
    if let Some(emotion) = making.emotion() {
        body["voice_setting"]["emotion"] = json!(emotion);
    }
    let mut waited = 0;
    let value = loop {
        match post(key, region, "/v1/t2a_v2", &body, Duration::from_secs(120)) {
            // (asked to pause: wait and ask again, a few times)
            Err(e) if e.to_string().contains(BUSY) && waited < 3 => {
                waited += 1;
                std::thread::sleep(Duration::from_secs(4 * waited));
            }
            // (a model or voice that can't time its words: the sound without them)
            Err(e) if e.to_string().contains("(2013)") && body.get("subtitle_enable").is_some() => {
                let fields = body.as_object_mut().expect("an object");
                fields.remove("subtitle_enable");
                fields.remove("subtitle_type");
            }
            other => break other?,
        }
    };
    let audio = value["data"]["audio"].as_str().and_then(unhex).filter(|a| !a.is_empty());
    let audio = audio.ok_or_else(|| AppError::msg("MiniMax answered without any sound."))?;
    // The timing is a file of its own to fetch. Without it the sentence still plays (only
    // its words aren't marked), so a failure here isn't one of the making.
    let timing = value["data"]["subtitle_file"].as_str().filter(|u| u.starts_with("https://")).and_then(|url| {
        let answer = ureq::get(url).timeout(Duration::from_secs(30)).call().ok()?;
        let mut text = String::new();
        use std::io::Read;
        answer.into_reader().take(4 * 1024 * 1024).read_to_string(&mut text).ok()?;
        Some(text).filter(|t| !t.trim().is_empty())
    });
    Ok(Spoken { audio, characters: value["extra_info"]["usage_characters"].as_u64().unwrap_or(0), timing })
}

/// When a word of a sentence is said.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Word {
    /// Where the word is in the sentence, and how long it is (as the page counts: UTF-16 units).
    pub at: usize,
    pub len: usize,
    /// Milliseconds from the start of the sound.
    pub begin: f64,
    pub end: f64,
}

/// The words of MiniMax's subtitle file, placed in `text` (the sentence as it was sent).
/// The file is read loosely (every entry with a word and its two times, wherever it sits),
/// and a word is placed by finding it in the text, in order: its own offset is used only when
/// the word is really there.
fn parse_words(raw: &str, text: &str) -> Vec<Word> {
    fn collect<'a>(v: &'a Value, out: &mut Vec<(&'a str, Option<usize>, f64, f64)>) {
        match v {
            Value::Array(items) => items.iter().for_each(|i| collect(i, out)),
            Value::Object(fields) => {
                if let (Some(word), Some(begin), Some(end)) =
                    (fields.get("word").and_then(Value::as_str), fields.get("time_begin").and_then(Value::as_f64), fields.get("time_end").and_then(Value::as_f64))
                {
                    out.push((word, fields.get("word_begin").and_then(Value::as_u64).map(|n| n as usize), begin, end));
                } else {
                    fields.values().for_each(|f| collect(f, out));
                }
            }
            _ => {}
        }
    }
    let Ok(value) = serde_json::from_str::<Value>(raw) else { return Vec::new() };
    let mut found = Vec::new();
    collect(&value, &mut found);
    let chars: Vec<char> = text.chars().collect();
    let units = |cs: &[char]| cs.iter().map(|c| c.len_utf16()).sum::<usize>();
    let mut cursor = 0;
    let mut out = Vec::new();
    for (word, offset, begin, end) in found {
        let w: Vec<char> = word.chars().collect();
        if w.is_empty() || end <= begin {
            continue;
        }
        let here = |at: usize| chars.get(at..at + w.len()) == Some(&w[..]);
        let at = offset.filter(|&o| here(o)).or_else(|| (cursor..chars.len()).find(|&i| here(i)));
        let Some(at) = at else { continue };
        cursor = at + w.len();
        out.push(Word { at: units(&chars[..at]), len: units(&w), begin, end });
    }
    out
}

fn timing_of(sound: &Path) -> PathBuf {
    sound.with_extension("json")
}

/// When each word of a made sentence is said (empty if it wasn't given, or isn't made).
pub fn words(app_data: &Path, making: &Making, text: &str) -> Vec<Word> {
    std::fs::read_to_string(timing_of(&file_of(app_data, making, text))).map(|raw| parse_words(&raw, tidy(text))).unwrap_or_default()
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    let s = s.trim().as_bytes();
    if s.len() % 2 != 0 {
        return None;
    }
    let digit = |c: u8| (c as char).to_digit(16).map(|d| d as u8);
    s.chunks(2).map(|p| Some(digit(p[0])? << 4 | digit(p[1])?)).collect()
}

// ---------- the files ----------

/// What a story's sentences are made with.
#[derive(Debug, Clone)]
pub struct Making {
    pub region: String,
    pub model: String,
    pub voice: String,
    /// How fast the voice speaks (MiniMax: 0.5 to 2, 1 = its natural speed).
    pub speed: f32,
    /// The feeling it reads with ("calm", "happy"...), or empty: MiniMax chooses from the text.
    pub emotion: String,
}

/// The feelings MiniMax can be asked for. The last two only its 2.6 models have.
const EMOTIONS: [&str; 9] = ["happy", "sad", "angry", "fearful", "disgusted", "surprised", "calm", "fluent", "whisper"];

impl Making {
    /// The feeling to ask for, if one is chosen that this model has (none: MiniMax chooses).
    fn emotion(&self) -> Option<&str> {
        let e = self.emotion.trim();
        let only_26 = matches!(e, "fluent" | "whisper");
        (EMOTIONS.contains(&e) && (!only_26 || self.model.contains("2.6"))).then_some(e)
    }

    /// The speed as it is sent and as it names files: two decimals, within MiniMax's range.
    fn speed(&self) -> f32 {
        if self.speed.is_finite() {
            (self.speed.clamp(0.5, 2.0) * 100.0).round() / 100.0
        } else {
            1.0
        }
    }
}

fn folder(app_data: &Path) -> PathBuf {
    app_data.join(FOLDER)
}

/// A sentence as it is sent and as its file is named: without the space around it.
fn tidy(text: &str) -> &str {
    text.trim()
}

/// Whether there is anything in it to say (a line of punctuation has no file).
pub fn speakable(text: &str) -> bool {
    text.chars().any(char::is_alphanumeric)
}

fn file_of(app_data: &Path, making: &Making, text: &str) -> PathBuf {
    let mut h = Sha256::new();
    for part in ["minimax", &making.model, &making.voice, tidy(text)] {
        h.update(part.as_bytes());
        h.update([0]);
    }
    // (the natural speed adds nothing, so what was made before there was a choice still counts)
    if making.speed() != 1.0 {
        h.update(format!("speed {:.2}", making.speed()).as_bytes());
    }
    if let Some(emotion) = making.emotion() {
        h.update(format!("emotion {emotion}").as_bytes());
    }
    let name: String = h.finalize().iter().take(20).map(|b| format!("{b:02x}")).collect();
    folder(app_data).join(format!("{name}.mp3"))
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// Sentences that have something to say.
    pub total: usize,
    /// Of those, the ones whose sound is made.
    pub have: usize,
    /// Characters in the ones still to make (what making them will be counted as, about).
    pub characters: usize,
}

pub fn status(app_data: &Path, making: &Making, parts: &[String]) -> Status {
    let mut s = Status::default();
    let mut have = Vec::new();
    for text in distinct(parts) {
        s.total += 1;
        let file = file_of(app_data, making, text);
        if file.exists() {
            s.have += 1;
            have.push(stem(&file));
        } else {
            s.characters += tidy(text).chars().count();
        }
    }
    // (sound from before there was a record is entered in it as its stories are opened)
    record(app_data, making, &have);
    s
}

// ---------- the record of what has been made, and with what ----------
//
// A file's name says nothing a person can read, so beside the files there is a list of the
// choices (model, voice, speed, feeling) that sound has been made with, and which files each
// made: what "lock in" (Settings) offers, so that nothing is chosen that would have to be
// made, and paid for, anew.

const RECORD: &str = "made.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Record {
    combos: Vec<Combo>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Combo {
    model: String,
    voice: String,
    speed: f32,
    emotion: String,
    /// The sound files (their names without ".mp3") made with these choices.
    files: Vec<String>,
}

fn stem(file: &Path) -> String {
    file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

fn load_record(app_data: &Path) -> Record {
    std::fs::read(folder(app_data).join(RECORD)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

/// (The record is a convenience: if it can't be written the sound still plays, so a failure
/// here is not passed on.)
fn save_record(app_data: &Path, record: &Record) {
    let path = folder(app_data).join(RECORD);
    let tmp = path.with_extension("part");
    if let Ok(bytes) = serde_json::to_vec(record) {
        if std::fs::write(&tmp, bytes).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }
}

/// Enter these files under the choices that made them (nothing is written if all are there).
fn record(app_data: &Path, making: &Making, stems: &[String]) {
    if stems.is_empty() {
        return;
    }
    let mut all = load_record(app_data);
    let (speed, emotion) = (making.speed(), making.emotion().unwrap_or(""));
    let at = all.combos.iter().position(|c| c.model == making.model && c.voice == making.voice && c.speed == speed && c.emotion == emotion);
    let combo = match at {
        Some(i) => &mut all.combos[i],
        None => {
            all.combos.push(Combo { model: making.model.clone(), voice: making.voice.clone(), speed, emotion: emotion.to_string(), files: Vec::new() });
            all.combos.last_mut().expect("just pushed")
        }
    };
    let known: std::collections::HashSet<&str> = combo.files.iter().map(String::as_str).collect();
    let new: Vec<String> = stems.iter().filter(|s| !known.contains(s.as_str())).cloned().collect();
    if new.is_empty() {
        return;
    }
    combo.files.extend(new);
    save_record(app_data, &all);
}

/// Choices that sound has been made with, and how many sentences each has.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MadeWith {
    pub model: String,
    pub voice: String,
    pub speed: f32,
    pub emotion: String,
    pub sentences: usize,
}

/// What has been made, most sentences first. Only files that are still there count.
pub fn made_with(app_data: &Path) -> Vec<MadeWith> {
    let dir = folder(app_data);
    let mut out: Vec<MadeWith> = load_record(app_data)
        .combos
        .into_iter()
        .map(|c| MadeWith {
            sentences: c.files.iter().filter(|f| dir.join(format!("{f}.mp3")).exists()).count(),
            model: c.model,
            voice: c.voice,
            speed: c.speed,
            emotion: c.emotion,
        })
        .filter(|m| m.sentences > 0)
        .collect();
    out.sort_by(|a, b| b.sentences.cmp(&a.sentences));
    out
}

/// Drop from the record the files that are gone (after a story's sound was removed).
fn tidy_record(app_data: &Path) {
    let dir = folder(app_data);
    let mut all = load_record(app_data);
    for c in &mut all.combos {
        c.files.retain(|f| dir.join(format!("{f}.mp3")).exists());
    }
    all.combos.retain(|c| !c.files.is_empty());
    save_record(app_data, &all);
}

/// The sentences to say, each once (a line repeated in a story is one file).
fn distinct(parts: &[String]) -> Vec<&str> {
    let mut seen = std::collections::HashSet::new();
    parts.iter().map(|p| tidy(p)).filter(|p| speakable(p) && seen.insert(*p)).collect()
}

/// A sentence's sound, if it has been made.
pub fn clip(app_data: &Path, making: &Making, text: &str) -> Option<Vec<u8>> {
    std::fs::read(file_of(app_data, making, text)).ok().filter(|b| !b.is_empty())
}

/// Set by `cancel`; a making that sees it stops after the sentence it is on.
static STOP: AtomicBool = AtomicBool::new(false);
/// One making at a time (two would ask for the same sentences twice).
static MAKING: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn cancel() {
    STOP.store(true, Ordering::SeqCst);
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Made {
    /// Sentences made just now.
    pub made: usize,
    /// Characters MiniMax counted for them.
    pub characters: u64,
    /// Stopped before the end (asked to, or something failed: `error` says what).
    pub stopped: bool,
    pub error: Option<String>,
    pub status: Status,
}

/// Make the sound of every sentence that doesn't have one yet, one after another;
/// `progress(done, total)` after each. What was made before a stop or a failure is kept, so
/// making again goes on from there.
pub fn make(app_data: &Path, key: &str, making: &Making, parts: &[String], mut progress: impl FnMut(usize, usize)) -> AppResult<Made> {
    let Ok(_one) = MAKING.try_lock() else {
        return Err(AppError::msg("Another story's audio is being made. Wait for it, or stop it first."));
    };
    STOP.store(false, Ordering::SeqCst);
    // (the folder may be a link to another one, as the development copy's is: Windows then
    // answers "already exists" to making it, which is all that was wanted)
    match std::fs::create_dir_all(folder(app_data)) {
        Err(e) if e.kind() != std::io::ErrorKind::AlreadyExists => return Err(e.into()),
        _ => {}
    }
    let all = distinct(parts);
    let mut out = Made::default();
    let mut done = all.iter().filter(|t| file_of(app_data, making, t).exists()).count();
    progress(done, all.len());
    // (entered in the record a few at a time, and at the end)
    let mut fresh: Vec<String> = Vec::new();
    for text in &all {
        let path = file_of(app_data, making, text);
        if path.exists() {
            continue;
        }
        if STOP.load(Ordering::SeqCst) {
            out.stopped = true;
            break;
        }
        if text.chars().count() > MAX_TEXT {
            out.stopped = true;
            out.error = Some("A sentence in this story is too long to be made into sound.".into());
            break;
        }
        match synthesize(key, making, text) {
            Ok(spoken) => {
                // (whole or not at all: a file cut short would play as a broken sentence for ever)
                let tmp = path.with_extension("part");
                if let Some(timing) = &spoken.timing {
                    std::fs::write(&tmp, timing)?;
                    std::fs::rename(&tmp, timing_of(&path))?;
                }
                std::fs::write(&tmp, &spoken.audio)?;
                std::fs::rename(&tmp, &path)?;
                fresh.push(stem(&path));
                if fresh.len() >= 25 {
                    record(app_data, making, &fresh);
                    fresh.clear();
                }
                out.made += 1;
                out.characters += spoken.characters;
                done += 1;
                progress(done, all.len());
            }
            Err(e) => {
                out.stopped = true;
                out.error = Some(e.to_string());
                break;
            }
        }
    }
    record(app_data, making, &fresh);
    out.status = status(app_data, making, parts);
    Ok(out)
}

/// Remove these sentences' sound (in the voice in use). Returns how many files went.
pub fn remove(app_data: &Path, making: &Making, parts: &[String]) -> usize {
    let gone = |t: &&str| {
        let sound = file_of(app_data, making, t);
        let _ = std::fs::remove_file(timing_of(&sound));
        std::fs::remove_file(sound).is_ok()
    };
    let n = distinct(parts).into_iter().filter(gone).count();
    tidy_record(app_data);
    n
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub files: usize,
    pub bytes: u64,
    pub folder: String,
}

/// Everything that has been made, in every voice.
pub fn usage(app_data: &Path) -> Usage {
    let dir = folder(app_data);
    let mut u = Usage { folder: dir.display().to_string(), ..Default::default() };
    for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
        if e.path().extension().is_some_and(|x| x == "mp3") {
            u.files += 1;
            u.bytes += e.metadata().map(|m| m.len()).unwrap_or(0);
        }
    }
    u
}

/// Remove all of it (the folder's sound files and any left half-written; nothing else).
pub fn clear(app_data: &Path) -> AppResult<()> {
    for e in std::fs::read_dir(folder(app_data)).into_iter().flatten().flatten() {
        let path = e.path();
        if path.extension().is_some_and(|x| x == "mp3" || x == "part" || x == "json") {
            std::fs::remove_file(path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder of its own for one test, removed when the test is done.
    struct Scratch(PathBuf);
    impl Scratch {
        fn new(tag: &str) -> Scratch {
            let dir = std::env::temp_dir().join(format!("hanzi-story-audio-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Scratch(dir)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn making(voice: &str) -> Making {
        Making { region: "io".into(), model: DEFAULT_MODEL.into(), voice: voice.into(), speed: 1.0, emotion: String::new() }
    }

    #[test]
    fn words_are_placed_in_the_sentence() {
        let text = "我叫小明，今天很好。";
        // as the documentation describes it: segments, each with its words
        let raw = r#"[{"text":"我叫小明，今天很好。","text_begin":0,"text_end":10,"time_begin":0,"time_end":2100.5,
            "timestamped_words":[
              {"word":"我","word_begin":0,"word_end":1,"time_begin":0,"time_end":180},
              {"word":"叫","word_begin":1,"word_end":2,"time_begin":180,"time_end":400},
              {"word":"小明","word_begin":2,"word_end":4,"time_begin":400,"time_end":900.5},
              {"word":"今天","word_begin":50,"word_end":52,"time_begin":1100,"time_end":1500},
              {"word":"不在","word_begin":7,"word_end":9,"time_begin":1500,"time_end":1600},
              {"word":"很好","time_begin":1600,"time_end":2100}]}]"#;
        let w = parse_words(raw, text);
        let at: Vec<(usize, usize)> = w.iter().map(|x| (x.at, x.len)).collect();
        // an offset that is wrong is not believed (今天 is found), a word that isn't in the
        // sentence is left out, and one without an offset is found after the last
        assert_eq!(at, [(0, 1), (1, 1), (2, 2), (5, 2), (7, 2)]);
        assert_eq!((w[2].begin, w[2].end), (400.0, 900.5));
        // wrapped in an object, as it may also come
        let wrapped = format!(r#"{{"subtitles":{raw}}}"#);
        assert_eq!(parse_words(&wrapped, text), w);
        // sentence timing only, or nothing readable: no words
        assert!(parse_words(r#"[{"text":"我叫小明","time_begin":0,"time_end":900,"timestamped_words":null}]"#, text).is_empty());
        assert!(parse_words("not json", text).is_empty());
        // a character outside the basic plane counts as two for the page
        let w = parse_words(r#"[{"word":"好","time_begin":10,"time_end":20}]"#, "😀好");
        assert_eq!((w[0].at, w[0].len), (2, 1));
    }

    #[test]
    fn hex_is_read() {
        assert_eq!(unhex("fffb90c4"), Some(vec![0xff, 0xfb, 0x90, 0xc4]));
        assert_eq!(unhex(" FFfb \n"), Some(vec![0xff, 0xfb]));
        assert_eq!(unhex("fff"), None);
        assert_eq!(unhex("zz"), None);
        assert_eq!(unhex(""), Some(vec![]));
    }

    #[test]
    fn a_sentence_has_one_file_per_voice() {
        let dir = Scratch::new("names");
        let (a, b) = (making("one"), making("two"));
        // the same sentence, with or without the space around it, is one file
        assert_eq!(file_of(dir.path(), &a, "你好。"), file_of(dir.path(), &a, "  你好。\n"));
        assert_ne!(file_of(dir.path(), &a, "你好。"), file_of(dir.path(), &a, "你好！"));
        assert_ne!(file_of(dir.path(), &a, "你好。"), file_of(dir.path(), &b, "你好。"));
        let other_model = Making { model: "speech-2.8-turbo".into(), ..a.clone() };
        assert_ne!(file_of(dir.path(), &a, "你好。"), file_of(dir.path(), &other_model, "你好。"));
        assert!(file_of(dir.path(), &a, "你好。").starts_with(dir.path().join(FOLDER)));
        // each speed has its own files; the natural speed is the file from before speeds
        let slow = Making { speed: 0.8, ..a.clone() };
        assert_ne!(file_of(dir.path(), &a, "你好。"), file_of(dir.path(), &slow, "你好。"));
        assert_eq!(file_of(dir.path(), &slow, "你好。"), file_of(dir.path(), &Making { speed: 0.801, ..a.clone() }, "你好。"));
        assert_eq!(Making { speed: 9.0, ..a.clone() }.speed(), 2.0);
        assert_eq!(Making { speed: f32::NAN, ..a.clone() }.speed(), 1.0);
        // and each feeling its own; one this model hasn't, or isn't one, is the same as none
        let calm = Making { emotion: "calm".into(), ..a.clone() };
        assert_ne!(file_of(dir.path(), &a, "你好。"), file_of(dir.path(), &calm, "你好。"));
        assert_ne!(file_of(dir.path(), &calm, "你好。"), file_of(dir.path(), &Making { emotion: "happy".into(), ..a.clone() }, "你好。"));
        for none in ["", "whisper", "fluent", "bored"] {
            let m = Making { emotion: none.into(), ..a.clone() };
            assert_eq!(m.emotion(), None, "{none}");
            assert_eq!(file_of(dir.path(), &a, "你好。"), file_of(dir.path(), &m, "你好。"));
        }
        assert_eq!(Making { emotion: "whisper".into(), model: "speech-2.6-hd".into(), ..a.clone() }.emotion(), Some("whisper"));
        let mut h = Sha256::new();
        for part in ["minimax", DEFAULT_MODEL, "one", "你好。"] {
            h.update(part.as_bytes());
            h.update([0]);
        }
        let name: String = h.finalize().iter().take(20).map(|b| format!("{b:02x}")).collect();
        assert_eq!(file_of(dir.path(), &a, "你好。").file_stem().unwrap().to_str().unwrap(), name);
    }

    #[test]
    fn what_is_made_is_counted_played_and_removed() {
        let dir = Scratch::new("files");
        let m = making("one");
        let parts: Vec<String> = ["你好。", "“……”", "你好。", "再见！", "  "].iter().map(|s| s.to_string()).collect();
        // two sentences to say: the repeat is one file, and punctuation alone has none
        let s = status(dir.path(), &m, &parts);
        assert_eq!((s.total, s.have, s.characters), (2, 0, 6));
        assert!(clip(dir.path(), &m, "你好。").is_none());

        std::fs::create_dir_all(folder(dir.path())).unwrap();
        std::fs::write(file_of(dir.path(), &m, "你好。"), b"mp3").unwrap();
        let s = status(dir.path(), &m, &parts);
        assert_eq!((s.total, s.have, s.characters), (2, 1, 3));
        assert_eq!(clip(dir.path(), &m, " 你好。").as_deref(), Some(&b"mp3"[..]));
        // another voice has nothing yet
        assert_eq!(status(dir.path(), &making("two"), &parts).have, 0);

        let u = usage(dir.path());
        assert_eq!((u.files, u.bytes), (1, 3));
        // something else in the folder is left alone
        std::fs::write(folder(dir.path()).join("notes.txt"), b"mine").unwrap();
        // a sentence's timing sits beside its sound, and goes with it
        std::fs::write(timing_of(&file_of(dir.path(), &m, "你好。")), r#"[{"word":"你好","time_begin":0,"time_end":500}]"#).unwrap();
        assert_eq!(words(dir.path(), &m, " 你好。"), [Word { at: 0, len: 2, begin: 0.0, end: 500.0 }]);
        assert!(words(dir.path(), &m, "再见！").is_empty());
        assert_eq!(usage(dir.path()).files, 1);
        // what has been made, and with what, is on record (here: found when the story was looked at)
        assert_eq!(made_with(dir.path()), [MadeWith { model: DEFAULT_MODEL.into(), voice: "one".into(), speed: 1.0, emotion: String::new(), sentences: 1 }]);
        let calm = Making { emotion: "calm".into(), speed: 0.8, ..m.clone() };
        std::fs::write(file_of(dir.path(), &calm, "你好。"), b"mp3").unwrap();
        std::fs::write(file_of(dir.path(), &calm, "再见！"), b"mp3").unwrap();
        status(dir.path(), &calm, &parts);
        status(dir.path(), &calm, &parts); // (twice enters nothing twice)
        let made = made_with(dir.path());
        assert_eq!(made.iter().map(|x| (x.emotion.as_str(), x.speed, x.sentences)).collect::<Vec<_>>(), [("calm", 0.8, 2), ("", 1.0, 1)]);
        assert_eq!(remove(dir.path(), &calm, &parts), 2);
        assert_eq!(made_with(dir.path()).len(), 1);
        assert_eq!(remove(dir.path(), &m, &parts), 1);
        assert!(made_with(dir.path()).is_empty());
        assert!(!timing_of(&file_of(dir.path(), &m, "你好。")).exists());
        assert_eq!(status(dir.path(), &m, &parts).have, 0);
        std::fs::write(file_of(dir.path(), &m, "再见！"), b"mp3").unwrap();
        clear(dir.path()).unwrap();
        assert_eq!(usage(dir.path()).files, 0);
        assert!(folder(dir.path()).join("notes.txt").exists());
    }
}
