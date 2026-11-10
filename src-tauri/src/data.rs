//! The cards database file: where it is, what's in it, backups, and switching to another
//! file (a backup, or a database copied from another computer). Shown in Settings › Your data.

use crate::db;
use crate::error::{AppError, AppResult};
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use std::path::{Path, PathBuf};

pub const DB_FILE: &str = "hanzi-cards.db";
const BACKUP_PREFIX: &str = "hanzi-cards.db.backup-";

pub fn db_path(app_data: &Path) -> PathBuf {
    app_data.join(DB_FILE)
}

/// What a database file holds.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Contents {
    pub cards: i64,
    pub decks: i64,
    pub reviews: i64,
    /// Unix seconds of the latest review, if any.
    pub last_review: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
    /// Unix seconds.
    pub modified: i64,
    /// None when the file can't be read as a Hanzi Cards database.
    pub contents: Option<Contents>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataInfo {
    pub current: FileInfo,
    pub folder: String,
    /// Backups in the data folder, newest first.
    pub backups: Vec<FileInfo>,
}

fn contents(conn: &Connection) -> rusqlite::Result<Contents> {
    let n = |sql: &str| conn.query_row(sql, [], |r| r.get::<_, i64>(0));
    Ok(Contents {
        cards: n("SELECT COUNT(*) FROM cards")?,
        decks: n("SELECT COUNT(*) FROM decks")?,
        reviews: n("SELECT COUNT(*) FROM review_log")?,
        last_review: conn.query_row("SELECT MAX(reviewed_at) FROM review_log", [], |r| r.get(0))?,
    })
}

/// Open another database file without changing it.
fn open_read_only(path: &Path) -> rusqlite::Result<Connection> {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)
}

fn file_info(path: &Path, conn: Option<&Connection>) -> FileInfo {
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
        Some(c) => contents(c).ok(),
        None => open_read_only(path).ok().and_then(|c| contents(&c).ok()),
    };
    FileInfo {
        name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        path: path.display().to_string(),
        size_bytes: meta.map(|m| m.len()).unwrap_or(0) + wal,
        modified,
        contents,
    }
}

pub fn info(conn: &Connection, app_data: &Path) -> AppResult<DataInfo> {
    let mut backups: Vec<FileInfo> = std::fs::read_dir(app_data)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            name.starts_with(BACKUP_PREFIX) && !name.ends_with("-wal") && !name.ends_with("-shm")
        })
        .map(|p| file_info(&p, None))
        .collect();
    backups.sort_by_key(|b| std::cmp::Reverse(b.modified));
    Ok(DataInfo { current: file_info(&db_path(app_data), Some(conn)), folder: app_data.display().to_string(), backups })
}

/// Look at a database file before using it.
pub fn inspect(path: &Path) -> AppResult<FileInfo> {
    let conn = open_read_only(path).map_err(|e| AppError::msg(format!("Can't open {}: {e}", path.display())))?;
    let info = file_info(path, Some(&conn));
    if info.contents.is_none() {
        return Err(AppError::msg(format!("{} isn't a Hanzi Cards database (it has no cards).", path.display())));
    }
    Ok(info)
}

/// Copy the open database to a new backup file next to it (a consistent snapshot, recent
/// changes included). `label` is added to the name ("before-switch").
pub fn backup(conn: &Connection, app_data: &Path, label: &str) -> AppResult<PathBuf> {
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let name = if label.is_empty() { format!("{BACKUP_PREFIX}{stamp}") } else { format!("{BACKUP_PREFIX}{stamp}-{label}") };
    let mut path = app_data.join(&name);
    for n in 2.. {
        if !path.exists() {
            break;
        }
        path = app_data.join(format!("{name}-{n}"));
    }
    conn.execute("VACUUM INTO ?1", [path.to_string_lossy()])?;
    Ok(path)
}

/// Automatic backups made when a new version first starts: the newest few are kept.
const KEEP_VERSION_BACKUPS: usize = 3;

/// Open the app's database. The first time a new version of the app starts (an install or
/// update), the existing database is backed up before anything changes it: the new version
/// may upgrade its tables. A fresh install has no database yet and starts with the sample
/// deck.
pub fn open_for_version(app_data: &Path, version: &str) -> AppResult<Connection> {
    let path = db_path(app_data);
    if path.exists() {
        let raw = Connection::open(&path)?;
        let last: Option<String> = raw.query_row("SELECT value FROM settings WHERE key = 'app_version'", [], |r| r.get(0)).ok();
        if last.as_deref() != Some(version) {
            match backup(&raw, app_data, &format!("before-{version}")) {
                Ok(_) => prune_version_backups(app_data),
                // still start: not being able to open the app at all would be worse
                Err(e) => eprintln!("backup before {version}: {e}"),
            }
        }
    }
    let conn = db::open(&path)?;
    db::set_setting(&conn, "app_version", version)?;
    Ok(conn)
}

/// Delete all but the newest `KEEP_VERSION_BACKUPS` automatic version backups
/// (".backup-<time>-before-<version>"; other backups are left alone).
fn prune_version_backups(app_data: &Path) {
    let Ok(dir) = std::fs::read_dir(app_data) else { return };
    let mut auto: Vec<(std::time::SystemTime, PathBuf)> = dir
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            name.starts_with(BACKUP_PREFIX)
                && name.split_once("-before-").is_some_and(|(_, v)| v.starts_with(|c: char| c.is_ascii_digit()))
                && !name.ends_with("-wal")
                && !name.ends_with("-shm")
        })
        .filter_map(|p| Some((std::fs::metadata(&p).ok()?.modified().ok()?, p)))
        .collect();
    auto.sort_by_key(|(t, _)| std::cmp::Reverse(*t));
    for (_, p) in auto.into_iter().skip(KEEP_VERSION_BACKUPS) {
        let _ = std::fs::remove_file(p);
    }
}

/// Save a copy of the open database to `dest` (chosen by the user, e.g. to move the cards to
/// another computer or a fresh install). An existing file there is replaced, never the
/// database in use.
pub fn save_copy(conn: &Connection, app_data: &Path, dest: &Path) -> AppResult<()> {
    let target = db_path(app_data);
    if dest == target || same_file(dest, &target) {
        return Err(AppError::msg("Choose another file: that's the database in use."));
    }
    if dest.exists() {
        std::fs::remove_file(dest)?;
    }
    conn.execute("VACUUM INTO ?1", [dest.to_string_lossy()])?;
    Ok(())
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Make `source` the app's database: back up the current one, copy `source` into place and
/// reopen it (`source` itself is left as it is). `conn` is the app's connection, swapped in
/// place. Returns the backup of the database that was replaced.
pub fn use_file(conn: &mut Connection, app_data: &Path, source: &Path) -> AppResult<PathBuf> {
    let target = db_path(app_data);
    if same_file(source, &target) {
        return Err(AppError::msg("That's the database already in use."));
    }
    inspect(source)?;
    let saved = backup(conn, app_data, "before-switch")?;
    // a consistent copy of the source (including its unmerged recent changes)
    let incoming = app_data.join(format!("{DB_FILE}.incoming"));
    let _ = std::fs::remove_file(&incoming);
    open_read_only(source)?.execute("VACUUM INTO ?1", [incoming.to_string_lossy()])?;

    // close the current database, put the copy in its place, reopen
    let old = std::mem::replace(conn, Connection::open_in_memory()?);
    old.close().map_err(|(_, e)| e)?;
    let swap = || -> std::io::Result<()> {
        for ext in ["-wal", "-shm"] {
            let p = PathBuf::from(format!("{}{ext}", target.display()));
            if p.exists() {
                std::fs::remove_file(p)?;
            }
        }
        std::fs::rename(&incoming, &target)
    };
    let opened = swap().map_err(AppError::from).and_then(|_| db::open(&target).map_err(AppError::from));
    match opened {
        Ok(c) => {
            *conn = c;
            Ok(saved)
        }
        Err(e) => {
            // put the previous database back
            let _ = std::fs::copy(&saved, &target);
            *conn = db::open(&target)?;
            Err(AppError::msg(format!("Couldn't switch databases ({e}); the previous one is back in place.")))
        }
    }
}

/// Show the database file in the system's file manager.
pub fn show_in_folder(path: &Path) -> AppResult<()> {
    #[cfg(target_os = "windows")]
    let r = std::process::Command::new("explorer").arg(format!("/select,{}", path.display())).spawn();
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg("-R").arg(path).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let r = std::process::Command::new("xdg-open").arg(path.parent().unwrap_or(path)).spawn();
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

    #[test]
    fn backs_up_when_a_new_version_starts() {
        let dir = temp_dir("versions");
        let names = || -> Vec<String> {
            let mut v: Vec<String> = std::fs::read_dir(&dir)
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .filter(|n| n.starts_with(BACKUP_PREFIX))
                .collect();
            v.sort();
            v
        };
        // fresh install: the sample deck, nothing to back up
        let conn = open_for_version(&dir, "0.2.1").unwrap();
        assert!(contents(&conn).unwrap().cards > 0);
        drop(conn);
        assert!(names().is_empty());
        // same version again: no backup
        drop(open_for_version(&dir, "0.2.1").unwrap());
        assert!(names().is_empty());
        // an update: one backup, holding the cards
        drop(open_for_version(&dir, "0.3.0").unwrap());
        assert_eq!(names().len(), 1);
        assert!(names()[0].ends_with("-before-0.3.0"));
        assert!(inspect(&dir.join(&names()[0])).unwrap().contents.unwrap().cards > 0);
        // only the newest few automatic backups are kept; others are left alone
        let conn = db::open(&db_path(&dir)).unwrap();
        backup(&conn, &dir, "before-switch").unwrap();
        drop(conn);
        for v in ["0.4.0", "0.5.0", "0.6.0", "0.7.0"] {
            std::thread::sleep(std::time::Duration::from_millis(20));
            drop(open_for_version(&dir, v).unwrap());
        }
        let n = names();
        assert_eq!(n.iter().filter(|x| x.contains("-before-0.")).count(), KEEP_VERSION_BACKUPS, "{n:?}");
        assert!(n.iter().any(|x| x.contains("-before-0.7.0")));
        assert!(n.iter().any(|x| x.ends_with("-before-switch")), "{n:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn backup_and_switch() {
        let a = temp_dir("a");
        let other = temp_dir("b");
        let mut conn = db::open(&db_path(&a)).unwrap(); // the seed deck: 1 deck, some cards
        let seeded = contents(&conn).unwrap();
        assert!(seeded.cards > 0);

        // another database with an extra deck
        let b_path = db_path(&other);
        {
            let b = db::open(&b_path).unwrap();
            b.execute("INSERT INTO decks (name, description, created_at) VALUES ('Extra', '', 0)", []).unwrap();
        }
        assert_eq!(inspect(&b_path).unwrap().contents.unwrap().decks, seeded.decks + 1);
        assert!(use_file(&mut conn, &a, &db_path(&a)).is_err(), "the file in use");

        let saved = use_file(&mut conn, &a, &b_path).unwrap();
        assert_eq!(contents(&conn).unwrap().decks, seeded.decks + 1, "now on the other database");
        assert_eq!(inspect(&saved).unwrap().contents.unwrap().decks, seeded.decks, "backup of the old one");
        let listed = info(&conn, &a).unwrap();
        assert_eq!(listed.backups.len(), 1);
        assert!(listed.backups[0].name.ends_with("-before-switch"));

        // a copy elsewhere, then into a fresh install's folder
        let copy = other.join("my-cards.db");
        save_copy(&conn, &a, &copy).unwrap();
        save_copy(&conn, &a, &copy).unwrap(); // replaces
        assert!(save_copy(&conn, &a, &db_path(&a)).is_err(), "never the database in use");
        let fresh = temp_dir("fresh");
        let mut fresh_conn = db::open(&db_path(&fresh)).unwrap();
        use_file(&mut fresh_conn, &fresh, &copy).unwrap();
        assert_eq!(contents(&fresh_conn).unwrap().decks, seeded.decks + 1, "the fresh install has the copied cards");
        drop(fresh_conn);
        let _ = std::fs::remove_dir_all(&fresh);

        // and back again from the backup
        use_file(&mut conn, &a, &saved).unwrap();
        assert_eq!(contents(&conn).unwrap().decks, seeded.decks);
        assert!(inspect(&a.join("nothing.db")).is_err());
        drop(conn);
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&other);
    }
}
