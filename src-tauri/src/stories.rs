//! Reading: stories, kept in collections that nest like decks ("Graded readers › Level 1").
//!
//! A story can also sit outside any collection. A collection includes its subcollections when
//! listing stories.
//!
//! Stories live in their own database file (`stories.db`), apart from the cards database:
//! switching or restoring the cards database leaves them alone, and the other way round.

use crate::db::now;
use crate::decks::{natural_cmp, PATH_LABEL_SEP};
use crate::error::{AppError, AppResult};
use crate::pinyin;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const DB_FILE: &str = "stories.db";

const SCHEMA_V1: &str = "
CREATE TABLE IF NOT EXISTS story_folders (
    id          INTEGER PRIMARY KEY,
    parent_id   INTEGER REFERENCES story_folders(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    created_at  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_story_folders_parent ON story_folders(parent_id);

CREATE TABLE IF NOT EXISTS stories (
    id         INTEGER PRIMARY KEY,
    folder_id  INTEGER REFERENCES story_folders(id) ON DELETE CASCADE,
    title      TEXT NOT NULL,
    body       TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_stories_folder ON stories(folder_id);
";

/// Open (creating if needed) the stories database.
pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    init(Connection::open(path)?)
}

#[cfg(test)]
pub fn open_in_memory() -> rusqlite::Result<Connection> {
    init(Connection::open_in_memory()?)
}

fn init(conn: Connection) -> rusqlite::Result<Connection> {
    conn.busy_timeout(crate::db::OPEN_WAIT)?;
    crate::db::secure(&conn)?; // exclusive, every change straight into the main file
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version < 1 {
        conn.execute_batch(&format!("BEGIN; {SCHEMA_V1} PRAGMA user_version = 1; COMMIT;"))?;
    }
    if version < 2 {
        // v2: an order for collections and stories (e.g. "01_…" file numbers, Newbie before
        // Advanced), and a story's key vocabulary.
        conn.execute_batch(
            "BEGIN;
             ALTER TABLE story_folders ADD COLUMN position INTEGER NOT NULL DEFAULT 0;
             ALTER TABLE stories ADD COLUMN position INTEGER NOT NULL DEFAULT 0;
             ALTER TABLE stories ADD COLUMN vocab TEXT NOT NULL DEFAULT '';
             PRAGMA user_version = 2;
             COMMIT;",
        )?;
    }
    Ok(conn)
}

/// How many stories there are in all.
pub fn total(conn: &Connection) -> AppResult<i64> {
    Ok(conn.query_row("SELECT COUNT(*) FROM stories", [], |r| r.get(0))?)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryFolder {
    pub id: i64,
    pub parent_id: Option<i64>,
    pub name: String,
    /// Names from the top, e.g. "Graded readers › Level 1".
    pub path: String,
    pub depth: i64,
    pub child_count: i64,
    pub description: String,
    pub created_at: i64,
    /// Sort order among siblings (then by name).
    pub position: i64,
    /// Stories directly in this collection.
    pub story_count: i64,
    /// Including all subcollections.
    pub total_stories: i64,
}

/// A story without its text, for lists.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryInfo {
    pub id: i64,
    pub folder_id: Option<i64>,
    pub title: String,
    /// Chinese characters in the text.
    pub chars: i64,
    /// The start of the text.
    pub preview: String,
    /// Sort order in its collection (then by title).
    pub position: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Story {
    pub id: i64,
    pub folder_id: Option<i64>,
    pub title: String,
    pub body: String,
    /// Key vocabulary, one "word – pinyin – meaning" per line ("" for none).
    pub vocab: String,
    pub position: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

const PREVIEW_CHARS: usize = 80;

/// All collections in tree order (parents first, siblings in natural order).
pub fn list_folders(conn: &Connection) -> AppResult<Vec<StoryFolder>> {
    let mut stmt = conn.prepare(
        "SELECT f.id, f.parent_id, f.name, f.description, f.created_at,
            (SELECT COUNT(*) FROM stories s WHERE s.folder_id = f.id), f.position
         FROM story_folders f",
    )?;
    let all: Vec<StoryFolder> = stmt
        .query_map([], |r| {
            Ok(StoryFolder {
                id: r.get(0)?,
                parent_id: r.get(1)?,
                name: r.get(2)?,
                description: r.get(3)?,
                created_at: r.get(4)?,
                story_count: r.get(5)?,
                position: r.get(6)?,
                path: String::new(),
                depth: 0,
                child_count: 0,
                total_stories: 0,
            })
        })?
        .collect::<Result<_, _>>()?;

    let ids: std::collections::HashSet<i64> = all.iter().map(|f| f.id).collect();
    let mut children: HashMap<Option<i64>, Vec<usize>> = HashMap::new();
    for (i, f) in all.iter().enumerate() {
        let parent = f.parent_id.filter(|p| ids.contains(p));
        children.entry(parent).or_default().push(i);
    }
    for list in children.values_mut() {
        list.sort_by(|a, b| {
            let (x, y) = (&all[*a], &all[*b]);
            x.position.cmp(&y.position).then_with(|| natural_cmp(&x.name, &y.name))
        });
    }

    fn visit(
        i: usize,
        depth: i64,
        prefix: &str,
        all: &[StoryFolder],
        children: &HashMap<Option<i64>, Vec<usize>>,
        out: &mut Vec<StoryFolder>,
    ) -> i64 {
        let mut f = all[i].clone();
        f.depth = depth;
        f.path = if prefix.is_empty() { f.name.clone() } else { format!("{prefix}{PATH_LABEL_SEP}{}", f.name) };
        let kids = children.get(&Some(f.id)).cloned().unwrap_or_default();
        f.child_count = kids.len() as i64;
        let at = out.len();
        let path = f.path.clone();
        out.push(f);
        let mut total = all[i].story_count;
        for k in kids {
            total += visit(k, depth + 1, &path, all, children, out);
        }
        out[at].total_stories = total;
        total
    }

    let mut out = Vec::with_capacity(all.len());
    for root in children.get(&None).cloned().unwrap_or_default() {
        visit(root, 0, "", &all, &children, &mut out);
    }
    Ok(out)
}

fn subtree_ids(conn: &Connection, id: i64) -> AppResult<Vec<i64>> {
    let mut stmt = conn.prepare(
        "WITH RECURSIVE sub(id) AS (SELECT ?1 UNION ALL SELECT f.id FROM story_folders f JOIN sub ON f.parent_id = sub.id)
         SELECT id FROM sub",
    )?;
    let rows = stmt.query_map([id], |r| r.get(0))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn clean(s: &str, what: &str) -> AppResult<String> {
    let s = crate::input::clean(s, false).trim().to_string();
    if s.is_empty() {
        return Err(AppError::msg(format!("{what} cannot be empty")));
    }
    crate::input::fits(what, &s, crate::input::MAX_NAME)?;
    Ok(s)
}

fn clean_description(description: &str) -> AppResult<String> {
    let d = crate::input::clean(description, true).trim().to_string();
    crate::input::fits("The description", &d, crate::input::MAX_DESCRIPTION)?;
    Ok(d)
}

fn check_folder(conn: &Connection, folder: Option<i64>) -> AppResult<()> {
    if let Some(f) = folder {
        let exists: bool =
            conn.query_row("SELECT EXISTS(SELECT 1 FROM story_folders WHERE id = ?1)", [f], |r| r.get(0))?;
        if !exists {
            return Err(AppError::msg("That collection doesn't exist"));
        }
    }
    Ok(())
}

pub fn create_folder(conn: &Connection, name: &str, description: &str, parent: Option<i64>) -> AppResult<i64> {
    check_folder(conn, parent)?;
    conn.execute(
        "INSERT INTO story_folders (name, description, created_at, parent_id) VALUES (?1, ?2, ?3, ?4)",
        params![clean(name, "The name")?, clean_description(description)?, now(), parent],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn update_folder(conn: &Connection, id: i64, name: &str, description: &str, parent: Option<i64>) -> AppResult<()> {
    check_folder(conn, parent)?;
    if let Some(p) = parent {
        if subtree_ids(conn, id)?.contains(&p) {
            return Err(AppError::msg("A collection can't be moved inside itself"));
        }
    }
    conn.execute(
        "UPDATE story_folders SET name = ?2, description = ?3, parent_id = ?4 WHERE id = ?1",
        params![id, clean(name, "The name")?, clean_description(description)?, parent],
    )?;
    Ok(())
}

/// Delete a collection, its subcollections and all their stories.
pub fn delete_folder(conn: &mut Connection, id: i64) -> AppResult<()> {
    let ids = subtree_ids(conn, id)?;
    let tx = conn.transaction()?;
    for f in ids.iter().rev() {
        tx.execute("DELETE FROM stories WHERE folder_id = ?1", [f])?;
        tx.execute("DELETE FROM story_folders WHERE id = ?1", [f])?;
    }
    tx.commit()?;
    Ok(())
}

/// Stories in a collection and its subcollections (`None` = every story), in natural title order.
pub fn list_stories(conn: &Connection, folder: Option<i64>) -> AppResult<Vec<StoryInfo>> {
    let map = |r: &rusqlite::Row| -> rusqlite::Result<(StoryInfo, String)> {
        Ok((
            StoryInfo {
                id: r.get(0)?,
                folder_id: r.get(1)?,
                title: r.get(2)?,
                created_at: r.get(4)?,
                updated_at: r.get(5)?,
                position: r.get(6)?,
                chars: 0,
                preview: String::new(),
            },
            r.get(3)?,
        ))
    };
    const COLS: &str = "SELECT id, folder_id, title, body, created_at, updated_at, position FROM stories";
    let rows: Vec<(StoryInfo, String)> = match folder {
        None => conn.prepare(COLS)?.query_map([], map)?.collect::<Result<_, _>>()?,
        Some(f) => {
            let ids = subtree_ids(conn, f)?;
            let list = ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
            conn.prepare(&format!("{COLS} WHERE folder_id IN ({list})"))?
                .query_map([], map)?
                .collect::<Result<_, _>>()?
        }
    };
    let mut out: Vec<StoryInfo> = rows
        .into_iter()
        .map(|(mut s, body)| {
            s.chars = body.chars().filter(|c| pinyin::is_hanzi(*c)).count() as i64;
            s.preview = preview(&body);
            s
        })
        .collect();
    out.sort_by(|a, b| a.position.cmp(&b.position).then_with(|| natural_cmp(&a.title, &b.title)));
    Ok(out)
}

fn preview(body: &str) -> String {
    let flat: String = body.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut p: String = flat.chars().take(PREVIEW_CHARS).collect();
    if flat.chars().count() > PREVIEW_CHARS {
        p.push('…');
    }
    p
}

pub fn get_story(conn: &Connection, id: i64) -> AppResult<Story> {
    conn.query_row(
        "SELECT id, folder_id, title, body, created_at, updated_at, vocab, position FROM stories WHERE id = ?1",
        [id],
        |r| {
            Ok(Story {
                id: r.get(0)?,
                folder_id: r.get(1)?,
                title: r.get(2)?,
                body: r.get(3)?,
                created_at: r.get(4)?,
                updated_at: r.get(5)?,
                vocab: r.get(6)?,
                position: r.get(7)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| AppError::msg("That story doesn't exist"))
}

/// Line endings made "\n", and no blank lines at the ends.
fn clean_body(body: &str) -> AppResult<String> {
    let body = body.trim_start_matches('\u{feff}').replace("\r\n", "\n").replace('\r', "\n");
    let body = crate::input::clean(&body, true);
    let body = body.trim_matches(|c: char| c == '\n' || c.is_whitespace() && c != '\u{3000}');
    if body.is_empty() {
        return Err(AppError::msg("The story's text cannot be empty"));
    }
    crate::input::fits("The story's text", body, crate::input::MAX_STORY)?;
    Ok(body.to_string())
}

pub fn create_story(conn: &Connection, folder: Option<i64>, title: &str, body: &str) -> AppResult<i64> {
    insert_story(conn, folder, &Parsed { title: title.to_string(), body: body.to_string(), vocab: String::new(), position: 0 })
}

fn insert_story(conn: &Connection, folder: Option<i64>, s: &Parsed) -> AppResult<i64> {
    check_folder(conn, folder)?;
    let t = now();
    conn.execute(
        "INSERT INTO stories (folder_id, title, body, vocab, position, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
        params![
            folder,
            clean(&s.title, "The title")?,
            clean_body(&s.body)?,
            crate::input::clip(crate::input::clean(&s.vocab, true).trim(), crate::input::MAX_NOTES),
            s.position,
            t
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn update_story(conn: &Connection, id: i64, folder: Option<i64>, title: &str, body: &str) -> AppResult<()> {
    check_folder(conn, folder)?;
    conn.execute(
        "UPDATE stories SET folder_id = ?2, title = ?3, body = ?4, updated_at = ?5 WHERE id = ?1",
        params![id, folder, clean(title, "The title")?, clean_body(body)?, now()],
    )?;
    Ok(())
}

pub fn delete_story(conn: &Connection, id: i64) -> AppResult<()> {
    conn.execute("DELETE FROM stories WHERE id = ?1", [id])?;
    Ok(())
}

/// A story read from a file, ready to add.
#[derive(Debug, Clone)]
pub struct Parsed {
    pub title: String,
    pub body: String,
    pub vocab: String,
    pub position: i64,
}

/// What importing a folder added.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub collections: usize,
    pub stories: usize,
    /// Already in that collection (same title), or empty.
    pub skipped: usize,
}

/// The number a name starts with ("01_Story" -> 1) and the rest ("Story").
fn split_number(name: &str) -> (Option<i64>, &str) {
    let digits = name.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 {
        return (None, name);
    }
    let rest = name[digits..].trim_start_matches(['_', '.', ' ', '-']);
    (name[..digits].parse().ok(), if rest.is_empty() { name } else { rest })
}

/// Learner levels, easiest first, for ordering collections named after them.
const LEVELS: &[&str] =
    &["newbie", "beginner", "elementary", "pre-intermediate", "intermediate", "upper-intermediate", "advanced", "native"];

fn decode(bytes: Vec<u8>, path: &Path) -> AppResult<String> {
    Ok(match bytes.as_slice() {
        [0xff, 0xfe, rest @ ..] => String::from_utf16_lossy(
            &rest.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>(),
        ),
        [0xfe, 0xff, rest @ ..] => String::from_utf16_lossy(
            &rest.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect::<Vec<_>>(),
        ),
        _ => String::from_utf8(bytes).map_err(|_| AppError::msg(format!("{} isn't UTF-8 text", path.display())))?,
    })
}

/// A section header line: "--- Story ---".
fn header(line: &str) -> Option<&str> {
    let l = line.trim();
    l.strip_prefix("---")?.strip_suffix("---").map(str::trim).filter(|h| !h.is_empty())
}

/// A story file. Plain text is all story, titled by the file name (without a leading
/// number, which orders it instead). Files with sections use the first line as the title:
///
/// ```text
/// Title
/// --- Key Vocab ---
/// 减法 – jiǎn fǎ – subtraction
/// --- Story ---
/// 今天上数学课……
/// ```
pub fn parse_story(stem: &str, text: &str) -> Parsed {
    let text = text.trim_start_matches('\u{feff}').replace("\r\n", "\n").replace('\r', "\n");
    let (number, rest) = split_number(stem.trim());
    let position = number.unwrap_or(0);
    let lines: Vec<&str> = text.lines().collect();
    if !lines.iter().any(|l| header(l).is_some_and(|h| h.eq_ignore_ascii_case("story"))) {
        return Parsed { title: rest.trim().to_string(), body: text, vocab: String::new(), position };
    }
    let mut title = String::new();
    let mut vocab = Vec::new();
    let mut body = Vec::new();
    let mut section: Option<String> = None;
    for line in &lines {
        if let Some(h) = header(line) {
            section = Some(h.to_lowercase());
            continue;
        }
        match section.as_deref() {
            None => {
                if title.is_empty() && !line.trim().is_empty() {
                    title = line.trim().to_string();
                }
            }
            Some("story") => body.push(*line),
            Some(h) if h.contains("vocab") => {
                if !line.trim().is_empty() {
                    vocab.push(line.trim());
                }
            }
            _ => {}
        }
    }
    if title.is_empty() {
        title = rest.trim().to_string();
    }
    Parsed { title, body: body.join("\n"), vocab: vocab.join("\n"), position }
}

/// Read a story file. UTF-8, or UTF-16 with a byte-order mark.
pub fn read_file(path: &Path) -> AppResult<Parsed> {
    let text = decode(crate::input::read_file(path, crate::input::MAX_STORY_FILE)?, path)?;
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    Ok(parse_story(&stem, &text))
}

/// Add stories to a collection, skipping empty ones and titles already there; returns
/// (added, skipped).
fn add_stories(conn: &Connection, folder: Option<i64>, files: &[Parsed]) -> AppResult<(usize, usize)> {
    let (mut added, mut skipped) = (0, 0);
    for f in files {
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM stories WHERE folder_id IS ?1 AND title = ?2)",
            params![folder, f.title.trim()],
            |r| r.get(0),
        )?;
        if exists || clean_body(&f.body).is_err() || clean(&f.title, "The title").is_err() {
            skipped += 1;
            continue;
        }
        insert_story(conn, folder, f)?;
        added += 1;
    }
    Ok((added, skipped))
}

/// Add text files as stories; returns how many were added.
pub fn import_files(conn: &mut Connection, folder: Option<i64>, files: &[Parsed]) -> AppResult<usize> {
    let tx = conn.transaction()?;
    let (added, _) = add_stories(&tx, folder, files)?;
    tx.commit()?;
    Ok(added)
}

/// A folder of story files, read before touching the database.
#[derive(Debug)]
pub struct FolderTree {
    pub name: String,
    pub position: i64,
    pub stories: Vec<Parsed>,
    pub children: Vec<FolderTree>,
    /// Files that couldn't be read as stories (not text, or too large).
    pub unreadable: usize,
}

/// Folders inside folders that are followed when importing (a link back up the tree would
/// otherwise never end), and files taken from one folder.
const MAX_FOLDER_DEPTH: usize = 12;
const MAX_FOLDER_FILES: usize = 5_000;

/// Read a folder: its `.txt` files are stories, its subfolders collections inside it.
pub fn read_folder(path: &Path) -> AppResult<FolderTree> {
    read_folder_at(path, 0)
}

fn read_folder_at(path: &Path, depth: usize) -> AppResult<FolderTree> {
    let raw = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Stories".into());
    let (number, rest) = split_number(&raw);
    let level = LEVELS.iter().position(|l| l.eq_ignore_ascii_case(rest.trim()));
    let mut name = rest.trim().to_string();
    if let Some(c) = name.chars().next() {
        // "newbie" -> "Newbie"
        name = c.to_uppercase().collect::<String>() + &name[c.len_utf8()..];
    }
    let mut tree = FolderTree {
        name,
        position: number.or(level.map(|l| l as i64 + 1)).unwrap_or(0),
        stories: Vec::new(),
        children: Vec::new(),
        unreadable: 0,
    };
    let mut entries: Vec<PathBuf> = std::fs::read_dir(path)?.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        // links aren't followed: only what is really inside the chosen folder
        let Ok(meta) = std::fs::symlink_metadata(&p) else { continue };
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            if depth < MAX_FOLDER_DEPTH {
                tree.children.push(read_folder_at(&p, depth + 1)?);
            }
        } else if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("txt")) && tree.stories.len() < MAX_FOLDER_FILES {
            // one file that isn't text doesn't stop the rest of the folder
            match read_file(&p) {
                Ok(story) => tree.stories.push(story),
                Err(_) => tree.unreadable += 1,
            }
        }
    }
    Ok(tree)
}

// ---------- export: stories as files to keep or to share ----------
//
// The files are the ones the import reads (see docs/import-formats.md), so what one learner
// exports another can import, and exporting then importing gives the same stories back.

/// What an export wrote.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSummary {
    pub stories: usize,
    pub collections: usize,
    /// The file or folder that was written.
    pub path: String,
}

/// A story as a text file: its title, its key vocabulary if it has any, then the story.
pub fn story_text(s: &Story) -> String {
    let mut out = format!("{}\n", s.title.trim());
    if !s.vocab.trim().is_empty() {
        out.push_str("\n--- Key Vocab ---\n");
        out.push_str(s.vocab.trim());
        out.push('\n');
    }
    out.push_str("\n--- Story ---\n");
    out.push_str(&s.body);
    out.push('\n');
    out
}

/// A name that is a valid file or folder name on Windows, macOS and Linux.
fn safe_name(name: &str) -> String {
    let cleaned: String =
        name.chars().map(|c| if c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') { '_' } else { c }).collect();
    let cleaned: String = cleaned.trim().trim_end_matches('.').chars().take(80).collect();
    let cleaned = cleaned.trim().to_string();
    // names Windows keeps for devices
    const RESERVED: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2",
        "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if cleaned.is_empty() {
        "untitled".into()
    } else if RESERVED.iter().any(|r| cleaned.eq_ignore_ascii_case(r)) {
        format!("_{cleaned}")
    } else {
        cleaned
    }
}

/// The name a story or collection is exported under. Its place in the order goes in front
/// ("03_Name"), which is where the import reads it from; a name that itself starts with a
/// number gets "00_" so that number isn't taken for the order.
fn export_name(position: i64, name: &str) -> String {
    let safe = safe_name(name);
    if position > 0 {
        format!("{position:02}_{safe}")
    } else if safe.starts_with(|c: char| c.is_ascii_digit()) {
        format!("00_{safe}")
    } else {
        safe
    }
}

/// `dir/name.ext`, or `dir/name (2).ext` … when that is taken.
fn free_path(dir: &Path, name: &str, ext: &str) -> PathBuf {
    let with = |n: usize| {
        let stem = if n == 1 { name.to_string() } else { format!("{name} ({n})") };
        dir.join(if ext.is_empty() { stem } else { format!("{stem}.{ext}") })
    };
    (1..).map(with).find(|p| !p.exists()).expect("some name is free")
}

/// Write one story to `path` (a file the learner chose).
pub fn export_story(conn: &Connection, id: i64, path: &Path) -> AppResult<ExportSummary> {
    let story = get_story(conn, id)?;
    std::fs::write(path, story_text(&story))?;
    Ok(ExportSummary { stories: 1, collections: 0, path: path.display().to_string() })
}

/// Write a collection as a new folder inside `dir`: its stories as text files, its
/// subcollections as folders inside it. `None` = everything: a folder "Hanzi Cards stories"
/// holding the top collections and the stories that are in none.
pub fn export_folder(conn: &Connection, folder: Option<i64>, dir: &Path) -> AppResult<ExportSummary> {
    if !dir.is_dir() {
        return Err(AppError::msg("Choose a folder to export into."));
    }
    let folders = list_folders(conn)?;
    let mut sum = ExportSummary::default();
    let target = match folder {
        Some(id) => {
            let f = folders.iter().find(|f| f.id == id).ok_or_else(|| AppError::msg("That collection doesn't exist"))?;
            free_path(dir, &export_name(f.position, &f.name), "")
        }
        None => free_path(dir, "Hanzi Cards stories", ""),
    };

    fn write(conn: &Connection, folders: &[StoryFolder], id: Option<i64>, target: &Path, sum: &mut ExportSummary) -> AppResult<()> {
        std::fs::create_dir_all(target)?;
        sum.collections += 1;
        // the stories directly in this collection, in their order
        let mut stmt = conn.prepare("SELECT id FROM stories WHERE folder_id IS ?1 ORDER BY position, title, id")?;
        let ids: Vec<i64> = stmt.query_map([id], |r| r.get(0))?.collect::<Result<_, _>>()?;
        for story in ids {
            let s = get_story(conn, story)?;
            std::fs::write(free_path(target, &export_name(s.position, &s.title), "txt"), story_text(&s))?;
            sum.stories += 1;
        }
        for child in folders.iter().filter(|f| f.parent_id == id) {
            write(conn, folders, Some(child.id), &free_path(target, &export_name(child.position, &child.name), ""), sum)?;
        }
        Ok(())
    }
    write(conn, &folders, folder, &target, &mut sum)?;
    sum.path = target.display().to_string();
    Ok(sum)
}

/// The collection called `name` in `parent`, made if there isn't one; true when made.
fn ensure_folder(conn: &Connection, parent: Option<i64>, name: &str, position: i64) -> AppResult<(i64, bool)> {
    let found: Option<i64> = conn
        .query_row(
            "SELECT id FROM story_folders WHERE parent_id IS ?1 AND name = ?2 COLLATE NOCASE ORDER BY id LIMIT 1",
            params![parent, name.trim()],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(id) = found {
        return Ok((id, false));
    }
    let id = create_folder(conn, name, "", parent)?;
    conn.execute("UPDATE story_folders SET position = ?2 WHERE id = ?1", params![id, position])?;
    Ok((id, true))
}

/// Add a folder as a collection inside `parent` (`None` = top level), with its subfolders
/// as subcollections. Importing the same folder again adds only what's new.
pub fn import_folder(conn: &mut Connection, parent: Option<i64>, tree: &FolderTree) -> AppResult<ImportSummary> {
    fn walk(conn: &Connection, parent: Option<i64>, t: &FolderTree, sum: &mut ImportSummary) -> AppResult<()> {
        let (id, made) = ensure_folder(conn, parent, &t.name, t.position)?;
        if made {
            sum.collections += 1;
        }
        let (added, skipped) = add_stories(conn, Some(id), &t.stories)?;
        sum.stories += added;
        sum.skipped += skipped + t.unreadable;
        for c in &t.children {
            walk(conn, Some(id), c, sum)?;
        }
        Ok(())
    }
    check_folder(conn, parent)?;
    let tx = conn.transaction()?;
    let mut sum = ImportSummary::default();
    walk(&tx, parent, tree, &mut sum)?;
    tx.commit()?;
    Ok(sum)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_story_files() {
        let p = parse_story(
            "01_小明的减法课",
            "\u{feff}小明的减法课\r\n\r\n--- Key Vocab ---\r\n减法 – jiǎn fǎ – subtraction\r\n\r\n--- Story ---\r\n今天上数学课。\r\n老师笑了。\r\n",
        );
        assert_eq!(p.title, "小明的减法课");
        assert_eq!(p.position, 1);
        assert_eq!(p.vocab, "减法 – jiǎn fǎ – subtraction");
        assert_eq!(p.body.trim(), "今天上数学课。\n老师笑了。");
        let plain = parse_story("12 The Fox", "狐狸");
        assert_eq!((plain.title.as_str(), plain.position, plain.body.as_str()), ("The Fox", 12, "狐狸"));
        assert_eq!(parse_story("Notes", "x").position, 0);
        assert_eq!(split_number("2024"), (Some(2024), "2024"));
    }

    #[test]
    fn imports_folders() {
        let root = std::env::temp_dir().join(format!("hc-stories-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (dir, file, text) in [
            ("advanced", "02_B.txt", "B title\n--- Story ---\n第二"),
            ("advanced", "01_A.txt", "A title\n--- Story ---\n第一"),
            ("newbie", "01_C.txt", "C\n--- Story ---\n第三"),
            ("newbie", "notes.md", "not a story"),
        ] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
            std::fs::write(root.join(dir).join(file), text).unwrap();
        }
        let mut conn = open_in_memory().unwrap();
        let tree = read_folder(&root).unwrap();
        let sum = import_folder(&mut conn, None, &tree).unwrap();
        assert_eq!((sum.collections, sum.stories, sum.skipped), (3, 3, 0));
        let paths: Vec<String> = list_folders(&conn).unwrap().into_iter().map(|f| f.path).collect();
        let top = paths[0].clone();
        assert_eq!(paths[1..], [format!("{top} › Newbie"), format!("{top} › Advanced")], "levels in order");
        let adv = list_folders(&conn).unwrap().into_iter().find(|f| f.name == "Advanced").unwrap();
        let titles: Vec<String> = list_stories(&conn, Some(adv.id)).unwrap().into_iter().map(|s| s.title).collect();
        assert_eq!(titles, ["A title", "B title"], "by file number");
        // again: nothing new
        let again = import_folder(&mut conn, None, &read_folder(&root).unwrap()).unwrap();
        assert_eq!((again.collections, again.stories, again.skipped), (0, 0, 3));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn exported_stories_import_as_they_were() {
        let root = std::env::temp_dir().join(format!("hc-export-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let conn = open_in_memory().unwrap();
        // collections in an order of their own, a story with vocabulary, awkward names
        let graded = create_folder(&conn, "Graded: readers?", "", None).unwrap();
        let level = create_folder(&conn, "2 stars", "", Some(graded)).unwrap();
        conn.execute("UPDATE story_folders SET position = 3 WHERE id = ?1", [level]).unwrap();
        let fox = insert_story(
            &conn,
            Some(level),
            &Parsed { title: "狐狸和葡萄".into(), body: "狐狸饿了。\n\n葡萄很高。".into(), vocab: "狐狸 – hú li – fox\n葡萄 – pú tao – grape".into(), position: 2 },
        )
        .unwrap();
        insert_story(&conn, Some(level), &Parsed { title: "100 个苹果".into(), body: "苹果很多。".into(), vocab: String::new(), position: 0 }).unwrap();
        create_story(&conn, Some(graded), "CON", "一个名字。").unwrap();
        create_story(&conn, None, "Loose / story", "在外面。").unwrap();

        // one story to a file
        let file = root.join("fox.txt");
        assert_eq!(export_story(&conn, fox, &file).unwrap().stories, 1);
        let text = std::fs::read_to_string(&file).unwrap();
        assert_eq!(text, "狐狸和葡萄\n\n--- Key Vocab ---\n狐狸 – hú li – fox\n葡萄 – pú tao – grape\n\n--- Story ---\n狐狸饿了。\n\n葡萄很高。\n");
        let back = parse_story("fox", &text);
        assert_eq!((back.title.as_str(), back.body.trim(), back.vocab.lines().count()), ("狐狸和葡萄", "狐狸饿了。\n\n葡萄很高。", 2));

        // a collection to a folder, and everything to a folder
        let sum = export_folder(&conn, Some(graded), &root).unwrap();
        assert_eq!((sum.stories, sum.collections), (3, 2));
        assert!(std::path::Path::new(&sum.path).join("03_2 stars").join("02_狐狸和葡萄.txt").is_file(), "{}", sum.path);
        assert!(std::path::Path::new(&sum.path).join("03_2 stars").join("00_100 个苹果.txt").is_file());
        assert!(std::path::Path::new(&sum.path).join("_CON.txt").is_file(), "a name Windows reserves");
        let everything = export_folder(&conn, None, &root).unwrap();
        assert_eq!((everything.stories, everything.collections), (4, 3));
        assert!(export_folder(&conn, Some(graded), &root).unwrap().path.ends_with("(2)"), "never into an existing folder");
        assert!(export_folder(&conn, None, &file).is_err(), "a folder is needed");

        // another learner imports the collection: the same stories, order and vocabulary
        let mut theirs = open_in_memory().unwrap();
        let got = import_folder(&mut theirs, None, &read_folder(std::path::Path::new(&sum.path)).unwrap()).unwrap();
        assert_eq!((got.stories, got.collections, got.skipped), (3, 2, 0));
        let folders = list_folders(&theirs).unwrap();
        let stars = folders.iter().find(|f| f.name == "2 stars").unwrap();
        assert_eq!(stars.position, 3);
        let stories = list_stories(&theirs, Some(stars.id)).unwrap();
        assert_eq!(stories.iter().map(|s| (s.title.as_str(), s.position)).collect::<Vec<_>>(), [("100 个苹果", 0), ("狐狸和葡萄", 2)]);
        let fox_again = get_story(&theirs, stories[1].id).unwrap();
        let fox_mine = get_story(&conn, fox).unwrap();
        assert_eq!((fox_again.title, fox_again.body, fox_again.vocab), (fox_mine.title, fox_mine.body, fox_mine.vocab));
        assert!(list_stories(&theirs, None).unwrap().iter().any(|s| s.title == "CON"), "the title is the one in the file");
        // and the single file imports too
        assert_eq!(import_files(&mut theirs, None, &[read_file(&file).unwrap()]).unwrap(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn collections_and_stories() {
        let mut conn = open_in_memory().unwrap();
        let graded = create_folder(&conn, "Graded", "", None).unwrap();
        let l10 = create_folder(&conn, "Level 10", "", Some(graded)).unwrap();
        let l2 = create_folder(&conn, "Level 2", "", Some(graded)).unwrap();
        create_story(&conn, Some(l2), "Story 10", "\u{feff}\r\n第一段。\r\n\r\n第二段。\r\n").unwrap();
        create_story(&conn, Some(l2), "Story 9", "你好").unwrap();
        create_story(&conn, Some(l10), "Other", "再见").unwrap();
        create_story(&conn, None, "Loose", "谢谢").unwrap();
        assert!(create_story(&conn, None, "Empty", " \n ").is_err());

        let folders = list_folders(&conn).unwrap();
        let order: Vec<&str> = folders.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(order, ["Graded", "Graded › Level 2", "Graded › Level 10"]);
        assert_eq!(folders[0].total_stories, 3);
        assert_eq!(folders[0].child_count, 2);

        let in_graded = list_stories(&conn, Some(graded)).unwrap();
        let titles: Vec<&str> = in_graded.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["Other", "Story 9", "Story 10"]);
        let s10 = in_graded.iter().find(|s| s.title == "Story 10").unwrap();
        assert_eq!(s10.chars, 6);
        assert_eq!(get_story(&conn, s10.id).unwrap().body, "第一段。\n\n第二段。");
        assert_eq!(list_stories(&conn, None).unwrap().len(), 4);
        assert_eq!(total(&conn).unwrap(), 4);

        assert!(update_folder(&conn, graded, "Graded", "", Some(l2)).is_err(), "no cycles");
        delete_folder(&mut conn, graded).unwrap();
        assert!(list_folders(&conn).unwrap().is_empty());
        let left: Vec<String> = list_stories(&conn, None).unwrap().into_iter().map(|s| s.title).collect();
        assert_eq!(left, ["Loose"]);
    }
}
