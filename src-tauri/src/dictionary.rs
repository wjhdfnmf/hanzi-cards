//! CC-CEDICT dictionary: parsing, import, download and search.
//!
//! CC-CEDICT is published by MDBG under Creative Commons Attribution-ShareAlike
//! 4.0 (https://cc-cedict.org/wiki/). Users download it from inside the app.

use crate::db::{self, now};
use crate::error::{AppError, AppResult};
use crate::models::{DictEntry, DictStatus};
use crate::pinyin;
use flate2::read::GzDecoder;
use crate::models::{Candidate, Segment};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::sync::OnceLock;

pub const CEDICT_URL: &str =
    "https://www.mdbg.net/chinese/export/cedict/cedict_1_0_ts_utf-8_mdbg.txt.gz";

// ---------- the dictionary's own file ----------
//
// The dictionary lives in `dictionary.db`, next to the cards database, and is attached to the
// cards connection as `dict`, so every query here names plain `dictionary` and can still look
// at the cards (is this word already a card?). An import writes it through a connection of
// its own (`open`). It holds nothing of the learner's: if the file is ever damaged it is
// simply thrown away and made again from the copy that ships with the app.

pub const DB_FILE: &str = "dictionary.db";

/// `{s}` is the schema prefix: "dict." when attached, "" on the file's own connection.
const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS {s}dictionary (
    id              INTEGER PRIMARY KEY,
    traditional     TEXT NOT NULL,
    simplified      TEXT NOT NULL,
    pinyin_num      TEXT NOT NULL,
    pinyin          TEXT NOT NULL,
    pinyin_key      TEXT NOT NULL,
    pinyin_key_tone TEXT NOT NULL,
    definitions     TEXT NOT NULL,
    score           INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS {s}idx_dict_simp ON dictionary(simplified);
CREATE INDEX IF NOT EXISTS {s}idx_dict_trad ON dictionary(traditional);
CREATE INDEX IF NOT EXISTS {s}idx_dict_pkey ON dictionary(pinyin_key, score);
CREATE INDEX IF NOT EXISTS {s}idx_dict_pkey_tone ON dictionary(pinyin_key_tone);
CREATE TABLE IF NOT EXISTS {s}dictionary_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
";

/// Whether an error means the file itself is bad (as opposed to busy, or missing rights).
fn is_damage(e: &rusqlite::Error) -> bool {
    matches!(e.sqlite_error_code(), Some(rusqlite::ErrorCode::DatabaseCorrupt | rusqlite::ErrorCode::NotADatabase))
}

/// Attach the dictionary file (`None` = an empty one in memory, for tests) to the cards
/// connection as `dict`. A damaged file is replaced by an empty one, which the app then
/// fills again from its own copy of CC-CEDICT.
pub fn attach(conn: &Connection, path: Option<&std::path::Path>) -> rusqlite::Result<()> {
    let try_attach = || -> rusqlite::Result<()> {
        let target = path.map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| ":memory:".into());
        conn.execute("ATTACH DATABASE ?1 AS dict", [target])?;
        let ready = (|| -> rusqlite::Result<()> {
            if path.is_some() {
                // write-ahead mode here: the app goes on reading while an import writes
                conn.query_row("PRAGMA dict.journal_mode=WAL", [], |_| Ok(()))?;
                // (and the log doesn't stay the size of an import once it has been merged)
                conn.query_row("PRAGMA dict.journal_size_limit=1048576", [], |_| Ok(()))?;
                let sound: String = conn.query_row("PRAGMA dict.quick_check(1)", [], |r| r.get(0))?;
                if sound != "ok" {
                    return Err(rusqlite::Error::SqliteFailure(
                        rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CORRUPT),
                        Some(sound),
                    ));
                }
            }
            conn.execute_batch(&SCHEMA.replace("{s}", "dict."))
        })();
        if ready.is_err() {
            let _ = conn.execute_batch("DETACH DATABASE dict");
        }
        ready
    };
    match (try_attach(), path) {
        (Err(e), Some(p)) if is_damage(&e) => {
            eprintln!("dictionary file damaged ({e}): starting it again");
            for ext in ["", "-wal", "-shm"] {
                let _ = std::fs::remove_file(format!("{}{ext}", p.display()));
            }
            try_attach()
        }
        (result, _) => result,
    }
}

/// The dictionary file on a connection of its own, for an import.
pub fn open(path: &std::path::Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.busy_timeout(db::BUSY_WAIT)?;
    conn.query_row("PRAGMA journal_mode=WAL", [], |_| Ok(()))?;
    conn.execute_batch(&SCHEMA.replace("{s}", ""))?;
    Ok(conn)
}

#[derive(Debug, PartialEq)]
pub struct ParsedEntry {
    pub traditional: String,
    pub simplified: String,
    pub pinyin_num: String,
    pub definitions: Vec<String>,
}

/// Parse one line: `傳統 传统 [chuan2 tong3] /tradition/traditional/`
pub fn parse_line(line: &str) -> Option<ParsedEntry> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let (traditional, rest) = line.split_once(' ')?;
    let (simplified, rest) = rest.split_once(' ')?;
    let rest = rest.strip_prefix('[')?;
    let (pinyin_num, rest) = rest.split_once(']')?;
    let defs: Vec<String> = rest
        .trim()
        .trim_matches('/')
        .split('/')
        .map(|d| d.trim().to_string())
        .filter(|d| !d.is_empty())
        .collect();
    if defs.is_empty() {
        return None;
    }
    Some(ParsedEntry {
        traditional: traditional.to_string(),
        simplified: simplified.to_string(),
        pinyin_num: pinyin_num.trim().to_string(),
        definitions: defs,
    })
}

/// CEDICT writes ü as "u:" and keeps some tone numbers on non-syllables; tidy for display.
fn display_pinyin(num: &str) -> String {
    pinyin::numbered_to_marks(num)
}

/// Replace the dictionary table with the contents of a CC-CEDICT text file.
pub fn import(conn: &mut Connection, text: &str) -> AppResult<i64> {
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM dictionary", [])?;
    let mut n = 0i64;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO dictionary (traditional, simplified, pinyin_num, pinyin, pinyin_key, pinyin_key_tone, definitions)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )?;
        for line in text.lines() {
            if let Some(e) = parse_line(line) {
                stmt.execute(params![
                    e.traditional,
                    e.simplified,
                    e.pinyin_num,
                    display_pinyin(&e.pinyin_num),
                    pinyin::toneless_key(&e.pinyin_num),
                    pinyin::toned_key(&e.pinyin_num),
                    format!("/{}/", e.definitions.join("/")),
                ])?;
                n += 1;
            }
        }
    }
    if n == 0 {
        return Err(AppError::msg(
            "No dictionary entries found. Is this a CC-CEDICT file?",
        ));
    }
    compute_scores(&tx)?;
    tx.execute(
        "INSERT INTO dictionary_meta (key, value) VALUES ('imported_at', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [now().to_string()],
    )?;
    tx.commit()?;
    Ok(n)
}

// ---------- usage scores ----------
//
// CC-CEDICT has no frequency information, so results are ranked by a score built from
// (a) per-reading character frequencies from Unihan kHanyuPinlu (data/char_freq.tsv,
// generated by scripts/gen_char_freq.py), and (b) how many other dictionary words
// contain the entry, which favours productive, everyday words.

struct CharFreq {
    total: u32,
    readings: Vec<(String, u32)>,
}

fn char_freqs() -> &'static HashMap<char, CharFreq> {
    static TABLE: OnceLock<HashMap<char, CharFreq>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut map = HashMap::new();
        for line in include_str!("../data/char_freq.tsv").lines() {
            if line.starts_with('#') {
                continue;
            }
            let Some((ch, rest)) = line.split_once('\t') else { continue };
            let Some(ch) = ch.chars().next() else { continue };
            let readings: Vec<(String, u32)> = rest
                .split_whitespace()
                .filter_map(|r| {
                    let (syl, n) = r.split_once(':')?;
                    Some((syl.to_string(), n.parse().ok()?))
                })
                .collect();
            let total = readings.iter().map(|(_, n)| n).sum();
            map.insert(ch, CharFreq { total, readings });
        }
        map
    })
}

/// Frequency of one character read as `syllable` (numbered pinyin, any case).
fn reading_freq(ch: char, syllable: Option<&str>) -> u32 {
    let Some(f) = char_freqs().get(&ch) else { return 0 };
    let key = syllable.map(pinyin::toned_key);
    match key.and_then(|k| f.readings.iter().find(|(s, _)| *s == k)) {
        Some((_, n)) => *n,
        // Known character, but this reading isn't among its common ones.
        None => f.total / 20,
    }
}

const MINOR_SENSES: [&str; 12] = [
    "surname ",
    "variant of",
    "old variant",
    "archaic variant",
    "japanese variant",
    "erhua variant",
    "see ",
    "used in ",
    "(archaic)",
    "(old)",
    "(literary)",
    "(dialect)",
];

/// Scores at or above this are everyday words: every character has a common reading.
pub const COMMON_SCORE: i64 = 2000;

fn entry_score(simplified: &str, pinyin_num: &str, definitions: &str, contained_in: u32) -> i64 {
    let chars: Vec<char> = simplified.chars().collect();
    let sylls: Vec<&str> = pinyin_num.split_whitespace().collect();
    let aligned = chars.len() == sylls.len();
    let freqs: Vec<u32> = chars
        .iter()
        .enumerate()
        .filter(|(_, c)| pinyin::is_hanzi(**c))
        .map(|(i, c)| reading_freq(*c, if aligned { Some(sylls[i]) } else { None }))
        .collect();
    let contained = contained_in.max(1) as f64;
    let mut score = match freqs.as_slice() {
        [] => 0.0,
        // A single character: its reading's frequency decides; word-building breaks ties.
        [f] => *f as f64 * 100.0 + contained.min(99.0),
        // A word is only as common as its rarest character (unlisted ones count as rare),
        // and everyday words appear inside many longer words.
        fs => {
            let rarest = fs.iter().map(|f| (*f).max(3)).min().unwrap_or(3);
            (rarest as f64 + 1.0) * (1.0 + contained.ln()) * 100.0
        }
    };
    let defs: Vec<String> = definitions
        .trim_matches('/')
        .split('/')
        .map(|d| d.to_lowercase())
        .collect();
    if defs.iter().all(|d| MINOR_SENSES.iter().any(|m| d.starts_with(m))) {
        score /= 20.0;
    }
    score as i64 + defs.len().min(9) as i64
}

/// (Re)compute the ranking score of every dictionary entry.
pub fn compute_scores(conn: &Connection) -> rusqlite::Result<()> {
    let rows: Vec<(i64, String, String, String)> = {
        let mut stmt = conn.prepare("SELECT id, simplified, pinyin_num, definitions FROM dictionary")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
        rows.collect::<Result<_, _>>()?
    };
    if rows.is_empty() {
        return Ok(());
    }
    // How many distinct headwords contain each substring of up to 4 chars.
    let mut contained: HashMap<String, u32> = HashMap::new();
    let mut seen_words = HashSet::new();
    for (_, simp, _, _) in &rows {
        if !seen_words.insert(simp.as_str()) {
            continue;
        }
        let chars: Vec<char> = simp.chars().collect();
        let mut subs = HashSet::new();
        for len in 1..=chars.len().min(4) {
            for w in chars.windows(len) {
                subs.insert(w.iter().collect::<String>());
            }
        }
        for s in subs {
            *contained.entry(s).or_default() += 1;
        }
    }
    let mut upd = conn.prepare("UPDATE dictionary SET score = ?1 WHERE id = ?2")?;
    for (id, simp, pn, defs) in &rows {
        let n = contained.get(simp).copied().unwrap_or(1);
        upd.execute(params![entry_score(simp, pn, defs, n), id])?;
    }
    Ok(())
}

/// Read a CC-CEDICT file from disk (plain text or .gz).
pub fn read_file(path: &std::path::Path) -> AppResult<String> {
    decode(crate::input::read_file(path, crate::input::MAX_DICTIONARY)?)
}

fn decode(bytes: Vec<u8>) -> AppResult<String> {
    if bytes.starts_with(&[0x1f, 0x8b]) {
        // (a small file can unpack to an enormous one: stop at the limit)
        let mut s = String::new();
        GzDecoder::new(&bytes[..]).take(crate::input::MAX_DICTIONARY + 1).read_to_string(&mut s)?;
        if s.len() as u64 > crate::input::MAX_DICTIONARY {
            return Err(AppError::msg("That file is too large to be a CC-CEDICT dictionary."));
        }
        Ok(s)
    } else {
        String::from_utf8(bytes).map_err(|_| AppError::msg("Dictionary file is not UTF-8 text"))
    }
}

/// Download the latest CC-CEDICT release from MDBG.
pub fn download() -> AppResult<String> {
    let resp = crate::net::get(CEDICT_URL)
        .call()
        .map_err(|e| AppError::msg(format!("Download failed: {e}")))?;
    let mut bytes = Vec::new();
    resp.into_reader()
        .take(64 * 1024 * 1024)
        .read_to_end(&mut bytes)?;
    decode(bytes)
}

pub fn status(conn: &Connection) -> AppResult<DictStatus> {
    let entries = conn.query_row("SELECT COUNT(*) FROM dictionary", [], |r| r.get(0))?;
    let imported_at = conn
        .query_row("SELECT value FROM dictionary_meta WHERE key = 'imported_at'", [], |r| r.get::<_, String>(0))
        .optional()?
        .and_then(|v| v.parse().ok());
    Ok(DictStatus {
        entries,
        imported_at,
    })
}

const ENTRY_COLS: &str = "d.id, d.traditional, d.simplified, d.pinyin, d.pinyin_num, d.definitions,
    EXISTS(SELECT 1 FROM cards c WHERE c.hanzi = d.simplified OR c.hanzi = d.traditional)";

fn entry_from_row(r: &rusqlite::Row) -> rusqlite::Result<DictEntry> {
    let defs: String = r.get(5)?;
    Ok(DictEntry {
        id: r.get(0)?,
        traditional: r.get(1)?,
        simplified: r.get(2)?,
        pinyin: r.get(3)?,
        pinyin_num: r.get(4)?,
        definitions: defs
            .trim_matches('/')
            .split('/')
            .map(String::from)
            .collect(),
        in_cards: r.get(6)?,
    })
}

/// Search by characters, pinyin (with or without tone numbers) or English.
pub fn search(conn: &Connection, query: &str, limit: i64) -> AppResult<Vec<DictEntry>> {
    let q = crate::input::clip(query.trim(), crate::input::MAX_QUERY);
    let q = q.as_str();
    if q.is_empty() {
        return Ok(vec![]);
    }
    let limit = limit.clamp(1, 200);
    // typed text inside LIKE patterns matches itself (% and _ are not wildcards there)
    let like = crate::input::like_escape(q);

    if pinyin::has_hanzi(q) {
        let sql = format!(
            "SELECT {ENTRY_COLS},
               CASE WHEN d.simplified = ?1 OR d.traditional = ?1 THEN 0
                    WHEN d.simplified LIKE ?3 || '%' ESCAPE '\\' OR d.traditional LIKE ?3 || '%' ESCAPE '\\' THEN 1
                    ELSE 2 END AS rank
             FROM dictionary d
             WHERE d.simplified LIKE '%' || ?3 || '%' ESCAPE '\\' OR d.traditional LIKE '%' || ?3 || '%' ESCAPE '\\'
             ORDER BY rank, d.score DESC, length(d.simplified), d.id
             LIMIT ?2"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![q, limit, like], entry_from_row)?;
        return Ok(rows.collect::<Result<_, _>>()?);
    }

    let has_digits = q.chars().any(|c| c.is_ascii_digit());
    let pkey = if has_digits {
        pinyin::toned_key(q)
    } else {
        pinyin::toneless_key(q)
    };
    let pcol = if has_digits { "pinyin_key_tone" } else { "pinyin_key" };
    let english = like.to_lowercase();
    // Short pinyin prefixes ("a", "s") match half the dictionary; only use exact match then.
    let allow_prefix = pkey.len() >= 2;

    let sql = format!(
        "SELECT {ENTRY_COLS},
           CASE WHEN ?1 <> '' AND d.{pcol} = ?1 THEN 0
                WHEN d.definitions LIKE '%/' || ?2 || '/%' ESCAPE '\\' OR d.definitions LIKE '%/to ' || ?2 || '/%' ESCAPE '\\' THEN 1
                WHEN ?3 AND ?1 <> '' AND d.{pcol} LIKE ?1 || '%' THEN 2
                WHEN (' ' || replace(replace(replace(replace(d.definitions, '/', ' '), ',', ' '), ';', ' '), '(', ' ') || ' ')
                     LIKE '% ' || ?2 || ' %' ESCAPE '\\' THEN 3
                ELSE 4 END AS rank
         FROM dictionary d
         WHERE (?1 <> '' AND (d.{pcol} = ?1 OR (?3 AND d.{pcol} LIKE ?1 || '%')))
            OR d.definitions LIKE '%' || ?2 || '%' ESCAPE '\\'
         ORDER BY rank, d.score DESC, length(d.simplified), d.id
         LIMIT ?4"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![pkey, english, allow_prefix, limit], entry_from_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Exact lookup by characters (used to fill in new cards).
pub fn lookup(conn: &Connection, hanzi: &str) -> AppResult<Vec<DictEntry>> {
    let sql = format!(
        "SELECT {ENTRY_COLS} FROM dictionary d
         WHERE d.simplified = ?1 OR d.traditional = ?1
         ORDER BY (d.simplified = ?1) DESC, d.score DESC, d.id"
    );
    // kept ready: imports and the card editor look words up one after another
    let mut stmt = conn.prepare_cached(&sql)?;
    let rows = stmt.query_map([hanzi.trim()], entry_from_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

// ---------- typing characters by pinyin ----------

/// Does typed pinyin (letters with optional tone digits, e.g. "ni3hao") match an entry's
/// toned key ("ni3hao3")? `None` = no; `Some(true)` = the whole word; `Some(false)` = a prefix.
fn tone_match(typed: &[u8], entry: &[u8]) -> Option<bool> {
    let mut j = 0;
    for &c in typed {
        if !c.is_ascii_digit() {
            // Tone digits are optional in what the user types.
            while j < entry.len() && entry[j].is_ascii_digit() {
                j += 1;
            }
        }
        if j < entry.len() && entry[j] == c {
            j += 1;
        } else {
            return None;
        }
    }
    Some(entry[j..].iter().all(u8::is_ascii_digit))
}

/// A dictionary entry matching typed pinyin.
struct Match {
    entry: DictEntry,
    /// The typed pinyin is the whole word (not just its start).
    whole: bool,
    /// An everyday word (score >= COMMON_SCORE).
    common: bool,
    score: i64,
}

/// Entries whose pinyin matches `typed` exactly (or, with `prefix`, starts with it).
/// Sorted: whole words first, then by score.
fn pinyin_matches(conn: &Connection, typed: &str, prefix: bool) -> AppResult<Vec<Match>> {
    let bare = pinyin::toneless_key(typed);
    if bare.is_empty() {
        return Ok(vec![]);
    }
    let toned = pinyin::toned_key(typed);
    let cond = if prefix {
        "d.pinyin_key >= ?1 AND d.pinyin_key < ?2" // '{' sorts after every key character
    } else {
        "d.pinyin_key = ?1 AND ?2 <> ''"
    };
    let sql = format!(
        "SELECT {ENTRY_COLS}, d.pinyin_key_tone, d.score FROM dictionary d
         WHERE {cond}
         ORDER BY (d.pinyin_key = ?1) DESC, d.score DESC
         LIMIT 800"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![bare, format!("{bare}{{")], |r| {
        Ok((entry_from_row(r)?, r.get::<_, String>(7)?, r.get::<_, i64>(8)?))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (entry, key, score) = row?;
        // Skip mixed-script entries like 卡拉OK: they can't be typed this way anyway.
        if entry.simplified.chars().any(|c| c.is_ascii_alphanumeric()) {
            continue;
        }
        if let Some(whole) = tone_match(toned.as_bytes(), key.as_bytes()) {
            out.push(Match { entry, whole, common: score >= COMMON_SCORE, score });
        }
    }
    out.sort_by_key(|m| !m.whole); // stable: keeps score order within each group
    Ok(out)
}

/// Can typed pinyin be split before position `end` (a new syllable starts there)?
fn can_split(q: &[char], end: usize) -> bool {
    end > 0 && end < q.len() && q[end - 1].is_ascii_alphanumeric() && (q[end].is_alphabetic() || q[end] == '\'')
}

/// Common words for the longest start of `q` that has any: (words, characters used).
///
/// `end` fixes where the first word stops (the first word of the best phrase); otherwise
/// the start whose best word is most common is used.
fn first_word(conn: &Connection, q: &[char], end: Option<usize>) -> AppResult<Option<(Vec<Match>, usize)>> {
    let common = |end: usize| -> AppResult<Vec<Match>> {
        Ok(pinyin_matches(conn, &q[..end].iter().collect::<String>(), false)?
            .into_iter()
            .filter(|m| m.common)
            .collect())
    };
    if let Some(end) = end {
        let m = common(end)?;
        return Ok((!m.is_empty()).then_some((m, end)));
    }
    let mut best: Option<(Vec<Match>, usize)> = None;
    for end in 1..q.len() {
        if !can_split(q, end) {
            continue;
        }
        let m = common(end)?;
        if !m.is_empty() && best.as_ref().is_none_or(|(b, _)| m[0].score >= b[0].score) {
            best = Some((m, end));
        }
    }
    Ok(best)
}

/// The best everyday word spelled exactly by `typed`: (id, score).
fn best_common(conn: &Connection, typed: &str) -> AppResult<Option<(i64, i64)>> {
    let bare = pinyin::toneless_key(typed);
    if bare.is_empty() {
        return Ok(None);
    }
    let toned = pinyin::toned_key(typed);
    let mut stmt = conn.prepare_cached(
        "SELECT id, pinyin_key_tone, score FROM dictionary
         WHERE pinyin_key = ?1 AND score >= ?2 ORDER BY score DESC LIMIT 50",
    )?;
    let mut rows = stmt.query(params![bare, COMMON_SCORE])?;
    while let Some(r) = rows.next()? {
        let key: String = r.get(1)?;
        if tone_match(toned.as_bytes(), key.as_bytes()) == Some(true) {
            return Ok(Some((r.get(0)?, r.get(2)?)));
        }
    }
    Ok(None)
}

/// Split typed pinyin into everyday words, choosing the split whose rarest word is the
/// most common. Each extra word divides the score by 4, so a real word beats spelling it
/// out character by character ("nihao" -> 你好), but common characters beat a rare word
/// ("woshi" -> 我 + 是, not 卧室). Returns each word's (id, end position in `q`) and the
/// phrase's score; `None` unless the best split has at least two words.
fn best_phrase(conn: &Connection, q: &[char]) -> AppResult<Option<(Vec<(i64, usize)>, i64)>> {
    let n = q.len();
    let boundary = |i: usize| i == 0 || i == n || can_split(q, i);
    let value = |min: f64, words: usize| min / 4f64.powi(words as i32 - 1);
    // best[i] = (rarest word's score, words) for the best split of q[i..]
    let mut best: Vec<Option<(f64, Vec<(i64, usize)>)>> = vec![None; n + 1];
    best[n] = Some((f64::INFINITY, vec![]));
    for i in (0..n).rev() {
        if !boundary(i) {
            continue;
        }
        let start = i + q[i..].iter().take_while(|c| **c == '\'').count();
        let mut choice: Option<(f64, Vec<(i64, usize)>)> = None;
        for j in (start + 1)..=n.min(start + 12) {
            if !boundary(j) {
                continue;
            }
            let Some((rest_min, rest_ids)) = &best[j] else { continue };
            let Some((id, score)) = best_common(conn, &q[start..j].iter().collect::<String>())? else {
                continue;
            };
            let min = rest_min.min(score as f64);
            let words = rest_ids.len() + 1;
            if choice.as_ref().is_none_or(|(m, ids)| value(min, words) > value(*m, ids.len())) {
                let mut ids = vec![(id, j)];
                ids.extend(rest_ids);
                choice = Some((min, ids));
            }
        }
        best[i] = choice;
    }
    Ok(best[0].take().and_then(|(min, ids)| {
        let score = value(min, ids.len()) as i64;
        (ids.len() >= 2).then_some((ids, score))
    }))
}

fn entry_by_id(conn: &Connection, id: i64) -> AppResult<DictEntry> {
    let sql = format!("SELECT {ENTRY_COLS} FROM dictionary d WHERE d.id = ?1");
    Ok(conn.query_row(&sql, [id], entry_from_row)?)
}

fn phrase_entry(parts: &[DictEntry]) -> DictEntry {
    let join = |f: &dyn Fn(&DictEntry) -> String, sep: &str| parts.iter().map(f).collect::<Vec<_>>().join(sep);
    DictEntry {
        id: -1,
        traditional: join(&|e| e.traditional.clone(), ""),
        simplified: join(&|e| e.simplified.clone(), ""),
        pinyin: join(&|e| e.pinyin.clone(), " "),
        pinyin_num: join(&|e| e.pinyin_num.clone(), " "),
        definitions: vec![join(&|e| e.definitions.first().cloned().unwrap_or_default(), " + ")],
        in_cards: false,
    }
}

/// Words to offer while the user types pinyin in the characters field, most likely first:
///
/// 1. everyday words spelled exactly by the input, and the best phrase made of several
///    words, ranked together ("woshi" -> 我是, "ni" -> 你, 泥 …)
/// 2. other words spelled exactly by the input (rarer characters for one syllable)
/// 3. everyday words the input is the start of ("zhongg" -> 中国)
/// 4. everyday words for the first part of the input, with `consumed` saying how much
///    of it they use ("woshi" -> 我, consumed 2), so the rest can be picked next
/// 5. anything else that matches
pub fn candidates(conn: &Connection, query: &str, limit: usize) -> AppResult<Vec<Candidate>> {
    let q: Vec<char> = query.trim().chars().collect();
    let limit = limit.clamp(1, 50);
    if q.is_empty() {
        return Ok(vec![]);
    }
    let n = q.len();
    let (whole, prefix): (Vec<Match>, Vec<Match>) =
        pinyin_matches(conn, &q.iter().collect::<String>(), true)?.into_iter().partition(|m| m.whole);
    let (common_whole, rare_whole): (Vec<Match>, Vec<Match>) = whole.into_iter().partition(|m| m.common);
    let (common_prefix, rare_prefix): (Vec<Match>, Vec<Match>) = prefix.into_iter().partition(|m| m.common);
    let phrase = best_phrase(conn, &q)?;
    let first = if common_whole.len() < 3 {
        first_word(conn, &q, phrase.as_ref().map(|(words, _)| words[0].1))?
    } else {
        None
    };

    let single = |m: Match, consumed: usize| {
        let parts = vec![m.entry.id];
        Candidate { entry: m.entry, consumed, parts }
    };
    let mut top: Vec<(i64, Candidate)> = common_whole.into_iter().map(|m| (m.score, single(m, n))).collect();
    if let Some((words, score)) = phrase {
        let ids: Vec<i64> = words.iter().map(|(id, _)| *id).collect();
        let parts = ids.iter().map(|id| entry_by_id(conn, *id)).collect::<AppResult<Vec<_>>>()?;
        top.push((score, Candidate { entry: phrase_entry(&parts), consumed: n, parts: ids }));
    }
    top.sort_by_key(|(score, _)| std::cmp::Reverse(*score));

    let mut out: Vec<Candidate> = Vec::new();
    let mut seen = HashSet::new();
    let mut add = |out: &mut Vec<Candidate>, c: Candidate, cap: usize| {
        if out.len() < cap.min(limit) && seen.insert(c.entry.simplified.clone()) {
            out.push(c);
        }
    };
    for (_, c) in top {
        add(&mut out, c, limit);
    }
    let mut rare_whole = rare_whole.into_iter();
    while out.len() < limit.saturating_sub(3) {
        let Some(m) = rare_whole.next() else { break };
        add(&mut out, single(m, n), limit);
    }
    let reserve = if first.is_some() { 3 } else { 0 };
    for m in common_prefix {
        add(&mut out, single(m, n), limit.saturating_sub(reserve));
    }
    if let Some((words, end)) = first {
        for m in words {
            add(&mut out, single(m, end), limit);
        }
    }
    for m in rare_whole.chain(rare_prefix) {
        add(&mut out, single(m, n), limit);
    }
    Ok(out)
}

/// The statement `segment` runs for every word it tries (a story is thousands of them).
fn best_entry_sql(prefer_sql: &str) -> String {
    format!(
        "SELECT {ENTRY_COLS} FROM dictionary d
         WHERE d.simplified = ?1 OR d.traditional = ?1
         ORDER BY d.id IN ({prefer_sql}) DESC, (d.simplified = ?1) DESC, d.score DESC
         LIMIT 1"
    )
}

/// Split characters into dictionary words, longest match first ("我爱你们" -> 我 / 爱 / 你们).
/// Entries in `prefer` (ids the user picked) win over the default reading of the same word.
pub fn segment(conn: &Connection, text: &str, prefer: &[i64]) -> AppResult<Vec<Segment>> {
    let prefer_sql = if prefer.is_empty() {
        "-1".to_string()
    } else {
        prefer.iter().map(i64::to_string).collect::<Vec<_>>().join(",")
    };
    // prepared once, not once per word
    let mut best_entry = conn.prepare(&best_entry_sql(&prefer_sql))?;
    let chars: Vec<char> = text.chars().collect();
    let mut out: Vec<Segment> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if !pinyin::is_hanzi(chars[i]) {
            match out.last_mut() {
                Some(s) if s.entry.is_none() && !pinyin::has_hanzi(&s.text) => s.text.push(chars[i]),
                _ => out.push(Segment { text: chars[i].to_string(), entry: None }),
            }
            i += 1;
            continue;
        }
        let run = chars[i..].iter().take_while(|c| pinyin::is_hanzi(**c)).count();
        let mut matched = false;
        for len in (1..=run.min(8)).rev() {
            let word: String = chars[i..i + len].iter().collect();
            if let Some(entry) = best_entry.query_row([&word], entry_from_row).optional()? {
                out.push(Segment { text: word, entry: Some(entry) });
                i += len;
                matched = true;
                break;
            }
        }
        if !matched {
            out.push(Segment { text: chars[i].to_string(), entry: None });
            i += 1;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# CC-CEDICT sample
傳統 传统 [chuan2 tong3] /tradition/traditional/convention/CL:個|个[ge4]/
你好 你好 [ni3 hao3] /hello/hi/
綠色 绿色 [lu:4 se4] /green/
北京 北京 [Bei3 jing1] /Beijing, capital of the People's Republic of China/
學習 学习 [xue2 xi2] /to learn/to study/
學 学 [xue2] /to learn/to study/science/-ology/
傳 传 [chuan2] /to pass on/to spread/to transmit/
";

    #[test]
    fn parses_lines() {
        let e = parse_line("傳統 传统 [chuan2 tong3] /tradition/traditional/").unwrap();
        assert_eq!(e.simplified, "传统");
        assert_eq!(e.traditional, "傳統");
        assert_eq!(e.pinyin_num, "chuan2 tong3");
        assert_eq!(e.definitions, vec!["tradition", "traditional"]);
        assert!(parse_line("# comment").is_none());
    }

    #[test]
    fn import_and_search() {
        let mut conn = crate::db::open_in_memory().unwrap();
        assert_eq!(import(&mut conn, SAMPLE).unwrap(), 7);
        assert_eq!(status(&conn).unwrap().entries, 7);

        let r = search(&conn, "传", 10).unwrap();
        assert_eq!(r[0].simplified, "传");
        assert_eq!(r.len(), 2);

        let r = search(&conn, "beijing", 10).unwrap();
        assert_eq!(r[0].simplified, "北京");
        assert_eq!(r[0].pinyin, "Běi jīng");

        let r = search(&conn, "lv4se4", 10).unwrap();
        assert_eq!(r[0].simplified, "绿色");
        assert_eq!(r[0].pinyin, "lǜ sè");

        let r = search(&conn, "study", 10).unwrap();
        assert_eq!(r[0].simplified, "学"); // exact "to study" sense, shortest word first
        assert!(r.iter().any(|e| e.simplified == "学习"));

        let r = search(&conn, "ni hao", 10).unwrap();
        assert_eq!(r[0].simplified, "你好");
        assert!(r[0].in_cards, "seed deck contains 你好");

        assert_eq!(lookup(&conn, "學習").unwrap()[0].simplified, "学习");

        // typed text is matched as it is: % and _ are not wildcards
        assert!(search(&conn, "%", 10).unwrap().is_empty());
        assert!(search(&conn, "_", 10).unwrap().is_empty());
        assert!(search(&conn, "传%", 10).unwrap().is_empty());
        assert_eq!(search(&conn, "People's Republic", 10).unwrap()[0].simplified, "北京");
    }

    /// The dictionary is a file of its own. An import writes it through a second connection,
    /// so nothing else waits: the app's connection keeps reading (the old contents) and still
    /// writes cards meanwhile, and sees the new dictionary once it is complete.
    #[test]
    fn the_dictionary_has_its_own_file_and_import_connection() {
        let dir = std::env::temp_dir().join(format!("hanzi-dict-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let main = crate::db::open(&dir.join("cards.db")).unwrap();
        let dict_path = dir.join(DB_FILE);
        assert!(dict_path.exists(), "made next to the cards database");
        let in_cards_file: bool = main
            .query_row("SELECT EXISTS(SELECT 1 FROM main.sqlite_schema WHERE name = 'dictionary')", [], |r| r.get(0))
            .unwrap();
        assert!(!in_cards_file, "no dictionary table in the cards file");

        let mut side = open(&dict_path).unwrap();
        {
            // an import, held open before it commits
            let tx = side.transaction().unwrap();
            tx.execute("INSERT INTO dictionary (traditional, simplified, pinyin_num, pinyin, pinyin_key, pinyin_key_tone, definitions) VALUES ('x', 'x', 'x', 'x', 'x', 'x', '/x/')", []).unwrap();
            assert_eq!(status(&main).unwrap().entries, 0, "readable meanwhile, still the old contents");
            crate::db::set_setting(&main, "language", "zh").unwrap(); // and the cards file is free to write
        }
        assert_eq!(import(&mut side, SAMPLE).unwrap(), 7);
        drop(side);
        let st = status(&main).unwrap();
        assert_eq!(st.entries, 7);
        assert!(st.imported_at.is_some());
        let r = search(&main, "ni hao", 10).unwrap();
        assert!(r[0].in_cards, "dictionary results still know the cards");
        drop(main);

        // a damaged dictionary file is thrown away and started again; the cards are untouched
        let cards_before = std::fs::read(dir.join("cards.db")).unwrap();
        let mut bytes = std::fs::read(&dict_path).unwrap();
        for b in &mut bytes[4096..] {
            *b = 0xAB;
        }
        std::fs::write(&dict_path, bytes).unwrap();
        let main = crate::db::open(&dir.join("cards.db")).unwrap();
        assert_eq!(status(&main).unwrap().entries, 0, "empty again, to be re-imported");
        assert_eq!(crate::decks::list_decks(&main).unwrap().len(), 1);
        drop(main);
        assert_eq!(std::fs::read(dir.join("cards.db")).unwrap().len(), cards_before.len());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A cards database from before the dictionary had its own file: the entries move over.
    #[test]
    fn moves_the_dictionary_out_of_an_older_cards_database() {
        let dir = std::env::temp_dir().join(format!("hanzi-dict-old-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cards.db");
        {
            // as version 9 left it: the table in the cards file, in write-ahead mode
            let old = Connection::open(&path).unwrap();
            old.execute_batch(
                "PRAGMA journal_mode=WAL;
                 CREATE TABLE decks (id INTEGER PRIMARY KEY, name TEXT NOT NULL, description TEXT NOT NULL DEFAULT '', created_at INTEGER NOT NULL, parent_id INTEGER);
                 CREATE TABLE cards (id INTEGER PRIMARY KEY, deck_id INTEGER NOT NULL, hanzi TEXT NOT NULL, pinyin TEXT NOT NULL DEFAULT '', meaning TEXT NOT NULL DEFAULT '', notes TEXT NOT NULL DEFAULT '', tags TEXT NOT NULL DEFAULT '', created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);
                 CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
                 CREATE TABLE progress (card_id INTEGER NOT NULL, direction TEXT NOT NULL, state INTEGER NOT NULL, stability REAL NOT NULL, difficulty REAL NOT NULL, due INTEGER NOT NULL, last_review INTEGER NOT NULL, reps INTEGER NOT NULL, lapses INTEGER NOT NULL, step INTEGER NOT NULL DEFAULT 0, PRIMARY KEY (card_id, direction));
                 CREATE TABLE review_log (id INTEGER PRIMARY KEY, card_id INTEGER NOT NULL, direction TEXT NOT NULL, rating INTEGER NOT NULL, state_before INTEGER NOT NULL, reviewed_at INTEGER NOT NULL, duration_ms INTEGER NOT NULL, scheduled_secs INTEGER NOT NULL);
                 CREATE TABLE dictionary (id INTEGER PRIMARY KEY, traditional TEXT NOT NULL, simplified TEXT NOT NULL, pinyin_num TEXT NOT NULL, pinyin TEXT NOT NULL, pinyin_key TEXT NOT NULL, pinyin_key_tone TEXT NOT NULL, definitions TEXT NOT NULL, score INTEGER NOT NULL DEFAULT 0);
                 CREATE INDEX idx_dict_simp ON dictionary(simplified);
                 INSERT INTO dictionary (traditional, simplified, pinyin_num, pinyin, pinyin_key, pinyin_key_tone, definitions, score) VALUES ('你好', '你好', 'ni3 hao3', 'nǐ hǎo', 'nihao', 'ni3hao3', '/hello/', 5000);
                 INSERT INTO settings VALUES ('dict_imported_at', '1790000000');
                 INSERT INTO decks (name, created_at) VALUES ('Mine', 0);
                 INSERT INTO cards (deck_id, hanzi, created_at, updated_at) VALUES (1, '你好', 0, 0);
                 PRAGMA user_version = 9;",
            )
            .unwrap();
        }
        let conn = crate::db::open(&path).unwrap();
        let st = status(&conn).unwrap();
        assert_eq!((st.entries, st.imported_at), (1, Some(1_790_000_000)));
        assert!(lookup(&conn, "你好").unwrap()[0].in_cards);
        let left: i64 = conn.query_row("SELECT COUNT(*) FROM main.sqlite_schema WHERE name LIKE '%dict%'", [], |r| r.get(0)).unwrap();
        assert_eq!(left, 0, "table and indexes gone from the cards file");
        assert_eq!(conn.query_row("PRAGMA main.journal_mode", [], |r| r.get::<_, String>(0)).unwrap(), "truncate");
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM cards", [], |r| r.get::<_, i64>(0)).unwrap(), 1);
        drop(conn);
        assert!(!dir.join("cards.db-wal").exists(), "no write-ahead log left beside the cards");
        // opening again changes nothing
        let conn = crate::db::open(&path).unwrap();
        assert_eq!(status(&conn).unwrap().entries, 1);
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    const TYPING: &str = "泥 泥 [ni2] /mud/
尼 尼 [ni2] /Buddhist nun/
你 你 [ni3] /you (informal)/
你們 你们 [ni3 men5] /you (plural)/
窩 窝 [wo1] /nest/
我 我 [wo3] /I/me/my/
愛 爱 [ai4] /to love/
了 了 [liao3] /to finish/to understand/
了 了 [le5] /(completed action marker)/
好 好 [hao4] /to be fond of/
好 好 [hao3] /good/well/
妮 妮 [Ni1] /surname Ni/
是 是 [shi4] /to be/
臥室 卧室 [wo4 shi4] /bedroom/
";

    #[test]
    fn scores_rank_common_readings_first() {
        let mut conn = crate::db::open_in_memory().unwrap();
        import(&mut conn, TYPING).unwrap();
        assert_eq!(lookup(&conn, "了").unwrap()[0].pinyin, "le");
        assert_eq!(lookup(&conn, "好").unwrap()[0].pinyin, "hǎo");
        let r = search(&conn, "ni", 10).unwrap();
        assert_eq!(r[0].simplified, "你");
    }

    #[test]
    fn tone_matching() {
        assert_eq!(tone_match(b"ni", b"ni3"), Some(true));
        assert_eq!(tone_match(b"ni3", b"ni3"), Some(true));
        assert_eq!(tone_match(b"ni2", b"ni3"), None);
        assert_eq!(tone_match(b"nih", b"ni3hao3"), Some(false));
        assert_eq!(tone_match(b"ni3hao", b"ni3hao3"), Some(true));
        assert_eq!(tone_match(b"nihao4", b"ni3hao3"), None);
    }

    #[test]
    fn typing_candidates() {
        let mut conn = crate::db::open_in_memory().unwrap();
        import(&mut conn, TYPING).unwrap();
        let words = |q: &str| -> Vec<(String, usize)> {
            candidates(&conn, q, 9)
                .unwrap()
                .into_iter()
                .map(|c| (c.entry.simplified, c.consumed))
                .collect()
        };

        let ni = words("ni");
        assert_eq!(ni[0], ("你".to_string(), 2));
        assert!(ni.contains(&("你们".to_string(), 2)), "prefix words follow: {ni:?}");
        let whole = ni.iter().position(|w| w.0 == "泥").unwrap();
        let prefix = ni.iter().position(|w| w.0 == "你们").unwrap();
        assert!(whole < prefix, "whole-syllable matches before longer words");

        assert_eq!(words("ni2")[0].0, "泥");
        assert!(!words("ni2").iter().any(|w| w.0 == "你"));
        // Not a word: offer words for the start of the input.
        assert_eq!(words("woai")[0], ("我爱".to_string(), 4), "phrase of the best words");
        assert!(words("woai").contains(&("我".to_string(), 2)), "first word, rest typed next");
        let phrase = candidates(&conn, "woai", 9).unwrap().remove(0);
        assert_eq!(phrase.entry.pinyin, "wǒ ài");
        assert_eq!(phrase.parts.len(), 2);
        // A phrase of common characters outranks a rare word with the same pinyin.
        assert_eq!(words("woshi")[0].0, "我是");
        assert!(words("woshi").iter().any(|w| w.0 == "卧室"));
        // A real word beats spelling it out.
        assert_eq!(words("nimen")[0].0, "你们");
        assert_eq!(words("NI3MEN")[0], ("你们".to_string(), 6));
        assert!(words("x").is_empty());
    }

    #[test]
    fn segments_text() {
        let mut conn = crate::db::open_in_memory().unwrap();
        import(&mut conn, TYPING).unwrap();
        let seg = segment(&conn, "我爱你们!", &[]).unwrap();
        let texts: Vec<&str> = seg.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(texts, ["我", "爱", "你们", "!"]);
        assert!(seg[3].entry.is_none());

        let le = segment(&conn, "了", &[]).unwrap();
        assert_eq!(le[0].entry.as_ref().unwrap().pinyin, "le");
        let liao_id = lookup(&conn, "了").unwrap().iter().find(|e| e.pinyin == "liǎo").unwrap().id;
        let liao = segment(&conn, "了", &[liao_id]).unwrap();
        assert_eq!(liao[0].entry.as_ref().unwrap().pinyin, "liǎo");
    }
}
