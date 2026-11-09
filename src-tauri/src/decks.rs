//! Decks, which can be nested ("Class › Class 9"): the deck tree, CRUD and path helpers.
//!
//! A deck includes its subdecks when listing, studying and exporting cards.

use crate::db::now;
use crate::error::{AppError, AppResult};
use crate::models::Deck;
use rusqlite::{params, Connection, OptionalExtension};
use std::cmp::Ordering;
use std::collections::HashMap;

/// Separator used when showing a deck's path.
pub const PATH_LABEL_SEP: &str = " › ";
/// Separator used for deck paths in files (Pleco categories, CSV).
pub const PATH_FILE_SEP: char = '/';

/// SQL for the ids of deck `?N` and all its subdecks, for `deck_id IN (...)`.
pub fn subtree_sql(param: &str) -> String {
    format!(
        "WITH RECURSIVE sub(id) AS (SELECT {param} UNION ALL SELECT d.id FROM decks d JOIN sub ON d.parent_id = sub.id) SELECT id FROM sub"
    )
}

/// Natural order: "Class 2" before "Class 10", case-insensitive.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    fn chunks(s: &str) -> Vec<(bool, String)> {
        let mut out: Vec<(bool, String)> = Vec::new();
        for c in s.chars() {
            let digit = c.is_ascii_digit();
            match out.last_mut() {
                Some((d, text)) if *d == digit => text.push(c),
                _ => out.push((digit, c.to_string())),
            }
        }
        out
    }
    let (ca, cb) = (chunks(a), chunks(b));
    for ((da, ta), (db, tb)) in ca.iter().zip(cb.iter()) {
        let ord = if *da && *db {
            let (na, nb) = (ta.trim_start_matches('0'), tb.trim_start_matches('0'));
            na.len().cmp(&nb.len()).then_with(|| na.cmp(nb))
        } else {
            ta.to_lowercase().cmp(&tb.to_lowercase())
        };
        if ord != Ordering::Equal {
            return ord;
        }
    }
    ca.len().cmp(&cb.len()).then_with(|| a.cmp(b))
}

/// All decks in tree order (parents before children, siblings in natural order), with
/// their path, depth, own counts and totals including subdecks.
pub fn list_decks(conn: &Connection) -> AppResult<Vec<Deck>> {
    let t = now();
    let mut stmt = conn.prepare(
        "SELECT d.id, d.parent_id, d.name, d.description, d.created_at,
            (SELECT COUNT(*) FROM cards c WHERE c.deck_id = d.id),
            (SELECT COUNT(*) FROM progress p JOIN cards c ON c.id = p.card_id
              WHERE c.deck_id = d.id AND p.due <= ?1),
            (SELECT COUNT(*) FROM cards c WHERE c.deck_id = d.id
              AND NOT EXISTS (SELECT 1 FROM progress p WHERE p.card_id = c.id))
         FROM decks d",
    )?;
    let all: Vec<Deck> = stmt
        .query_map([t], |r| {
            Ok(Deck {
                id: r.get(0)?,
                parent_id: r.get(1)?,
                name: r.get(2)?,
                description: r.get(3)?,
                created_at: r.get(4)?,
                card_count: r.get(5)?,
                due_count: r.get(6)?,
                new_count: r.get(7)?,
                path: String::new(),
                depth: 0,
                child_count: 0,
                total_cards: 0,
                total_due: 0,
                total_new: 0,
            })
        })?
        .collect::<Result<_, _>>()?;

    let ids: std::collections::HashSet<i64> = all.iter().map(|d| d.id).collect();
    let mut children: HashMap<Option<i64>, Vec<usize>> = HashMap::new();
    for (i, d) in all.iter().enumerate() {
        // a parent that no longer exists makes this a top-level deck
        let parent = d.parent_id.filter(|p| ids.contains(p));
        children.entry(parent).or_default().push(i);
    }
    for list in children.values_mut() {
        list.sort_by(|a, b| natural_cmp(&all[*a].name, &all[*b].name));
    }

    fn visit(
        i: usize,
        depth: i64,
        prefix: &str,
        all: &[Deck],
        children: &HashMap<Option<i64>, Vec<usize>>,
        out: &mut Vec<Deck>,
    ) -> (i64, i64, i64) {
        let mut d = all[i].clone();
        d.depth = depth;
        d.path = if prefix.is_empty() { d.name.clone() } else { format!("{prefix}{PATH_LABEL_SEP}{}", d.name) };
        let kids = children.get(&Some(d.id)).cloned().unwrap_or_default();
        d.child_count = kids.len() as i64;
        let at = out.len();
        let path = d.path.clone();
        out.push(d);
        let (mut cards, mut due, mut new) = (all[i].card_count, all[i].due_count, all[i].new_count);
        for k in kids {
            let (c, du, n) = visit(k, depth + 1, &path, all, children, out);
            cards += c;
            due += du;
            new += n;
        }
        out[at].total_cards = cards;
        out[at].total_due = due;
        out[at].total_new = new;
        (cards, due, new)
    }

    let mut out = Vec::with_capacity(all.len());
    for root in children.get(&None).cloned().unwrap_or_default() {
        visit(root, 0, "", &all, &children, &mut out);
    }
    Ok(out)
}

/// Each deck's path as names from the top: id -> ["Class", "Class 9"].
pub fn paths(conn: &Connection) -> AppResult<HashMap<i64, Vec<String>>> {
    let mut stmt = conn.prepare("SELECT id, parent_id, name FROM decks")?;
    let rows: HashMap<i64, (Option<i64>, String)> = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, (r.get(1)?, r.get(2)?))))?
        .collect::<Result<_, _>>()?;
    let mut out = HashMap::new();
    for id in rows.keys() {
        let mut path = Vec::new();
        let mut cur = Some(*id);
        while let Some(c) = cur {
            let Some((parent, name)) = rows.get(&c) else { break };
            path.push(name.clone());
            cur = *parent;
            if path.len() > 64 {
                break; // corrupt cycle guard
            }
        }
        path.reverse();
        out.insert(*id, path);
    }
    Ok(out)
}

pub fn label(path: &[String]) -> String {
    path.join(PATH_LABEL_SEP)
}

/// Ids of a deck and all its subdecks.
pub fn subtree_ids(conn: &Connection, id: i64) -> AppResult<Vec<i64>> {
    let mut stmt = conn.prepare(&subtree_sql("?1"))?;
    let rows = stmt.query_map([id], |r| r.get(0))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn clean_name(name: &str) -> AppResult<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::msg("Deck name cannot be empty"));
    }
    Ok(name.to_string())
}

fn check_parent(conn: &Connection, parent: Option<i64>) -> AppResult<()> {
    if let Some(p) = parent {
        let exists: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM decks WHERE id = ?1)", [p], |r| r.get(0))?;
        if !exists {
            return Err(AppError::msg("The parent deck doesn't exist"));
        }
    }
    Ok(())
}

pub fn create_deck(conn: &Connection, name: &str, description: &str, parent: Option<i64>) -> AppResult<i64> {
    check_parent(conn, parent)?;
    conn.execute(
        "INSERT INTO decks (name, description, created_at, parent_id) VALUES (?1, ?2, ?3, ?4)",
        params![clean_name(name)?, description.trim(), now(), parent],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn update_deck(conn: &Connection, id: i64, name: &str, description: &str, parent: Option<i64>) -> AppResult<()> {
    check_parent(conn, parent)?;
    if let Some(p) = parent {
        if subtree_ids(conn, id)?.contains(&p) {
            return Err(AppError::msg("A deck can't be moved inside itself or one of its subdecks"));
        }
    }
    conn.execute(
        "UPDATE decks SET name = ?2, description = ?3, parent_id = ?4 WHERE id = ?1",
        params![id, clean_name(name)?, description.trim(), parent],
    )?;
    Ok(())
}

/// Delete a deck, its subdecks and all their cards.
pub fn delete_deck(conn: &mut Connection, id: i64) -> AppResult<()> {
    let ids = subtree_ids(conn, id)?;
    let tx = conn.transaction()?;
    for d in ids.iter().rev() {
        tx.execute("DELETE FROM decks WHERE id = ?1", [d])?;
    }
    tx.commit()?;
    Ok(())
}

/// The subdeck called `name` directly inside `parent` (`None` = top level).
pub fn find_child(conn: &Connection, parent: Option<i64>, name: &str) -> AppResult<Option<i64>> {
    Ok(conn
        .query_row(
            "SELECT id FROM decks WHERE parent_id IS ?1 AND name = ?2 COLLATE NOCASE ORDER BY id LIMIT 1",
            params![parent, name.trim()],
            |r| r.get(0),
        )
        .optional()?)
}

/// The deck at `path` below `parent`, if it exists.
pub fn find_path(conn: &Connection, parent: Option<i64>, path: &[String]) -> AppResult<Option<i64>> {
    let mut cur = parent;
    for name in path {
        match find_child(conn, cur, name)? {
            Some(id) => cur = Some(id),
            None => return Ok(None),
        }
    }
    Ok(cur)
}

/// The deck at `path` below `parent`, creating missing decks. Returns (id, decks created).
pub fn ensure_path(
    conn: &Connection,
    parent: Option<i64>,
    path: &[String],
    description: &str,
) -> AppResult<(Option<i64>, usize)> {
    let mut cur = parent;
    let mut created = 0;
    for name in path {
        cur = Some(match find_child(conn, cur, name)? {
            Some(id) => id,
            None => {
                created += 1;
                create_deck(conn, name, description, cur)?
            }
        });
    }
    Ok((cur, created))
}

/// Split a file path like "Class/Class 9" into names.
pub fn split_path(s: &str) -> Vec<String> {
    s.split(PATH_FILE_SEP).map(str::trim).filter(|p| !p.is_empty()).map(String::from).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_decks() {
        let mut conn = crate::db::open_in_memory().unwrap();
        let class = create_deck(&conn, "Class", "", None).unwrap();
        let c10 = create_deck(&conn, "Class 10", "", Some(class)).unwrap();
        let c9 = create_deck(&conn, "Class 9", "", Some(class)).unwrap();
        let (id, made) = ensure_path(&conn, None, &split_path("Class/Class 9/Extra"), "").unwrap();
        assert_eq!(made, 1);
        assert_eq!(find_path(&conn, None, &split_path("class/class 9")).unwrap(), Some(c9));

        conn.execute(
            "INSERT INTO cards (deck_id, hanzi, created_at, updated_at) VALUES (?1, '年', 0, 0), (?2, '岁', 0, 0)",
            [c9, id.unwrap()],
        )
        .unwrap();
        let decks = list_decks(&conn).unwrap();
        let order: Vec<&str> = decks.iter().map(|d| d.path.as_str()).collect();
        assert_eq!(order, ["Class", "Class › Class 9", "Class › Class 9 › Extra", "Class › Class 10", "Getting Started"]);
        assert_eq!(decks[0].total_cards, 2);
        assert_eq!(decks[0].child_count, 2);
        assert_eq!(decks[1].depth, 1);

        assert!(update_deck(&conn, class, "Class", "", Some(c10)).is_err(), "no cycles");
        update_deck(&conn, c10, "Class 10", "", None).unwrap();
        assert_eq!(list_decks(&conn).unwrap().iter().find(|d| d.id == c10).unwrap().depth, 0);

        delete_deck(&mut conn, class).unwrap();
        let left: i64 = conn.query_row("SELECT COUNT(*) FROM cards WHERE hanzi IN ('年', '岁')", [], |r| r.get(0)).unwrap();
        assert_eq!(left, 0, "subdecks and their cards are deleted too");
        assert_eq!(list_decks(&conn).unwrap().len(), 2);
    }

    #[test]
    fn natural_order() {
        let mut v = vec!["Class 10", "class 9", "Class 11", "Class 1", "Apple"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, ["Apple", "Class 1", "class 9", "Class 10", "Class 11"]);
    }
}
