//! The databases' side of keeping devices the same: what each row needs so that it can be
//! recognised on another device, reading everything out as a snapshot, and writing a merged
//! snapshot back.
//!
//! Row numbers are a database's own business, different on every device. So every deck,
//! card, answer, collection and story also has a permanent id (`uid`), decks and collections
//! record when they were last changed (cards and stories already did), and deletions are
//! written down (`sync_deleted`). Triggers do all of this, so no part of the app that adds,
//! changes or deletes a row can forget to.

use crate::error::{AppError, AppResult};
use hanzi_sync::snapshot::*;
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};

/// The id of a row that existed before rows had ids: worked out from the row, so that two
/// devices holding copies of the same database give the same row the same id.
fn old_uid(kind: &str, id: i64, created: i64, text: &str) -> String {
    let mut h = Sha256::new();
    h.update(format!("{kind}\u{1f}{id}\u{1f}{created}\u{1f}{text}"));
    hanzi_sync::hex(&h.finalize()[..16])
}

/// Not while a merged snapshot is being written in: those rows arrive with their own times,
/// and what it removes is not a deletion to pass on.
const QUIET: &str = "NOT EXISTS (SELECT 1 FROM sync_meta WHERE key = 'applying')";

const TABLES: &str = "
CREATE TABLE IF NOT EXISTS sync_deleted (
    kind TEXT NOT NULL,
    uid  TEXT NOT NULL,
    at   INTEGER NOT NULL,
    PRIMARY KEY (kind, uid)
);
CREATE TABLE IF NOT EXISTS sync_alias (
    old TEXT PRIMARY KEY,
    new TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS sync_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
";

/// A new row gets an id; a row whose content changes gets a new "changed" time (one later
/// than its last even if the clock hasn't moved, or was set back); a row that is deleted is
/// written down. (never as earlier than its last change: deleting it is the later act). `inside`: the column naming the row's container and that container's table:
/// a row that goes because its container went isn't written down itself (the container is).
fn triggers(table: &str, kind: &str, content: &[&str], inside: Option<(&str, &str)>) -> String {
    let differs = content.iter().map(|c| format!("NEW.{c} IS NOT OLD.{c}")).collect::<Vec<_>>().join(" OR ");
    let columns = content.join(", ");
    let still_there = match inside {
        Some((column, parent)) => format!("AND (OLD.{column} IS NULL OR EXISTS (SELECT 1 FROM {parent} WHERE id = OLD.{column}))"),
        None => String::new(),
    };
    format!(
        "CREATE TRIGGER IF NOT EXISTS sync_{table}_new AFTER INSERT ON {table} WHEN NEW.uid IS NULL BEGIN
           UPDATE {table} SET uid = lower(hex(randomblob(16))) WHERE id = NEW.id;
         END;
         CREATE TRIGGER IF NOT EXISTS sync_{table}_new_time AFTER INSERT ON {table} WHEN NEW.updated_at = 0 BEGIN
           UPDATE {table} SET updated_at = NEW.created_at WHERE id = NEW.id;
         END;
         CREATE TRIGGER IF NOT EXISTS sync_{table}_changed AFTER UPDATE OF {columns} ON {table}
         WHEN NEW.updated_at = OLD.updated_at AND ({differs}) AND {QUIET} BEGIN
           UPDATE {table} SET updated_at = MAX(OLD.updated_at + 1, CAST(strftime('%s', 'now') AS INTEGER)) WHERE id = NEW.id;
         END;
         CREATE TRIGGER IF NOT EXISTS sync_{table}_gone AFTER DELETE ON {table}
         WHEN OLD.uid IS NOT NULL AND {QUIET} {still_there} BEGIN
           INSERT OR REPLACE INTO sync_deleted (kind, uid, at)
           VALUES ('{kind}', OLD.uid, MAX(OLD.updated_at, CAST(strftime('%s', 'now') AS INTEGER)));
         END;"
    )
}

/// Answers are never changed, only added and (when one is taken back) removed.
const LOG_TRIGGERS: &str = "
CREATE TRIGGER IF NOT EXISTS sync_review_log_new AFTER INSERT ON review_log WHEN NEW.uid IS NULL BEGIN
  UPDATE review_log SET uid = lower(hex(randomblob(16))) WHERE id = NEW.id;
END;
CREATE TRIGGER IF NOT EXISTS sync_review_log_gone AFTER DELETE ON review_log
WHEN OLD.uid IS NOT NULL AND NOT EXISTS (SELECT 1 FROM sync_meta WHERE key = 'applying')
     AND EXISTS (SELECT 1 FROM cards WHERE id = OLD.card_id) BEGIN
  INSERT OR REPLACE INTO sync_deleted (kind, uid, at) VALUES ('log', OLD.uid, CAST(strftime('%s', 'now') AS INTEGER));
END;
";

/// Cards database, version 13.
pub fn upgrade_cards(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(TABLES)?;
    conn.execute_batch(
        "ALTER TABLE decks ADD COLUMN uid TEXT;
         ALTER TABLE decks ADD COLUMN updated_at INTEGER NOT NULL DEFAULT 0;
         UPDATE decks SET updated_at = created_at;
         ALTER TABLE cards ADD COLUMN uid TEXT;
         ALTER TABLE review_log ADD COLUMN uid TEXT;
         INSERT OR IGNORE INTO sync_meta (key, value) VALUES ('device_id', lower(hex(randomblob(16))));
         -- reading and tutor time: a total for each day *on each device* (a device only adds
         -- to its own, so two devices' totals can be told apart and added up). What was
         -- recorded before devices were told apart stays under no device.
         CREATE TABLE study_time_by_device (
           day     INTEGER NOT NULL,
           kind    TEXT NOT NULL,
           device  TEXT NOT NULL DEFAULT '',
           seconds INTEGER NOT NULL DEFAULT 0,
           PRIMARY KEY (day, kind, device)
         );
         INSERT INTO study_time_by_device (day, kind, device, seconds) SELECT day, kind, '', seconds FROM study_time;
         DROP TABLE study_time;
         ALTER TABLE study_time_by_device RENAME TO study_time;",
    )?;
    backfill(conn, "decks", "deck", "created_at", "name")?;
    backfill(conn, "cards", "card", "created_at", "hanzi")?;
    backfill(conn, "review_log", "log", "reviewed_at", "card_id || '/' || direction")?;
    conn.execute_batch(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_decks_uid ON decks(uid);
         CREATE UNIQUE INDEX IF NOT EXISTS idx_cards_uid ON cards(uid);
         CREATE UNIQUE INDEX IF NOT EXISTS idx_log_uid ON review_log(uid);",
    )?;
    conn.execute_batch(&triggers("decks", "deck", &["name", "description", "parent_id"], None))?;
    conn.execute_batch(&triggers("cards", "card", &["deck_id", "hanzi", "pinyin", "meaning", "notes", "tags"], Some(("deck_id", "decks"))))?;
    conn.execute_batch(LOG_TRIGGERS)
}

/// Reading database, version 4.
pub fn upgrade_stories(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(TABLES)?;
    conn.execute_batch(
        "ALTER TABLE story_folders ADD COLUMN uid TEXT;
         ALTER TABLE story_folders ADD COLUMN updated_at INTEGER NOT NULL DEFAULT 0;
         UPDATE story_folders SET updated_at = created_at;
         ALTER TABLE stories ADD COLUMN uid TEXT;",
    )?;
    backfill(conn, "story_folders", "folder", "created_at", "name")?;
    backfill(conn, "stories", "story", "created_at", "title")?;
    conn.execute_batch(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_story_folders_uid ON story_folders(uid);
         CREATE UNIQUE INDEX IF NOT EXISTS idx_stories_uid ON stories(uid);",
    )?;
    conn.execute_batch(&triggers("story_folders", "folder", &["name", "description", "parent_id", "position"], None))?;
    conn.execute_batch(&triggers("stories", "story", &["folder_id", "title", "body", "vocab", "position", "bookmarked"], Some(("folder_id", "story_folders"))))
}

fn backfill(conn: &Connection, table: &str, kind: &str, created: &str, text: &str) -> rusqlite::Result<()> {
    let rows: Vec<(i64, i64, String)> =
        conn.prepare(&format!("SELECT id, {created}, {text} FROM {table} WHERE uid IS NULL"))?.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<Result<_, _>>()?;
    let mut set = conn.prepare(&format!("UPDATE {table} SET uid = ?2 WHERE id = ?1"))?;
    for (id, created, text) in rows {
        set.execute(params![id, old_uid(kind, id, created, &text)])?;
    }
    Ok(())
}

/// This device's own id (for its reading and tutor time).
#[cfg(test)]
pub fn device_id(conn: &Connection) -> rusqlite::Result<String> {
    conn.query_row("SELECT value FROM sync_meta WHERE key = 'device_id'", [], |r| r.get(0))
}

/// A database brought in from somewhere else (a backup, another computer) may carry that
/// device's id: this device takes a new one, so the two never add to the same totals.
pub fn new_device_id(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute("INSERT OR REPLACE INTO sync_meta (key, value) VALUES ('device_id', lower(hex(randomblob(16))))", [])?;
    Ok(())
}

// ---------------------------------------------------------------- out

fn rows<T>(conn: &Connection, sql: &str, row: impl Fn(&rusqlite::Row) -> rusqlite::Result<T>) -> rusqlite::Result<Vec<T>> {
    conn.prepare(sql)?.query_map([], |r| row(r))?.collect()
}

/// Everything in both databases, as a snapshot (sorted, so that two exports of the same
/// data are the same snapshot).
pub fn export(cards: &Connection, stories: &Connection) -> AppResult<Snapshot> {
    let tomb = |r: &rusqlite::Row| Ok(Tomb { kind: r.get(0)?, uid: r.get(1)?, at: r.get(2)? });
    let alias = |r: &rusqlite::Row| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?));
    let mut deleted = rows(cards, "SELECT kind, uid, at FROM sync_deleted", tomb)?;
    deleted.extend(rows(stories, "SELECT kind, uid, at FROM sync_deleted", tomb)?);
    deleted.sort_by(|a, b| (&a.kind, &a.uid).cmp(&(&b.kind, &b.uid)));
    deleted.dedup_by(|b, a| {
        let same = a.kind == b.kind && a.uid == b.uid;
        if same {
            a.at = a.at.max(b.at);
        }
        same
    });
    let mut aliases = rows(cards, "SELECT old, new FROM sync_alias", alias)?;
    aliases.extend(rows(stories, "SELECT old, new FROM sync_alias", alias)?);
    aliases.sort();
    aliases.dedup();
    Ok(Snapshot {
        format: FORMAT,
        cards_schema: crate::db::SCHEMA_VERSION,
        stories_schema: crate::stories::SCHEMA_VERSION,
        decks: rows(
            cards,
            "SELECT d.uid, p.uid, d.name, d.description, d.created_at, d.updated_at
             FROM decks d LEFT JOIN decks p ON p.id = d.parent_id ORDER BY d.uid",
            |r| Ok(Deck { uid: r.get(0)?, parent: r.get(1)?, name: r.get(2)?, description: r.get(3)?, created_at: r.get(4)?, updated_at: r.get(5)? }),
        )?,
        cards: rows(
            cards,
            "SELECT c.uid, d.uid, c.hanzi, c.pinyin, c.meaning, c.notes, c.tags, c.created_at, c.updated_at
             FROM cards c JOIN decks d ON d.id = c.deck_id ORDER BY c.uid",
            |r| {
                Ok(Card {
                    uid: r.get(0)?,
                    deck: r.get(1)?,
                    hanzi: r.get(2)?,
                    pinyin: r.get(3)?,
                    meaning: r.get(4)?,
                    notes: r.get(5)?,
                    tags: r.get(6)?,
                    created_at: r.get(7)?,
                    updated_at: r.get(8)?,
                })
            },
        )?,
        progress: rows(
            cards,
            "SELECT c.uid, p.direction, p.state, p.stability, p.difficulty, p.due, p.last_review, p.reps, p.lapses, p.step
             FROM progress p JOIN cards c ON c.id = p.card_id ORDER BY c.uid, p.direction",
            |r| {
                Ok(Progress {
                    card: r.get(0)?,
                    direction: r.get(1)?,
                    state: r.get(2)?,
                    stability: r.get(3)?,
                    difficulty: r.get(4)?,
                    due: r.get(5)?,
                    last_review: r.get(6)?,
                    reps: r.get(7)?,
                    lapses: r.get(8)?,
                    step: r.get(9)?,
                })
            },
        )?,
        log: rows(
            cards,
            "SELECT l.uid, c.uid, l.direction, l.rating, l.state_before, l.reviewed_at, l.duration_ms, l.scheduled_secs
             FROM review_log l JOIN cards c ON c.id = l.card_id ORDER BY l.uid",
            |r| {
                Ok(Review {
                    uid: r.get(0)?,
                    card: r.get(1)?,
                    direction: r.get(2)?,
                    rating: r.get(3)?,
                    state_before: r.get(4)?,
                    reviewed_at: r.get(5)?,
                    duration_ms: r.get(6)?,
                    scheduled_secs: r.get(7)?,
                })
            },
        )?,
        time: rows(cards, "SELECT day, kind, device, seconds FROM study_time ORDER BY day, kind, device", |r| {
            Ok(Time { day: r.get(0)?, kind: r.get(1)?, device: r.get(2)?, seconds: r.get(3)? })
        })?,
        folders: rows(
            stories,
            "SELECT f.uid, p.uid, f.name, f.description, f.position, f.created_at, f.updated_at
             FROM story_folders f LEFT JOIN story_folders p ON p.id = f.parent_id ORDER BY f.uid",
            |r| Ok(Folder { uid: r.get(0)?, parent: r.get(1)?, name: r.get(2)?, description: r.get(3)?, position: r.get(4)?, created_at: r.get(5)?, updated_at: r.get(6)? }),
        )?,
        stories: rows(
            stories,
            "SELECT s.uid, f.uid, s.title, s.body, s.vocab, s.position, s.bookmarked, s.created_at, s.updated_at
             FROM stories s LEFT JOIN story_folders f ON f.id = s.folder_id ORDER BY s.uid",
            |r| {
                Ok(Story {
                    uid: r.get(0)?,
                    folder: r.get(1)?,
                    title: r.get(2)?,
                    body: r.get(3)?,
                    vocab: r.get(4)?,
                    position: r.get(5)?,
                    bookmarked: r.get(6)?,
                    created_at: r.get(7)?,
                    updated_at: r.get(8)?,
                })
            },
        )?,
        deleted,
        aliases,
    })
}

// ---------------------------------------------------------------- in

/// Which database a deletion's record is kept in.
fn in_reading(kind: &str) -> bool {
    matches!(kind, "folder" | "story")
}

/// Start writing a snapshot in: the triggers stand back, and ids that were given up for
/// another (the same thing made on two devices) are renamed, so the row keeps its place.
fn begin(tx: &Connection, s: &Snapshot, tables: &[&str]) -> rusqlite::Result<()> {
    tx.execute("INSERT OR REPLACE INTO sync_meta (key, value) VALUES ('applying', '1')", [])?;
    tx.execute_batch("CREATE TEMP TABLE IF NOT EXISTS sync_keep (uid TEXT PRIMARY KEY); DELETE FROM sync_keep;")?;
    for table in tables {
        let mut rename = tx.prepare(&format!("UPDATE {table} SET uid = ?2 WHERE uid = ?1 AND NOT EXISTS (SELECT 1 FROM {table} WHERE uid = ?2)"))?;
        for (old, new) in &s.aliases {
            rename.execute(params![old, new])?;
        }
    }
    Ok(())
}

/// Finish: the records of deletions and of ids given up are the snapshot's, and the
/// database must still be whole (nothing pointing at a row that isn't there).
fn finish(tx: &Connection, s: &Snapshot, reading: bool) -> AppResult<()> {
    tx.execute("DELETE FROM sync_deleted", [])?;
    let mut put = tx.prepare("INSERT OR REPLACE INTO sync_deleted (kind, uid, at) VALUES (?1, ?2, ?3)")?;
    for t in s.deleted.iter().filter(|t| in_reading(&t.kind) == reading) {
        put.execute(params![t.kind, t.uid, t.at])?;
    }
    tx.execute("DELETE FROM sync_alias", [])?;
    let mut put = tx.prepare("INSERT OR REPLACE INTO sync_alias (old, new) VALUES (?1, ?2)")?;
    for (old, new) in &s.aliases {
        put.execute(params![old, new])?;
    }
    tx.execute("DELETE FROM sync_meta WHERE key = 'applying'", [])?;
    tx.execute_batch("DROP TABLE IF EXISTS temp.sync_keep")?;
    let broken: i64 = tx.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| r.get(0))?;
    if broken > 0 {
        return Err(AppError::msg("The merged data didn't fit together; nothing was changed."));
    }
    Ok(())
}

fn keep(tx: &Connection, uids: impl Iterator<Item = impl AsRef<str>>) -> rusqlite::Result<()> {
    tx.execute("DELETE FROM sync_keep", [])?;
    let mut put = tx.prepare("INSERT OR IGNORE INTO sync_keep (uid) VALUES (?1)")?;
    for uid in uids {
        put.execute([uid.as_ref()])?;
    }
    Ok(())
}

/// Make the cards database hold exactly what the snapshot does. One transaction: all of it
/// or none of it.
pub fn apply_cards(conn: &mut Connection, s: &Snapshot) -> AppResult<()> {
    let tx = conn.transaction()?;
    begin(&tx, s, &["decks", "cards", "review_log"])?;
    {
        // decks: first each one, then where each one is (a parent may come later in the list)
        let mut put = tx.prepare(
            "INSERT INTO decks (uid, name, description, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(uid) DO UPDATE SET name = excluded.name, description = excluded.description,
               created_at = excluded.created_at, updated_at = excluded.updated_at",
        )?;
        for d in &s.decks {
            put.execute(params![d.uid, d.name, d.description, d.created_at, d.updated_at])?;
        }
        let mut place = tx.prepare("UPDATE decks SET parent_id = (SELECT p.id FROM decks p WHERE p.uid = ?2) WHERE uid = ?1")?;
        for d in &s.decks {
            place.execute(params![d.uid, d.parent])?;
        }
        let mut put = tx.prepare(
            "INSERT INTO cards (uid, deck_id, hanzi, pinyin, meaning, notes, tags, created_at, updated_at)
             VALUES (?1, (SELECT id FROM decks WHERE uid = ?2), ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(uid) DO UPDATE SET deck_id = excluded.deck_id, hanzi = excluded.hanzi, pinyin = excluded.pinyin,
               meaning = excluded.meaning, notes = excluded.notes, tags = excluded.tags,
               created_at = excluded.created_at, updated_at = excluded.updated_at
             WHERE cards.deck_id IS NOT excluded.deck_id OR cards.hanzi IS NOT excluded.hanzi OR cards.pinyin IS NOT excluded.pinyin
                OR cards.meaning IS NOT excluded.meaning OR cards.notes IS NOT excluded.notes OR cards.tags IS NOT excluded.tags
                OR cards.created_at IS NOT excluded.created_at OR cards.updated_at IS NOT excluded.updated_at",
        )?;
        for c in &s.cards {
            put.execute(params![c.uid, c.deck, c.hanzi, c.pinyin, c.meaning, c.notes, c.tags, c.created_at, c.updated_at])?;
        }
        // what the snapshot doesn't have goes: cards first, then decks
        keep(&tx, s.cards.iter().map(|c| &c.uid))?;
        tx.execute("DELETE FROM cards WHERE uid IS NOT NULL AND uid NOT IN (SELECT uid FROM sync_keep)", [])?;
        keep(&tx, s.decks.iter().map(|d| &d.uid))?;
        tx.execute("DELETE FROM decks WHERE uid IS NOT NULL AND uid NOT IN (SELECT uid FROM sync_keep)", [])?;

        // answers: the new ones in, the ones taken back out
        let mut put = tx.prepare(
            "INSERT INTO review_log (uid, card_id, direction, rating, state_before, reviewed_at, duration_ms, scheduled_secs)
             VALUES (?1, (SELECT id FROM cards WHERE uid = ?2), ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(uid) DO UPDATE SET card_id = excluded.card_id WHERE review_log.card_id IS NOT excluded.card_id",
        )?;
        for r in &s.log {
            put.execute(params![r.uid, r.card, r.direction, r.rating, r.state_before, r.reviewed_at, r.duration_ms, r.scheduled_secs])?;
        }
        keep(&tx, s.log.iter().map(|r| &r.uid))?;
        tx.execute("DELETE FROM review_log WHERE uid IS NOT NULL AND uid NOT IN (SELECT uid FROM sync_keep)", [])?;

        // schedules and time: as the snapshot has them
        tx.execute("DELETE FROM progress", [])?;
        let mut put = tx.prepare(
            "INSERT INTO progress (card_id, direction, state, stability, difficulty, due, last_review, reps, lapses, step)
             VALUES ((SELECT id FROM cards WHERE uid = ?1), ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )?;
        for p in &s.progress {
            put.execute(params![p.card, p.direction, p.state, p.stability, p.difficulty, p.due, p.last_review, p.reps, p.lapses, p.step])?;
        }
        tx.execute("DELETE FROM study_time", [])?;
        let mut put = tx.prepare("INSERT INTO study_time (day, kind, device, seconds) VALUES (?1, ?2, ?3, ?4)")?;
        for t in &s.time {
            put.execute(params![t.day, t.kind, t.device, t.seconds])?;
        }
    }
    finish(&tx, s, false)?;
    tx.commit()?;
    Ok(())
}

/// The same for the reading database.
pub fn apply_stories(conn: &mut Connection, s: &Snapshot) -> AppResult<()> {
    let tx = conn.transaction()?;
    begin(&tx, s, &["story_folders", "stories"])?;
    {
        let mut put = tx.prepare(
            "INSERT INTO story_folders (uid, name, description, position, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(uid) DO UPDATE SET name = excluded.name, description = excluded.description, position = excluded.position,
               created_at = excluded.created_at, updated_at = excluded.updated_at",
        )?;
        for f in &s.folders {
            put.execute(params![f.uid, f.name, f.description, f.position, f.created_at, f.updated_at])?;
        }
        let mut place = tx.prepare("UPDATE story_folders SET parent_id = (SELECT p.id FROM story_folders p WHERE p.uid = ?2) WHERE uid = ?1")?;
        for f in &s.folders {
            place.execute(params![f.uid, f.parent])?;
        }
        let mut put = tx.prepare(
            "INSERT INTO stories (uid, folder_id, title, body, vocab, position, bookmarked, created_at, updated_at)
             VALUES (?1, (SELECT id FROM story_folders WHERE uid = ?2), ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(uid) DO UPDATE SET folder_id = excluded.folder_id, title = excluded.title, body = excluded.body,
               vocab = excluded.vocab, position = excluded.position, bookmarked = excluded.bookmarked,
               created_at = excluded.created_at, updated_at = excluded.updated_at
             WHERE stories.folder_id IS NOT excluded.folder_id OR stories.title IS NOT excluded.title OR stories.body IS NOT excluded.body
                OR stories.vocab IS NOT excluded.vocab OR stories.position IS NOT excluded.position
                OR stories.bookmarked IS NOT excluded.bookmarked OR stories.created_at IS NOT excluded.created_at
                OR stories.updated_at IS NOT excluded.updated_at",
        )?;
        for st in &s.stories {
            put.execute(params![st.uid, st.folder, st.title, st.body, st.vocab, st.position, st.bookmarked, st.created_at, st.updated_at])?;
        }
        keep(&tx, s.stories.iter().map(|x| &x.uid))?;
        tx.execute("DELETE FROM stories WHERE uid IS NOT NULL AND uid NOT IN (SELECT uid FROM sync_keep)", [])?;
        keep(&tx, s.folders.iter().map(|x| &x.uid))?;
        tx.execute("DELETE FROM story_folders WHERE uid IS NOT NULL AND uid NOT IN (SELECT uid FROM sync_keep)", [])?;
    }
    finish(&tx, s, true)?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{cards, db, decks, stories};
    use hanzi_sync::merge;

    fn device() -> (Connection, Connection) {
        (db::open_in_memory().unwrap(), stories::open_in_memory().unwrap())
    }
    fn count(conn: &Connection, sql: &str) -> i64 {
        conn.query_row(sql, [], |r| r.get(0)).unwrap()
    }
    fn sync(a: &mut (Connection, Connection), boxed: &mut Snapshot) {
        let merged = merge(&export(&a.0, &a.1).unwrap(), boxed);
        apply_cards(&mut a.0, &merged).unwrap();
        apply_stories(&mut a.1, &merged).unwrap();
        assert_eq!(export(&a.0, &a.1).unwrap(), merged, "after a sync a device holds exactly the merged snapshot");
        *boxed = merged;
    }

    #[test]
    fn every_row_has_an_id_and_changes_are_noticed() {
        let (mut c, s) = device();
        // the starter deck's rows got ids as they were made
        assert_eq!(count(&c, "SELECT COUNT(*) FROM cards WHERE uid IS NULL"), 0);
        assert_eq!(count(&c, "SELECT COUNT(*) FROM decks WHERE uid IS NULL OR updated_at = 0"), 0);
        assert_eq!(count(&c, "SELECT COUNT(DISTINCT uid) FROM cards"), 12);
        assert_eq!(device_id(&c).unwrap().len(), 32);

        // a real change moves the "changed" time on; writing the same values again doesn't
        let before = count(&c, "SELECT updated_at FROM decks");
        c.execute("UPDATE decks SET name = name", []).unwrap();
        assert_eq!(count(&c, "SELECT updated_at FROM decks"), before);
        c.execute("UPDATE decks SET name = 'Renamed'", []).unwrap();
        assert!(count(&c, "SELECT updated_at FROM decks") > before);
        let card_before = count(&c, "SELECT updated_at FROM cards WHERE hanzi = '水'");
        c.execute("UPDATE cards SET tags = tags WHERE hanzi = '水'", []).unwrap();
        assert_eq!(count(&c, "SELECT updated_at FROM cards WHERE hanzi = '水'"), card_before);
        c.execute("UPDATE cards SET tags = 'HSK1 drink' WHERE hanzi = '水'", []).unwrap();
        assert!(count(&c, "SELECT updated_at FROM cards WHERE hanzi = '水'") > card_before);

        // a deleted card is written down; cards that go with their deck are not (the deck is)
        let id = count(&c, "SELECT id FROM cards WHERE hanzi = '水'");
        cards::delete_cards(&mut c, &[id]).unwrap();
        assert_eq!(count(&c, "SELECT COUNT(*) FROM sync_deleted WHERE kind = 'card'"), 1);
        let deck = count(&c, "SELECT id FROM decks");
        decks::delete_deck(&mut c, deck).unwrap();
        assert_eq!(count(&c, "SELECT COUNT(*) FROM cards"), 0);
        assert_eq!(count(&c, "SELECT COUNT(*) FROM sync_deleted WHERE kind = 'card'"), 1);
        assert_eq!(count(&c, "SELECT COUNT(*) FROM sync_deleted WHERE kind = 'deck'"), 1);

        // stories and collections the same way
        let folder = stories::create_folder(&s, "Level 1", "", None).unwrap();
        let story = stories::create_story(&s, Some(folder), "月亮", "从前……").unwrap();
        let loose = stories::create_story(&s, None, "Loose", "一二三").unwrap();
        assert_eq!(count(&s, "SELECT COUNT(*) FROM stories WHERE uid IS NULL"), 0);
        let t = count(&s, &format!("SELECT updated_at FROM stories WHERE id = {story}"));
        stories::set_bookmark(&s, story, true).unwrap();
        assert!(count(&s, &format!("SELECT updated_at FROM stories WHERE id = {story}")) > t);
        stories::delete_story(&s, loose).unwrap();
        assert_eq!(count(&s, "SELECT COUNT(*) FROM sync_deleted WHERE kind = 'story'"), 1);
    }

    #[test]
    fn two_devices_sync_through_a_box() {
        let (mut a, mut b) = (device(), device());
        let mut boxed = Snapshot::default();
        // both start with the starter deck, made separately: after a sync, one deck
        sync(&mut a, &mut boxed);
        sync(&mut b, &mut boxed);
        sync(&mut a, &mut boxed);
        assert_eq!(count(&a.0, "SELECT COUNT(*) FROM decks"), 1);
        assert_eq!(count(&b.0, "SELECT COUNT(*) FROM cards"), 12);
        assert_eq!(export(&a.0, &a.1).unwrap(), export(&b.0, &b.1).unwrap());

        // a studies a word and writes a story; b adds a deck with a card and deletes a card
        let word = count(&a.0, "SELECT id FROM cards WHERE hanzi = '你好'");
        crate::study::answer(&mut a.0, word, "recognition", 3, 1500).unwrap();
        let folder = stories::create_folder(&a.1, "Level 1", "", None).unwrap();
        stories::create_story(&a.1, Some(folder), "月亮", "从前，月亮上住着一只兔子。").unwrap();
        crate::stats::log_time(&a.0, "reading", 120).unwrap();
        let deck = decks::create_deck(&b.0, "Animals", "", None).unwrap();
        b.0.execute("INSERT INTO cards (deck_id, hanzi, pinyin, meaning, created_at, updated_at) VALUES (?1, '猫', 'māo', 'cat', 5, 5)", [deck]).unwrap();
        let water = count(&b.0, "SELECT id FROM cards WHERE hanzi = '水'");
        cards::delete_cards(&mut b.0, &[water]).unwrap();
        crate::stats::log_time(&b.0, "reading", 60).unwrap();

        sync(&mut a, &mut boxed);
        sync(&mut b, &mut boxed);
        sync(&mut a, &mut boxed);
        assert_eq!(export(&a.0, &a.1).unwrap(), export(&b.0, &b.1).unwrap());
        for d in [&a, &b] {
            assert_eq!(count(&d.0, "SELECT COUNT(*) FROM decks"), 2);
            assert_eq!(count(&d.0, "SELECT COUNT(*) FROM cards"), 12);
            assert_eq!(count(&d.0, "SELECT COUNT(*) FROM cards WHERE hanzi = '水'"), 0);
            assert_eq!(count(&d.0, "SELECT COUNT(*) FROM review_log"), 1);
            assert_eq!(count(&d.0, "SELECT COUNT(*) FROM progress"), 1);
            assert_eq!(count(&d.0, "SELECT SUM(seconds) FROM study_time"), 180, "each device's reading time counts");
            assert_eq!(count(&d.1, "SELECT COUNT(*) FROM stories"), 1);
            assert_eq!(count(&d.1, "SELECT COUNT(*) FROM story_folders"), 1);
            crate::data::check(&d.0).unwrap();
            crate::data::check(&d.1).unwrap();
        }
        // writing a snapshot in isn't itself a change to pass on: nothing new was written down
        assert_eq!(count(&a.0, "SELECT COUNT(*) FROM sync_deleted"), 1);
        assert_eq!(count(&a.0, "SELECT COUNT(*) FROM sync_meta WHERE key = 'applying'"), 0);

        // an answer taken back on one device goes from the other too
        crate::study::undo_answer(&mut a.0, word, "recognition").unwrap();
        sync(&mut a, &mut boxed);
        sync(&mut b, &mut boxed);
        assert_eq!(count(&b.0, "SELECT COUNT(*) FROM review_log"), 0);
    }

    /// Statistics reset on one device: the answers and the time go, the schedules stay, and
    /// a device that still has the old history doesn't hand it back.
    #[test]
    fn a_reset_of_the_statistics_reaches_the_other_devices() {
        let (mut a, mut b) = (device(), device());
        let mut boxed = Snapshot::default();
        let word = count(&a.0, "SELECT id FROM cards WHERE hanzi = '你好'");
        crate::study::answer_at(&mut a.0, word, "recognition", 3, 1500, db::now() - 3 * 86_400).unwrap();
        a.0.execute("INSERT INTO study_time (day, kind, device, seconds) VALUES (?1, 'reading', 'x', 600)", [db::now() / 86_400 - 5]).unwrap();
        sync(&mut a, &mut boxed);
        sync(&mut b, &mut boxed);
        assert_eq!(count(&b.0, "SELECT COUNT(*) FROM review_log"), 1);

        assert_eq!(crate::stats::reset(&mut b.0).unwrap(), 1);
        assert_eq!(count(&b.0, "SELECT COUNT(*) FROM review_log") + count(&b.0, "SELECT COUNT(*) FROM study_time"), 0);
        assert_eq!(count(&b.0, "SELECT COUNT(*) FROM progress"), 1, "when the word is due is not statistics");
        assert_eq!(count(&b.0, "SELECT COUNT(*) FROM sync_deleted"), 1, "one record of the reset, not one per answer");
        // an answer after the reset is kept
        crate::study::answer(&mut b.0, word, "recognition", 3, 900).unwrap();

        sync(&mut b, &mut boxed);
        sync(&mut a, &mut boxed);
        sync(&mut b, &mut boxed);
        for d in [&a, &b] {
            assert_eq!(count(&d.0, "SELECT COUNT(*) FROM review_log"), 1);
            assert_eq!(count(&d.0, "SELECT COUNT(*) FROM study_time"), 0);
            assert_eq!(count(&d.0, "SELECT COUNT(*) FROM progress"), 1);
        }
    }

    #[test]
    fn a_snapshot_that_doesnt_fit_changes_nothing() {
        let (mut c, s) = device();
        let mut bad = export(&c, &s).unwrap();
        // a card in a deck that isn't there
        bad.cards[0].deck = "no such deck".into();
        let before = export(&c, &s).unwrap();
        assert!(apply_cards(&mut c, &bad).is_err());
        assert_eq!(export(&c, &s).unwrap(), before);
        assert_eq!(count(&c, "SELECT COUNT(*) FROM sync_meta WHERE key = 'applying'"), 0);
    }

    #[test]
    fn rows_from_before_ids_get_the_same_id_on_every_copy() {
        assert_eq!(old_uid("card", 7, 1700000000, "你好"), old_uid("card", 7, 1700000000, "你好"));
        assert_ne!(old_uid("card", 7, 1700000000, "你好"), old_uid("card", 8, 1700000000, "你好"));
        assert_ne!(old_uid("card", 7, 1700000000, "你好"), old_uid("deck", 7, 1700000000, "你好"));
    }

    /// A learner's real databases, upgraded and taken through a sync with themselves. On
    /// byte copies only (COPY_DIR holds copies of hanzi-cards.db and stories.db; they are
    /// copied again before being opened), never on the files the app uses.
    /// COPY_DIR=<folder> cargo test --lib real_copy -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_copy_upgrades_and_round_trips() {
        let Ok(from) = std::env::var("COPY_DIR") else { return };
        let dir = std::env::temp_dir().join(format!("hanzi-sync-real-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for f in [crate::data::DB_FILE, stories::DB_FILE] {
            std::fs::copy(std::path::Path::new(&from).join(f), dir.join(f)).unwrap();
        }
        let before = |file: &str, sql: &str| -> i64 { Connection::open(dir.join(file)).unwrap().query_row(sql, [], |r| r.get(0)).unwrap() };
        let was = (
            before(crate::data::DB_FILE, "SELECT COUNT(*) FROM cards"),
            before(crate::data::DB_FILE, "SELECT COUNT(*) FROM review_log"),
            before(crate::data::DB_FILE, "SELECT COUNT(*) FROM progress"),
            before(crate::data::DB_FILE, "SELECT COALESCE(SUM(seconds), 0) FROM study_time"),
            before(stories::DB_FILE, "SELECT COUNT(*) FROM stories"),
        );
        let t = std::time::Instant::now();
        let mut c = db::open(&dir.join(crate::data::DB_FILE)).unwrap();
        let mut s = stories::open(&dir.join(stories::DB_FILE)).unwrap();
        println!("upgraded in {:?}", t.elapsed());
        let now = (count(&c, "SELECT COUNT(*) FROM cards"), count(&c, "SELECT COUNT(*) FROM review_log"), count(&c, "SELECT COUNT(*) FROM progress"), count(&c, "SELECT COALESCE(SUM(seconds), 0) FROM study_time"), count(&s, "SELECT COUNT(*) FROM stories"));
        println!("cards, answers, schedules, seconds of reading and tutor time, stories: {was:?} -> {now:?}");
        assert_eq!(was, now, "the upgrade must not add or lose anything");
        for (conn, table) in [(&c, "decks"), (&c, "cards"), (&c, "review_log"), (&s, "story_folders"), (&s, "stories")] {
            assert_eq!(count(conn, &format!("SELECT COUNT(*) FROM {table} WHERE uid IS NULL")), 0, "{table}");
            assert_eq!(count(conn, &format!("SELECT COUNT(DISTINCT uid) FROM {table}")), count(conn, &format!("SELECT COUNT(*) FROM {table}")), "{table}");
        }
        crate::data::check(&c).unwrap();
        crate::data::check(&s).unwrap();

        let t = std::time::Instant::now();
        let all = export(&c, &s).unwrap();
        let packed = all.pack().unwrap();
        println!("read out in {:?}: {} decks, {} cards, {} answers, {} stories; {} KB packed", t.elapsed(), all.decks.len(), all.cards.len(), all.log.len(), all.stories.len(), packed.len() / 1024);
        assert_eq!(Snapshot::unpack(&packed).unwrap(), all, "nothing changes on the way through the box");
        let t = std::time::Instant::now();
        let merged = merge(&all, &Snapshot::default());
        println!("merged in {:?}", t.elapsed());
        // copies of a word are brought to the same schedule; nothing else may differ
        let mut same_but_schedules = merged.clone();
        same_but_schedules.progress = all.progress.clone();
        assert_eq!(same_but_schedules, all, "merging a device with nothing changes only schedules of copies");
        println!("schedules: {} -> {}", all.progress.len(), merged.progress.len());
        let t = std::time::Instant::now();
        apply_cards(&mut c, &merged).unwrap();
        apply_stories(&mut s, &merged).unwrap();
        println!("written back in {:?}", t.elapsed());
        assert_eq!(export(&c, &s).unwrap(), merged);
        assert_eq!(merge(&merged, &all), merged);
        crate::data::check(&c).unwrap();
        crate::data::check(&s).unwrap();
        drop((c, s));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
