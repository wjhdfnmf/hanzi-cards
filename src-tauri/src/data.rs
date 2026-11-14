//! The app's two database files, cards & decks (`hanzi-cards.db`) and reading
//! (`stories.db`): where they are, what's in them, backups, and switching to another file (a
//! backup, or a database copied from another computer). Shown in Settings › Your data.
//!
//! Backups go in `backups/decks/` and `backups/reading/` inside the app-data folder: the ones
//! the learner makes in the folder itself, the ones the app makes by itself in `auto/` inside
//! it (see `Made`), so the two never mix and only the app's own are ever cleaned up.

use crate::error::{AppError, AppResult};
use crate::{db, stories};
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use std::path::{Path, PathBuf};

pub const DB_FILE: &str = "hanzi-cards.db";

/// Which database.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Cards,
    Reading,
}

impl Kind {
    /// "cards" (also when not given) or "reading".
    pub fn parse(s: Option<&str>) -> AppResult<Kind> {
        match s.unwrap_or("cards") {
            "cards" => Ok(Kind::Cards),
            "reading" => Ok(Kind::Reading),
            other => Err(AppError::msg(format!("Unknown database: {other}"))),
        }
    }

    pub fn file(self) -> &'static str {
        match self {
            Kind::Cards => DB_FILE,
            Kind::Reading => stories::DB_FILE,
        }
    }

    fn backup_prefix(self) -> String {
        format!("{}.backup-", self.file())
    }

    fn backup_folder(self) -> &'static str {
        match self {
            Kind::Cards => "decks",
            Kind::Reading => "reading",
        }
    }

    fn open(self, path: &Path) -> rusqlite::Result<Connection> {
        match self {
            Kind::Cards => db::open(path),
            Kind::Reading => stories::open(path),
        }
    }
}

/// Who made a backup, which decides where it goes and how long it stays.
///
/// The learner's own backups ("Back up now") go in `backups/<decks|reading>/` and are never
/// deleted by the app. The app's own go in `auto/` inside that folder and are cleaned up
/// each time one is made, so they can't pile up:
///
/// - `Daily`: once on each day the app is used. Kept: the newest `KEEP_DAILY` (one for each
///   of the last days of use), then the newest one of each of the `KEEP_WEEKLY` weeks
///   before those. Anything older is deleted.
/// - `Version`: before a new version first changes anything. The newest `KEEP_VERSION`.
/// - `Switch`: the database that was replaced when switching to another. The newest
///   `KEEP_SWITCH`.
///
/// At most `KEEP_DAILY + KEEP_WEEKLY + KEEP_VERSION + KEEP_SWITCH` automatic backups per
/// database, a megabyte or two each for the cards (the dictionary isn't in them).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Made<'a> {
    ByHand,
    Daily,
    /// The version about to run: "0.3.0".
    Version(&'a str),
    Switch,
    /// Before data from another device is merged in.
    Sync,
    /// Before the statistics are reset.
    Reset,
}

pub const KEEP_DAILY: usize = 7;
pub const KEEP_WEEKLY: usize = 4;
pub const KEEP_VERSION: usize = 3;
pub const KEEP_SWITCH: usize = 3;
pub const KEEP_SYNC: usize = 3;
pub const KEEP_RESET: usize = 3;

impl Made<'_> {
    /// What the file name ends with (nothing for the learner's own).
    fn label(self) -> String {
        match self {
            Made::ByHand => String::new(),
            Made::Daily => "daily".into(),
            Made::Version(v) => format!("before-{v}"),
            Made::Switch => "before-switch".into(),
            Made::Sync => "before-sync".into(),
            Made::Reset => "before-reset".into(),
        }
    }

    /// "daily" | "version" | "switch" for an automatic backup, from its file name.
    fn sort_of(name: &str) -> Option<&'static str> {
        // (a "-2" may follow when two were made in the same second)
        let ends = |label: &str| name.ends_with(label) || name.rsplit_once('-').is_some_and(|(head, n)| head.ends_with(label) && n.parse::<u32>().is_ok());
        if ends("-daily") {
            Some("daily")
        } else if ends("-before-switch") {
            Some("switch")
        } else if ends("-before-sync") {
            Some("sync")
        } else if ends("-before-reset") {
            Some("reset")
        } else if name.split_once("-before-").is_some_and(|(_, v)| v.starts_with(|c: char| c.is_ascii_digit())) {
            Some("version")
        } else {
            None
        }
    }
}

pub fn db_path(app_data: &Path, kind: Kind) -> PathBuf {
    app_data.join(kind.file())
}

pub fn backup_dir(app_data: &Path, kind: Kind) -> PathBuf {
    app_data.join("backups").join(kind.backup_folder())
}

/// Where the app's own backups of a database go (see `Made`).
pub fn auto_backup_dir(app_data: &Path, kind: Kind) -> PathBuf {
    backup_dir(app_data, kind).join("auto")
}

/// What a database file holds (the fields for the other kind are 0).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Contents {
    pub cards: i64,
    pub decks: i64,
    pub reviews: i64,
    /// Unix seconds of the latest review, if any.
    pub last_review: Option<i64>,
    pub stories: i64,
    pub collections: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
    /// Unix seconds.
    pub modified: i64,
    /// None when the file can't be read as this kind of Hanzi Cards database.
    pub contents: Option<Contents>,
    /// For an automatic backup: "daily", "version" (made before an update) or "switch" (the
    /// database that was replaced when switching).
    pub auto: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataInfo {
    pub current: FileInfo,
    pub folder: String,
    /// This database's backups folder.
    pub backup_folder: String,
    /// The learner's own backups, newest first. The app never deletes these.
    pub backups: Vec<FileInfo>,
    /// The app's own backups (in `auto/` inside the backups folder), newest first, and how
    /// many of each sort it keeps.
    pub auto_backups: Vec<FileInfo>,
    pub auto_folder: String,
    pub keep: Keep,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Keep {
    pub daily: usize,
    pub weekly: usize,
    pub version: usize,
    pub switch: usize,
}

fn contents(conn: &Connection, kind: Kind) -> rusqlite::Result<Contents> {
    let n = |sql: &str| conn.query_row(sql, [], |r| r.get::<_, i64>(0));
    Ok(match kind {
        Kind::Cards => Contents {
            cards: n("SELECT COUNT(*) FROM cards")?,
            decks: n("SELECT COUNT(*) FROM decks")?,
            reviews: n("SELECT COUNT(*) FROM review_log")?,
            last_review: conn.query_row("SELECT MAX(reviewed_at) FROM review_log", [], |r| r.get(0))?,
            stories: 0,
            collections: 0,
        },
        Kind::Reading => Contents {
            cards: 0,
            decks: 0,
            reviews: 0,
            last_review: None,
            stories: n("SELECT COUNT(*) FROM stories")?,
            collections: n("SELECT COUNT(*) FROM story_folders")?,
        },
    })
}

/// Open another database file without changing it.
fn open_read_only(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
    conn.busy_timeout(std::time::Duration::from_secs(1))?;
    Ok(conn)
}

fn file_info(path: &Path, conn: Option<&Connection>, kind: Kind) -> FileInfo {
    let meta = std::fs::metadata(path).ok();
    // the size includes the write-ahead log, which holds recent changes until they're merged
    let wal = std::fs::metadata(format!("{}-wal", path.display())).map(|m| m.len()).unwrap_or(0);
    let modified = meta
        .as_ref()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let contents = match conn {
        Some(c) => contents(c, kind).ok(),
        None => open_read_only(path).ok().and_then(|c| contents(&c, kind).ok()),
    };
    FileInfo {
        name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        path: path.display().to_string(),
        size_bytes: meta.map(|m| m.len()).unwrap_or(0) + wal,
        modified,
        contents,
        auto: None,
    }
}

/// Backup files (not their -wal/-shm companions) in `dir` named `prefix…`.
fn backup_files(dir: &Path, prefix: &str) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    rd.filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            name.starts_with(prefix) && !name.ends_with("-wal") && !name.ends_with("-shm") && p.is_file()
        })
        .collect()
}

pub fn info(conn: &Connection, app_data: &Path, kind: Kind) -> AppResult<DataInfo> {
    let dir = backup_dir(app_data, kind);
    let auto_dir = auto_backup_dir(app_data, kind);
    let list = |dir: &Path, auto: bool| -> Vec<FileInfo> {
        let mut files: Vec<FileInfo> = backup_files(dir, &kind.backup_prefix())
            .iter()
            .map(|p| {
                let mut f = file_info(p, None, kind);
                if auto {
                    f.auto = Some(Made::sort_of(&f.name).unwrap_or("daily").to_string());
                }
                f
            })
            .collect();
        files.sort_by_key(|b| std::cmp::Reverse(b.modified));
        files
    };
    Ok(DataInfo {
        current: file_info(&db_path(app_data, kind), Some(conn), kind),
        folder: app_data.display().to_string(),
        backup_folder: dir.display().to_string(),
        backups: list(&dir, false),
        auto_backups: list(&auto_dir, true),
        auto_folder: auto_dir.display().to_string(),
        keep: Keep { daily: KEEP_DAILY, weekly: KEEP_WEEKLY, version: KEEP_VERSION, switch: KEEP_SWITCH },
    })
}

/// Delete one backup (the learner chose to): only ever a backup file of this database, in
/// its backups folder or the automatic one inside it.
pub fn delete_backup(app_data: &Path, kind: Kind, path: &Path) -> AppResult<()> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let inside = [backup_dir(app_data, kind), auto_backup_dir(app_data, kind)]
        .iter()
        .any(|dir| path.parent().is_some_and(|parent| same_file(parent, dir)));
    if !inside || !name.starts_with(&kind.backup_prefix()) || !path.is_file() {
        return Err(AppError::msg("That isn't one of this database's backups."));
    }
    std::fs::remove_file(path)?;
    for side in sidecars(path) {
        let _ = std::fs::remove_file(side);
    }
    Ok(())
}

/// Whether a database's pages are sound: SQLite's quick structural check (about 50 ms for a
/// database with the full dictionary). A damaged file can still open and answer some queries
/// with old or partial data, so this is checked before a database is used or changed.
pub fn check(conn: &Connection) -> AppResult<()> {
    match conn.query_row("PRAGMA quick_check(1)", [], |r| r.get::<_, String>(0)) {
        Ok(r) if r == "ok" => Ok(()),
        Ok(r) => Err(AppError::msg(r)),
        Err(e) => Err(AppError::msg(e.to_string())),
    }
}

/// The files SQLite may keep next to a database (its journal; -wal and -shm from versions
/// that used a write-ahead log). They belong to that database file and no other: they are
/// moved with it and never left beside a file that replaces it.
fn sidecars(db: &Path) -> [PathBuf; 3] {
    ["-journal", "-wal", "-shm"].map(|ext| PathBuf::from(format!("{}{ext}", db.display())))
}

/// Whether an error means someone else has the file open (as opposed to it being bad).
fn is_busy(e: &rusqlite::Error) -> bool {
    matches!(e.sqlite_error_code(), Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked))
}

/// Look at a database file before using it.
pub fn inspect(path: &Path, kind: Kind) -> AppResult<FileInfo> {
    let conn = open_read_only(path).map_err(|e| AppError::msg(format!("Can't open {}: {e}", path.display())))?;
    check(&conn).map_err(|e| AppError::msg(format!("{} is damaged and can't be used ({e}).", path.display())))?;
    // (a file to switch to, or a backup: not one from a newer version of the app)
    let known = match kind {
        Kind::Cards => db::SCHEMA_VERSION,
        Kind::Reading => stories::SCHEMA_VERSION,
    };
    if conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0)).is_ok_and(|v| v > known) {
        return Err(AppError::msg(match used_by(&conn, kind) {
            Some(v) => format!("{} was last used by Hanzi Cards {v}, which is newer than this version ({}). Install version {v} or later to use it.", path.display(), env!("CARGO_PKG_VERSION")),
            None => format!("{} was made by a newer version of Hanzi Cards than this one ({}). Install the latest version to use it.", path.display(), env!("CARGO_PKG_VERSION")),
        }));
    }
    let info = file_info(path, Some(&conn), kind);
    if info.contents.is_none() {
        return Err(AppError::msg(match kind {
            Kind::Cards => format!("{} isn't a Hanzi Cards database (it has no cards).", path.display()),
            Kind::Reading => format!("{} isn't a Hanzi Cards reading database (it has no stories).", path.display()),
        }));
    }
    Ok(info)
}

/// Copy the open database to a new backup file (a consistent snapshot, recent changes
/// included): the learner's own in the backups folder, the app's own in `auto/` inside it,
/// where older ones of the same sort are then cleaned up (see `Made`).
pub fn backup(conn: &Connection, app_data: &Path, kind: Kind, made: Made) -> AppResult<PathBuf> {
    let dir = if made == Made::ByHand { backup_dir(app_data, kind) } else { auto_backup_dir(app_data, kind) };
    std::fs::create_dir_all(&dir)?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let prefix = kind.backup_prefix();
    let label = made.label();
    let name = if label.is_empty() { format!("{prefix}{stamp}") } else { format!("{prefix}{stamp}-{label}") };
    let mut path = dir.join(&name);
    for n in 2.. {
        if !path.exists() {
            break;
        }
        path = dir.join(format!("{name}-{n}"));
    }
    if let Err(e) = conn.execute("VACUUM INTO ?1", [path.to_string_lossy()]) {
        // no half-written file left looking like a backup
        let _ = std::fs::remove_file(&path);
        return Err(e.into());
    }
    single_file(&path);
    // a backup that can't be read back is worse than none: it would be trusted
    if let Err(e) = open_read_only(&path).map_err(AppError::from).and_then(|c| check(&c)) {
        let _ = std::fs::remove_file(&path);
        return Err(AppError::msg(format!("The backup couldn't be verified ({e}) and was removed.")));
    }
    if made != Made::ByHand {
        clean_up(app_data, kind);
    }
    Ok(path)
}

/// Take a copy that was just written out of write-ahead mode, so it's one plain file:
/// looking inside it later (to list what a backup holds) then leaves no -wal and -shm files
/// beside it. The app switches a database back to write-ahead mode when it opens one for use.
fn single_file(copy: &Path) {
    if let Ok(c) = Connection::open(copy) {
        let _ = c.query_row("PRAGMA journal_mode=DELETE", [], |r| r.get::<_, String>(0));
    }
}

/// Backups used to sit next to the database; move them into `backups/decks/`.
fn move_old_backups(app_data: &Path) {
    let prefix = Kind::Cards.backup_prefix();
    let Ok(rd) = std::fs::read_dir(app_data) else { return };
    let old: Vec<PathBuf> = rd
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.file_name().is_some_and(|n| n.to_string_lossy().starts_with(&prefix)))
        .collect();
    if old.is_empty() {
        return;
    }
    let dir = backup_dir(app_data, Kind::Cards);
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    for p in old {
        let target = dir.join(p.file_name().unwrap());
        if !target.exists() {
            if let Err(e) = std::fs::rename(&p, &target) {
                eprintln!("moving backup {}: {e}", p.display());
            }
        }
    }
}

/// Automatic backups used to be in the backups folder with the learner's own: move them into
/// `auto/` (by name: "-daily", "-before-<version>", "-before-switch").
fn sort_out_backups(app_data: &Path, kind: Kind) {
    let auto = auto_backup_dir(app_data, kind);
    for p in backup_files(&backup_dir(app_data, kind), &kind.backup_prefix()) {
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if Made::sort_of(&name).is_none() {
            continue;
        }
        if std::fs::create_dir_all(&auto).is_err() {
            return;
        }
        let target = auto.join(&name);
        if !target.exists() {
            if let Err(e) = std::fs::rename(&p, &target) {
                eprintln!("moving backup {}: {e}", p.display());
            }
        }
    }
}

/// The day a backup was made, from its name ("…backup-20261112-220144-daily" -> 2026-11-12).
fn backup_day(prefix: &str, name: &str) -> Option<chrono::NaiveDate> {
    chrono::NaiveDate::parse_from_str(name.strip_prefix(prefix)?.get(..8)?, "%Y%m%d").ok()
}

/// Delete the automatic backups that are no longer kept (see `Made` for what is). Only files
/// in `auto/` are ever looked at; the learner's own backups are never touched.
fn clean_up(app_data: &Path, kind: Kind) {
    let prefix = kind.backup_prefix();
    // newest first, by the time in the name
    let mut files: Vec<(String, PathBuf)> = backup_files(&auto_backup_dir(app_data, kind), &prefix)
        .into_iter()
        .map(|p| (p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), p))
        .collect();
    files.sort_by(|a, b| b.0.cmp(&a.0));
    // an empty file is what a failed backup used to leave behind: not one to count or keep
    files.retain(|(_, p)| {
        let empty = std::fs::metadata(p).is_ok_and(|m| m.len() == 0);
        if empty {
            let _ = std::fs::remove_file(p);
        }
        !empty
    });
    let of = |sort: &'static str| files.iter().filter(move |(name, _)| Made::sort_of(name) == Some(sort));
    let mut gone: Vec<&PathBuf> = Vec::new();
    gone.extend(of("version").skip(KEEP_VERSION).map(|(_, p)| p));
    gone.extend(of("switch").skip(KEEP_SWITCH).map(|(_, p)| p));
    gone.extend(of("sync").skip(KEEP_SYNC).map(|(_, p)| p));
    gone.extend(of("reset").skip(KEEP_RESET).map(|(_, p)| p));
    // daily: the newest few, then the newest one of each of a few weeks before the week
    // those reach back into
    use chrono::Datelike;
    let week_of = |name: &str| backup_day(&prefix, name).map(|d| (d.iso_week().year(), d.iso_week().week()));
    let mut weeks: Vec<(i32, u32)> = of("daily").take(KEEP_DAILY).last().and_then(|(name, _)| week_of(name)).into_iter().collect();
    let covered = weeks.len();
    for (name, p) in of("daily").skip(KEEP_DAILY) {
        match week_of(name) {
            Some(w) if !weeks.contains(&w) && weeks.len() < covered + KEEP_WEEKLY => weeks.push(w),
            _ => gone.push(p),
        }
    }
    for p in gone {
        let _ = std::fs::remove_file(p);
    }
}

/// The version of the app that last used a database, when it says.
fn used_by(conn: &Connection, kind: Kind) -> Option<String> {
    let sql = match kind {
        Kind::Cards => db::CARDS_APP_VERSION,
        Kind::Reading => db::READING_APP_VERSION,
    };
    conn.query_row(sql, [], |r| r.get(0)).ok()
}

/// A database's file, looked at before the app opens it for use: `None` when there isn't one
/// yet. Fails when the file is damaged or someone else has it open; nothing is changed then.
fn look(app_data: &Path, kind: Kind) -> AppResult<Option<Connection>> {
    let path = db_path(app_data, kind);
    if !path.exists() {
        return Ok(None);
    }
    let raw = Connection::open(&path)?;
    raw.busy_timeout(db::OPEN_WAIT)?;
    // one from a newer version of the app is left exactly as it is: no backup, no upgrade
    let known = match kind {
        Kind::Cards => db::SCHEMA_VERSION,
        Kind::Reading => stories::SCHEMA_VERSION,
    };
    if raw.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0)).is_ok_and(|v| v > known) {
        return Err(AppError::msg(db::newer_message(used_by(&raw, kind).as_deref())));
    }
    // before anything is changed: a damaged database isn't upgraded or studied on
    match raw.query_row("PRAGMA quick_check(1)", [], |r| r.get::<_, String>(0)) {
        Ok(r) if r == "ok" => Ok(Some(raw)),
        Ok(r) => Err(damaged(kind, &r)),
        Err(e) if is_busy(&e) => Err(in_use(kind)),
        Err(e) => Err(damaged(kind, &e.to_string())),
    }
}

/// Today's automatic backup, if there isn't one yet (failing is not a reason not to start).
fn daily_backup(raw: &Connection, app_data: &Path, kind: Kind) {
    let today = format!("{}{}-", kind.backup_prefix(), chrono::Local::now().format("%Y%m%d"));
    // (the backup made before a new version's first start counts as that day's)
    let made = backup_files(&auto_backup_dir(app_data, kind), &today).iter().any(|p| {
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        matches!(Made::sort_of(&name), Some("daily" | "version"))
    });
    if made {
        return;
    }
    if let Err(e) = backup(raw, app_data, kind, Made::Daily) {
        eprintln!("daily backup of {}: {e}", kind.file());
    }
}

/// Open the cards database. The first time a new version of the app starts (an install or
/// update), both databases are backed up before anything changes them: the new version may
/// upgrade their tables. Otherwise a backup is made once a day. A fresh install has no
/// database yet and starts with the sample deck.
pub fn open_for_version(app_data: &Path, version: &str) -> AppResult<Connection> {
    move_old_backups(app_data);
    let path = db_path(app_data, Kind::Cards);
    if let Some(raw) = look(app_data, Kind::Cards)? {
        // (only now that the database is known to be sound is anything among its backups moved
        // or cleaned up)
        sort_out_backups(app_data, Kind::Cards);
        clean_up(app_data, Kind::Cards);
        let last: Option<String> = raw.query_row("SELECT value FROM settings WHERE key = 'app_version'", [], |r| r.get(0)).ok();
        // (also when this build upgrades the database's tables without being a new version:
        // a development build)
        let schema: i64 = raw.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap_or(0);
        if last.as_deref() != Some(version) || schema < db::SCHEMA_VERSION {
            // still start if this fails: not being able to open the app at all would be worse
            if let Err(e) = backup(&raw, app_data, Kind::Cards, Made::Version(version)) {
                eprintln!("backup before {version}: {e}");
            }
            // (a damaged or busy reading database is reported when it is opened, next)
            if let Ok(Some(reading)) = look(app_data, Kind::Reading) {
                sort_out_backups(app_data, Kind::Reading);
                clean_up(app_data, Kind::Reading);
                if let Err(e) = backup(&reading, app_data, Kind::Reading, Made::Version(version)) {
                    eprintln!("reading backup before {version}: {e}");
                }
            }
        } else {
            daily_backup(&raw, app_data, Kind::Cards);
        }
        // `raw` closes here: the app's own connection has the file to itself
    }
    let conn = db::open(&path).map_err(|e| if is_busy(&e) { in_use(Kind::Cards) } else { e.into() })?;
    db::record_app_version(&conn, version)?;
    Ok(conn)
}

/// Open (creating if needed) the reading database, after today's backup of it.
pub fn open_reading(app_data: &Path) -> AppResult<Connection> {
    let new = !db_path(app_data, Kind::Reading).exists();
    if let Some(raw) = look(app_data, Kind::Reading)? {
        sort_out_backups(app_data, Kind::Reading);
        clean_up(app_data, Kind::Reading);
        // nothing to keep of one that has no stories yet
        if contents(&raw, Kind::Reading).is_ok_and(|c| c.stories > 0) {
            // before this build changes its tables, whatever else was backed up today
            let schema: i64 = raw.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap_or(0);
            if schema < stories::SCHEMA_VERSION {
                if let Err(e) = backup(&raw, app_data, Kind::Reading, Made::Version(env!("CARGO_PKG_VERSION"))) {
                    eprintln!("reading backup before upgrade: {e}");
                }
            } else {
                daily_backup(&raw, app_data, Kind::Reading);
            }
        }
    }
    let conn = stories::open(&db_path(app_data, Kind::Reading)).map_err(|e| if is_busy(&e) { in_use(Kind::Reading) } else { e.into() })?;
    // a new install starts with one short story, as it starts with one small deck
    if new {
        stories::seed(&conn)?;
    }
    Ok(conn)
}

fn what(kind: Kind) -> &'static str {
    match kind {
        Kind::Cards => "cards database",
        Kind::Reading => "reading database",
    }
}

fn damaged(kind: Kind, detail: &str) -> AppError {
    AppError::Damaged(format!("The {} ({}) is damaged and can't be used. Nothing has been changed. ({detail})", what(kind), kind.file()))
}

fn in_use(kind: Kind) -> AppError {
    AppError::InUse(format!(
        "The {} ({}) is open in another program, or in a copy of this app that hasn't finished closing. Close it (restarting the computer does) and try again. Nothing has been changed.",
        what(kind),
        kind.file()
    ))
}

/// Save a copy of the open database to `dest` (chosen by the user, e.g. to move it to another
/// computer or a fresh install). An existing file there is replaced, never the database in use.
pub fn save_copy(conn: &Connection, app_data: &Path, kind: Kind, dest: &Path) -> AppResult<()> {
    let target = db_path(app_data, kind);
    if dest == target || same_file(dest, &target) {
        return Err(AppError::msg("Choose another file: that's the database in use."));
    }
    if dest.exists() {
        std::fs::remove_file(dest)?;
    }
    conn.execute("VACUUM INTO ?1", [dest.to_string_lossy()])?;
    single_file(dest);
    Ok(())
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Make `source` the app's database of this kind: back up the current one, copy `source` into
/// place and reopen it (`source` itself is left as it is). `conn` is the app's connection,
/// swapped in place. Returns the backup of the database that was replaced.
pub fn use_file(conn: &mut Connection, app_data: &Path, kind: Kind, source: &Path) -> AppResult<PathBuf> {
    let target = db_path(app_data, kind);
    if same_file(source, &target) {
        return Err(AppError::msg("That's the database already in use."));
    }
    inspect(source, kind)?;
    // a consistent copy of the source (including its unmerged recent changes), made and
    // checked before the current database is touched
    let incoming = app_data.join(format!("{}.incoming", kind.file()));
    let _ = std::fs::remove_file(&incoming);
    let copied = open_read_only(source)
        .and_then(|src| src.execute("VACUUM INTO ?1", [incoming.to_string_lossy()]))
        .map_err(AppError::from)
        .and_then(|_| check(&open_read_only(&incoming)?));
    if let Err(e) = copied {
        let _ = std::fs::remove_file(&incoming);
        return Err(AppError::msg(format!("Couldn't copy {} ({e}); nothing was changed.", source.display())));
    }

    // Keep the current database: a backup when it can be read. When it can't (it's damaged,
    // or the app started without it and `conn` is only a stand-in), its files are set aside
    // as they are, in a "damaged-<time>" folder, once it's closed.
    let open_now = conn.path().is_some_and(|p| !p.is_empty());
    let saved = if open_now { backup(conn, app_data, kind, Made::Switch).ok() } else { None };
    let aside = app_data.join(format!("damaged-{}", chrono::Local::now().format("%Y%m%d-%H%M%S")));
    let aside_name = |p: &Path| aside.join(p.file_name().unwrap_or_default());
    let old_files = || std::iter::once(target.clone()).chain(sidecars(&target));

    // close the current database, put the copy in its place, reopen
    let old = std::mem::replace(conn, Connection::open_in_memory()?);
    if let Err((old, e)) = old.close() {
        *conn = old;
        let _ = std::fs::remove_file(&incoming);
        return Err(AppError::msg(format!("Couldn't close the database in use ({e}); nothing was changed.")));
    }
    let swap = || -> std::io::Result<()> {
        if saved.is_some() {
            // the backup holds everything; leftovers of the old file must not meet the new one
            for p in sidecars(&target) {
                if p.exists() {
                    std::fs::remove_file(p)?;
                }
            }
        } else if target.exists() {
            std::fs::create_dir_all(&aside)?;
            for p in old_files().filter(|p| p.exists()) {
                std::fs::rename(&p, aside_name(&p))?;
            }
        }
        std::fs::rename(&incoming, &target)
    };
    let opened = swap().map_err(AppError::from).and_then(|_| kind.open(&target).map_err(AppError::from));
    match opened {
        Ok(c) => {
            *conn = c;
            // (a database from elsewhere may carry another device's id for its study time)
            if kind == Kind::Cards {
                let _ = crate::sync_data::new_device_id(conn);
            }
            Ok(saved.unwrap_or(aside))
        }
        Err(e) => {
            // Put the previous database back. If the new file never got into place, the old
            // one is still there untouched; otherwise the new one goes and the old returns.
            let replaced = !incoming.exists();
            let _ = std::fs::remove_file(&incoming);
            if replaced {
                for p in old_files() {
                    let _ = std::fs::remove_file(p);
                }
            }
            match &saved {
                Some(saved) if replaced => {
                    std::fs::copy(saved, &target)?;
                }
                Some(_) => {}
                None => {
                    for p in old_files() {
                        let from = aside_name(&p);
                        if from.exists() && !p.exists() {
                            let _ = std::fs::rename(from, p);
                        }
                    }
                    let _ = std::fs::remove_dir(&aside); // only if it's empty again
                }
            }
            if open_now {
                *conn = kind.open(&target)?;
            }
            Err(AppError::msg(format!("Couldn't switch databases ({e}); the previous one is back in place.")))
        }
    }
}

/// For a database that couldn't be opened at startup (`conn` is its stand-in): set its files
/// aside, untouched, in a new "damaged-<time>" folder and start an empty database. Returns
/// that folder.
pub fn start_fresh(conn: &mut Connection, app_data: &Path, kind: Kind) -> AppResult<PathBuf> {
    let target = db_path(app_data, kind);
    let aside = app_data.join(format!("damaged-{}", chrono::Local::now().format("%Y%m%d-%H%M%S")));
    std::fs::create_dir_all(&aside)?;
    for p in std::iter::once(target.clone()).chain(sidecars(&target)).filter(|p| p.exists()) {
        std::fs::rename(&p, aside.join(p.file_name().unwrap_or_default()))?;
    }
    *conn = kind.open(&target)?;
    if kind == Kind::Cards {
        db::record_app_version(conn, env!("CARGO_PKG_VERSION"))?;
    }
    Ok(aside)
}

/// Open a folder (the app-data folder) in the system's file manager.
pub fn open_folder(dir: &Path) -> AppResult<()> {
    // a phone has no file manager to open a folder in
    #[cfg(mobile)]
    let r: std::io::Result<()> = {
        let _ = dir;
        Err(std::io::Error::other("This device can't open the app's folder."))
    };
    #[cfg(target_os = "windows")]
    let r = std::process::Command::new("explorer").arg(dir).spawn();
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg(dir).spawn();
    #[cfg(all(desktop, unix, not(target_os = "macos")))]
    let r = std::process::Command::new("xdg-open").arg(dir).spawn();
    r.map(|_| ()).map_err(|e| AppError::msg(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("hanzi-data-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn names_in(dir: &Path, kind: Kind) -> Vec<String> {
        let mut v: Vec<String> =
            backup_files(dir, &kind.backup_prefix()).iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        v.sort();
        v
    }

    /// The learner's own backups of a database.
    fn names(dir: &Path, kind: Kind) -> Vec<String> {
        names_in(&backup_dir(dir, kind), kind)
    }

    /// The app's own.
    fn auto_names(dir: &Path, kind: Kind) -> Vec<String> {
        names_in(&auto_backup_dir(dir, kind), kind)
    }

    #[test]
    fn automatic_backups_are_apart_and_cleaned_up() {
        let dir = temp_dir("versions");
        let manual = backup_dir(&dir, Kind::Cards);
        let auto = auto_backup_dir(&dir, Kind::Cards);
        // an old-style backup next to the database moves into backups/decks
        std::fs::write(dir.join(format!("{DB_FILE}.backup-20200101-000000")), b"old").unwrap();
        // fresh install: the sample deck, nothing to back up
        let conn = open_for_version(&dir, "0.2.1").unwrap();
        assert!(contents(&conn, Kind::Cards).unwrap().cards > 0);
        drop(conn);
        assert_eq!(names(&dir, Kind::Cards), [format!("{DB_FILE}.backup-20200101-000000")]);
        assert!(!dir.join(format!("{DB_FILE}.backup-20200101-000000")).exists());
        std::fs::remove_file(manual.join(format!("{DB_FILE}.backup-20200101-000000"))).unwrap();
        drop(open_reading(&dir).unwrap());

        // the same version again: the day's backup, once, among the app's own (an empty
        // reading database has none)
        drop(open_for_version(&dir, "0.2.1").unwrap());
        drop(open_for_version(&dir, "0.2.1").unwrap());
        assert!(names(&dir, Kind::Cards).is_empty(), "nothing among the learner's own");
        let daily = auto_names(&dir, Kind::Cards);
        assert_eq!(daily.len(), 1, "{daily:?}");
        assert!(daily[0].ends_with("-daily"));
        assert!(inspect(&auto.join(&daily[0]), Kind::Cards).unwrap().contents.unwrap().cards > 0);
        assert!(auto_names(&dir, Kind::Reading).is_empty());

        // the learner's own backup, and what an older version left in the same folder
        let conn = db::open(&db_path(&dir, Kind::Cards)).unwrap();
        let mine = backup(&conn, &dir, Kind::Cards, Made::ByHand).unwrap();
        drop(conn);
        assert_eq!(mine.parent().unwrap(), manual);
        let mine_name = mine.file_name().unwrap().to_string_lossy().into_owned();
        for old in ["20250101-000000-daily", "20250102-000000-before-0.1.0", "20250103-000000-before-switch", "20250104-000000-before-hsk1"] {
            std::fs::copy(&mine, manual.join(format!("{DB_FILE}.backup-{old}"))).unwrap();
        }
        // and a long run of earlier days: 40 in a row up to Monday 30 March 2026
        std::fs::remove_file(auto.join(&daily[0])).unwrap();
        let last = chrono::NaiveDate::from_ymd_opt(2026, 3, 30).unwrap();
        for back in 0..40 {
            let day = last - chrono::Duration::days(back);
            std::fs::copy(&mine, auto.join(format!("{DB_FILE}.backup-{}-120000-daily", day.format("%Y%m%d")))).unwrap();
        }
        drop(open_for_version(&dir, "0.2.1").unwrap());

        // the learner's own are where they were (a name of theirs that merely has "before" in it too)
        assert_eq!(names(&dir, Kind::Cards), [format!("{DB_FILE}.backup-20250104-000000-before-hsk1"), mine_name.clone()]);
        // the app's own are all in auto/, and only these daily ones are left: today's and
        // the 6 days before it, then the newest day of each of the 4 weeks before that week
        let all = auto_names(&dir, Kind::Cards);
        let dailies: Vec<&str> = all.iter().filter(|n| n.ends_with("-daily")).map(|n| &n[format!("{DB_FILE}.backup-").len()..][..8]).collect();
        let today = chrono::Local::now().format("%Y%m%d").to_string();
        assert_eq!(
            dailies,
            // (25-30 March are a Wednesday to a Monday; the Sundays before are 22, 15, 8 and 1 March)
            ["20260301", "20260308", "20260315", "20260322", "20260325", "20260326", "20260327", "20260328", "20260329", "20260330", today.as_str()],
            "{all:?}"
        );
        assert_eq!(dailies.len(), KEEP_DAILY + KEEP_WEEKLY);
        assert!(all.iter().any(|n| n.ends_with("-before-0.1.0")) && all.iter().any(|n| n.ends_with("-before-switch")), "{all:?}");
        for name in &all {
            std::fs::remove_file(auto.join(name)).unwrap();
        }

        // an update: one backup of each database, among the app's own
        drop(open_for_version(&dir, "0.3.0").unwrap());
        let cards = auto_names(&dir, Kind::Cards);
        assert_eq!(cards.len(), 1);
        assert!(cards[0].ends_with("-before-0.3.0"));
        assert!(inspect(&auto.join(&cards[0]), Kind::Cards).unwrap().contents.unwrap().cards > 0);
        let reading = auto_names(&dir, Kind::Reading);
        assert_eq!(reading.len(), 1);
        assert!(reading[0].starts_with("stories.db.backup-"));
        // only the newest few of each automatic sort are kept
        let conn = db::open(&db_path(&dir, Kind::Cards)).unwrap();
        for _ in 0..KEEP_SWITCH + 2 {
            backup(&conn, &dir, Kind::Cards, Made::Switch).unwrap();
        }
        drop(conn);
        for v in ["0.4.0", "0.5.0", "0.6.0", "0.7.0"] {
            std::thread::sleep(std::time::Duration::from_millis(1100)); // the names carry the second
            drop(open_for_version(&dir, v).unwrap());
        }
        let n = auto_names(&dir, Kind::Cards);
        assert_eq!(n.iter().filter(|x| x.contains("-before-0.")).count(), KEEP_VERSION, "{n:?}");
        assert!(n.iter().any(|x| x.contains("-before-0.7.0")) && !n.iter().any(|x| x.contains("-before-0.3.0")), "{n:?}");
        assert_eq!(n.iter().filter(|x| x.contains("-before-switch")).count(), KEEP_SWITCH, "{n:?}");
        assert_eq!(auto_names(&dir, Kind::Reading).len(), KEEP_VERSION);
        // and through all of it the learner's own two are untouched
        assert_eq!(names(&dir, Kind::Cards).len(), 2);

        // what Settings lists, and deleting a backup (only ever a backup file of this database)
        let conn = open_for_version(&dir, "0.7.0").unwrap();
        let listed = info(&conn, &dir, Kind::Cards).unwrap();
        assert_eq!((listed.backups.len(), listed.auto_backups.len()), (2, n.len()));
        assert!(listed.backups.iter().all(|b| b.auto.is_none()));
        assert!(listed.auto_backups.iter().any(|b| b.auto.as_deref() == Some("version")));
        assert!(listed.auto_backups.iter().any(|b| b.auto.as_deref() == Some("switch")));
        assert!(delete_backup(&dir, Kind::Cards, &db_path(&dir, Kind::Cards)).is_err(), "not the database itself");
        assert!(delete_backup(&dir, Kind::Cards, &dir.join(crate::dictionary::DB_FILE)).is_err());
        assert!(delete_backup(&dir, Kind::Reading, &mine).is_err(), "not another database's backup");
        delete_backup(&dir, Kind::Cards, &mine).unwrap();
        delete_backup(&dir, Kind::Cards, &auto.join(&n[0])).unwrap();
        let listed = info(&conn, &dir, Kind::Cards).unwrap();
        assert_eq!((listed.backups.len(), listed.auto_backups.len()), (1, n.len() - 1));
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn backup_and_switch() {
        let a = temp_dir("a");
        let other = temp_dir("b");
        let mut conn = db::open(&db_path(&a, Kind::Cards)).unwrap(); // the seed deck: 1 deck, some cards
        let seeded = contents(&conn, Kind::Cards).unwrap();
        assert!(seeded.cards > 0);

        // another database with an extra deck
        let b_path = db_path(&other, Kind::Cards);
        {
            let b = db::open(&b_path).unwrap();
            b.execute("INSERT INTO decks (name, description, created_at) VALUES ('Extra', '', 0)", []).unwrap();
        }
        assert_eq!(inspect(&b_path, Kind::Cards).unwrap().contents.unwrap().decks, seeded.decks + 1);
        assert!(use_file(&mut conn, &a, Kind::Cards, &db_path(&a, Kind::Cards)).is_err(), "the file in use");

        let saved = use_file(&mut conn, &a, Kind::Cards, &b_path).unwrap();
        assert_eq!(contents(&conn, Kind::Cards).unwrap().decks, seeded.decks + 1, "now on the other database");
        assert_eq!(inspect(&saved, Kind::Cards).unwrap().contents.unwrap().decks, seeded.decks, "backup of the old one");
        let listed = info(&conn, &a, Kind::Cards).unwrap();
        assert!(listed.backups.is_empty(), "the replaced database is among the app's own backups");
        assert_eq!(listed.auto_backups.len(), 1);
        assert!(listed.auto_backups[0].name.ends_with("-before-switch"));
        assert_eq!(listed.auto_backups[0].auto.as_deref(), Some("switch"));
        // a backup is one plain file, also after it has been looked into
        let beside: Vec<String> = std::fs::read_dir(auto_backup_dir(&a, Kind::Cards))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(beside, [listed.auto_backups[0].name.clone()], "no -wal or -shm files");

        // a copy elsewhere, then into a fresh install's folder
        let copy = other.join("my-cards.db");
        save_copy(&conn, &a, Kind::Cards, &copy).unwrap();
        save_copy(&conn, &a, Kind::Cards, &copy).unwrap(); // replaces
        assert!(save_copy(&conn, &a, Kind::Cards, &db_path(&a, Kind::Cards)).is_err(), "never the database in use");
        let fresh = temp_dir("fresh");
        let mut fresh_conn = db::open(&db_path(&fresh, Kind::Cards)).unwrap();
        use_file(&mut fresh_conn, &fresh, Kind::Cards, &copy).unwrap();
        assert_eq!(contents(&fresh_conn, Kind::Cards).unwrap().decks, seeded.decks + 1, "the fresh install has the copied cards");
        drop(fresh_conn);
        let _ = std::fs::remove_dir_all(&fresh);

        // and back again from the backup
        use_file(&mut conn, &a, Kind::Cards, &saved).unwrap();
        assert_eq!(contents(&conn, Kind::Cards).unwrap().decks, seeded.decks);
        assert!(inspect(&a.join("nothing.db"), Kind::Cards).is_err());
        drop(conn);
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&other);
    }

    #[test]
    fn reading_database() {
        let a = temp_dir("ra");
        let other = temp_dir("rb");
        // (a new reading database starts with the starter story: one more than is made here)
        let mut conn = open_reading(&a).unwrap();
        stories::create_story(&conn, None, "One", "你好").unwrap();
        // a cards database isn't a reading database, and the other way round
        let cards = db_path(&other, Kind::Cards);
        drop(db::open(&cards).unwrap());
        assert!(inspect(&cards, Kind::Reading).is_err());
        assert!(inspect(&db_path(&a, Kind::Reading), Kind::Cards).is_err());

        let copy = other.join("reading.db");
        save_copy(&conn, &a, Kind::Reading, &copy).unwrap();
        stories::create_story(&conn, None, "Two", "再见").unwrap();
        assert_eq!(info(&conn, &a, Kind::Reading).unwrap().current.contents.unwrap().stories, 3);
        let saved = use_file(&mut conn, &a, Kind::Reading, &copy).unwrap();
        assert_eq!(contents(&conn, Kind::Reading).unwrap().stories, 2);
        assert!(saved.starts_with(backup_dir(&a, Kind::Reading)));
        assert_eq!(inspect(&saved, Kind::Reading).unwrap().contents.unwrap().stories, 3);
        assert_eq!(info(&conn, &a, Kind::Reading).unwrap().auto_backups.len(), 1);
        drop(conn);
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&other);
    }

    /// Overwrite everything after the first page, as a crash or a stale log can.
    fn damage(db: &Path) {
        let mut bytes = std::fs::read(db).unwrap();
        assert!(bytes.len() > 8192, "more than the first page to damage");
        for b in &mut bytes[4096..] {
            *b = 0xAB;
        }
        std::fs::write(db, bytes).unwrap();
    }

    #[test]
    fn a_damaged_database_is_refused_and_can_be_replaced() {
        let dir = temp_dir("damaged");
        let path = db_path(&dir, Kind::Cards);
        // a healthy database, a backup of it, then damage
        let conn = open_for_version(&dir, "0.1.0").unwrap();
        let cards = contents(&conn, Kind::Cards).unwrap().cards;
        let good = backup(&conn, &dir, Kind::Cards, Made::ByHand).unwrap();
        drop(conn);
        damage(&path);

        // not opened, not upgraded, and no failed backup left behind
        let before = names(&dir, Kind::Cards);
        let err = open_for_version(&dir, "0.2.0").unwrap_err().to_string();
        assert!(err.contains("damaged"), "{err}");
        assert_eq!(names(&dir, Kind::Cards), before, "no backup of a damaged file");
        assert!(inspect(&path, Kind::Cards).is_err(), "nor can it be switched to");
        let raw = Connection::open(&path).unwrap();
        assert!(backup(&raw, &dir, Kind::Cards, Made::ByHand).is_err());
        drop(raw);
        assert_eq!(names(&dir, Kind::Cards), before, "a failed backup leaves no file");

        // the app starts with a stand-in connection; a backup can then be put in place, and
        // the damaged files are set aside untouched
        let mut conn = Connection::open_in_memory().unwrap();
        let damaged_bytes = std::fs::read(&path).unwrap();
        let aside = use_file(&mut conn, &dir, Kind::Cards, &good).unwrap();
        assert!(aside.file_name().unwrap().to_string_lossy().starts_with("damaged-"), "{aside:?}");
        assert_eq!(std::fs::read(aside.join(DB_FILE)).unwrap(), damaged_bytes);
        assert_eq!(contents(&conn, Kind::Cards).unwrap().cards, cards);
        drop(conn);
        // and the next start is a normal one
        let conn = open_for_version(&dir, "0.2.0").unwrap();
        assert_eq!(contents(&conn, Kind::Cards).unwrap().cards, cards);
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A dry run of this version's first start on COPIES of real databases (never the app's
    /// own folder): everything a real start does, with the before and after printed.
    /// HANZI_DRYRUN=<folder holding copies of hanzi-cards.db and stories.db>
    ///   cargo test --lib dry_run -- --ignored --nocapture
    #[test]
    #[ignore]
    fn dry_run() {
        let Ok(dir) = std::env::var("HANZI_DRYRUN") else { return };
        let dir = PathBuf::from(dir);
        assert!(!dir.to_string_lossy().contains("com.hanzicards.app"), "copies only, not the app's folder");
        let size = |name: &str| std::fs::metadata(dir.join(name)).map(|m| m.len()).unwrap_or(0);
        let sums = |c: &Connection| -> (i64, i64, i64, i64, String) {
            let n = |sql: &str| c.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap();
            let digest: String = c
                .query_row(
                    "SELECT COALESCE(SUM(length(hanzi) + length(pinyin) + length(meaning) + length(notes) + length(tags) + deck_id), 0)
                            || '/' || (SELECT COALESCE(SUM(reps + lapses + due + state), 0) FROM progress WHERE EXISTS (SELECT 1 FROM review_log l WHERE l.card_id = progress.card_id))
                            || '/' || (SELECT COALESCE(SUM(rating + reviewed_at), 0) FROM review_log) FROM cards",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            (n("SELECT COUNT(*) FROM cards"), n("SELECT COUNT(*) FROM decks"), n("SELECT COUNT(*) FROM review_log"), n("SELECT COUNT(*) FROM progress"), digest)
        };
        let before = sums(&Connection::open(db_path(&dir, Kind::Cards)).unwrap());
        println!("before: {before:?}; hanzi-cards.db {} bytes", size(DB_FILE));

        let t = std::time::Instant::now();
        let conn = open_for_version(&dir, env!("CARGO_PKG_VERSION")).unwrap();
        println!("first start took {:?}", t.elapsed());
        let after = sums(&conn);
        println!("after:  {after:?}; hanzi-cards.db {} bytes, dictionary.db {} bytes", size(DB_FILE), size(crate::dictionary::DB_FILE));
        assert_eq!((before.0, before.1, before.2, &before.4), (after.0, after.1, after.2, &after.4), "cards, decks and reviews unchanged");
        println!("progress rows {} -> {} (copies of studied words take up their progress)", before.3, after.3);
        let pragma = |p: &str| conn.query_row(&format!("PRAGMA {p}"), [], |r| r.get::<_, rusqlite::types::Value>(0)).unwrap();
        println!("user_version {:?}, journal {:?}, locking {:?}, dict journal {:?}", pragma("main.user_version"), pragma("main.journal_mode"), pragma("main.locking_mode"), pragma("dict.journal_mode"));
        check(&conn).unwrap();
        println!("dictionary: {:?}", crate::dictionary::status(&conn).unwrap());
        println!("search 你好: {:?}", crate::dictionary::search(&conn, "你好", 3).unwrap().iter().map(|e| (&e.simplified, e.in_cards)).collect::<Vec<_>>());
        let decks = crate::decks::list_decks(&conn).unwrap();
        println!(
            "decks {}; top-level cards {}, due {}, new {}",
            decks.len(),
            decks.iter().filter(|d| d.depth == 0).map(|d| d.total_cards).sum::<i64>(),
            decks.iter().filter(|d| d.depth == 0).map(|d| d.total_due).sum::<i64>(),
            decks.iter().filter(|d| d.depth == 0).map(|d| d.total_new).sum::<i64>()
        );
        let stats = crate::stats::get_stats(&conn, &Default::default()).unwrap();
        println!("stats: cards {}, unstudied {}, learning {}, young {}, mature {}, due {}", stats.total_cards, stats.unstudied_cards, stats.learning, stats.young, stats.mature, stats.due_now);
        println!("card backups: {:?} + automatic {:?}", names(&dir, Kind::Cards), auto_names(&dir, Kind::Cards));

        let reading = open_reading(&dir).unwrap();
        println!("reading: {:?}; backups {:?}", contents(&reading, Kind::Reading).unwrap(), auto_names(&dir, Kind::Reading));
        check(&reading).unwrap();
        drop(conn);
        drop(reading);
        // a second start: nothing left to do, and quick
        let t = std::time::Instant::now();
        let conn = open_for_version(&dir, env!("CARGO_PKG_VERSION")).unwrap();
        let reading = open_reading(&dir).unwrap();
        println!("second start took {:?}", t.elapsed());
        assert_eq!(sums(&conn), after);
        drop((conn, reading));
        let mut left: Vec<String> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        left.sort();
        println!("files: {left:?}");
    }

    /// The app has its databases to itself: while it holds one, nothing else can open it, and
    /// when something else holds one the app says so instead of joining in.
    #[test]
    fn a_database_in_use_elsewhere_is_not_shared() {
        let dir = temp_dir("in-use");
        let conn = open_for_version(&dir, "0.3.0").unwrap();
        // every finished change is in the main file: no log beside it to fall out of step
        assert_eq!(conn.query_row("PRAGMA main.journal_mode", [], |r| r.get::<_, String>(0)).unwrap(), "truncate");
        assert!(!sidecars(&db_path(&dir, Kind::Cards))[1].exists(), "no write-ahead log");
        // a second opener (another copy of the app, a tool, a test) is turned away, both ways
        let other = Connection::open(db_path(&dir, Kind::Cards)).unwrap();
        other.busy_timeout(std::time::Duration::from_millis(50)).unwrap();
        assert!(other.query_row("SELECT COUNT(*) FROM cards", [], |r| r.get::<_, i64>(0)).is_err());
        drop(other);
        let again = open_for_version(&dir, "0.3.0");
        assert!(matches!(again, Err(AppError::InUse(_))), "{:?}", again.err());
        // the reading database likewise
        let reading = open_reading(&dir).unwrap();
        assert!(matches!(open_reading(&dir), Err(AppError::InUse(_))));
        // and once it is let go, it opens as usual
        drop(conn);
        drop(reading);
        drop(open_for_version(&dir, "0.3.0").unwrap());
        drop(open_reading(&dir).unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_switch_leaves_the_database_in_use() {
        let dir = temp_dir("failed-switch");
        let other = temp_dir("failed-switch-src");
        let mut conn = db::open(&db_path(&dir, Kind::Cards)).unwrap();
        let decks = contents(&conn, Kind::Cards).unwrap().decks;
        // not a database at all, and a damaged one
        let junk = other.join("junk.db");
        std::fs::write(&junk, b"not a database").unwrap();
        assert!(use_file(&mut conn, &dir, Kind::Cards, &junk).is_err());
        let broken = db_path(&other, Kind::Cards);
        drop(db::open(&broken).unwrap());
        damage(&broken);
        assert!(use_file(&mut conn, &dir, Kind::Cards, &broken).is_err());
        // still on the same database, with nothing left lying around
        assert_eq!(contents(&conn, Kind::Cards).unwrap().decks, decks);
        conn.execute("INSERT INTO decks (name, description, created_at) VALUES ('Still here', '', 0)", []).unwrap();
        assert!(!dir.join(format!("{DB_FILE}.incoming")).exists());
        assert!(names(&dir, Kind::Cards).is_empty() && auto_names(&dir, Kind::Cards).is_empty(), "no backup when nothing was switched");
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&other);
    }

    /// A new install has one story to read, written with the starter deck's words; an existing
    /// library is left as it is, and so is one the learner has emptied.
    #[test]
    fn a_new_install_starts_with_one_story() {
        let dir = std::env::temp_dir().join(format!("hanzi-data-starter-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let cards = open_for_version(&dir, "9.0.0").unwrap();
        let reading = open_reading(&dir).unwrap();
        let (title, body, vocab): (String, String, String) = reading.query_row("SELECT title, body, vocab FROM stories", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap();
        assert_eq!(title, stories::STARTER_TITLE);
        assert_eq!(vocab.lines().count(), 4);
        // every word of the starter deck is in it
        let words: Vec<String> = cards.prepare("SELECT hanzi FROM cards").unwrap().query_map([], |r| r.get(0)).unwrap().collect::<Result<_, _>>().unwrap();
        assert_eq!(words.len(), 12);
        for w in &words {
            assert!(body.contains(w.as_str()), "{w} is not in the story");
        }
        // deleted, it doesn't come back at the next start
        stories::delete_story(&reading, 1).unwrap();
        drop(reading);
        assert_eq!(open_reading(&dir).unwrap().query_row::<i64, _, _>("SELECT COUNT(*) FROM stories", [], |r| r.get(0)).unwrap(), 0);
        drop(cards);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Data from a newer version of the app: this version leaves it alone, and so does a
    /// version from before versions checked (it opens anything, but its first write fails).
    #[test]
    fn newer_data_is_not_opened_and_older_versions_stop() {
        let dir = std::env::temp_dir().join(format!("hanzi-data-newer-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        drop(open_for_version(&dir, "9.0.0").unwrap());
        drop(open_reading(&dir).unwrap());

        // what a version from before this did at every start: open, then record its name
        {
            let old = Connection::open(db_path(&dir, Kind::Cards)).unwrap();
            let e = db::set_setting(&old, "app_version", "0.3.2").unwrap_err().to_string();
            assert!(e.contains(&format!("updated by Hanzi Cards {}", env!("CARGO_PKG_VERSION"))), "{e}");
            let e = old.execute("INSERT OR REPLACE INTO settings (key, value) VALUES ('app_version', '0.3.1')", []).unwrap_err().to_string();
            assert!(e.contains("newer than this version"), "{e}");
            let recorded: String = old.query_row("SELECT value FROM settings WHERE key = 'app_version'", [], |r| r.get(0)).unwrap();
            assert_eq!(recorded, "9.0.0", "nothing was written");
            // (its other settings are not in the way)
            db::set_setting(&old, "theme", "dark").unwrap();
        }
        // this version starts as usual, any number of times
        drop(open_for_version(&dir, "9.0.0").unwrap());
        let conn = open_for_version(&dir, "9.0.1").unwrap();
        assert_eq!(db::get_setting(&conn, "app_version").unwrap().as_deref(), Some("9.0.1"));
        assert_eq!(conn.query_row::<i64, _, _>("SELECT COUNT(*) FROM sync_meta WHERE key = 'opened_by'", [], |r| r.get(0)).unwrap(), 0);
        drop(conn);

        // a database from a version after this one: refused before anything is touched
        for (kind, file) in [(Kind::Cards, DB_FILE), (Kind::Reading, stories::DB_FILE)] {
            let c = Connection::open(dir.join(file)).unwrap();
            c.execute_batch("PRAGMA user_version = 999").unwrap();
            // (as that later version would have left its name)
            match kind {
                Kind::Cards => c.execute_batch("DROP TRIGGER guard_older_insert; DROP TRIGGER guard_older_update; UPDATE settings SET value = '12.5.0' WHERE key = 'app_version'").unwrap(),
                Kind::Reading => c.execute_batch("UPDATE sync_meta SET value = '12.5.0' WHERE key = 'app_version'").unwrap(),
            }
            drop(c);
            let before = std::fs::read(dir.join(file)).unwrap();
            let backups = auto_names(&dir, kind).len();
            let e = match kind {
                Kind::Cards => open_for_version(&dir, "9.0.2").unwrap_err(),
                Kind::Reading => open_reading(&dir).unwrap_err(),
            }
            .to_string();
            assert!(e.contains("last used by Hanzi Cards 12.5.0") && e.contains("Install version 12.5.0 or later"), "{e}");
            assert_eq!(std::fs::read(dir.join(file)).unwrap(), before, "the file is exactly as it was");
            assert_eq!(auto_names(&dir, kind).len(), backups);
            assert!(inspect(&dir.join(file), kind).unwrap_err().to_string().contains("Hanzi Cards 12.5.0"));
            assert!(kind.open(&dir.join(file)).unwrap_err().to_string().contains("12.5.0"), "nor by any other way in");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
