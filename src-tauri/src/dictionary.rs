//! CC-CEDICT dictionary: parsing, import, download and search.
//!
//! CC-CEDICT is published by MDBG under Creative Commons Attribution-ShareAlike
//! 4.0 (https://cc-cedict.org/wiki/). Users download it from inside the app.

use crate::db::{self, now};
use crate::error::{AppError, AppResult};
use crate::models::{DictEntry, DictStatus};
use crate::pinyin;
use flate2::read::GzDecoder;
use rusqlite::{params, Connection};
use std::io::Read;

pub const CEDICT_URL: &str =
    "https://www.mdbg.net/chinese/export/cedict/cedict_1_0_ts_utf-8_mdbg.txt.gz";

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
    db::set_setting(&tx, "dict_imported_at", &now().to_string())?;
    tx.commit()?;
    Ok(n)
}

/// Read a CC-CEDICT file from disk (plain text or .gz).
pub fn read_file(path: &std::path::Path) -> AppResult<String> {
    let bytes = std::fs::read(path)?;
    decode(bytes)
}

fn decode(bytes: Vec<u8>) -> AppResult<String> {
    if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut s = String::new();
        GzDecoder::new(&bytes[..]).read_to_string(&mut s)?;
        Ok(s)
    } else {
        String::from_utf8(bytes).map_err(|_| AppError::msg("Dictionary file is not UTF-8 text"))
    }
}

/// Download the latest CC-CEDICT release from MDBG.
pub fn download() -> AppResult<String> {
    let resp = ureq::get(CEDICT_URL)
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
    let imported_at = db::get_setting(conn, "dict_imported_at")?.and_then(|v| v.parse().ok());
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
    let q = query.trim();
    if q.is_empty() {
        return Ok(vec![]);
    }
    let limit = limit.clamp(1, 200);

    if pinyin::has_hanzi(q) {
        let sql = format!(
            "SELECT {ENTRY_COLS},
               CASE WHEN d.simplified = ?1 OR d.traditional = ?1 THEN 0
                    WHEN d.simplified LIKE ?1 || '%' OR d.traditional LIKE ?1 || '%' THEN 1
                    ELSE 2 END AS rank
             FROM dictionary d
             WHERE d.simplified LIKE '%' || ?1 || '%' OR d.traditional LIKE '%' || ?1 || '%'
             ORDER BY rank, length(d.simplified), d.id
             LIMIT ?2"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![q, limit], entry_from_row)?;
        return Ok(rows.collect::<Result<_, _>>()?);
    }

    let has_digits = q.chars().any(|c| c.is_ascii_digit());
    let pkey = if has_digits {
        pinyin::toned_key(q)
    } else {
        pinyin::toneless_key(q)
    };
    let pcol = if has_digits { "pinyin_key_tone" } else { "pinyin_key" };
    let english = q.to_lowercase();
    // Short pinyin prefixes ("a", "s") match half the dictionary; only use exact match then.
    let allow_prefix = pkey.len() >= 2;

    let sql = format!(
        "SELECT {ENTRY_COLS},
           CASE WHEN ?1 <> '' AND d.{pcol} = ?1 THEN 0
                WHEN d.definitions LIKE '%/' || ?2 || '/%' OR d.definitions LIKE '%/to ' || ?2 || '/%' THEN 1
                WHEN ?3 AND ?1 <> '' AND d.{pcol} LIKE ?1 || '%' THEN 2
                WHEN (' ' || replace(replace(replace(replace(d.definitions, '/', ' '), ',', ' '), ';', ' '), '(', ' ') || ' ')
                     LIKE '% ' || ?2 || ' %' THEN 3
                ELSE 4 END AS rank
         FROM dictionary d
         WHERE (?1 <> '' AND (d.{pcol} = ?1 OR (?3 AND d.{pcol} LIKE ?1 || '%')))
            OR d.definitions LIKE '%' || ?2 || '%'
         ORDER BY rank, length(d.simplified), length(d.definitions), d.id
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
         ORDER BY (d.simplified = ?1) DESC, d.id"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([hanzi.trim()], entry_from_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
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
    }
}
