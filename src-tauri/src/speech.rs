//! Spoken answers: recognise what the learner says and check it against a card.
//!
//! Two engines:
//! - **Windows** (Windows.Media.SpeechRecognition, zh-Hans-CN): picks from a short phrase
//!   list - the card's answers plus distractors (the same syllables in other tones, other
//!   words in the session). Very reliable for a learner's accent; offline; Windows only.
//! - **Whisper** (see whisper.rs): transcribes freely on any platform; the transcript is
//!   compared with the answer by characters, then by toned pinyin.
//!
//! The "speech_engine" setting picks one ("auto" = Windows when available).
//!
//! On an iPhone there is only Apple's recognizer (apple_speech.rs, engine "apple"), which
//! transcribes freely like Whisper.

use crate::cards;
use crate::dictionary;
use crate::error::AppResult;
use crate::pinyin;
use rusqlite::Connection;
use serde::Serialize;

/// Whether spoken answers can be checked on this machine, and with which engine.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechStatus {
    pub available: bool,
    /// Why not, when unavailable.
    pub reason: Option<String>,
    /// The engine spoken answers will use: "windows", "whisper" or "apple" (an iPhone).
    pub engine: Option<String>,
    /// The Windows recognizer is usable.
    pub windows: bool,
    /// Downloaded Whisper models.
    pub whisper_models: Vec<String>,
    /// The Whisper model that will be used.
    pub whisper_model: Option<String>,
}

/// The outcome of one spoken attempt.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpokenAnswer {
    /// "heard", "silence" (nothing said), "unclear" (said something not on the list,
    /// or too unclear to match) or "cancelled".
    pub status: String,
    /// What was recognised (characters), empty if nothing.
    pub heard: String,
    pub heard_pinyin: String,
    /// "high" | "medium" | "low" | "rejected" | "" (nothing heard)
    pub confidence: String,
    pub correct: bool,
    /// Suggested rating 1-4 for this attempt.
    pub suggested: u8,
    /// "tone" when the syllables were right but a tone was wrong (Whisper only).
    pub note: String,
}

impl SpokenAnswer {
    /// Nothing was said.
    #[cfg_attr(not(target_os = "ios"), allow(dead_code))]
    pub fn silence() -> Self {
        SpokenAnswer {
            status: "silence".into(),
            heard: String::new(),
            heard_pinyin: String::new(),
            confidence: String::new(),
            correct: false,
            suggested: 1,
            note: String::new(),
        }
    }
}

// ---------- saying the meaning in English ----------

const FILLER: [&str; 22] = [
    "to", "a", "an", "the", "of", "be", "is", "are", "sb", "sth", "someone", "something", "one", "ones",
    "it", "its", "etc", "or", "and", "for", "in", "on",
];

/// Lowercase words without punctuation, with simple plural/verb endings removed.
fn english_words(s: &str) -> Vec<String> {
    let cleaned: String =
        s.to_lowercase().chars().map(|c| if c.is_alphanumeric() || c == '\'' { c } else { ' ' }).collect();
    cleaned
        .split_whitespace()
        .map(|w| w.trim_matches('\'').trim_end_matches("'s").to_string())
        .filter(|w| !w.is_empty())
        .map(|w| stem(&w))
        .collect()
}

fn stem(w: &str) -> String {
    for (suffix, keep) in [("ies", "y"), ("ing", ""), ("ed", ""), ("es", ""), ("s", "")] {
        if w.len() > suffix.len() + 2 {
            if let Some(base) = w.strip_suffix(suffix) {
                return format!("{base}{keep}");
            }
        }
    }
    w.to_string()
}

fn content(words: &[String]) -> Vec<String> {
    words.iter().filter(|w| !FILLER.contains(&w.as_str())).cloned().collect()
}

/// The senses a meaning lists ("to like; to be fond of" -> two), without bracketed notes.
pub fn meaning_senses(meaning: &str) -> Vec<String> {
    let mut no_brackets = String::new();
    let mut depth = 0;
    for c in meaning.chars() {
        match c {
            '(' | '[' | '（' => depth += 1,
            ')' | ']' | '）' => depth = (depth - 1).max(0),
            _ if depth == 0 => no_brackets.push(c),
            _ => {}
        }
    }
    no_brackets
        .split([';', ',', '/', '；', '，'])
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Judge an English answer against a card's meaning: right when it says one of the senses
/// (all of that sense's words, or the answer's words all belong to one sense and cover at
/// least half of it). Lenient on purpose: the learner can always rate it themselves.
pub fn judge_meaning(meaning: &str, heard: &str) -> SpokenAnswer {
    let said = content(&english_words(heard));
    let mut answer = SpokenAnswer {
        status: if said.is_empty() { "unclear" } else { "heard" }.into(),
        heard: heard.trim().to_string(),
        heard_pinyin: String::new(),
        confidence: String::new(),
        correct: false,
        suggested: 1,
        note: String::new(),
    };
    if said.is_empty() {
        return answer;
    }
    let right = meaning_senses(meaning).iter().any(|sense| {
        let want = content(&english_words(sense));
        if want.is_empty() {
            return false;
        }
        let all_said = want.iter().all(|w| said.contains(w));
        let hits = said.iter().filter(|w| want.contains(w)).count();
        all_said || (hits == said.len() && hits * 2 >= want.len())
    });
    answer.correct = right;
    answer.suggested = if right { 3 } else { 1 };
    answer
}

/// Spoken forms that count as the card's answer: its characters, with and without
/// optional parts ("点（钟）" -> 点钟, 点). Punctuation and Latin letters are dropped.
pub fn answer_forms(hanzi: &str) -> Vec<String> {
    let mut with = String::new();
    let mut without = String::new();
    let mut depth = 0;
    for c in hanzi.chars() {
        match c {
            '(' | '（' => depth += 1,
            ')' | '）' => depth = (depth - 1).max(0),
            c if pinyin::is_hanzi(c) => {
                with.push(c);
                if depth == 0 {
                    without.push(c);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for f in [with, without] {
        if !f.is_empty() && !out.contains(&f) {
            out.push(f);
        }
    }
    out
}

/// Phrases that sound like the answer but aren't it: the same syllables in other tones
/// (买 mǎi / 卖 mài), from the dictionary, most common first.
pub fn tone_distractors(conn: &Connection, hanzi: &str, card_pinyin: &str, max: usize) -> AppResult<Vec<String>> {
    Ok(tone_variants(conn, hanzi, card_pinyin, max)?.into_iter().map(|(simp, _)| simp).collect())
}

/// Like [`tone_distractors`], with each word's numbered pinyin ("ma4").
pub fn tone_variants(conn: &Connection, hanzi: &str, card_pinyin: &str, max: usize) -> AppResult<Vec<(String, String)>> {
    let forms = answer_forms(hanzi);
    let Some(main) = forms.first() else { return Ok(vec![]) };
    let bare = pinyin::toneless_key(card_pinyin);
    if bare.is_empty() {
        return Ok(vec![]);
    }
    let toned = pinyin::toned_key(&pinyin::marks_to_numbered(card_pinyin));
    let len = main.chars().count() as i64;
    let mut stmt = conn.prepare(
        "SELECT simplified, pinyin_num FROM dictionary
         WHERE pinyin_key = ?1 AND length(simplified) = ?2
         ORDER BY score DESC LIMIT 60",
    )?;
    let rows = stmt.query_map(rusqlite::params![bare, len], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    let mut out: Vec<(String, String)> = Vec::new();
    for row in rows {
        let (simp, num) = row?;
        if pinyin::toned_key(&num) == toned || forms.contains(&simp) || out.iter().any(|(s, _)| *s == simp) {
            continue;
        }
        if !simp.chars().all(pinyin::is_hanzi) {
            continue;
        }
        out.push((simp, num));
        if out.len() >= max {
            break;
        }
    }
    Ok(out)
}

/// All phrases the recognizer should listen for, answer first.
pub fn phrases_for(conn: &Connection, hanzi: &str, card_pinyin: &str, others: &[String]) -> AppResult<Vec<String>> {
    let mut out = answer_forms(hanzi);
    for d in tone_distractors(conn, hanzi, card_pinyin, 6)? {
        if !out.contains(&d) {
            out.push(d);
        }
    }
    for o in others.iter().flat_map(|o| answer_forms(o)).take(40) {
        if !out.contains(&o) {
            out.push(o);
        }
        if out.len() >= 50 {
            break;
        }
    }
    Ok(out)
}

/// Grade what was heard: correct and clear -> Good (3); correct but unsure -> Hard (2);
/// anything else -> Again (1). The learner can always pick another rating.
pub fn grade(heard: &str, accepted: &[String], confidence: &str) -> (bool, u8) {
    let heard: String = heard.chars().filter(|c| pinyin::is_hanzi(*c)).collect();
    let correct = !heard.is_empty() && accepted.iter().any(|a| *a == heard);
    let suggested = match (correct, confidence) {
        (true, "high" | "medium") => 3,
        (true, _) => 2,
        _ => 1,
    };
    (correct, suggested)
}

/// Pinyin to show for what was heard.
fn heard_pinyin(conn: &Connection, heard: &str, card_hanzi: &str, card_pinyin: &str) -> AppResult<String> {
    if heard.is_empty() {
        return Ok(String::new());
    }
    if answer_forms(card_hanzi).first().map(String::as_str) == Some(heard) {
        return Ok(card_pinyin.to_string());
    }
    Ok(dictionary::lookup(conn, heard)?.into_iter().next().map(|e| e.pinyin).unwrap_or_default())
}

/// Everything needed to listen for one card (computed with the database, then the
/// database is released while listening).
pub struct ListenPlan {
    pub phrases: Vec<String>,
    pub accepted: Vec<String>,
    pub hanzi: String,
    pub pinyin: String,
}

pub fn plan(conn: &Connection, card_id: i64, others: &[String]) -> AppResult<ListenPlan> {
    let card = cards::get_card(conn, card_id)?;
    Ok(ListenPlan {
        phrases: phrases_for(conn, &card.hanzi, &card.pinyin, others)?,
        accepted: answer_forms(&card.hanzi),
        hanzi: card.hanzi,
        pinyin: card.pinyin,
    })
}

/// Turn a recognition into an answer (needs the database for the heard word's pinyin).
pub fn judge(conn: &Connection, plan: &ListenPlan, heard: Heard) -> AppResult<SpokenAnswer> {
    let (correct, suggested) = grade(&heard.text, &plan.accepted, &heard.confidence);
    let status = match heard.status {
        HeardStatus::Cancelled => "cancelled",
        HeardStatus::Silence => "silence",
        HeardStatus::Heard if heard.text.is_empty() || heard.confidence == "rejected" => "unclear",
        HeardStatus::Heard => "heard",
    };
    Ok(SpokenAnswer {
        status: status.to_string(),
        heard_pinyin: heard_pinyin(conn, &heard.text, &plan.hanzi, &plan.pinyin)?,
        heard: heard.text,
        confidence: heard.confidence,
        correct,
        suggested,
        note: String::new(),
    })
}

// ---------- free transcription (Whisper) ----------

/// Simplified characters for traditional ones (Whisper sometimes writes traditional).
pub fn to_simplified(conn: &Connection, text: &str) -> AppResult<String> {
    // A character that is already a simplified form stays as it is: CC-CEDICT also lists
    // some simplified characters as the "traditional" side of another entry (么 -> 幺).
    let mut stmt = conn.prepare_cached(
        "SELECT CASE WHEN EXISTS(SELECT 1 FROM dictionary WHERE simplified = ?1) THEN ?1
                ELSE (SELECT simplified FROM dictionary WHERE traditional = ?1 AND length(traditional) = 1
                      ORDER BY score DESC LIMIT 1) END",
    )?;
    let mut out = String::new();
    for c in text.chars() {
        if pinyin::is_hanzi(c) {
            let s: Option<String> = stmt.query_row([c.to_string()], |r| r.get(0)).ok().flatten();
            out.push_str(s.as_deref().unwrap_or(&c.to_string()));
        } else {
            out.push(c);
        }
    }
    Ok(out)
}

/// Toned pinyin key of characters ("ni3hao3"), via dictionary words; None if a
/// character isn't in the dictionary.
fn toned_key_of(conn: &Connection, hanzi: &str) -> AppResult<Option<String>> {
    let mut key = String::new();
    for seg in dictionary::segment(conn, hanzi, &[])? {
        match seg.entry {
            Some(e) => key.push_str(&pinyin::toned_key(&e.pinyin_num)),
            None if seg.text.chars().any(pinyin::is_hanzi) => return Ok(None),
            None => {}
        }
    }
    Ok((!key.is_empty()).then_some(key))
}

/// Judge a free transcription against the card: the same characters, or (for words that
/// sound alike, 他/她/它) the same toned pinyin, counts as correct. The right syllables
/// with a wrong tone are flagged with note "tone".
/// Save 16 kHz mono samples as a 16-bit WAV file.
pub fn write_wav(path: &std::path::Path, pcm: &[f32]) -> std::io::Result<()> {
    let data_len = (pcm.len() * 2) as u32;
    let mut b = Vec::with_capacity(44 + pcm.len() * 2);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes()); // PCM
    b.extend_from_slice(&1u16.to_le_bytes()); // mono
    b.extend_from_slice(&16000u32.to_le_bytes());
    b.extend_from_slice(&32000u32.to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for v in pcm {
        b.extend_from_slice(&((v.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    std::fs::write(path, b)
}

pub fn judge_text(conn: &Connection, plan: &ListenPlan, text: &str) -> AppResult<SpokenAnswer> {
    let heard: String = to_simplified(conn, text)?.chars().filter(|c| pinyin::is_hanzi(*c)).collect();
    if heard.is_empty() {
        return Ok(SpokenAnswer {
            status: "unclear".into(),
            heard: String::new(),
            heard_pinyin: String::new(),
            confidence: String::new(),
            correct: false,
            suggested: 1,
            note: String::new(),
        });
    }
    let len = |s: &str| s.chars().count();
    let mut correct = plan.accepted.contains(&heard)
        // a little extra around the answer ("是学生", "学生了") still counts
        || plan.accepted.iter().any(|a| heard.contains(a.as_str()) && len(&heard) <= len(a) + 2);
    let mut note = String::new();
    if !correct {
        let want = pinyin::toned_key(&pinyin::marks_to_numbered(&plan.pinyin));
        if let Some(got) = toned_key_of(conn, &heard)? {
            if got == want {
                correct = true; // a homophone: the pronunciation was right
            } else if pinyin::toneless_key(&got) == pinyin::toneless_key(&want) {
                note = "tone".into();
            }
        }
    }
    Ok(SpokenAnswer {
        status: "heard".into(),
        heard_pinyin: heard_pinyin(conn, &heard, &plan.hanzi, &plan.pinyin)?,
        heard,
        confidence: String::new(),
        correct,
        suggested: if correct { 3 } else { 1 },
        note,
    })
}

/// Which engine spoken answers use, from the "speech_engine" setting and what is
/// installed. (On an iPhone: Apple's recognizer, always.)
pub fn engine_status(conn: &Connection, app_data: &std::path::Path) -> SpeechStatus {
    #[cfg(target_os = "ios")]
    {
        let _ = (conn, app_data);
        let reason = crate::apple_speech::unavailable();
        SpeechStatus {
            available: reason.is_none(),
            engine: reason.is_none().then(|| "apple".to_string()),
            reason,
            windows: false,
            whisper_models: vec![],
            whisper_model: None,
        }
    }
    #[cfg(not(target_os = "ios"))]
    computer_engine_status(conn, app_data)
}

#[cfg(not(target_os = "ios"))]
fn computer_engine_status(conn: &Connection, app_data: &std::path::Path) -> SpeechStatus {
    let windows = platform::status();
    let installed: Vec<String> = crate::whisper::list(app_data).into_iter().filter(|m| m.installed).map(|m| m.name).collect();
    let wanted_model = crate::db::get_setting(conn, "whisper_model").ok().flatten().unwrap_or_else(|| "base".into());
    let mut whisper_model = if installed.contains(&wanted_model) { Some(wanted_model) } else { installed.first().cloned() };
    // A graphics-card model on the processor takes several seconds per answer: use the
    // largest other model instead while there's no GPU.
    if whisper_model.as_deref().is_some_and(crate::whisper::needs_gpu) && !crate::whisper::using_gpu() {
        if let Some(cpu) = installed.iter().rev().find(|m| !crate::whisper::needs_gpu(m)) {
            whisper_model = Some(cpu.clone());
        }
    }
    let choice = crate::db::get_setting(conn, "speech_engine").ok().flatten().unwrap_or_else(|| "auto".into());
    let engine = match choice.as_str() {
        "windows" if windows.available => Some("windows"),
        "whisper" if whisper_model.is_some() => Some("whisper"),
        "windows" | "whisper" => None,
        // Whisper hears what was actually said; the Windows recognizer must pick from a
        // list (including wrong-tone look-alikes), so it prefers Whisper when it's there.
        _ if whisper_model.is_some() => Some("whisper"),
        _ if windows.available => Some("windows"),
        _ => None,
    };
    let reason = match (engine, choice.as_str()) {
        (Some(_), _) => None,
        (None, "whisper") => Some("Download a Whisper model in Settings › Speech recognition.".to_string()),
        (None, _) => Some(format!(
            "{} Or download a Whisper model in Settings › Speech recognition.",
            windows.reason.clone().unwrap_or_default()
        )),
    };
    SpeechStatus {
        available: engine.is_some(),
        reason,
        engine: engine.map(String::from),
        windows: windows.available,
        whisper_models: installed,
        whisper_model,
    }
}

#[derive(Debug, PartialEq)]
#[cfg_attr(not(windows), allow(dead_code))] // only the Windows recognizer reports these
pub enum HeardStatus {
    Heard,
    Silence,
    Cancelled,
}

#[derive(Debug)]
pub struct Heard {
    pub status: HeardStatus,
    pub text: String,
    pub confidence: String,
}

// ---------- Windows recognizer ----------

#[cfg(windows)]
mod platform {
    use super::{Heard, HeardStatus, SpeechStatus};
    use std::sync::Mutex;
    use windows::core::HSTRING;
    use windows::Foundation::TimeSpan;
    use windows::Globalization::Language;
    use windows::Media::SpeechRecognition::{
        SpeechRecognitionConfidence as Confidence, SpeechRecognitionListConstraint, SpeechRecognitionResult,
        SpeechRecognitionResultStatus as Status, SpeechRecognizer,
    };
    use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};
    use windows_collections::IIterable;
    use windows_future::IAsyncOperation;

    const LANGUAGE: &str = "zh-Hans-CN";
    /// The recognition in progress, so it can be cancelled.
    static CURRENT: Mutex<Option<IAsyncOperation<SpeechRecognitionResult>>> = Mutex::new(None);

    fn init() {
        // Already-initialised threads return an error we can ignore.
        let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
    }

    fn secs(s: f64) -> TimeSpan {
        TimeSpan { Duration: (s * 10_000_000.0) as i64 }
    }

    fn explain(e: &windows::core::Error) -> String {
        match e.code().0 as u32 {
            0x80070005 => "Windows blocked the microphone. Turn on Settings › Privacy & security › Microphone › \
                           \"Let desktop apps access your microphone\", then try again."
                .into(),
            0x8004503A => "Chinese speech recognition isn't installed. Add Chinese (Simplified) with its speech \
                           option in Settings › Time & language › Language & region."
                .into(),
            _ => format!("Speech recognition failed: {}", e.message()),
        }
    }

    pub fn status() -> SpeechStatus {
        init();
        let found = SpeechRecognizer::SupportedGrammarLanguages().and_then(|langs| {
            let mut any = false;
            for l in langs {
                if l.LanguageTag()?.to_string().to_lowercase().starts_with("zh") {
                    any = true;
                }
            }
            Ok(any)
        });
        match found {
            Ok(true) => SpeechStatus::windows(true, None),
            Ok(false) => SpeechStatus::windows(
                false,
                Some(
                    "Chinese speech recognition isn't installed. Add Chinese (Simplified) with its speech option \
                     in Windows Settings › Time & language › Language & region."
                        .into(),
                ),
            ),
            Err(e) => SpeechStatus::windows(false, Some(explain(&e))),
        }
    }

    pub fn listen(phrases: &[String]) -> Result<Heard, String> {
        init();
        let run = || -> windows::core::Result<Heard> {
            let lang = Language::CreateLanguage(&HSTRING::from(LANGUAGE))?;
            let recognizer = SpeechRecognizer::Create(&lang)?;
            let list = IIterable::<HSTRING>::from(phrases.iter().map(HSTRING::from).collect::<Vec<_>>());
            let constraint = SpeechRecognitionListConstraint::Create(&list)?;
            recognizer.Constraints()?.Append(&constraint)?;
            let compiled = recognizer.CompileConstraintsAsync()?.join()?;
            if compiled.Status()? != Status::Success {
                return Err(windows::core::Error::new(
                    windows::core::HRESULT(0x8004503Au32 as i32),
                    "could not prepare the phrase list",
                ));
            }
            let t = recognizer.Timeouts()?;
            t.SetInitialSilenceTimeout(secs(6.0))?;
            t.SetEndSilenceTimeout(secs(0.9))?;
            t.SetBabbleTimeout(secs(8.0))?;

            let op = recognizer.RecognizeAsync()?;
            *CURRENT.lock().unwrap_or_else(|e| e.into_inner()) = Some(op.clone());
            let result = op.join();
            CURRENT.lock().unwrap_or_else(|e| e.into_inner()).take();
            let result = match result {
                Ok(r) => r,
                // cancelled from speech_cancel
                Err(e) if e.code().0 as u32 == 0x800704C7 => {
                    return Ok(Heard { status: HeardStatus::Cancelled, text: String::new(), confidence: String::new() })
                }
                Err(e) => return Err(e),
            };
            let status = result.Status()?;
            if status == Status::TimeoutExceeded {
                return Ok(Heard { status: HeardStatus::Silence, text: String::new(), confidence: String::new() });
            }
            if status == Status::UserCanceled {
                return Ok(Heard { status: HeardStatus::Cancelled, text: String::new(), confidence: String::new() });
            }
            if status == Status::MicrophoneUnavailable {
                return Err(windows::core::Error::new(windows::core::HRESULT(0x80070005u32 as i32), "microphone unavailable"));
            }
            let confidence = match result.Confidence()? {
                Confidence::High => "high",
                Confidence::Medium => "medium",
                Confidence::Low => "low",
                _ => "rejected",
            };
            Ok(Heard {
                status: HeardStatus::Heard,
                text: result.Text()?.to_string(),
                confidence: confidence.to_string(),
            })
        };
        run().map_err(|e| explain(&e))
    }

    pub fn cancel() {
        if let Some(op) = CURRENT.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = op.Cancel();
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::{Heard, SpeechStatus};

    pub fn status() -> SpeechStatus {
        SpeechStatus::windows(false, Some("The Windows speech recognizer is only available on Windows.".into()))
    }

    pub fn listen(_phrases: &[String]) -> Result<Heard, String> {
        Err(status().reason.unwrap_or_default())
    }

    pub fn cancel() {}
}

pub use platform::{cancel, listen};

impl SpeechStatus {
    /// Status of the Windows recognizer alone.
    fn windows(available: bool, reason: Option<String>) -> Self {
        SpeechStatus {
            available,
            reason,
            engine: available.then(|| "windows".to_string()),
            windows: available,
            whisper_models: vec![],
            whisper_model: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answer_forms_handle_optional_parts() {
        assert_eq!(answer_forms("你好"), ["你好"]);
        assert_eq!(answer_forms("点（钟）"), ["点钟", "点"]);
        assert_eq!(answer_forms("块(钱)"), ["块钱", "块"]);
        assert_eq!(answer_forms("太…了"), ["太了"]);
        assert!(answer_forms("OK").is_empty());
    }

    #[test]
    fn judges_english_meanings() {
        let m = "to like; to be fond of (sth)";
        assert!(judge_meaning(m, "like").correct);
        assert!(judge_meaning(m, "To like.").correct);
        assert!(judge_meaning(m, "be fond of").correct);
        assert!(judge_meaning(m, "I like it").correct, "extra filler words are fine");
        assert!(!judge_meaning(m, "hate").correct);
        let m = "hello; hi";
        assert!(judge_meaning(m, "Hi!").correct);
        assert!(judge_meaning("teacher", "teachers").correct, "plural");
        assert!(judge_meaning("to study; to learn", "learning").correct);
        assert!(!judge_meaning("Chinese (language)", "language").correct, "notes in brackets don't count");
        assert!(judge_meaning("Chinese (language)", "Chinese").correct);
        assert!(judge_meaning("red wine", "wine").correct, "half of a two-word sense");
        assert!(!judge_meaning("red wine", "red car").correct);
        assert_eq!(judge_meaning("hello", "").status, "unclear");
    }

    #[test]
    fn grading() {
        let ok = vec!["买".to_string()];
        assert_eq!(grade("买", &ok, "high"), (true, 3));
        assert_eq!(grade("买。", &ok, "medium"), (true, 3), "punctuation ignored");
        assert_eq!(grade("买", &ok, "low"), (true, 2));
        assert_eq!(grade("卖", &ok, "high"), (false, 1));
        assert_eq!(grade("", &ok, ""), (false, 1));
    }

    #[test]
    fn distractors_are_other_tones() {
        let mut conn = crate::db::open_in_memory().unwrap();
        dictionary::import(
            &mut conn,
            "買 买 [mai3] /to buy/\n賣 卖 [mai4] /to sell/\n埋 埋 [mai2] /to bury/\n麥 麦 [mai4] /wheat/\n你 你 [ni3] /you/\n",
        )
        .unwrap();
        let d = tone_distractors(&conn, "买", "mǎi", 6).unwrap();
        assert!(d.contains(&"卖".to_string()) && d.contains(&"埋".to_string()) && d.contains(&"麦".to_string()));
        assert!(!d.contains(&"买".to_string()), "never the answer itself");

        let p = phrases_for(&conn, "买", "mǎi", &["你".to_string(), "买".to_string()]).unwrap();
        assert_eq!(p[0], "买", "answer first");
        assert!(p.contains(&"你".to_string()));
        assert_eq!(p.iter().filter(|x| *x == "买").count(), 1, "no duplicates");

        let plan = ListenPlan { phrases: p, accepted: vec!["买".into()], hanzi: "买".into(), pinyin: "mǎi".into() };
        let heard = Heard { status: HeardStatus::Heard, text: "卖".into(), confidence: "high".into() };
        let a = judge(&conn, &plan, heard).unwrap();
        assert_eq!((a.status.as_str(), a.correct, a.suggested), ("heard", false, 1));
        assert_eq!(a.heard_pinyin, "mài");
        let silent = Heard { status: HeardStatus::Silence, text: String::new(), confidence: String::new() };
        assert_eq!(judge(&conn, &plan, silent).unwrap().status, "silence");
    }

    #[test]
    fn judging_free_transcriptions() {
        let mut conn = crate::db::open_in_memory().unwrap();
        dictionary::import(
            &mut conn,
            "買 买 [mai3] /to buy/\n賣 卖 [mai4] /to sell/\n他 他 [ta1] /he/\n她 她 [ta1] /she/\n\
             學生 学生 [xue2 sheng5] /student/\n學 学 [xue2] /to learn/\n生 生 [sheng1] /to be born/\n是 是 [shi4] /to be/\n\
             麼 么 [me5] /suffix/\n么 幺 [yao1] /youngest/\n甚麼 什么 [shen2 me5] /what?/\n",
        )
        .unwrap();
        let plan = |h: &str, p: &str| ListenPlan {
            phrases: vec![],
            accepted: answer_forms(h),
            hanzi: h.into(),
            pinyin: p.into(),
        };
        let j = |pl: &ListenPlan, t: &str| judge_text(&conn, pl, t).unwrap();

        let a = j(&plan("学生", "xué sheng"), "學生。");
        assert!(a.correct, "traditional output and punctuation are fine");
        assert_eq!(a.heard, "学生");
        assert!(j(&plan("学生", "xué sheng"), "是学生").correct, "a little extra is fine");
        assert_eq!(to_simplified(&conn, "什么 甚麼").unwrap(), "什么 甚么", "simplified 么 is kept, 麼 converted");
        assert!(j(&plan("他", "tā"), "她").correct, "homophones count");
        let wrong_tone = j(&plan("买", "mǎi"), "卖");
        assert!(!wrong_tone.correct);
        assert_eq!(wrong_tone.note, "tone");
        assert_eq!(wrong_tone.heard_pinyin, "mài");
        assert_eq!(j(&plan("买", "mǎi"), "").status, "unclear");
        assert_eq!(j(&plan("买", "mǎi"), "他").note, "", "a different word is not a tone slip");
    }
}
