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
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
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
    let s = s.trim();
    if s.is_empty() {
        return Err(AppError::msg(format!("{what} cannot be empty")));
    }
    Ok(s.to_string())
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
        params![clean(name, "The name")?, description.trim(), now(), parent],
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
        params![id, clean(name, "The name")?, description.trim(), parent],
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
    let body = body.trim_matches(|c: char| c == '\n' || c.is_whitespace() && c != '\u{3000}');
    clean(body, "The story's text").map(|_| body.to_string())
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
        params![folder, clean(&s.title, "The title")?, clean_body(&s.body)?, s.vocab.trim(), s.position, t],
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
    let text = decode(std::fs::read(path)?, path)?;
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
        if exists || clean_body(&f.body).is_err() || f.title.trim().is_empty() {
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
}

/// Read a folder: its `.txt` files are stories, its subfolders collections inside it.
pub fn read_folder(path: &Path) -> AppResult<FolderTree> {
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
    };
    let mut entries: Vec<PathBuf> = std::fs::read_dir(path)?.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            tree.children.push(read_folder(&p)?);
        } else if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("txt")) {
            tree.stories.push(read_file(&p)?);
        }
    }
    Ok(tree)
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
        sum.skipped += skipped;
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
