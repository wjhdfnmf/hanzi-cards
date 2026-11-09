use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

pub const DAY: i64 = 86_400;

const SCHEMA_V1: &str = r#"
CREATE TABLE IF NOT EXISTS decks (
    id          INTEGER PRIMARY KEY,
    name        TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    created_at  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS cards (
    id         INTEGER PRIMARY KEY,
    deck_id    INTEGER NOT NULL REFERENCES decks(id) ON DELETE CASCADE,
    hanzi      TEXT NOT NULL,
    pinyin     TEXT NOT NULL DEFAULT '',
    meaning    TEXT NOT NULL DEFAULT '',
    notes      TEXT NOT NULL DEFAULT '',
    tags       TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_cards_deck  ON cards(deck_id);
CREATE INDEX IF NOT EXISTS idx_cards_hanzi ON cards(hanzi);

-- Scheduling state for one study direction of one card.
-- state: 1 learning, 2 review, 3 relearning (no row = new)
CREATE TABLE IF NOT EXISTS progress (
    card_id     INTEGER NOT NULL REFERENCES cards(id) ON DELETE CASCADE,
    direction   TEXT NOT NULL,
    state       INTEGER NOT NULL,
    stability   REAL NOT NULL,
    difficulty  REAL NOT NULL,
    due         INTEGER NOT NULL,
    last_review INTEGER NOT NULL,
    reps        INTEGER NOT NULL,
    lapses      INTEGER NOT NULL,
    PRIMARY KEY (card_id, direction)
);
CREATE INDEX IF NOT EXISTS idx_progress_due ON progress(due);

CREATE TABLE IF NOT EXISTS review_log (
    id             INTEGER PRIMARY KEY,
    card_id        INTEGER NOT NULL REFERENCES cards(id) ON DELETE CASCADE,
    direction      TEXT NOT NULL,
    rating         INTEGER NOT NULL,
    state_before   INTEGER NOT NULL,
    reviewed_at    INTEGER NOT NULL,
    duration_ms    INTEGER NOT NULL,
    scheduled_secs INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_log_time ON review_log(reviewed_at);
CREATE INDEX IF NOT EXISTS idx_log_card ON review_log(card_id);

CREATE TABLE IF NOT EXISTS dictionary (
    id              INTEGER PRIMARY KEY,
    traditional     TEXT NOT NULL,
    simplified      TEXT NOT NULL,
    pinyin_num      TEXT NOT NULL,
    pinyin          TEXT NOT NULL,
    pinyin_key      TEXT NOT NULL,
    pinyin_key_tone TEXT NOT NULL,
    definitions     TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_dict_simp ON dictionary(simplified);
CREATE INDEX IF NOT EXISTS idx_dict_trad ON dictionary(traditional);

CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#;

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    init(conn)
}

#[cfg(test)]
pub fn open_in_memory() -> rusqlite::Result<Connection> {
    init(Connection::open_in_memory()?)
}

fn init(conn: Connection) -> rusqlite::Result<Connection> {
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    migrate(&conn)?;
    Ok(conn)
}

fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version < 1 {
        conn.execute_batch(SCHEMA_V1)?;
        seed(conn)?;
        conn.execute_batch("PRAGMA user_version = 1")?;
    }
    Ok(())
}

/// A small starter deck so the app is usable on first launch.
fn seed(conn: &Connection) -> rusqlite::Result<()> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM decks", [], |r| r.get(0))?;
    if count > 0 {
        return Ok(());
    }
    let t = now();
    conn.execute(
        "INSERT INTO decks (name, description, created_at) VALUES (?1, ?2, ?3)",
        params!["Getting Started", "Common HSK 1 words to try the app with", t],
    )?;
    let deck_id = conn.last_insert_rowid();
    let words = [
        ("你好", "nǐ hǎo", "hello"),
        ("谢谢", "xiè xie", "thank you"),
        ("我", "wǒ", "I; me"),
        ("你", "nǐ", "you"),
        ("是", "shì", "to be; yes"),
        ("中国", "Zhōng guó", "China"),
        ("学习", "xué xí", "to study; to learn"),
        ("汉语", "Hàn yǔ", "Chinese (language)"),
        ("朋友", "péng you", "friend"),
        ("吃", "chī", "to eat"),
        ("喝", "hē", "to drink"),
        ("水", "shuǐ", "water"),
    ];
    let mut stmt = conn.prepare(
        "INSERT INTO cards (deck_id, hanzi, pinyin, meaning, tags, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, 'HSK1', ?5, ?5)",
    )?;
    for (i, (h, p, m)) in words.iter().enumerate() {
        stmt.execute(params![deck_id, h, p, m, t + i as i64])?;
    }
    Ok(())
}

pub fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Seconds east of UTC for the local timezone (used to bucket reviews by local day).
pub fn local_offset() -> i64 {
    chrono::Local::now().offset().local_minus_utc() as i64
}

/// Local day number (days since the Unix epoch in local time).
pub fn local_day(ts: i64, offset: i64) -> i64 {
    (ts + offset).div_euclid(DAY)
}

/// UTC timestamp of the start of the local day containing `ts`.
pub fn day_start(ts: i64, offset: i64) -> i64 {
    local_day(ts, offset) * DAY - offset
}

pub fn get_setting(conn: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
        .optional()
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

pub fn setting_f64(conn: &Connection, key: &str, default: f64) -> f64 {
    get_setting(conn, key)
        .ok()
        .flatten()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

pub fn setting_i64(conn: &Connection, key: &str, default: i64) -> i64 {
    get_setting(conn, key)
        .ok()
        .flatten()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

pub const DEFAULT_RETENTION: f64 = 0.9;
pub const DEFAULT_NEW_PER_DAY: i64 = 20;

pub fn desired_retention(conn: &Connection) -> f64 {
    setting_f64(conn, "desired_retention", DEFAULT_RETENTION).clamp(0.7, 0.99)
}
