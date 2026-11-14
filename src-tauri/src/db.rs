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

/// Open the cards database, with the dictionary (its own file, next to it) attached.
pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    init(conn, Some(&path.with_file_name(crate::dictionary::DB_FILE)))
}

#[cfg(test)]
pub fn open_in_memory() -> rusqlite::Result<Connection> {
    init(Connection::open_in_memory()?, None)
}

/// A private copy of the app's databases, for the benchmark tests that need a learner's real
/// cards and the dictionary. Tests must never open the real files: a test process that hangs
/// (the GPU ones do) keeps them open, and that is how the database was damaged twice.
#[cfg(test)]
pub fn open_copy(app_data: &Path) -> Connection {
    let dir = std::env::temp_dir().join(format!("hanzi-copy-{}-{}", std::process::id(), now()));
    std::fs::create_dir_all(&dir).unwrap();
    for file in [crate::data::DB_FILE, crate::dictionary::DB_FILE] {
        if app_data.join(file).exists() {
            std::fs::copy(app_data.join(file), dir.join(file)).unwrap();
        }
    }
    open(&dir.join(crate::data::DB_FILE)).unwrap()
}

/// How the app holds a database of the learner's own work (cards, reading). Each choice
/// closes off a way the file could end up damaged or out of step:
///
/// - **Exclusive**: this connection alone has the file for as long as the app runs. Another
///   program (a second copy, a tool, a test) can't open it meanwhile, and if one already
///   has it, opening fails here instead of two programs sharing it.
/// - **Rollback journal, not a write-ahead log**: every finished change is in the main file
///   at once. With a log, changes live in a second file until merged, and for a database
///   that is open for days that second file is one more thing to lose or mix up (both times
///   the cards database was damaged, the main file was days behind its log).
/// - **Full sync**: a change is on disk before the app carries on, so a power cut loses at
///   most the change being made.
/// - Cell-size checks and defensive mode: damage is noticed when a page is read rather than
///   acted on, and nothing in SQL can rewrite the file's own tables.
pub fn secure(conn: &Connection) -> rusqlite::Result<()> {
    conn.query_row("PRAGMA main.locking_mode=EXCLUSIVE", [], |_| Ok(()))?;
    // (leaves write-ahead mode if the file was in it: merges and removes the log)
    conn.query_row("PRAGMA main.journal_mode=TRUNCATE", [], |_| Ok(()))?;
    conn.execute_batch("PRAGMA main.synchronous=FULL; PRAGMA foreign_keys=ON; PRAGMA cell_size_check=ON; PRAGMA trusted_schema=OFF;")?;
    conn.set_db_config(rusqlite::config::DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)?;
    Ok(())
}

/// How long to wait for a database someone else is using: a moment when opening (is it
/// free at all?), longer afterwards (the dictionary's own import, see commands.rs).
pub const OPEN_WAIT: std::time::Duration = std::time::Duration::from_secs(3);
pub const BUSY_WAIT: std::time::Duration = std::time::Duration::from_secs(30);

fn init(conn: Connection, dictionary: Option<&Path>) -> rusqlite::Result<Connection> {
    conn.busy_timeout(OPEN_WAIT)?;
    secure(&conn)?;
    crate::dictionary::attach(&conn, dictionary)?;
    migrate(&conn)?;
    // the dictionary's log merged into its file (after an upgrade it holds the whole dictionary)
    let _ = conn.execute_batch("PRAGMA dict.wal_checkpoint(TRUNCATE)");
    conn.busy_timeout(BUSY_WAIT)?;
    Ok(conn)
}

/// The version `migrate` brings a cards database to.
pub const SCHEMA_VERSION: i64 = 13;

/// What an app is told when the data is from a newer version than itself. `from`: the
/// version that last used the data, when the data says.
pub fn newer_message(from: Option<&str>) -> String {
    let by = match from {
        Some(v) if !v.trim().is_empty() => format!("Hanzi Cards {}", v.trim()),
        _ => "a newer version of Hanzi Cards".to_string(),
    };
    format!(
        "This data was last used by {by}, which is newer than this version ({}), and this version can't use it. Install version {} or later. Nothing has been changed.",
        env!("CARGO_PKG_VERSION"),
        from.map(str::trim).filter(|v| !v.is_empty()).unwrap_or("the latest")
    )
}

/// Where each database says which version of the app last used it.
pub const CARDS_APP_VERSION: &str = "SELECT value FROM settings WHERE key = 'app_version'";
pub const READING_APP_VERSION: &str = "SELECT value FROM sync_meta WHERE key = 'app_version'";

/// A database from a newer version of the app is not opened: this version doesn't know what
/// its tables mean, and working on them could spoil what the newer version put there.
/// `used_by`: the query for the app version recorded in it (see above).
pub fn refuse_newer(conn: &Connection, found: i64, known: i64, used_by: &str) -> rusqlite::Result<()> {
    if found > known {
        let from: Option<String> = conn.query_row(used_by, [], |r| r.get(0)).ok();
        return Err(rusqlite::Error::SqliteFailure(rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_ERROR), Some(newer_message(from.as_deref()))));
    }
    Ok(())
}

/// Versions of the app released before `refuse_newer` existed (0.3.2 and earlier) open
/// whatever they find. What stops them is in the database itself: every version records its
/// name in the settings as it starts, and these triggers refuse that record unless the app
/// has first said (in `sync_meta`, for the length of one transaction) that it knows this
/// version of the tables. An older app says nothing, its start-up write fails with the
/// message below, and it shows that message instead of working on the data.
/// Made again at every start, so the number is always this version's.
fn guard_older_versions(conn: &Connection) -> rusqlite::Result<()> {
    let refuse = format!(
        "WHEN NEW.key = 'app_version'
           AND COALESCE((SELECT CAST(value AS INTEGER) FROM sync_meta WHERE key = 'opened_by'), 0) < {SCHEMA_VERSION}
         BEGIN SELECT RAISE(ABORT, '{}'); END;",
        // (the older app can't say which version it is, so the message names only this one)
        format!(
            "This data was updated by Hanzi Cards {v}, which is newer than this version, and this version can't use it. Install version {v} or later. Nothing has been changed.",
            v = env!("CARGO_PKG_VERSION")
        )
        .replace('\'', "''")
    );
    conn.execute_batch(&format!(
        "DROP TRIGGER IF EXISTS main.guard_older_insert;
         DROP TRIGGER IF EXISTS main.guard_older_update;
         CREATE TRIGGER main.guard_older_insert BEFORE INSERT ON settings {refuse}
         CREATE TRIGGER main.guard_older_update BEFORE UPDATE ON settings {refuse}"
    ))
}

/// Record which version of the app is using the database (see `guard_older_versions`).
pub fn record_app_version(conn: &Connection, version: &str) -> rusqlite::Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute("INSERT OR REPLACE INTO sync_meta (key, value) VALUES ('opened_by', ?1)", [SCHEMA_VERSION])?;
    set_setting(&tx, "app_version", version)?;
    tx.execute("DELETE FROM sync_meta WHERE key = 'opened_by'", [])?;
    tx.commit()
}

fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let version: i64 = conn.query_row("PRAGMA main.user_version", [], |r| r.get(0))?;
    refuse_newer(conn, version, SCHEMA_VERSION, CARDS_APP_VERSION)?;
    if version < 1 {
        conn.execute_batch(SCHEMA_V1)?;
        seed(conn)?;
        conn.execute_batch("PRAGMA main.user_version = 1")?;
    }
    if version < 2 {
        // v2: a usage score for ranking dictionary results, and pinyin indexes for
        // typing characters by pinyin. Scores are computed for an existing dictionary.
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(
            "ALTER TABLE dictionary ADD COLUMN score INTEGER NOT NULL DEFAULT 0;
             CREATE INDEX IF NOT EXISTS idx_dict_pkey ON dictionary(pinyin_key, score);
             CREATE INDEX IF NOT EXISTS idx_dict_pkey_tone ON dictionary(pinyin_key_tone);",
        )?;
        crate::dictionary::compute_scores(&tx)?;
        tx.execute_batch("PRAGMA main.user_version = 2")?;
        tx.commit()?;
    }
    if version < 3 {
        // v3: revised scoring (reading frequency for single characters, no proper-noun penalty).
        let tx = conn.unchecked_transaction()?;
        crate::dictionary::compute_scores(&tx)?;
        tx.execute_batch("PRAGMA main.user_version = 3")?;
        tx.commit()?;
    }
    if version < 4 {
        // v4: nested decks.
        conn.execute_batch(
            "BEGIN;
             ALTER TABLE decks ADD COLUMN parent_id INTEGER REFERENCES decks(id) ON DELETE CASCADE;
             CREATE INDEX IF NOT EXISTS idx_decks_parent ON decks(parent_id);
             PRAGMA main.user_version = 4;
             COMMIT;",
        )?;
    }
    if version < 5 {
        // v5: a record of AI tutor spending.
        conn.execute_batch(
            "BEGIN;
             CREATE TABLE IF NOT EXISTS ai_usage (
               id INTEGER PRIMARY KEY,
               at INTEGER NOT NULL,
               kind TEXT NOT NULL,
               model TEXT NOT NULL,
               session_id INTEGER,
               input_tokens INTEGER NOT NULL DEFAULT 0,
               output_tokens INTEGER NOT NULL DEFAULT 0,
               cache_read_tokens INTEGER NOT NULL DEFAULT 0,
               cache_write_tokens INTEGER NOT NULL DEFAULT 0,
               cost REAL NOT NULL DEFAULT 0
             );
             CREATE INDEX IF NOT EXISTS idx_ai_usage_at ON ai_usage(at);
             PRAGMA main.user_version = 5;
             COMMIT;",
        )?;
    }
    if version < 6 {
        // v6: tag existing cards with their HSK level (2026 syllabus); only adds tags.
        let tx = conn.unchecked_transaction()?;
        crate::hsk::tag_all(&tx).map_err(|e| rusqlite::Error::ToSqlConversionFailure(e.to_string().into()))?;
        tx.execute_batch("PRAGMA main.user_version = 6")?;
        tx.commit()?;
    }
    if version < 7 {
        // v7: combinations of list words are tagged too (highest level of their parts).
        let tx = conn.unchecked_transaction()?;
        crate::hsk::tag_all(&tx).map_err(|e| rusqlite::Error::ToSqlConversionFailure(e.to_string().into()))?;
        tx.execute_batch("PRAGMA main.user_version = 7")?;
        tx.commit()?;
    }
    if version < 8 {
        // v8: words missing from the 2026 syllabus take their 2021 HSK 3.0 level.
        let tx = conn.unchecked_transaction()?;
        crate::hsk::tag_all(&tx).map_err(|e| rusqlite::Error::ToSqlConversionFailure(e.to_string().into()))?;
        tx.execute_batch("PRAGMA main.user_version = 8")?;
        tx.commit()?;
    }
    if version < 9 {
        // v9: correct recalls so far while (re)learning (a new word needs 3 before it graduates).
        conn.execute_batch(
            "BEGIN;
             ALTER TABLE progress ADD COLUMN step INTEGER NOT NULL DEFAULT 0;
             PRAGMA main.user_version = 9;
             COMMIT;",
        )?;
    }
    if version < 10 {
        // v10: the dictionary moves to its own file (dictionary.db, attached as `dict`). It is
        // five times the size of everything else and can always be made again, so backups of
        // the cards are small, and trouble with one file can't reach the other.
        let tx = conn.unchecked_transaction()?;
        let old: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM main.sqlite_schema WHERE type = 'table' AND name = 'dictionary')",
            [],
            |r| r.get(0),
        )?;
        if old {
            let empty: bool = tx.query_row("SELECT NOT EXISTS(SELECT 1 FROM dict.dictionary)", [], |r| r.get(0))?;
            if empty {
                tx.execute_batch(
                    "INSERT INTO dict.dictionary
                        (id, traditional, simplified, pinyin_num, pinyin, pinyin_key, pinyin_key_tone, definitions, score)
                     SELECT id, traditional, simplified, pinyin_num, pinyin, pinyin_key, pinyin_key_tone, definitions, score
                       FROM main.dictionary;
                     INSERT OR REPLACE INTO dict.dictionary_meta (key, value)
                     SELECT 'imported_at', value FROM main.settings WHERE key = 'dict_imported_at';",
                )?;
            }
            tx.execute_batch("DROP TABLE main.dictionary; DELETE FROM main.settings WHERE key = 'dict_imported_at';")?;
        }
        tx.execute_batch("PRAGMA main.user_version = 10")?;
        tx.commit()?;
        if old {
            // hand the dictionary's space back (it stays as it is if this can't be done now)
            let _ = conn.execute_batch("VACUUM main");
        }
    }
    if version < 11 {
        // v11: a copy of a word added to another deck after the word was studied takes up the
        // progress its other copies share (new copies do from now on).
        let tx = conn.unchecked_transaction()?;
        crate::study::share_progress(&tx, None).map_err(|e| rusqlite::Error::ToSqlConversionFailure(e.to_string().into()))?;
        tx.execute_batch("PRAGMA main.user_version = 11")?;
        tx.commit()?;
    }
    if version < 12 {
        // v12: time spent reading and with the tutor, a total for each day (the time spent
        // answering flashcards is in review_log, answer by answer).
        conn.execute_batch(
            "BEGIN;
             CREATE TABLE IF NOT EXISTS study_time (
               day     INTEGER NOT NULL,
               kind    TEXT NOT NULL,
               seconds INTEGER NOT NULL DEFAULT 0,
               PRIMARY KEY (day, kind)
             );
             PRAGMA main.user_version = 12;
             COMMIT;",
        )?;
    }
    if version < 13 {
        // v13: permanent ids for decks, cards and answers, a "changed" time for decks, a record
        // of deletions, and reading/tutor time kept per device (see sync_data.rs).
        let tx = conn.unchecked_transaction()?;
        crate::sync_data::upgrade_cards(&tx)?;
        tx.execute_batch("PRAGMA main.user_version = 13")?;
        tx.commit()?;
    }
    guard_older_versions(conn)?;
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
