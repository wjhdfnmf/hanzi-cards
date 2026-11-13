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
            // due reviews first (so new cards never crowd them out), the ones you're most likely
            // to be forgetting first when they don't all fit; then today's new cards
            let mut due: Vec<StudyItem> = items.iter().filter(|i| is_due(i)).cloned().collect();
            shuffle(&mut due);
            due.sort_by(|a, b| recall_chance(a, t).total_cmp(&recall_chance(b, t)));
            let mut new: Vec<StudyItem> = items.iter().filter(|i| i.progress.is_none()).cloned().collect();
            order_new(&mut new, &new_order(conn));
            new.truncate(new_remaining_today as usize);
            due.into_iter().chain(new).collect()
        }
        "new" => {
            let mut new: Vec<StudyItem> = items.into_iter().filter(|i| i.progress.is_none()).collect();
            order_new(&mut new, &new_order(conn));
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
/// FSRS's estimate of the chance of recalling a card now (1.0 for one never studied).
fn recall_chance(i: &StudyItem, now: i64) -> f64 {
    i.progress.as_ref().map_or(1.0, |p| {
        let elapsed = (now - p.last_review).max(0) as f64 / db::DAY as f64;
        scheduler::retrievability(elapsed, p.stability.max(0.01))
    })
}

/// How new words are introduced (Settings › Studying › Advanced): "hsk" (default: HSK1
/// first, then HSK2 …, words without a level last, each in deck order), "deck" or "random".
fn new_order(conn: &Connection) -> String {
    db::get_setting(conn, "new_order").ok().flatten().unwrap_or_else(|| "hsk".into())
}

/// 1 for HSK1 … 7 for HSK7-9; 8 for no level.
fn hsk_rank(card: &crate::models::Card) -> u8 {
    card.tags
        .iter()
        .filter_map(|t| {
            let t = t.trim().to_ascii_uppercase();
            match t.strip_prefix("HSK")? {
                "7-9" => Some(7),
                n => n.parse::<u8>().ok().filter(|n| (1..=6).contains(n)),
            }
        })
        .min()
        .unwrap_or(8)
}

/// Put new cards in the order they should be introduced. Items arrive in deck order
/// (creation order).
fn order_new(new: &mut [StudyItem], order: &str) {
    match order {
        "random" => shuffle(new),
        "deck" => {}
        _ => new.sort_by_key(|i| hsk_rank(&i.card)), // stable: deck order within a level
    }
}

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

/// Copies of one word (the same characters in several decks) share their progress: an answer
/// updates every copy (`answer_at`), and a copy made later takes up what the others have, so
/// it isn't a "new" word in its deck. `card` = just that card (a new one); `None` = every
/// card (fills in for copies made before this was done).
pub fn share_progress(conn: &Connection, card: Option<i64>) -> AppResult<usize> {
    Ok(conn.execute(
        "INSERT OR IGNORE INTO progress (card_id, direction, state, stability, difficulty, due, last_review, reps, lapses, step)
         SELECT c.id, p.direction, p.state, p.stability, p.difficulty, p.due, p.last_review, p.reps, p.lapses, p.step
         FROM cards c
         JOIN cards other ON other.hanzi = c.hanzi AND other.id <> c.id
         JOIN progress p ON p.card_id = other.id
         WHERE (?1 IS NULL OR c.id = ?1)
           AND NOT EXISTS (SELECT 1 FROM progress mine WHERE mine.card_id = c.id AND mine.direction = p.direction)
         ORDER BY p.last_review DESC",
        [card],
    )?)
}

fn load_memory(conn: &Connection, card_id: i64, direction: &str) -> AppResult<Option<Memory>> {
    Ok(conn
        .query_row(
            "SELECT state, stability, difficulty, due, last_review, reps, lapses, step
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
                    step: r.get(7)?,
                })
            },
        )
        .optional()?)
}

/// Picks a card's interval fuzz: the same for the preview and the answer, different per card
/// and per review.
fn fuzz_seed(card_id: i64, direction: &str, mem: Option<&Memory>) -> u64 {
    let dir = direction.bytes().fold(0u64, |h, b| h.wrapping_mul(31).wrapping_add(b as u64));
    (card_id as u64).wrapping_mul(1_000_003) ^ dir.wrapping_mul(7919) ^ (mem.map(|m| m.reps).unwrap_or(0) as u64).wrapping_mul(104_729)
}

/// A word forgotten this many times is a "leech": tagged so it can be rewritten or given a
/// memory aid (and again every 4 lapses after, if it stays one). Settings can change it
/// ("leech_lapses", 0 = off).
pub const LEECH_LAPSES: i64 = 8;

/// The scheduling setup from Settings › Studying (defaults for anything not set).
pub fn config(conn: &Connection) -> scheduler::Config {
    let d = scheduler::Config::default();
    let minutes = |key: &str, default_secs: i64| db::setting_f64(conn, key, default_secs as f64 / 60.0).clamp(0.5, 1440.0);
    let gaps: Vec<i64> = db::get_setting(conn, "learn_gaps")
        .ok()
        .flatten()
        .map(|v| {
            v.split([' ', ',', ';'])
                .filter_map(|x| x.trim().parse::<f64>().ok())
                .filter(|m| *m > 0.0)
                .map(|m| (m.min(1440.0) * 60.0).round() as i64)
                .collect()
        })
        .filter(|g: &Vec<i64>| !g.is_empty())
        .unwrap_or(d.gaps_secs.clone());
    scheduler::Config {
        retention: db::desired_retention(conn),
        new_recalls: db::setting_i64(conn, "new_recalls", d.new_recalls).clamp(1, 6),
        relearn_recalls: db::setting_i64(conn, "relearn_recalls", d.relearn_recalls).clamp(1, 4),
        again_secs: (minutes("again_minutes", d.again_secs) * 60.0).round() as i64,
        gaps_secs: gaps,
        relearn_secs: (minutes("relearn_minutes", d.relearn_secs) * 60.0).round() as i64,
        fuzz: db::get_setting(conn, "fuzz").ok().flatten().map(|v| v != "false").unwrap_or(d.fuzz),
        max_days: db::setting_i64(conn, "max_interval_days", d.max_days).clamp(1, d.max_days),
    }
}

fn leech_lapses(conn: &Connection) -> i64 {
    db::setting_i64(conn, "leech_lapses", LEECH_LAPSES).max(0)
}
pub const LEECH_TAG: &str = "leech";

pub fn preview(conn: &Connection, card_id: i64, direction: &str) -> AppResult<Vec<IntervalPreview>> {
    check_direction(direction)?;
    let mem = load_memory(conn, card_id, direction)?;
    let t = now();
    let outcomes = scheduler::schedule_all(
        mem.as_ref(),
        t,
        day_start(t, local_offset()),
        &config(conn),
        fuzz_seed(card_id, direction, mem.as_ref()),
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
    let cfg = config(conn);
    let leech_at = leech_lapses(conn);
    let tx = conn.transaction()?;
    let mem = load_memory(&tx, card_id, direction)?;
    let seed = fuzz_seed(card_id, direction, mem.as_ref());
    let outcome = scheduler::schedule_all(mem.as_ref(), t, day_start(t, local_offset()), &cfg, seed)
        .swap_remove((rating - 1) as usize);
    let m = &outcome.mem;
    tx.execute(
        "INSERT INTO progress (card_id, direction, state, stability, difficulty, due, last_review, reps, lapses, step)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT(card_id, direction) DO UPDATE SET
            state = excluded.state, stability = excluded.stability, difficulty = excluded.difficulty,
            due = excluded.due, last_review = excluded.last_review, reps = excluded.reps,
            lapses = excluded.lapses, step = excluded.step",
        params![card_id, direction, m.state, m.stability, m.difficulty, m.due, m.last_review, m.reps, m.lapses, m.step],
    )?;
    // copies of the same word in other decks share this progress
    tx.execute(
        "INSERT INTO progress (card_id, direction, state, stability, difficulty, due, last_review, reps, lapses, step)
         SELECT c.id, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10 FROM cards c
         WHERE c.hanzi = (SELECT hanzi FROM cards WHERE id = ?1) AND c.id <> ?1
         ON CONFLICT(card_id, direction) DO UPDATE SET
            state = excluded.state, stability = excluded.stability, difficulty = excluded.difficulty,
            due = excluded.due, last_review = excluded.last_review, reps = excluded.reps,
            lapses = excluded.lapses, step = excluded.step",
        params![card_id, direction, m.state, m.stability, m.difficulty, m.due, m.last_review, m.reps, m.lapses, m.step],
    )?;
    // a lapse that makes the word a leech: tag it (and its copies) "leech"
    let lapsed = mem.as_ref().is_some_and(|old| m.lapses > old.lapses);
    let leech = leech_at > 0 && lapsed && m.lapses >= leech_at && (m.lapses - leech_at) % 4 == 0;
    if leech {
        set_leech_tag(&tx, card_id, true)?;
    }
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
    // a leech that's been remembered at review several times in a row has been learned:
    // the tag comes off
    let passed_review = rating > 1 && mem.as_ref().is_some_and(|old| old.state == scheduler::REVIEW);
    let unleeched = passed_review && !leech && recent_review_passes(&tx, card_id, direction)? >= LEECH_CURE_PASSES
        && set_leech_tag(&tx, card_id, false)?;
    tx.commit()?;
    Ok(AnswerResult {
        due: m.due,
        state: m.state,
        interval_secs: m.due - t,
        days: outcome.days,
        leech,
        unleeched,
    })
}

/// Passes at review in a row that take the leech tag off again.
pub const LEECH_CURE_PASSES: i64 = 3;

/// How many of the latest reviews of this card and direction (in review state) were passes,
/// counting back to the last miss.
fn recent_review_passes(conn: &Connection, card_id: i64, direction: &str) -> AppResult<i64> {
    let mut stmt = conn.prepare(
        "SELECT rating FROM review_log WHERE card_id = ?1 AND direction = ?2 AND state_before = ?3
         ORDER BY reviewed_at DESC, id DESC LIMIT 20",
    )?;
    let ratings: Vec<i64> =
        stmt.query_map(params![card_id, direction, scheduler::REVIEW], |r| r.get(0))?.collect::<Result<_, _>>()?;
    Ok(ratings.iter().take_while(|r| **r > 1).count() as i64)
}

/// Add or remove the "leech" tag on a word and its copies in other decks; true if any changed.
fn set_leech_tag(conn: &Connection, card_id: i64, on: bool) -> AppResult<bool> {
    let rows: Vec<(i64, String)> = {
        let mut stmt = conn.prepare("SELECT id, tags FROM cards WHERE hanzi = (SELECT hanzi FROM cards WHERE id = ?1)")?;
        let v = stmt.query_map([card_id], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
        v
    };
    let mut changed = false;
    for (id, tags) in rows {
        let mut list = crate::cards::split_tags(&tags);
        let has = list.iter().any(|x| x.eq_ignore_ascii_case(LEECH_TAG));
        if on && !has {
            list.push(LEECH_TAG.to_string());
            list.sort();
        } else if !on && has {
            list.retain(|x| !x.eq_ignore_ascii_case(LEECH_TAG));
        } else {
            continue;
        }
        conn.execute("UPDATE cards SET tags = ?2 WHERE id = ?1", params![id, list.join(",")])?;
        changed = true;
    }
    Ok(changed)
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
    fn new_words_easiest_first_and_reviews_most_at_risk_first() {
        let mut conn = crate::db::open_in_memory().unwrap();
        let deck: i64 = conn.query_row("SELECT id FROM decks LIMIT 1", [], |r| r.get(0)).unwrap();
        conn.execute("DELETE FROM cards", []).unwrap();
        for (h, tag) in [("难", "HSK5"), ("没", ""), ("好", "HSK1"), ("用", "HSK3"), ("人", "HSK1")] {
            conn.execute(
                "INSERT INTO cards (deck_id, hanzi, tags, created_at, updated_at) VALUES (?1, ?2, ?3, 0, 0)",
                params![deck, h, tag],
            )
            .unwrap();
        }
        let mut f = filter("new");
        f.limit = 0;
        let order = |conn: &Connection, f: &StudyFilter| -> Vec<String> {
            build_session(conn, f).unwrap().items.into_iter().map(|i| i.card.hanzi).collect()
        };
        assert_eq!(order(&conn, &f), ["好", "人", "用", "难", "没"], "HSK1 first, deck order within a level");
        crate::db::set_setting(&conn, "new_order", "deck").unwrap();
        assert_eq!(order(&conn, &f), ["难", "没", "好", "用", "人"]);
        crate::db::set_setting(&conn, "new_order", "hsk").unwrap();

        // reviews: when they don't all fit, the one most likely forgotten comes first
        let ids: Vec<i64> = conn.prepare("SELECT id FROM cards ORDER BY id").unwrap()
            .query_map([], |r| r.get(0)).unwrap().map(|x| x.unwrap()).collect();
        let t = now();
        answer_at(&mut conn, ids[0], "recognition", 4, 1000, t - 40 * db::DAY).unwrap(); // long overdue
        answer_at(&mut conn, ids[1], "recognition", 4, 1000, t - 12 * db::DAY).unwrap(); // just due
        let mut f = filter("due");
        f.limit = 1;
        assert_eq!(order(&conn, &f), ["难"]);
    }

    #[test]
    fn leeches_are_tagged() {
        let mut conn = crate::db::open_in_memory().unwrap();
        let s = build_session(&conn, &filter("new")).unwrap();
        let card = s.items[0].card.id;
        let mut t = 1_800_000_000;
        answer_at(&mut conn, card, "recognition", 4, 1000, t).unwrap(); // straight to review
        let mut flagged = Vec::new();
        for _ in 0..LEECH_LAPSES {
            t += 30 * crate::db::DAY;
            let r = answer_at(&mut conn, card, "recognition", 1, 1000, t).unwrap(); // a lapse
            flagged.push(r.leech);
            t += 600;
            answer_at(&mut conn, card, "recognition", 3, 1000, t).unwrap(); // relearned
        }
        assert_eq!(flagged.iter().filter(|x| **x).count(), 1, "{flagged:?}");
        assert!(*flagged.last().unwrap(), "the 8th lapse makes it a leech");
        let tags: String = conn.query_row("SELECT tags FROM cards WHERE id = ?1", [card], |r| r.get(0)).unwrap();
        assert!(tags.split(',').any(|x| x == LEECH_TAG), "{tags}");
        // learned again: three passes at review in a row remove the tag
        let mut cured = Vec::new();
        for _ in 0..LEECH_CURE_PASSES {
            t += 60 * crate::db::DAY;
            cured.push(answer_at(&mut conn, card, "recognition", 3, 1000, t).unwrap().unleeched);
        }
        assert_eq!(cured, [false, false, true]);
        let tags: String = conn.query_row("SELECT tags FROM cards WHERE id = ?1", [card], |r| r.get(0)).unwrap();
        assert!(!tags.split(',').any(|x| x == LEECH_TAG), "{tags}");
        let step: i64 = conn
            .query_row("SELECT step FROM progress WHERE card_id = ?1 AND direction = 'recognition'", [card], |r| r.get(0))
            .unwrap();
        assert_eq!(step, 0, "in review");
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
    fn due_cards_sample_randomly_new_cards_in_order() {
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
        // new-only mode: new words are introduced in order (HSK level, then deck order; these
        // have no level), not at random
        assert_eq!(pick(&conn, "new"), ids[10..14].to_vec());
        db::set_setting(&conn, "new_order", "random").unwrap();
        let seen: std::collections::HashSet<Vec<i64>> = (0..10).map(|_| pick(&conn, "new")).collect();
        assert!(seen.len() > 1, "random order when chosen");
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

        // a copy added later takes up the word's progress: it isn't new in its deck
        let extra = crate::decks::create_deck(&conn, "Extra", "", None).unwrap();
        let input = crate::models::CardInput {
            deck_id: extra,
            hanzi: "会".into(),
            pinyin: "huì".into(),
            meaning: "to be able to".into(),
            notes: String::new(),
            tags: vec![],
        };
        let c = crate::cards::create_card(&conn, &input).unwrap();
        let reps: i64 = conn.query_row("SELECT reps FROM progress WHERE card_id = ?1 AND direction = 'recognition'", [c.id], |r| r.get(0)).unwrap();
        assert_eq!(reps, 1);
        f.deck_ids = vec![extra];
        f.mode = "new".into();
        assert!(build_session(&conn, &f).unwrap().items.is_empty(), "not a new word there");

        // counted once: the word is in three decks, due in one direction
        conn.execute("UPDATE progress SET due = 0", []).unwrap();
        let decks = crate::decks::list_decks(&conn).unwrap();
        let of = |id: i64| decks.iter().find(|d| d.id == id).unwrap();
        assert_eq!((of(class).due_count, of(class).total_due, of(class).total_cards), (1, 1, 2), "Class and Class 9 hold the same word");
        assert_eq!((of(lesson).due_count, of(extra).due_count), (1, 1));
        assert_eq!(of(extra).new_count, 0);
        let stats = crate::stats::get_stats(&conn, 30).unwrap();
        assert_eq!(stats.due_now, 1, "one word in one direction, whatever its number of copies");
        assert_eq!(stats.forecast[0], 1);
        // a second direction of the same word is a second thing to study
        answer(&mut conn, b, "recall", 1, 1000).unwrap();
        conn.execute("UPDATE progress SET due = 0", []).unwrap();
        assert_eq!(crate::stats::get_stats(&conn, 30).unwrap().due_now, 2);
        assert_eq!(crate::decks::list_decks(&conn).unwrap().iter().find(|d| d.id == class).unwrap().total_due, 2);
    }
}
