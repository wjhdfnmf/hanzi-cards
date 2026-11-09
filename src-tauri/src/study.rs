//! Study sessions: picking cards to study and recording answers.

use crate::cards::{card_from_row, CARD_COLS};
use crate::db::{self, day_start, local_offset, now};
use crate::error::{AppError, AppResult};
use crate::models::{AnswerResult, IntervalPreview, Progress, SessionInfo, StudyFilter, StudyItem};
use crate::scheduler::{self, Memory};
use rusqlite::{params, Connection, OptionalExtension};

/// "speaking" = see the meaning, say the Chinese; "read_aloud" = see the characters, say
/// them in Chinese; "say_meaning" = see the characters, say the meaning in English (all
/// checked by speech recognition).
pub const DIRECTIONS: [&str; 6] = ["recognition", "recall", "listening", "speaking", "read_aloud", "say_meaning"];

fn check_direction(d: &str) -> AppResult<()> {
    if DIRECTIONS.contains(&d) {
        Ok(())
    } else {
        Err(AppError::msg(format!("Unknown study direction: {d}")))
    }
}

/// New cards introduced today (first-ever review happened since local midnight).
pub fn new_studied_today(conn: &Connection) -> AppResult<i64> {
    let start = day_start(now(), local_offset());
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM review_log WHERE state_before = 0 AND reviewed_at >= ?1",
        [start],
        |r| r.get(0),
    )?)
}

pub fn build_session(conn: &Connection, f: &StudyFilter) -> AppResult<SessionInfo> {
    if f.directions.is_empty() {
        return Err(AppError::msg("Choose at least one study direction"));
    }
    let t = now();
    // selected decks include their subdecks
    let deck_clause = if f.deck_ids.is_empty() {
        String::new()
    } else {
        let ids: Vec<String> = f.deck_ids.iter().map(|i| i.to_string()).collect();
        format!(
            "WHERE c.deck_id IN (WITH RECURSIVE sub(id) AS (SELECT id FROM decks WHERE id IN ({})
               UNION SELECT d.id FROM decks d JOIN sub ON d.parent_id = sub.id) SELECT id FROM sub)",
            ids.join(",")
        )
    };
    let sql = format!(
        "SELECT {CARD_COLS}, p.state, p.stability, p.difficulty, p.due, p.last_review, p.reps, p.lapses
         FROM cards c LEFT JOIN progress p ON p.card_id = c.id AND p.direction = ?1
         {deck_clause}
         ORDER BY c.created_at, c.id"
    );
    let wanted_tags: Vec<String> = f.tags.iter().map(|t| t.to_lowercase()).collect();

    let mut items: Vec<StudyItem> = Vec::new();
    for dir in &f.directions {
        check_direction(dir)?;
        let mut these: Vec<StudyItem> = Vec::new();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([dir], |r| {
            let card = card_from_row(r, 0)?;
            let state: Option<i64> = r.get(9)?;
            let progress = match state {
                Some(state) => Some(Progress {
                    state,
                    stability: r.get(10)?,
                    difficulty: r.get(11)?,
                    due: r.get(12)?,
                    last_review: r.get(13)?,
                    reps: r.get(14)?,
                    lapses: r.get(15)?,
                }),
                None => None,
            };
            Ok(StudyItem {
                card,
                direction: dir.clone(),
                progress,
            })
        })?;
        for item in rows {
            let item = item?;
            if !wanted_tags.is_empty()
                && !item
                    .card
                    .tags
                    .iter()
                    .any(|t| wanted_tags.contains(&t.to_lowercase()))
            {
                continue;
            }
            these.push(item);
        }
        items.extend(merge_duplicates(these));
    }

    let is_due = |i: &StudyItem| i.progress.as_ref().is_some_and(|p| p.due <= t);
    let due_count = items.iter().filter(|i| is_due(i)).count() as i64;
    let new_count = items.iter().filter(|i| i.progress.is_none()).count() as i64;
    let new_per_day = db::setting_i64(conn, "new_per_day", db::DEFAULT_NEW_PER_DAY);
    let new_remaining_today = (new_per_day - new_studied_today(conn)?).max(0);
    let limit = if f.limit <= 0 { usize::MAX } else { f.limit as usize };

    // Each mode picks a group of cards; when the session size can't take the whole group,
    // the session gets a random sample of it rather than the first cards in some order.
    let mut picked: Vec<StudyItem> = match f.mode.as_str() {
        "due" => {
            // due reviews first (so new cards never crowd them out), then today's new cards
            let mut due: Vec<StudyItem> = items.iter().filter(|i| is_due(i)).cloned().collect();
            shuffle(&mut due);
            let mut new: Vec<StudyItem> = items.iter().filter(|i| i.progress.is_none()).cloned().collect();
            shuffle(&mut new);
            new.truncate(new_remaining_today as usize);
            due.into_iter().chain(new).collect()
        }
        "new" => {
            let mut new: Vec<StudyItem> = items.into_iter().filter(|i| i.progress.is_none()).collect();
            shuffle(&mut new);
            new
        }
        "difficult" => {
            let mut hard: Vec<StudyItem> = items
                .into_iter()
                .filter(|i| {
                    i.progress
                        .as_ref()
                        .is_some_and(|p| p.lapses >= 2 || p.difficulty >= 7.0)
                })
                .collect();
            shuffle(&mut hard);
            hard
        }
        "all" => {
            // Cram: the cards gone longest without a review first (never reviewed counts as
            // longest), in random order among equals, so limited sessions rotate through the
            // whole selection instead of always taking the oldest cards.
            let mut all = items;
            shuffle(&mut all);
            all.sort_by_key(|i| i.progress.as_ref().map(|p| p.last_review).unwrap_or(0));
            all
        }
        other => return Err(AppError::msg(format!("Unknown study mode: {other}"))),
    };
    picked.truncate(limit);

    Ok(SessionInfo {
        items: picked,
        due_count,
        new_count,
        new_remaining_today,
    })
}

/// The same word in several decks is studied once: keep one card per set of characters
/// (the one furthest along, else the oldest) and show the meanings of all copies.
/// Fisher-Yates shuffle with a small xorshift generator seeded from the clock (no extra crate).
fn shuffle<T>(items: &mut [T]) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SALT: AtomicU64 = AtomicU64::new(0x9E37_79B9_7F4A_7C15);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1);
    let mut x = nanos ^ SALT.fetch_add(0x9E37_79B9_7F4A_7C15, Ordering::Relaxed) | 1;
    let mut next = move || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    for i in (1..items.len()).rev() {
        items.swap(i, (next() % (i as u64 + 1)) as usize);
    }
}

fn merge_duplicates(items: Vec<StudyItem>) -> Vec<StudyItem> {
    let mut groups: Vec<Vec<StudyItem>> = Vec::new();
    let mut index: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for item in items {
        let key = item.card.hanzi.trim().to_string();
        match index.get(&key) {
            Some(&g) => groups[g].push(item),
            None => {
                index.insert(key, groups.len());
                groups.push(vec![item]);
            }
        }
    }
    groups
        .into_iter()
        .map(|group| {
            let mut meanings: Vec<String> = Vec::new();
            for m in group.iter().map(|i| i.card.meaning.trim()) {
                if !m.is_empty() && !meanings.iter().any(|x| x == m) {
                    meanings.push(m.to_string());
                }
            }
            let best = group
                .iter()
                .enumerate()
                .min_by_key(|(k, i)| (i.progress.is_none(), i.progress.as_ref().map(|p| p.due).unwrap_or(0), *k))
                .map(|(k, _)| k)
                .unwrap_or(0);
            let mut item = group.into_iter().nth(best).expect("group is never empty");
            if meanings.len() > 1 {
                item.card.meaning = meanings.join(" / ");
            }
            item
        })
        .collect()
}

fn load_memory(conn: &Connection, card_id: i64, direction: &str) -> AppResult<Option<Memory>> {
    Ok(conn
        .query_row(
            "SELECT state, stability, difficulty, due, last_review, reps, lapses
             FROM progress WHERE card_id = ?1 AND direction = ?2",
            params![card_id, direction],
            |r| {
                Ok(Memory {
                    state: r.get(0)?,
                    stability: r.get(1)?,
                    difficulty: r.get(2)?,
                    due: r.get(3)?,
                    last_review: r.get(4)?,
                    reps: r.get(5)?,
                    lapses: r.get(6)?,
                })
            },
        )
        .optional()?)
}

pub fn preview(conn: &Connection, card_id: i64, direction: &str) -> AppResult<Vec<IntervalPreview>> {
    check_direction(direction)?;
    let mem = load_memory(conn, card_id, direction)?;
    let t = now();
    let outcomes = scheduler::schedule_all(
        mem.as_ref(),
        t,
        day_start(t, local_offset()),
        db::desired_retention(conn),
    );
    Ok(outcomes
        .into_iter()
        .map(|o| IntervalPreview {
            secs: o.mem.due - t,
            days: o.days,
        })
        .collect())
}

pub fn answer(
    conn: &mut Connection,
    card_id: i64,
    direction: &str,
    rating: u8,
    duration_ms: i64,
) -> AppResult<AnswerResult> {
    answer_at(conn, card_id, direction, rating, duration_ms, now())
}

pub fn answer_at(
    conn: &mut Connection,
    card_id: i64,
    direction: &str,
    rating: u8,
    duration_ms: i64,
    t: i64,
) -> AppResult<AnswerResult> {
    check_direction(direction)?;
    if !(1..=4).contains(&rating) {
        return Err(AppError::msg("Rating must be 1-4"));
    }
    let retention = db::desired_retention(conn);
    let tx = conn.transaction()?;
    let mem = load_memory(&tx, card_id, direction)?;
    let outcome = scheduler::schedule_all(mem.as_ref(), t, day_start(t, local_offset()), retention)
        .swap_remove((rating - 1) as usize);
    let m = &outcome.mem;
    tx.execute(
        "INSERT INTO progress (card_id, direction, state, stability, difficulty, due, last_review, reps, lapses)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(card_id, direction) DO UPDATE SET
            state = excluded.state, stability = excluded.stability, difficulty = excluded.difficulty,
            due = excluded.due, last_review = excluded.last_review, reps = excluded.reps,
            lapses = excluded.lapses",
        params![card_id, direction, m.state, m.stability, m.difficulty, m.due, m.last_review, m.reps, m.lapses],
    )?;
    // copies of the same word in other decks share this progress
    tx.execute(
        "INSERT INTO progress (card_id, direction, state, stability, difficulty, due, last_review, reps, lapses)
         SELECT c.id, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9 FROM cards c
         WHERE c.hanzi = (SELECT hanzi FROM cards WHERE id = ?1) AND c.id <> ?1
         ON CONFLICT(card_id, direction) DO UPDATE SET
            state = excluded.state, stability = excluded.stability, difficulty = excluded.difficulty,
            due = excluded.due, last_review = excluded.last_review, reps = excluded.reps,
            lapses = excluded.lapses",
        params![card_id, direction, m.state, m.stability, m.difficulty, m.due, m.last_review, m.reps, m.lapses],
    )?;
    tx.execute(
        "INSERT INTO review_log (card_id, direction, rating, state_before, reviewed_at, duration_ms, scheduled_secs)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            card_id,
            direction,
            rating,
            mem.as_ref().map(|m| m.state).unwrap_or(0),
            t,
            duration_ms.clamp(0, 10 * 60 * 1000),
            m.due - t
        ],
    )?;
    tx.commit()?;
    Ok(AnswerResult {
        due: m.due,
        state: m.state,
        interval_secs: m.due - t,
        days: outcome.days,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filter(mode: &str) -> StudyFilter {
        StudyFilter {
            deck_ids: vec![],
            tags: vec![],
            directions: vec!["recognition".into()],
            mode: mode.into(),
            limit: 0,
        }
    }

    #[test]
    fn session_and_answer() {
        let mut conn = crate::db::open_in_memory().unwrap();
        let s = build_session(&conn, &filter("due")).unwrap();
        assert_eq!(s.new_count, 12);
        assert_eq!(s.items.len(), 12); // under the default 20/day

        let card = s.items[0].card.id;
        let p = preview(&conn, card, "recognition").unwrap();
        assert_eq!(p.len(), 4);
        assert_eq!(p[0].secs, 60);

        let r = answer(&mut conn, card, "recognition", 3, 2500).unwrap();
        assert_eq!(r.state, scheduler::LEARNING);
        assert_eq!(new_studied_today(&conn).unwrap(), 1);

        let s2 = build_session(&conn, &filter("new")).unwrap();
        assert_eq!(s2.items.len(), 11);

        // Answer "easy" on another -> review state, not due now.
        let other = s2.items[0].card.id;
        let r2 = answer(&mut conn, other, "recognition", 4, 1000).unwrap();
        assert_eq!(r2.state, scheduler::REVIEW);
        assert!(r2.days.unwrap() >= 1);

        let mut both = filter("all");
        both.directions = vec!["recognition".into(), "recall".into()];
        both.limit = 5;
        assert_eq!(build_session(&conn, &both).unwrap().items.len(), 5);
        assert!(answer(&mut conn, card, "sideways", 3, 0).is_err());
    }

    #[test]
    fn cram_rotates_through_the_selection() {
        let mut conn = db::open_in_memory().unwrap();
        let deck: i64 = conn
            .query_row("INSERT INTO decks (name, created_at) VALUES ('Cram', 0) RETURNING id", [], |r| r.get(0))
            .unwrap();
        for i in 0..20 {
            conn.execute(
                "INSERT INTO cards (deck_id, hanzi, created_at, updated_at) VALUES (?1, ?2, ?3, ?3)",
                params![deck, format!("字{i}"), i],
            )
            .unwrap();
        }
        let filter = StudyFilter {
            deck_ids: vec![deck],
            tags: vec![],
            directions: vec!["recognition".into()],
            mode: "all".into(),
            limit: 5,
        };
        let names = |s: &SessionInfo| s.items.iter().map(|i| i.card.hanzi.clone()).collect::<Vec<_>>();
        // review the first five; the next cram session must pick other cards
        let first = build_session(&conn, &filter).unwrap();
        for item in &first.items {
            answer_at(&mut conn, item.card.id, "recognition", 3, 1000, now()).unwrap();
        }
        let second = build_session(&conn, &filter).unwrap();
        assert!(names(&second).iter().all(|h| !names(&first).contains(h)), "{:?} then {:?}", names(&first), names(&second));
        // and the picks aren't simply the oldest cards in order
        let orders: std::collections::HashSet<Vec<String>> = (0..8).map(|_| names(&build_session(&conn, &filter).unwrap())).collect();
        assert!(orders.len() > 1, "random among equally-unreviewed cards");
    }

    #[test]
    fn other_modes_sample_randomly_within_their_group() {
        let mut conn = db::open_in_memory().unwrap();
        db::set_setting(&conn, "new_per_day", "100").unwrap();
        let deck: i64 = conn
            .query_row("INSERT INTO decks (name, created_at) VALUES ('Mix', 0) RETURNING id", [], |r| r.get(0))
            .unwrap();
        let mut ids = Vec::new();
        for i in 0..30 {
            let id: i64 = conn
                .query_row(
                    "INSERT INTO cards (deck_id, hanzi, created_at, updated_at) VALUES (?1, ?2, ?3, ?3) RETURNING id",
                    params![deck, format!("词{i}"), i],
                    |r| r.get(0),
                )
                .unwrap();
            ids.push(id);
        }
        // ten cards reviewed long ago: due now
        for &id in &ids[..10] {
            answer_at(&mut conn, id, "recognition", 3, 1000, now() - 400 * 86400).unwrap();
        }
        let filter = |mode: &str| StudyFilter {
            deck_ids: vec![deck],
            tags: vec![],
            directions: vec!["recognition".into()],
            mode: mode.into(),
            limit: 4,
        };
        let pick = |conn: &Connection, mode: &str| -> Vec<i64> {
            build_session(conn, &filter(mode)).unwrap().items.iter().map(|i| i.card.id).collect()
        };
        // due mode: only due cards fit in 4, and which ones varies
        let mut seen = std::collections::HashSet::new();
        for _ in 0..10 {
            let p = pick(&conn, "due");
            assert!(p.iter().all(|id| ids[..10].contains(id)), "due before new");
            seen.insert(p);
        }
        assert!(seen.len() > 1, "a random sample of the due cards");
        // new-only mode: random new cards, not the first-added ones every time
        let mut seen = std::collections::HashSet::new();
        for _ in 0..10 {
            let p = pick(&conn, "new");
            assert!(p.iter().all(|id| ids[10..].contains(id)));
            seen.insert(p);
        }
        assert!(seen.len() > 1);
    }

    #[test]
    fn same_word_in_two_decks_is_studied_once() {
        let mut conn = crate::db::open_in_memory().unwrap();
        let class = crate::decks::create_deck(&conn, "Class", "", None).unwrap();
        let lesson = crate::decks::create_deck(&conn, "Class 9", "", Some(class)).unwrap();
        let add = |deck: i64, meaning: &str| {
            conn.execute(
                "INSERT INTO cards (deck_id, hanzi, pinyin, meaning, created_at, updated_at) VALUES (?1, '会', 'huì', ?2, 0, 0)",
                params![deck, meaning],
            )
            .unwrap();
            conn.last_insert_rowid()
        };
        let a = add(class, "can");
        let b = add(lesson, "meeting");

        let mut f = filter("all");
        f.deck_ids = vec![class]; // includes Class 9
        let s = build_session(&conn, &f).unwrap();
        assert_eq!(s.items.len(), 1, "one item for both copies");
        assert_eq!(s.items[0].card.meaning, "can / meeting");

        answer(&mut conn, a, "recognition", 4, 1000).unwrap();
        let shared: i64 = conn
            .query_row("SELECT reps FROM progress WHERE card_id = ?1 AND direction = 'recognition'", [b], |r| r.get(0))
            .unwrap();
        assert_eq!(shared, 1, "the copy shares the progress");
        let logged: i64 = conn.query_row("SELECT COUNT(*) FROM review_log", [], |r| r.get(0)).unwrap();
        assert_eq!(logged, 1, "but the review is logged once");

        f.deck_ids = vec![lesson];
        assert_eq!(build_session(&conn, &f).unwrap().items.len(), 1);
    }
}
