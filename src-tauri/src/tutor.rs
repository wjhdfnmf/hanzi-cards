//! AI conversation tutor: role-play in simple Mandarin with Claude, built around the
//! words in the decks the learner picks.
//!
//! The conversation lives here (in memory) for the length of a session. Claude can call
//! two tools: `grade_word` (the learner used one of their words, correctly or not) and
//! `suggest_card` (a useful new word to add). Both are collected for the end-of-session
//! summary, where the learner decides what to record.

use crate::ai::{self, Usage};
use crate::error::{AppError, AppResult};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Grade {
    pub hanzi: String,
    pub correct: bool,
    /// The learner's card with these characters, if any.
    pub card_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    pub hanzi: String,
    pub pinyin: String,
    pub meaning: String,
    /// Already a card in some deck.
    pub exists: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub grades: Vec<Grade>,
    pub suggestions: Vec<Suggestion>,
    pub usage: Usage,
    pub cost: f64,
    pub model: String,
    pub turns: u32,
}

/// Something that happened during a turn, streamed to the page.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TurnEvent {
    Text { text: String },
    Grade { grade: Grade },
    Suggestion { suggestion: Suggestion },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnResult {
    /// The tutor's reply (Chinese).
    pub text: String,
    /// "end_turn", "refusal", "max_tokens", "cancelled", ...
    pub stop_reason: String,
    pub cost: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Started {
    pub session_id: u64,
    pub words: usize,
}

struct Session {
    model: String,
    system: String,
    messages: Vec<Value>,
    /// characters -> card id, for the learner's words
    words: HashMap<String, i64>,
    grades: Vec<Grade>,
    suggestions: Vec<Suggestion>,
    usage: Usage,
    /// Usage since the last write to the spending record.
    unbilled: Usage,
    turns: u32,
    cancel: Arc<AtomicBool>,
}

static SESSIONS: Mutex<Option<HashMap<u64, Session>>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn with_sessions<T>(f: impl FnOnce(&mut HashMap<u64, Session>) -> T) -> T {
    let mut guard = SESSIONS.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(HashMap::new))
}

pub const SCENARIOS: [(&str, &str); 7] = [
    ("meet", "Meeting a new classmate at school: greetings, names, nationality, what you study."),
    ("cafe", "Ordering drinks and snacks at a café, asking prices, paying."),
    ("family", "Talking about your family: who is in it, ages, jobs, pets."),
    ("weekend", "Making plans for the weekend: when, where, what to do together."),
    ("fruit", "Buying fruit at a market: prices per jin, how many, giving change."),
    ("doctor", "At the doctor: saying what hurts, how long, getting advice."),
    ("free", "Free conversation about everyday life, following the learner's interests."),
];

fn tools() -> Vec<Value> {
    vec![
        json!({
            "name": "grade_word",
            "description": "Record that the learner just tried to use one of THEIR words (from the word list) in what they said. \
                Call it once per word, right after the learner's message: correct=true if they used it correctly, \
                false if they used it wrongly or clearly meant it but said something else. Don't call it for your own words.",
            "strict": true,
            "eager_input_streaming": true,
            "input_schema": {
                "type": "object",
                "properties": {
                    "hanzi": { "type": "string", "description": "The word exactly as in the word list (Simplified characters)." },
                    "correct": { "type": "boolean" }
                },
                "required": ["hanzi", "correct"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "suggest_card",
            "description": "Suggest a useful NEW word the learner doesn't have yet (not in their word list), \
                usually one you just introduced or one they needed. At most one per turn.",
            "strict": true,
            "eager_input_streaming": true,
            "input_schema": {
                "type": "object",
                "properties": {
                    "hanzi": { "type": "string", "description": "Simplified characters." },
                    "pinyin": { "type": "string", "description": "Pinyin with tone marks, syllables separated by spaces, e.g. 'kā fēi'." },
                    "meaning": { "type": "string", "description": "Short English meaning." }
                },
                "required": ["hanzi", "pinyin", "meaning"],
                "additionalProperties": false
            }
        }),
    ]
}

/// How the tutor should behave in one conversation (chosen on the setup page).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TutorOptions {
    /// "hsk1" | "hsk2" | "hsk3"
    pub level: String,
    /// "tiny" | "short" | "normal" | "long"
    pub length: String,
    /// "off" | "gentle" | "every"
    pub corrections: String,
    /// "never" | "asked" | "hints"
    pub english: String,
    /// New words the tutor may introduce per turn (0-2).
    pub new_words: u8,
    /// "tutor" (asks the questions) | "learner" (follows the learner)
    pub lead: String,
    /// Build replies only from the learner's words and basic grammar words.
    pub only_my_words: bool,
    /// Model for this conversation ("" = the one in Settings).
    pub model: String,
    /// Anything else, in the learner's words.
    pub notes: String,
}

impl Default for TutorOptions {
    fn default() -> Self {
        TutorOptions {
            level: "hsk1".into(),
            length: "normal".into(),
            corrections: "gentle".into(),
            english: "asked".into(),
            new_words: 1,
            lead: "tutor".into(),
            only_my_words: false,
            model: String::new(),
            notes: String::new(),
        }
    }
}

fn system_prompt(scenario: &str, words: &[(String, String, String, bool)], o: &TutorOptions) -> String {
    let list: Vec<String> = words
        .iter()
        .map(|(h, p, m, due)| format!("{}{h} ({p}): {m}", if *due { "* " } else { "" }))
        .collect();
    let level = match o.level.as_str() {
        "hsk2" => "an elementary learner (around HSK 2): simple sentences, common measure words, 了 and 过, basic comparisons",
        "hsk3" => "a lower-intermediate learner (around HSK 3-4): everyday topics in natural but clear Mandarin, some longer sentences and common connectives",
        _ => "a complete beginner (around HSK 1): very short, simple sentences with the most common words and patterns",
    };
    let length = match o.length.as_str() {
        "tiny" => "only one short statement or question per turn (about 3 to 10 characters, e.g. 你喜欢喝茶吗？), never more",
        "short" => "one short sentence per turn",
        "long" => "two to five sentences per turn",
        _ => "one to three short sentences per turn",
    };
    let new_words = match o.new_words {
        0 => "Don't introduce new words; if you need one, rephrase with the learner's words.".to_string(),
        1 => "Introduce at most one new word per turn.".to_string(),
        n => format!("Introduce at most {n} new words per turn."),
    };
    let vocab = if o.only_my_words {
        "Build your sentences only from the learner's words below plus the most basic grammar words (我, 你, 是, 的, 吗, 呢, 不, 很, 也, 都, 了, 在, 有, 这, 那, 什么, 一, 个 and numbers)."
    } else {
        "Build your sentences from the learner's words below as much as you can."
    };
    let lead = match o.lead.as_str() {
        "learner" => "Let the learner lead: answer what they say and follow their topic; ask a question only if they stop.",
        _ => "Lead the conversation: end most turns with a question the learner can answer.",
    };
    let english = match o.english.as_str() {
        "never" => "Never use English, even if asked; explain in simpler Chinese instead.",
        "hints" => "Write only Chinese, except: after a new or difficult word you may add its English meaning in brackets, e.g. 冰（ice）. If the learner writes in English or asks what something means, answer briefly in English, then continue in Chinese.",
        _ => "Exception: if the learner writes in English or asks what something means (for example 什么意思 or 'English please'), answer that briefly in English, then continue in Chinese.",
    };
    let corrections = match o.corrections.as_str() {
        "off" => "Don't correct mistakes; just keep the conversation going naturally.",
        "every" => "Correct every real mistake (grammar, word choice, word order, measure words): reply naturally first, then give the corrected sentence starting with 「更好的说法：」, and if the reason isn't obvious add one short English explanation in brackets.",
        _ => "When the learner makes a real mistake (grammar, a wrong word, a missing measure word), reply naturally first, then add one short corrected version of their sentence, starting with 「更好的说法：」.",
    };
    let notes = o.notes.trim();
    let notes = if notes.is_empty() {
        String::new()
    } else {
        format!("\nThe learner's own requests for this conversation (follow them unless they conflict with the rules about tools):\n{}\n", notes.chars().take(600).collect::<String>())
    };
    format!(
        "You are a warm, patient Mandarin conversation partner for an English-speaking learner: {level}.

How to talk:
- Speak Mandarin in Simplified characters, {length}, then hand the turn back.
- {vocab} Words marked * are due for review: steer the conversation so the learner gets to use them. {new_words}
- {lead}
- Your replies are read aloud by a Chinese speech synthesizer, so write Chinese characters and Chinese punctuation: no pinyin, no emoji, no lists or formatting. The app shows pinyin and translations on request.
- {english}
- The learner speaks through speech recognition, or types with a pinyin keyboard, so their message may contain wrong characters that sound like the right ones, or bits of pinyin. Read it for sound and intent; don't correct those slips.
- {corrections}
- Keep the scene moving and stay in character; if the learner seems stuck, offer a simpler question or a choice.
{notes}
Tools:
- After each learner message, call grade_word for each of the learner's words they tried to use.
- When you introduce a useful new word, call suggest_card with it.
- Tool calls are silent bookkeeping: never mention them.

Scene: {scenario}

The learner's words (* = due for review):
{}",
        list.join("\n")
    )
}

/// Start a session with words from the given decks (and their subdecks; empty = all).
pub fn start(conn: &Connection, model: &str, deck_ids: &[i64], scenario: &str, opts: &TutorOptions) -> AppResult<Started> {
    let model = if ai::MODELS.iter().any(|m| m.0 == opts.model) { opts.model.as_str() } else { model };
    let model = if ai::MODELS.iter().any(|m| m.0 == model) { model } else { ai::DEFAULT_MODEL };
    let scenario = SCENARIOS.iter().find(|(k, _)| *k == scenario).map(|(_, s)| s.to_string()).unwrap_or_else(|| {
        let s = scenario.trim();
        if s.is_empty() { SCENARIOS[6].1.to_string() } else { s.to_string() }
    });
    // The learner's words: one per set of characters, due ones marked, at most 400.
    let deck_clause = if deck_ids.is_empty() {
        String::new()
    } else {
        let ids: Vec<String> = deck_ids.iter().map(i64::to_string).collect();
        format!(
            "WHERE c.deck_id IN (WITH RECURSIVE sub(id) AS (SELECT id FROM decks WHERE id IN ({})
               UNION SELECT d.id FROM decks d JOIN sub ON d.parent_id = sub.id) SELECT id FROM sub)",
            ids.join(",")
        )
    };
    let sql = format!(
        "SELECT c.id, c.hanzi, c.pinyin, c.meaning,
                EXISTS(SELECT 1 FROM progress p WHERE p.card_id = c.id AND p.due <= strftime('%s','now'))
         FROM cards c {deck_clause} ORDER BY c.created_at DESC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, bool>(4)?))
    })?;
    let mut words: HashMap<String, i64> = HashMap::new();
    let mut list: Vec<(String, String, String, bool)> = Vec::new();
    for row in rows {
        let (id, hanzi, pinyin, meaning, due) = row?;
        let hanzi = hanzi.trim().to_string();
        if hanzi.is_empty() || words.contains_key(&hanzi) || list.len() >= 400 {
            continue;
        }
        words.insert(hanzi.clone(), id);
        // Keep meanings short in the prompt.
        let short: String = meaning.split(';').next().unwrap_or("").chars().take(60).collect();
        list.push((hanzi, pinyin, short.trim().to_string(), due));
    }
    if list.is_empty() {
        return Err(AppError::msg("The chosen decks have no cards to talk about."));
    }
    list.sort_by_key(|w| !w.3); // due words first
    // Ids start from the current time so they stay unique across launches (the spending
    // record groups calls by conversation).
    NEXT_ID.fetch_max(chrono::Utc::now().timestamp_millis() as u64, Ordering::Relaxed);
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let session = Session {
        model: model.to_string(),
        system: system_prompt(&scenario, &list, opts),
        messages: vec![],
        words,
        grades: vec![],
        suggestions: vec![],
        usage: Usage::default(),
        unbilled: Usage::default(),
        turns: 0,
        cancel: Arc::new(AtomicBool::new(false)),
    };
    let n = list.len();
    with_sessions(|s| s.insert(id, session));
    Ok(Started { session_id: id, words: n })
}

/// What the tutor needs to run a turn without holding the session lock.
struct TurnState {
    model: String,
    system: String,
    messages: Vec<Value>,
    words: HashMap<String, i64>,
    cancel: Arc<AtomicBool>,
}

/// What a turn produced so far (kept even when the turn fails part-way).
#[derive(Default)]
struct TurnOutcome {
    text: String,
    usage: Usage,
    grades: Vec<Grade>,
    suggestions: Vec<Suggestion>,
    stop: String,
}

/// Send the learner's message (None = let the tutor open the conversation) and stream the
/// reply. `known` tells whether a suggested word is already a card. Whatever the API calls
/// cost, even in a cancelled or failed turn, is added to the session's unbilled usage.
pub fn turn(
    session_id: u64,
    user_text: Option<&str>,
    mut emit: impl FnMut(TurnEvent),
    known: impl Fn(&str) -> bool,
) -> AppResult<TurnResult> {
    let mut st = with_sessions(|s| {
        s.get(&session_id).map(|x| TurnState {
            model: x.model.clone(),
            system: x.system.clone(),
            messages: x.messages.clone(),
            words: x.words.clone(),
            cancel: x.cancel.clone(),
        })
    })
    .ok_or_else(|| AppError::msg("This conversation has ended."))?;
    st.cancel.store(false, Ordering::Relaxed);

    let history_len = st.messages.len();
    let opening = "（开始对话：请你先用中文打招呼，开始这个情景。）";
    st.messages.push(json!({ "role": "user", "content": user_text.map(str::trim).filter(|t| !t.is_empty()).unwrap_or(opening) }));

    let mut out = TurnOutcome::default();
    let result = run_turn(&mut st, &mut out, &mut emit, &known);
    // A turn that didn't finish normally leaves no trace in the conversation.
    let keep = result.is_ok() && !matches!(out.stop.as_str(), "refusal" | "cancelled");
    let model = st.model.clone();
    let cost = with_sessions(|s| {
        let Some(x) = s.get_mut(&session_id) else { return 0.0 };
        if keep {
            x.messages = std::mem::take(&mut st.messages);
            x.grades.extend(std::mem::take(&mut out.grades));
            x.suggestions.extend(std::mem::take(&mut out.suggestions));
            x.turns += 1;
        } else {
            x.messages.truncate(history_len);
        }
        x.usage.add(&out.usage);
        x.unbilled.add(&out.usage);
        x.usage.cost(&model)
    });
    result?;
    Ok(TurnResult { text: out.text, stop_reason: out.stop, cost })
}

fn run_turn(
    st: &mut TurnState,
    out: &mut TurnOutcome,
    emit: &mut impl FnMut(TurnEvent),
    known: &impl Fn(&str) -> bool,
) -> AppResult<()> {
    let tools = tools();
    // A turn may take a few rounds of tool calls before the final reply.
    for _round in 0..4 {
        let body = ai::base_body(&st.model, &st.system, &st.messages, &tools, true);
        let reply = ai::stream(&body, &st.cancel, |t| {
            out.text.push_str(t);
            emit(TurnEvent::Text { text: t.to_string() });
        })?;
        out.usage.add(&reply.usage);
        out.stop = reply.stop_reason.clone();
        if matches!(out.stop.as_str(), "refusal" | "cancelled") {
            return Ok(());
        }
        let tool_calls: Vec<Value> = reply.tool_uses().cloned().collect();
        st.messages.push(json!({ "role": "assistant", "content": ai::echo_content(reply.content) }));
        if out.stop != "tool_use" {
            return Ok(());
        }
        let mut results = Vec::new();
        for call in &tool_calls {
            let id = call["id"].as_str().unwrap_or("");
            let input = &call["input"];
            let ok = match call["name"].as_str().unwrap_or("") {
                "grade_word" if input["hanzi"].is_string() && input["correct"].is_boolean() => {
                    let hanzi = input["hanzi"].as_str().unwrap_or("").trim().to_string();
                    let g = Grade { card_id: st.words.get(&hanzi).copied(), hanzi, correct: input["correct"] == true };
                    emit(TurnEvent::Grade { grade: g.clone() });
                    out.grades.push(g);
                    true
                }
                "suggest_card" if input["hanzi"].is_string() && input["meaning"].is_string() => {
                    let hanzi = input["hanzi"].as_str().unwrap_or("").trim().to_string();
                    let s = Suggestion {
                        exists: st.words.contains_key(&hanzi) || known(&hanzi),
                        pinyin: input["pinyin"].as_str().unwrap_or("").to_string(),
                        meaning: input["meaning"].as_str().unwrap_or("").to_string(),
                        hanzi,
                    };
                    emit(TurnEvent::Suggestion { suggestion: s.clone() });
                    out.suggestions.push(s);
                    true
                }
                _ => false,
            };
            results.push(if ok {
                json!({ "type": "tool_result", "tool_use_id": id, "content": "ok" })
            } else {
                json!({ "type": "tool_result", "tool_use_id": id, "content": "Invalid input; check the tool's schema.", "is_error": true })
            });
        }
        st.messages.push(json!({ "role": "user", "content": results }));
    }
    Ok(())
}

/// Usage not yet written to the spending record (and the model it was used with).
pub fn take_unbilled(session_id: u64) -> Option<(String, Usage)> {
    with_sessions(|s| {
        let x = s.get_mut(&session_id)?;
        let u = std::mem::take(&mut x.unbilled);
        (u.input_tokens + u.output_tokens + u.cache_read_tokens + u.cache_write_tokens > 0).then(|| (x.model.clone(), u))
    })
}

/// Text to steer speech recognition for the learner's next reply: the tutor's last line
/// (what the reply answers).
pub fn hint(session_id: u64) -> String {
    with_sessions(|s| {
        let Some(x) = s.get(&session_id) else { return String::new() };
        let last = x
            .messages
            .iter()
            .rev()
            .find(|m| m["role"] == "assistant")
            .map(|m| {
                m["content"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|b| b["type"] == "text")
                    .filter_map(|b| b["text"].as_str())
                    .collect::<String>()
            })
            .unwrap_or_default();
        // A list of words here makes Whisper expect a list and garble sentences (measured
        // with whisper_bench); the learner's words are favoured during decoding instead.
        last.trim().to_string()
    })
}

pub fn cancel(session_id: u64) {
    with_sessions(|s| {
        if let Some(x) = s.get(&session_id) {
            x.cancel.store(true, Ordering::Relaxed);
        }
    });
}

/// End a session and return what happened in it.
pub fn end(session_id: u64) -> AppResult<Summary> {
    let x = with_sessions(|s| s.remove(&session_id)).ok_or_else(|| AppError::msg("This conversation has ended."))?;
    x.cancel.store(true, Ordering::Relaxed);
    // One entry per word: a word counts as correct only if every attempt was.
    let mut grades: Vec<Grade> = Vec::new();
    for g in x.grades {
        match grades.iter_mut().find(|e| e.hanzi == g.hanzi) {
            Some(e) => e.correct &= g.correct,
            None => grades.push(g),
        }
    }
    let mut suggestions: Vec<Suggestion> = Vec::new();
    for s in x.suggestions {
        if !suggestions.iter().any(|e| e.hanzi == s.hanzi) {
            suggestions.push(s);
        }
    }
    Ok(Summary { cost: x.usage.cost(&x.model), usage: x.usage, model: x.model, grades, suggestions, turns: x.turns })
}

/// Translate one of the tutor's lines into English.
pub fn translate(model: &str, text: &str) -> AppResult<(String, String, Usage)> {
    let model = if ai::MODELS.iter().any(|m| m.0 == model) { model } else { ai::DEFAULT_MODEL };
    let body = ai::base_body(
        model,
        "Translate the user's Chinese text into natural English for a beginner learner. Reply with the translation only.",
        &[json!({ "role": "user", "content": text })],
        &[],
        false,
    );
    let reply = ai::complete(&body)?;
    let text = if reply.stop_reason == "refusal" { String::new() } else { reply.text().trim().to_string() };
    Ok((text, model.to_string(), reply.usage))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_lists_words_due_first() {
        let conn = crate::db::open_in_memory().unwrap();
        let opts = TutorOptions { corrections: "every".into(), notes: "Talk about tea".into(), ..Default::default() };
        let s = start(&conn, "claude-opus-5-5", &[], "cafe", &opts).unwrap();
        assert_eq!(s.words, 12, "seed deck");
        let sys = with_sessions(|m| m.get(&s.session_id).unwrap().system.clone());
        assert!(sys.contains("Ordering drinks"));
        assert!(sys.contains("你好 (nǐ hǎo): hello"));
        assert!(sys.contains("Correct every real mistake"));
        assert!(sys.contains("Talk about tea"));
        let summary = end(s.session_id).unwrap();
        assert_eq!(summary.turns, 0);
        assert!(end(s.session_id).is_err(), "ended sessions are gone");
    }

    #[test]
    fn reply_lengths() {
        let words = [("茶".to_string(), "chá".to_string(), "tea".to_string(), false)];
        let p = |len: &str| system_prompt("", &words, &TutorOptions { length: len.into(), ..Default::default() });
        assert!(p("tiny").contains("only one short statement or question per turn"));
        assert!(p("short").contains("one short sentence per turn"));
        assert!(p("normal").contains("one to three short sentences"));
    }

    #[test]
    fn tools_are_strict_schemas() {
        for t in tools() {
            assert_eq!(t["strict"], true);
            assert_eq!(t["input_schema"]["additionalProperties"], false);
        }
    }

    /// A real conversation turn (costs a little). Needs ANTHROPIC_API_KEY or a stored key:
    /// cargo test --lib tutor_live -- --ignored --nocapture
    #[test]
    #[ignore]
    fn tutor_live() {
        if let Ok(k) = std::env::var("ANTHROPIC_API_KEY") {
            ai::set_key(&k).unwrap();
        }
        let conn = crate::db::open_in_memory().unwrap();
        let model = std::env::var("TUTOR_MODEL").unwrap_or_else(|_| ai::DEFAULT_MODEL.into());
        let s = start(&conn, &model, &[], "meet", &TutorOptions::default()).unwrap();
        let t0 = std::time::Instant::now();
        let r = turn(s.session_id, None, |e| println!("EVENT {e:?}"), |_| false).unwrap();
        println!("OPENING {} [{}] {:.1}s ${:.4}", r.text, r.stop_reason, t0.elapsed().as_secs_f32(), r.cost);
        let t1 = std::time::Instant::now();
        let r = turn(s.session_id, Some("你好！我叫威廉。我是学生。"), |e| println!("EVENT {e:?}"), |_| false).unwrap();
        println!("REPLY {} [{}] {:.1}s ${:.4}", r.text, r.stop_reason, t1.elapsed().as_secs_f32(), r.cost);
        let sum = end(s.session_id).unwrap();
        println!("SUMMARY {:?}", sum);
        println!("TRANSLATE {:?}", translate(&model, &r.text).unwrap());
    }
}
