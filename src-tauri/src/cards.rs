//! Cards: CRUD, tag handling, duplicate checks, CSV/TSV import and export.

use crate::db::now;
use crate::decks;
use crate::error::{AppError, AppResult};
use crate::models::{Card, CardInput, CardRow, DuplicateWord, ImportPreview, ImportResult};
use crate::pinyin;
use rusqlite::{params, Connection, Row};
use std::collections::{BTreeSet, HashMap};
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
    let t = crate::hsk::tags_for_new(conn, &t, &h);
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

/// Cards in a deck and its subdecks (`None` = all cards), newest first.
pub fn list_cards(conn: &Connection, deck_id: Option<i64>, search: &str) -> AppResult<Vec<CardRow>> {
    let sub = decks::subtree_sql("?1");
    let sql = format!(
        "SELECT {CARD_COLS}, MIN(p.due), COALESCE(SUM(p.reps), 0), COALESCE(SUM(p.lapses), 0),
                MAX(CASE WHEN p.state IN (1, 3) THEN 1 ELSE 0 END), COUNT(p.card_id)
         FROM cards c LEFT JOIN progress p ON p.card_id = c.id
         WHERE (?1 IS NULL OR c.deck_id IN ({sub}))
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

/// Remove these tags (any case) from every card; returns how many cards changed.
pub fn remove_tags(conn: &mut Connection, tags: &[String]) -> AppResult<usize> {
    let drop: BTreeSet<String> = tags.iter().map(|t| t.trim().to_lowercase()).filter(|t| !t.is_empty()).collect();
    if drop.is_empty() {
        return Ok(0);
    }
    let rows: Vec<(i64, String)> = {
        let mut stmt = conn.prepare("SELECT id, tags FROM cards WHERE tags <> ''")?;
        let v = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
        v
    };
    let tx = conn.transaction()?;
    let t = now();
    let mut n = 0;
    for (id, tags) in rows {
        let before = split_tags(&tags);
        let kept: Vec<String> = before.iter().filter(|x| !drop.contains(&x.to_lowercase())).cloned().collect();
        if kept.len() != before.len() {
            tx.execute("UPDATE cards SET tags = ?2, updated_at = ?3 WHERE id = ?1", params![id, kept.join(","), t])?;
            n += 1;
        }
    }
    tx.commit()?;
    Ok(n)
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

/// Cards with exactly these characters, with their deck paths (duplicate check).
pub fn cards_by_hanzi(conn: &Connection, hanzi: &str) -> AppResult<Vec<(Card, String)>> {
    let sql = format!("SELECT {CARD_COLS} FROM cards c WHERE c.hanzi = ?1 ORDER BY c.id");
    let mut stmt = conn.prepare(&sql)?;
    let cards: Vec<Card> = stmt.query_map([hanzi.trim()], |r| card_from_row(r, 0))?.collect::<Result<_, _>>()?;
    let paths = decks::paths(conn)?;
    Ok(cards
        .into_iter()
        .map(|c| {
            let label = paths.get(&c.deck_id).map(|p| decks::label(p)).unwrap_or_default();
            (c, label)
        })
        .collect())
}

// ---------- duplicates ----------
//
// The same word may live in several decks (it is studied once; see study.rs), but never
// twice in one deck. Imports skip same-deck duplicates and ask about other-deck ones.

/// Where an imported card goes: an existing deck, or one that will be created
/// (`parent`, then the names below it).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DeckRef {
    Id(i64),
    New(Option<i64>, Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Dup {
    Fresh,
    SameDeck,
    /// Already in these other decks (paths).
    OtherDecks(Vec<String>),
}

pub struct DupChecker {
    existing: HashMap<String, Vec<i64>>,
    planned: HashMap<String, Vec<DeckRef>>,
    paths: HashMap<i64, Vec<String>>,
}

impl DupChecker {
    pub fn load(conn: &Connection) -> AppResult<Self> {
        let mut existing: HashMap<String, Vec<i64>> = HashMap::new();
        let mut stmt = conn.prepare("SELECT hanzi, deck_id FROM cards")?;
        for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))? {
            let (h, d) = row?;
            existing.entry(h.trim().to_string()).or_default().push(d);
        }
        Ok(DupChecker { existing, planned: HashMap::new(), paths: decks::paths(conn)? })
    }

    fn deck_label(&self, deck: &DeckRef) -> String {
        match deck {
            DeckRef::Id(id) => decks::label(self.paths.get(id).map(Vec::as_slice).unwrap_or(&[])),
            DeckRef::New(parent, names) => {
                let mut path = parent.and_then(|p| self.paths.get(&p).cloned()).unwrap_or_default();
                path.extend(names.iter().cloned());
                decks::label(&path)
            }
        }
    }

    pub fn check(&self, deck: &DeckRef, hanzi: &str) -> Dup {
        let hanzi = hanzi.trim();
        let in_db: &[i64] = self.existing.get(hanzi).map(Vec::as_slice).unwrap_or(&[]);
        let planned: &[DeckRef] = self.planned.get(hanzi).map(Vec::as_slice).unwrap_or(&[]);
        let same = matches!(deck, DeckRef::Id(id) if in_db.contains(id)) || planned.contains(deck);
        if same {
            return Dup::SameDeck;
        }
        let mut others: Vec<String> = in_db
            .iter()
            .map(|id| self.deck_label(&DeckRef::Id(*id)))
            .chain(planned.iter().map(|d| self.deck_label(d)))
            .collect();
        others.sort();
        others.dedup();
        if others.is_empty() {
            Dup::Fresh
        } else {
            Dup::OtherDecks(others)
        }
    }

    /// Note that `hanzi` will be in `deck` (an earlier row of the same import).
    pub fn add(&mut self, deck: DeckRef, hanzi: &str) {
        self.planned.entry(hanzi.trim().to_string()).or_default().push(deck);
    }

    /// Classify every (deck, hanzi) of an import, as if all were imported.
    pub fn preview<'a>(mut self, items: impl Iterator<Item = (DeckRef, &'a str)>, invalid: i64) -> ImportPreview {
        let mut p = ImportPreview { invalid, ..Default::default() };
        let mut words: Vec<DuplicateWord> = Vec::new();
        for (deck, hanzi) in items {
            p.rows += 1;
            match self.check(&deck, hanzi) {
                Dup::Fresh => p.fresh += 1,
                Dup::SameDeck => {
                    p.same_deck += 1;
                    continue;
                }
                Dup::OtherDecks(decks) => match words.iter_mut().find(|w| w.hanzi == hanzi.trim()) {
                    Some(w) => {
                        w.decks.extend(decks);
                        w.decks.sort();
                        w.decks.dedup();
                    }
                    None => words.push(DuplicateWord { hanzi: hanzi.trim().to_string(), decks }),
                },
            }
            self.add(deck, hanzi);
        }
        p.rows += invalid;
        p.other_decks = words;
        p
    }
}

/// Resolve where a card goes, creating the deck if needed. Returns (deck id, decks created).
pub fn resolve_deck(conn: &Connection, deck: &DeckRef, description: &str) -> AppResult<(i64, usize)> {
    match deck {
        DeckRef::Id(id) => Ok((*id, 0)),
        DeckRef::New(parent, names) => {
            let (id, made) = decks::ensure_path(conn, *parent, names, description)?;
            id.map(|id| (id, made)).ok_or_else(|| AppError::msg("Empty deck path"))
        }
    }
}

/// An existing deck at `names` below `parent`, or one to be created.
pub fn deck_ref(conn: &Connection, parent: Option<i64>, names: &[String]) -> AppResult<DeckRef> {
    if names.is_empty() {
        if let Some(p) = parent {
            return Ok(DeckRef::Id(p));
        }
    }
    Ok(match decks::find_path(conn, parent, names)? {
        Some(id) => DeckRef::Id(id),
        None => DeckRef::New(parent, names.to_vec()),
    })
}

// ---------- import / export ----------

const HEADER_WORDS: [&str; 7] = ["hanzi", "character", "characters", "simplified", "chinese", "汉字", "word"];

/// One card from a CSV/TSV file. `deck` is an optional subdeck path ("Lesson 1/Food").
pub struct CsvCard {
    pub hanzi: String,
    pub pinyin: String,
    pub meaning: String,
    pub notes: String,
    pub tags: String,
    pub deck: Vec<String>,
}

pub fn read_text(path: &Path) -> AppResult<String> {
    let bytes = std::fs::read(path)?;
    Ok(String::from_utf8_lossy(&bytes).trim_start_matches('\u{feff}').to_string())
}

/// Parse CSV or TSV. Columns: hanzi, pinyin, meaning, notes, tags, deck (subdeck path).
/// Works with Pleco and Anki plain-text exports (tab separated). A header row is
/// detected and skipped. Numbered pinyin is converted to tone marks.
/// Returns the cards and the number of rows without Chinese characters.
pub fn parse_csv(text: &str) -> AppResult<(Vec<CsvCard>, i64)> {
    let first_line = text.lines().find(|l| !l.trim().is_empty() && !l.starts_with('#')).unwrap_or("");
    let delimiter = if first_line.contains('\t') { b'\t' } else { b',' };
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(false)
        .flexible(true)
        .comment(Some(b'#'))
        .from_reader(text.as_bytes());
    let mut out = Vec::new();
    let mut invalid = 0;
    for (i, rec) in reader.records().enumerate() {
        let rec = rec?;
        let field = |n: usize| rec.get(n).unwrap_or("").trim().to_string();
        let hanzi = field(0);
        if i == 0 && HEADER_WORDS.contains(&hanzi.to_lowercase().as_str()) {
            continue;
        }
        if hanzi.is_empty() || !pinyin::has_hanzi(&hanzi) {
            invalid += 1;
            continue;
        }
        out.push(CsvCard {
            pinyin: pinyin::numbered_to_marks(&field(1)),
            meaning: field(2),
            notes: field(3),
            tags: join_tags(&[field(4)]),
            deck: decks::split_path(&field(5)),
            hanzi,
        });
    }
    Ok((out, invalid))
}

/// What importing CSV text into `deck_id` would do (duplicates, invalid rows).
pub fn preview_csv(conn: &Connection, deck_id: i64, text: &str) -> AppResult<ImportPreview> {
    let (cards, invalid) = parse_csv(text)?;
    let mut items = Vec::new();
    for c in &cards {
        items.push((deck_ref(conn, Some(deck_id), &c.deck)?, c.hanzi.as_str()));
    }
    Ok(DupChecker::load(conn)?.preview(items.into_iter(), invalid))
}

/// Import CSV text into `deck_id` (cards with a deck column go into subdecks of it).
/// Words already in the same deck are skipped; words in other decks are imported only
/// with `include_other_decks`.
pub fn import_text(
    conn: &mut Connection,
    deck_id: i64,
    text: &str,
    include_other_decks: bool,
) -> AppResult<ImportResult> {
    let (cards, invalid) = parse_csv(text)?;
    let tx = conn.transaction()?;
    let ts = now();
    let mut res = ImportResult { skipped: invalid, ..Default::default() };
    let mut dups = DupChecker::load(&tx)?;
    for c in &cards {
        let target = deck_ref(&tx, Some(deck_id), &c.deck)?;
        match dups.check(&target, &c.hanzi) {
            Dup::SameDeck => {
                res.skipped += 1;
                continue;
            }
            Dup::OtherDecks(_) if !include_other_decks => {
                res.skipped_other += 1;
                continue;
            }
            _ => {}
        }
        let (deck, _) = resolve_deck(&tx, &target, "")?;
        tx.execute(
            "INSERT INTO cards (deck_id, hanzi, pinyin, meaning, notes, tags, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
            params![deck, c.hanzi, c.pinyin, c.meaning, c.notes, crate::hsk::tags_for_new(&tx, &c.tags, &c.hanzi), ts + res.imported],
        )?;
        dups.add(DeckRef::Id(deck), &c.hanzi);
        res.imported += 1;
    }
    tx.commit()?;
    Ok(res)
}

/// Export cards of a deck and its subdecks (or all cards) as CSV. The last column is
/// the subdeck path below the exported deck (the full path when exporting everything).
pub fn export_file(conn: &Connection, deck_id: Option<i64>, path: &Path) -> AppResult<i64> {
    let cards = list_cards(conn, deck_id, "")?;
    let paths = decks::paths(conn)?;
    let base = deck_id.and_then(|d| paths.get(&d)).map(Vec::len).unwrap_or(0);
    let sep = decks::PATH_FILE_SEP.to_string();
    let mut w = csv::Writer::from_path(path)?;
    w.write_record(["hanzi", "pinyin", "meaning", "notes", "tags", "deck"])?;
    for row in cards.iter().rev() {
        let c = &row.card;
        let deck = paths.get(&c.deck_id).map(|p| p[base.min(p.len())..].join(&sep)).unwrap_or_default();
        w.write_record([&c.hanzi, &c.pinyin, &c.meaning, &c.notes, &c.tags.join(";"), &deck])?;
    }
    w.flush()?;
    Ok(cards.len() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn removes_tags() {
        let mut conn = db::open_in_memory().unwrap();
        let deck: i64 = conn.query_row("SELECT id FROM decks LIMIT 1", [], |r| r.get(0)).unwrap();
        conn.execute(
            "INSERT INTO cards (deck_id, hanzi, tags, created_at, updated_at) VALUES (?1, '旅', 'HSK2,Travel,travel', 0, 0), (?1, '菜', 'Menu', 0, 0)",
            [deck],
        )
        .unwrap();
        assert_eq!(remove_tags(&mut conn, &["TRAVEL".into(), "Menu".into()]).unwrap(), 2);
        let tags: Vec<String> = list_tags(&conn).unwrap();
        assert!(!tags.iter().any(|t| t.eq_ignore_ascii_case("travel") || t == "Menu"), "{tags:?}");
        let lv: String = conn.query_row("SELECT tags FROM cards WHERE hanzi = '旅'", [], |r| r.get(0)).unwrap();
        assert_eq!(lv, "HSK2");
    }

    #[test]
    fn crud_and_import() {
        let mut conn = db::open_in_memory().unwrap();
        let all_decks = decks::list_decks(&conn).unwrap();
        assert_eq!(all_decks.len(), 1, "seed deck");
        assert_eq!(all_decks[0].new_count, 12);

        let deck = decks::create_deck(&conn, "Food", "", None).unwrap();
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
            false,
        )
        .unwrap();
        assert_eq!(res.imported, 2);
        assert_eq!(res.skipped, 1);
        let all = list_cards(&conn, Some(deck), "").unwrap();
        assert_eq!(all.len(), 3);
        assert!(all.iter().any(|r| r.card.pinyin == "miàntiáo"));

        let tags = list_tags(&conn).unwrap();
        assert!(tags.contains(&"food".to_string()));
        decks::delete_deck(&mut conn, deck).unwrap();
        assert_eq!(list_cards(&conn, Some(deck), "").unwrap().len(), 0);
    }

    #[test]
    fn duplicates_and_subdecks() {
        let mut conn = db::open_in_memory().unwrap();
        let class = decks::create_deck(&conn, "Class", "", None).unwrap();
        // the seed deck "Getting Started" has 你好
        let csv = "hanzi,pinyin,meaning,notes,tags,deck\n\
                   你好,ni3 hao3,hello,,,\n\
                   年,nian2,year,,,Class 9\n\
                   岁,sui4,years old,,,Class 9\n\
                   年,nian2,year,,,Class 9\n\
                   年,nian2,year,,,\n";
        let p = preview_csv(&conn, class, csv).unwrap();
        assert_eq!((p.rows, p.fresh, p.same_deck), (5, 2, 1));
        let words: Vec<(&str, Vec<String>)> =
            p.other_decks.iter().map(|w| (w.hanzi.as_str(), w.decks.clone())).collect();
        assert_eq!(
            words,
            [("你好", vec!["Getting Started".to_string()]), ("年", vec!["Class › Class 9".to_string()])]
        );

        let r = import_text(&mut conn, class, csv, false).unwrap();
        assert_eq!((r.imported, r.skipped, r.skipped_other), (2, 1, 2));
        let r = import_text(&mut conn, class, csv, true).unwrap();
        assert_eq!((r.imported, r.skipped, r.skipped_other), (2, 3, 0), "now only same-deck rows are skipped");

        assert_eq!(list_cards(&conn, Some(class), "").unwrap().len(), 4, "Class includes Class 9");
        let found = cards_by_hanzi(&conn, "年").unwrap();
        let decks_of: Vec<&str> = found.iter().map(|(_, d)| d.as_str()).collect();
        assert_eq!(decks_of, ["Class › Class 9", "Class"]);

        let out = std::env::temp_dir().join(format!("hanzi-cards-test-{}.csv", std::process::id()));
        assert_eq!(export_file(&conn, Some(class), &out).unwrap(), 4);
        let text = std::fs::read_to_string(&out).unwrap();
        std::fs::remove_file(&out).ok();
        assert!(text.contains("年,nián,year,,HSK1,Class 9"), "{text}"); // tagged with its HSK level
        assert!(text.contains("你好,nǐ hǎo,hello,,HSK1,\n"), "{text}"); // 你 + 好, both HSK1
    }
}
