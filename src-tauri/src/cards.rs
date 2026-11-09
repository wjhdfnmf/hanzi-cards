//! Decks and cards: CRUD, tag handling, CSV/TSV import and export.

use crate::db::now;
use crate::error::{AppError, AppResult};
use crate::models::{Card, CardInput, CardRow, Deck, ImportResult};
use crate::pinyin;
use rusqlite::{params, Connection, Row};
use std::collections::BTreeSet;
use std::path::Path;

pub const CARD_COLS: &str =
    "c.id, c.deck_id, c.hanzi, c.pinyin, c.meaning, c.notes, c.tags, c.created_at, c.updated_at";

pub fn card_from_row(r: &Row, o: usize) -> rusqlite::Result<Card> {
    Ok(Card {
        id: r.get(o)?,
        deck_id: r.get(o + 1)?,
        hanzi: r.get(o + 2)?,
        pinyin: r.get(o + 3)?,
        meaning: r.get(o + 4)?,
        notes: r.get(o + 5)?,
        tags: split_tags(&r.get::<_, String>(o + 6)?),
        created_at: r.get(o + 7)?,
        updated_at: r.get(o + 8)?,
    })
}

pub fn split_tags(s: &str) -> Vec<String> {
    s.split(',')
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
        .map(String::from)
        .collect()
}

fn join_tags(tags: &[String]) -> String {
    let set: BTreeSet<String> = tags
        .iter()
        .flat_map(|t| t.split([',', ';']))
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect();
    set.into_iter().collect::<Vec<_>>().join(",")
}

// ---------- decks ----------

pub fn list_decks(conn: &Connection) -> AppResult<Vec<Deck>> {
    let t = now();
    let mut stmt = conn.prepare(
        "SELECT d.id, d.name, d.description, d.created_at,
            (SELECT COUNT(*) FROM cards c WHERE c.deck_id = d.id),
            (SELECT COUNT(*) FROM progress p JOIN cards c ON c.id = p.card_id
              WHERE c.deck_id = d.id AND p.due <= ?1),
            (SELECT COUNT(*) FROM cards c WHERE c.deck_id = d.id
              AND NOT EXISTS (SELECT 1 FROM progress p WHERE p.card_id = c.id))
         FROM decks d ORDER BY d.name COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([t], |r| {
        Ok(Deck {
            id: r.get(0)?,
            name: r.get(1)?,
            description: r.get(2)?,
            created_at: r.get(3)?,
            card_count: r.get(4)?,
            due_count: r.get(5)?,
            new_count: r.get(6)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn clean_name(name: &str) -> AppResult<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::msg("Deck name cannot be empty"));
    }
    Ok(name.to_string())
}

pub fn create_deck(conn: &Connection, name: &str, description: &str) -> AppResult<i64> {
    conn.execute(
        "INSERT INTO decks (name, description, created_at) VALUES (?1, ?2, ?3)",
        params![clean_name(name)?, description.trim(), now()],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn update_deck(conn: &Connection, id: i64, name: &str, description: &str) -> AppResult<()> {
    conn.execute(
        "UPDATE decks SET name = ?2, description = ?3 WHERE id = ?1",
        params![id, clean_name(name)?, description.trim()],
    )?;
    Ok(())
}

pub fn delete_deck(conn: &Connection, id: i64) -> AppResult<()> {
    conn.execute("DELETE FROM decks WHERE id = ?1", [id])?;
    Ok(())
}

// ---------- cards ----------

fn normalise(input: &CardInput) -> AppResult<(String, String, String, String, String)> {
    let hanzi = input.hanzi.trim().to_string();
    if hanzi.is_empty() {
        return Err(AppError::msg("Characters cannot be empty"));
    }
    Ok((
        hanzi,
        pinyin::numbered_to_marks(input.pinyin.trim()),
        input.meaning.trim().to_string(),
        input.notes.trim().to_string(),
        join_tags(&input.tags),
    ))
}

pub fn get_card(conn: &Connection, id: i64) -> AppResult<Card> {
    let sql = format!("SELECT {CARD_COLS} FROM cards c WHERE c.id = ?1");
    Ok(conn.query_row(&sql, [id], |r| card_from_row(r, 0))?)
}

pub fn create_card(conn: &Connection, input: &CardInput) -> AppResult<Card> {
    let (h, p, m, n, t) = normalise(input)?;
    let ts = now();
    conn.execute(
        "INSERT INTO cards (deck_id, hanzi, pinyin, meaning, notes, tags, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
        params![input.deck_id, h, p, m, n, t, ts],
    )?;
    get_card(conn, conn.last_insert_rowid())
}

pub fn update_card(conn: &Connection, id: i64, input: &CardInput) -> AppResult<Card> {
    let (h, p, m, n, t) = normalise(input)?;
    conn.execute(
        "UPDATE cards SET deck_id = ?2, hanzi = ?3, pinyin = ?4, meaning = ?5, notes = ?6,
             tags = ?7, updated_at = ?8 WHERE id = ?1",
        params![id, input.deck_id, h, p, m, n, t, now()],
    )?;
    get_card(conn, id)
}

pub fn delete_cards(conn: &mut Connection, ids: &[i64]) -> AppResult<()> {
    let tx = conn.transaction()?;
    for id in ids {
        tx.execute("DELETE FROM cards WHERE id = ?1", [id])?;
    }
    tx.commit()?;
    Ok(())
}

pub fn move_cards(conn: &mut Connection, ids: &[i64], deck_id: i64) -> AppResult<()> {
    let tx = conn.transaction()?;
    for id in ids {
        tx.execute(
            "UPDATE cards SET deck_id = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, deck_id, now()],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub fn list_cards(conn: &Connection, deck_id: Option<i64>, search: &str) -> AppResult<Vec<CardRow>> {
    let sql = format!(
        "SELECT {CARD_COLS}, MIN(p.due), COALESCE(SUM(p.reps), 0), COALESCE(SUM(p.lapses), 0),
                MAX(CASE WHEN p.state IN (1, 3) THEN 1 ELSE 0 END), COUNT(p.card_id)
         FROM cards c LEFT JOIN progress p ON p.card_id = c.id
         WHERE (?1 IS NULL OR c.deck_id = ?1)
         GROUP BY c.id
         ORDER BY c.created_at DESC, c.id DESC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([deck_id], |r| {
        let card = card_from_row(r, 0)?;
        let learning: i64 = r.get(12)?;
        let studied: i64 = r.get(13)?;
        Ok(CardRow {
            card,
            next_due: r.get(9)?,
            reps: r.get(10)?,
            lapses: r.get(11)?,
            status: if studied == 0 {
                "new"
            } else if learning == 1 {
                "learning"
            } else {
                "review"
            }
            .into(),
        })
    })?;
    let all: Vec<CardRow> = rows.collect::<Result<_, _>>()?;

    let q = search.trim().to_lowercase();
    if q.is_empty() {
        return Ok(all);
    }
    let qkey = pinyin::toneless_key(&q);
    Ok(all
        .into_iter()
        .filter(|row| {
            let c = &row.card;
            c.hanzi.contains(&q)
                || c.meaning.to_lowercase().contains(&q)
                || c.notes.to_lowercase().contains(&q)
                || c.tags.iter().any(|t| t.to_lowercase().contains(&q))
                || (!qkey.is_empty() && pinyin::toneless_key(&c.pinyin).contains(&qkey))
        })
        .collect())
}

pub fn list_tags(conn: &Connection) -> AppResult<Vec<String>> {
    let mut stmt = conn.prepare("SELECT DISTINCT tags FROM cards WHERE tags <> ''")?;
    let mut set = BTreeSet::new();
    for row in stmt.query_map([], |r| r.get::<_, String>(0))? {
        for t in split_tags(&row?) {
            set.insert(t);
        }
    }
    Ok(set.into_iter().collect())
}

/// Cards with exactly these characters, with their deck names (duplicate check).
pub fn cards_by_hanzi(conn: &Connection, hanzi: &str) -> AppResult<Vec<(Card, String)>> {
    let sql = format!(
        "SELECT {CARD_COLS}, d.name FROM cards c JOIN decks d ON d.id = c.deck_id WHERE c.hanzi = ?1"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([hanzi.trim()], |r| Ok((card_from_row(r, 0)?, r.get(9)?)))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

// ---------- import / export ----------

const HEADER_WORDS: [&str; 7] = ["hanzi", "character", "characters", "simplified", "chinese", "汉字", "word"];

/// Import cards from a CSV or TSV file. Columns: hanzi, pinyin, meaning, notes, tags.
/// Works with Pleco and Anki plain-text exports (tab separated). A header row is
/// detected and skipped. Numbered pinyin is converted to tone marks.
pub fn import_file(conn: &mut Connection, deck_id: i64, path: &Path) -> AppResult<ImportResult> {
    let bytes = std::fs::read(path)?;
    let text = String::from_utf8_lossy(&bytes);
    let text = text.trim_start_matches('\u{feff}');
    import_text(conn, deck_id, text)
}

pub fn import_text(conn: &mut Connection, deck_id: i64, text: &str) -> AppResult<ImportResult> {
    let first_line = text.lines().find(|l| !l.trim().is_empty() && !l.starts_with('#')).unwrap_or("");
    let delimiter = if first_line.contains('\t') { b'\t' } else { b',' };
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(false)
        .flexible(true)
        .comment(Some(b'#'))
        .from_reader(text.as_bytes());

    let tx = conn.transaction()?;
    let ts = now();
    let (mut imported, mut skipped) = (0i64, 0i64);
    {
        let mut stmt = tx.prepare(
            "INSERT INTO cards (deck_id, hanzi, pinyin, meaning, notes, tags, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
        )?;
        for (i, rec) in reader.records().enumerate() {
            let rec = rec?;
            let field = |n: usize| rec.get(n).unwrap_or("").trim().to_string();
            let hanzi = field(0);
            if i == 0 && HEADER_WORDS.contains(&hanzi.to_lowercase().as_str()) {
                continue;
            }
            if hanzi.is_empty() || !pinyin::has_hanzi(&hanzi) {
                skipped += 1;
                continue;
            }
            let tags: Vec<String> = vec![field(4)];
            stmt.execute(params![
                deck_id,
                hanzi,
                pinyin::numbered_to_marks(&field(1)),
                field(2),
                field(3),
                join_tags(&tags),
                ts + imported
            ])?;
            imported += 1;
        }
    }
    tx.commit()?;
    Ok(ImportResult { imported, skipped })
}

pub fn export_file(conn: &Connection, deck_id: Option<i64>, path: &Path) -> AppResult<i64> {
    let cards = list_cards(conn, deck_id, "")?;
    let mut w = csv::Writer::from_path(path)?;
    w.write_record(["hanzi", "pinyin", "meaning", "notes", "tags"])?;
    for row in cards.iter().rev() {
        let c = &row.card;
        w.write_record([&c.hanzi, &c.pinyin, &c.meaning, &c.notes, &c.tags.join(";")])?;
    }
    w.flush()?;
    Ok(cards.len() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn crud_and_import() {
        let mut conn = db::open_in_memory().unwrap();
        let decks = list_decks(&conn).unwrap();
        assert_eq!(decks.len(), 1, "seed deck");
        assert_eq!(decks[0].new_count, 12);

        let deck = create_deck(&conn, "Food", "").unwrap();
        let card = create_card(
            &conn,
            &CardInput {
                deck_id: deck,
                hanzi: " 米饭 ".into(),
                pinyin: "mi3 fan4".into(),
                meaning: "cooked rice".into(),
                notes: String::new(),
                tags: vec!["food, HSK1".into()],
            },
        )
        .unwrap();
        assert_eq!(card.hanzi, "米饭");
        assert_eq!(card.pinyin, "mǐ fàn");
        assert_eq!(card.tags, vec!["HSK1", "food"]);

        let found = list_cards(&conn, Some(deck), "mifan").unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].status, "new");

        let res = import_text(
            &mut conn,
            deck,
            "hanzi\tpinyin\tmeaning\n面条\tmian4tiao2\tnoodles\nnot chinese\tx\ty\n饺子\tjiao3 zi5\tdumplings\n",
        )
        .unwrap();
        assert_eq!(res.imported, 2);
        assert_eq!(res.skipped, 1);
        let all = list_cards(&conn, Some(deck), "").unwrap();
        assert_eq!(all.len(), 3);
        assert!(all.iter().any(|r| r.card.pinyin == "miàntiáo"));

        let tags = list_tags(&conn).unwrap();
        assert!(tags.contains(&"food".to_string()));
        delete_deck(&conn, deck).unwrap();
        assert_eq!(list_cards(&conn, Some(deck), "").unwrap().len(), 0);
    }
}
