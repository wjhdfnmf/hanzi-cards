//! Study statistics: what was studied, for how long, how well it is remembered and what is
//! coming. One call answers for a scope (every deck, or a deck with its subdecks; one study
//! direction or all) and a period (the last N days, or all time).
//!
//! A word that several decks hold is one thing to study: its copies share their progress, and
//! here they count once. A review of a word counts for every deck that holds the word.

use crate::db::{local_day, local_offset, now, DAY};
use crate::error::AppResult;
use crate::{decks, scheduler};
use rusqlite::{params_from_iter, types::Value, Connection};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};

/// An answer counts for at most this long: a card left open isn't study time.
const ANSWER_CAP_MS: i64 = 120_000;
/// A word whose memory lasts three weeks or more is "mature" (as in Anki).
const MATURE_DAYS: f64 = 21.0;
/// Upper edges (in days) of the memory-strength groups; the last group is everything longer.
const STABILITY_EDGES: [f64; 8] = [1.0, 3.0, 7.0, 14.0, 30.0, 90.0, 180.0, 365.0];
const HSK_LEVELS: [&str; 7] = ["HSK1", "HSK2", "HSK3", "HSK4", "HSK5", "HSK6", "HSK7-9"];
/// Kinds of study time kept besides flashcard answers.
pub const TIME_KINDS: [&str; 2] = ["reading", "tutor"];

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsFilter {
    /// A deck with its subdecks; none = everything.
    #[serde(default)]
    pub deck_id: Option<i64>,
    /// The last N days including today; none = all time.
    #[serde(default)]
    pub days: Option<i64>,
    /// One study direction; none = all.
    #[serde(default)]
    pub direction: Option<String>,
    /// The directions the learner studies (the Study page's choice): what is due and the
    /// forecast count these only, so they agree with the Study page. Empty or none = all.
    #[serde(default)]
    pub due_directions: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayStat {
    pub date: String,
    pub reviews: i64,
    pub correct: i64,
    pub new_cards: i64,
    /// Answering flashcards.
    pub time_ms: i64,
    pub reading_ms: i64,
    pub tutor_ms: i64,
}

/// Totals over a stretch of days.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sum {
    pub reviews: i64,
    pub correct: i64,
    pub new_words: i64,
    pub time_ms: i64,
    pub reading_ms: i64,
    pub tutor_ms: i64,
    pub days_studied: i64,
    /// True retention: of the reviews of words already learned (the first one of each day
    /// only), how many were remembered.
    pub retained: i64,
    pub retention_of: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RetentionRow {
    /// "today" | "yesterday" | "week" | "month" | "year" | "all"
    pub range: String,
    /// [remembered, reviews] of young words (memory under three weeks) and of mature ones.
    pub young: [i64; 2],
    pub mature: [i64; 2],
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Bucket {
    pub reviews: i64,
    pub correct: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirStat {
    pub direction: String,
    pub reviews: i64,
    pub correct: i64,
    pub time_ms: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckStat {
    pub deck_id: i64,
    pub name: String,
    pub words: i64,
    pub studied: i64,
    pub mature: i64,
    pub reviews: i64,
    pub correct: i64,
    pub time_ms: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct HskStat {
    pub level: String,
    pub words: i64,
    pub studied: i64,
    pub mature: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HardCard {
    pub card_id: i64,
    pub hanzi: String,
    pub pinyin: String,
    pub meaning: String,
    pub direction: String,
    pub lapses: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub total_cards: i64,
    pub total_words: i64,
    /// Words never studied.
    pub unstudied_cards: i64,
    // word + direction pairs
    pub learning: i64,
    pub young: i64,
    pub mature: i64,
    pub due_now: i64,
    pub due_tomorrow: i64,
    pub leeches: i64,
    /// Days in a row with any study (not narrowed by the filters).
    pub streak: i64,
    pub best_streak: i64,
    pub target_retention: f64,
    pub today: Sum,
    pub period: Sum,
    /// The stretch of the same length just before the period (none for all time).
    pub previous: Option<Sum>,
    pub all_time: Sum,
    pub period_days: Option<i64>,
    /// One entry per day, oldest first: at least the last 371 days, and the whole period.
    pub days: Vec<DayStat>,
    pub retention_table: Vec<RetentionRow>,
    /// Again, Hard, Good, Easy for: words being learned, young words, mature words.
    pub buttons: [[i64; 4]; 3],
    /// Time answering each of those three.
    pub kind_time_ms: [i64; 3],
    /// By hour of the day (24) and day of the week (Monday first).
    pub hours: Vec<Bucket>,
    pub weekdays: Vec<Bucket>,
    pub by_direction: Vec<DirStat>,
    /// The decks inside the chosen one (or the top-level decks).
    pub by_deck: Vec<DeckStat>,
    pub hsk: Vec<HskStat>,
    /// How long memories last: counts per group of STABILITY_EDGES.
    pub stability: Vec<i64>,
    /// How hard words are: FSRS difficulty 1 to 10.
    pub difficulty: Vec<i64>,
    /// The chance, right now, of remembering a studied word (average), and their sum:
    /// about how many would be remembered if all were asked today.
    pub avg_retrievability: Option<f64>,
    pub known_estimate: f64,
    pub studied_items: i64,
    /// Reviews a day the present intervals lead to, on average.
    pub daily_load: f64,
    /// Due counts for today (index 0, includes overdue) through the next 29 days.
    pub forecast: Vec<i64>,
    pub hardest: Vec<HardCard>,
    /// Reading and tutor time are in the totals (they can't be told apart by deck or direction).
    pub other_time_included: bool,
}

fn date_label(day: i64) -> String {
    chrono::DateTime::from_timestamp(day * DAY, 0)
        .map(|d| d.date_naive().format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

/// Start the statistics over: every recorded answer and all reading and tutor time go. The
/// cards and what the scheduler remembers of each (when it is due, how well it is known)
/// stay, so studying carries on where it was. Returns how many answers were removed.
///
/// The reset is written down with its time, so that devices kept the same as this one drop
/// their copies of the same history instead of handing it back (see `hanzi_sync::merge`).
pub fn reset(conn: &mut Connection) -> AppResult<usize> {
    let tx = conn.transaction()?;
    // (one record of the reset, not one of every answer: the triggers stand back)
    tx.execute("INSERT OR REPLACE INTO sync_meta (key, value) VALUES ('applying', '1')", [])?;
    let answers = tx.execute("DELETE FROM review_log", [])?;
    tx.execute("DELETE FROM study_time", [])?;
    tx.execute("DELETE FROM sync_deleted WHERE kind = 'log'", [])?;
    tx.execute("INSERT OR REPLACE INTO sync_deleted (kind, uid, at) VALUES ('reset', 'statistics', ?1)", [now()])?;
    // the last answer can no longer be taken back: its record is gone
    tx.execute("DELETE FROM settings WHERE key = 'last_answer'", [])?;
    tx.execute("DELETE FROM sync_meta WHERE key = 'applying'", [])?;
    tx.commit()?;
    Ok(answers)
}

/// Add reading or tutor time to today's total. Small steps only: a page reports as it goes.
pub fn log_time(conn: &Connection, kind: &str, seconds: i64) -> AppResult<()> {
    if !TIME_KINDS.contains(&kind) {
        return Err(crate::error::AppError::msg("Unknown kind of study time"));
    }
    let seconds = seconds.clamp(0, 300);
    if seconds == 0 {
        return Ok(());
    }
    let day = local_day(now(), local_offset());
    conn.execute(
        "INSERT INTO study_time (day, kind, device, seconds)
         VALUES (?1, ?2, COALESCE((SELECT value FROM sync_meta WHERE key = 'device_id'), ''), ?3)
         ON CONFLICT(day, kind, device) DO UPDATE SET seconds = seconds + excluded.seconds",
        rusqlite::params![day, kind, seconds],
    )?;
    Ok(())
}

/// One answer from the log, with what the answers before it say about it.
struct Answer {
    day: i64,
    hour: usize,
    rating: usize,
    /// 0 being learned (new, learning, relearning), 1 young, 2 mature
    kind: usize,
    new: bool,
    /// a review of a learned word, and the first one of that word that day
    counts_for_retention: bool,
    ms: i64,
    direction: String,
    deck: i64,
}

fn add(sum: &mut Sum, a: &Answer) {
    sum.reviews += 1;
    sum.correct += (a.rating > 1) as i64;
    sum.new_words += a.new as i64;
    sum.time_ms += a.ms;
    if a.counts_for_retention {
        sum.retention_of += 1;
        sum.retained += (a.rating > 1) as i64;
    }
}

pub fn get_stats(conn: &Connection, filter: &StatsFilter) -> AppResult<Stats> {
    let t = now();
    let off = local_offset();
    let today = local_day(t, off);
    let days = filter.days.map(|d| d.clamp(1, 3660));
    let direction = filter.direction.as_deref().filter(|d| !d.is_empty());
    let deck = filter.deck_id;
    let scoped = deck.is_some() || direction.is_some();

    // SQL for "this card is in the chosen deck" and "this word is"
    let sub = decks::subtree_sql("?1");
    let (card_in, word_in) = match deck {
        Some(_) => (
            format!("c.deck_id IN ({sub})"),
            format!("c.hanzi IN (SELECT hanzi FROM cards WHERE deck_id IN ({sub}))"),
        ),
        None => ("?1 IS NULL".to_string(), "?1 IS NULL".to_string()),
    };
    let deck_param = || vec![deck.map(Value::Integer).unwrap_or(Value::Null)];

    // ---------- decks: which group (a deck directly inside the chosen one) a card belongs to ----------
    let mut parent: HashMap<i64, Option<i64>> = HashMap::new();
    let mut deck_name: HashMap<i64, String> = HashMap::new();
    {
        let mut stmt = conn.prepare("SELECT id, parent_id, name FROM decks")?;
        for row in stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?, r.get::<_, String>(2)?)))? {
            let (id, p, name) = row?;
            parent.insert(id, p);
            deck_name.insert(id, name);
        }
    }
    let group_of = |mut id: i64| -> Option<i64> {
        // climb until the parent is the chosen deck (or there is none)
        for _ in 0..64 {
            let p = parent.get(&id).copied().flatten();
            if p == deck {
                return Some(id);
            }
            if deck == Some(id) {
                return Some(id); // a card directly in the chosen deck
            }
            id = p?;
        }
        None
    };

    // ---------- the words in scope ----------
    struct Word {
        hsk: Option<usize>,
        leech: bool,
        groups: BTreeSet<i64>,
        studied: bool,
        mature: bool,
    }
    let mut words: HashMap<String, Word> = HashMap::new();
    let mut total_cards = 0i64;
    {
        let mut stmt = conn.prepare(&format!("SELECT c.hanzi, c.tags, c.deck_id FROM cards c WHERE {card_in}"))?;
        let rows = stmt.query_map(params_from_iter(deck_param()), |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?))
        })?;
        for row in rows {
            let (hanzi, tags, deck_id) = row?;
            total_cards += 1;
            let w = words.entry(hanzi).or_insert(Word { hsk: None, leech: false, groups: BTreeSet::new(), studied: false, mature: false });
            for tag in tags.split(',').map(str::trim) {
                if let Some(i) = HSK_LEVELS.iter().position(|l| l.eq_ignore_ascii_case(tag)) {
                    w.hsk = Some(w.hsk.map_or(i, |old| old.min(i)));
                }
                w.leech |= tag.eq_ignore_ascii_case("leech");
            }
            if let Some(g) = group_of(deck_id) {
                w.groups.insert(g);
            }
        }
    }

    // ---------- memory: one row for a word and direction ----------
    let mut st = Stats {
        total_cards,
        total_words: words.len() as i64,
        target_retention: crate::db::desired_retention(conn),
        period_days: days,
        stability: vec![0; STABILITY_EDGES.len() + 1],
        difficulty: vec![0; 10],
        forecast: vec![0; 30],
        hours: vec![Bucket::default(); 24],
        weekdays: vec![Bucket::default(); 7],
        other_time_included: !scoped,
        ..Stats::default()
    };
    {
        let mut stmt = conn.prepare(&format!(
            "SELECT c.hanzi, p.direction, p.state, p.stability, p.difficulty, p.due, p.last_review
             FROM progress p JOIN cards c ON c.id = p.card_id
             WHERE {card_in} AND (?2 IS NULL OR p.direction = ?2)
             GROUP BY c.hanzi, p.direction"
        ))?;
        let mut p = deck_param();
        p.push(direction.map(|d| Value::Text(d.to_string())).unwrap_or(Value::Null));
        let rows = stmt.query_map(params_from_iter(p), |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(2)?,
                r.get::<_, f64>(3)?,
                r.get::<_, f64>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, i64>(6)?,
                r.get::<_, String>(1)?,
            ))
        })?;
        let studying = filter.due_directions.as_ref().filter(|d| !d.is_empty() && direction.is_none());
        let mut recall_sum = 0.0;
        for row in rows {
            let (hanzi, state, stability, difficulty, due, last_review, dir) = row?;
            let waits = studying.is_none_or(|d| d.contains(&dir));
            let mature = state == scheduler::REVIEW && stability >= MATURE_DAYS;
            if let Some(w) = words.get_mut(&hanzi) {
                w.studied = true;
                w.mature |= mature;
            }
            st.studied_items += 1;
            match state {
                scheduler::REVIEW if mature => st.mature += 1,
                scheduler::REVIEW => st.young += 1,
                _ => st.learning += 1,
            }
            let due_in = (local_day(due, off) - today).max(0);
            if waits {
                st.due_now += (due <= t) as i64;
                st.due_tomorrow += (due_in == 1 && due > t) as i64;
                if due_in < 30 {
                    st.forecast[due_in as usize] += 1;
                }
            }
            let group = STABILITY_EDGES.iter().position(|&edge| stability < edge).unwrap_or(STABILITY_EDGES.len());
            st.stability[group] += 1;
            st.difficulty[(difficulty.round() as i64).clamp(1, 10) as usize - 1] += 1;
            let elapsed = (t - last_review).max(0) as f64 / DAY as f64;
            recall_sum += scheduler::retrievability(elapsed, stability.max(0.01)).clamp(0.0, 1.0);
            let interval = ((due - last_review) as f64 / DAY as f64).max(1.0);
            if waits {
                st.daily_load += 1.0 / interval;
            }
        }
        st.known_estimate = recall_sum;
        st.avg_retrievability = (st.studied_items > 0).then(|| recall_sum / st.studied_items as f64);
    }
    st.unstudied_cards = words.values().filter(|w| !w.studied).count() as i64;
    st.leeches = words.values().filter(|w| w.leech).count() as i64;

    // ---------- the answers ----------
    let mut answers: Vec<Answer> = Vec::new();
    {
        let mut stmt = conn.prepare(&format!(
            "SELECT c.hanzi, r.direction, r.rating, r.state_before, r.reviewed_at, r.duration_ms, r.scheduled_secs, c.deck_id
             FROM review_log r JOIN cards c ON c.id = r.card_id
             WHERE {word_in} AND (?2 IS NULL OR r.direction = ?2)
             ORDER BY c.hanzi, r.direction, r.reviewed_at, r.id"
        ))?;
        let mut p = deck_param();
        p.push(direction.map(|d| Value::Text(d.to_string())).unwrap_or(Value::Null));
        let rows = stmt.query_map(params_from_iter(p), |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, i64>(6)?,
                r.get::<_, i64>(7)?,
            ))
        })?;
        // within one word and direction, in order: the interval the answer before this one
        // gave says whether the word was young or mature, and the day says whether this is
        // the day's first review of it
        let mut key: Option<(String, String)> = None;
        let mut interval_before: Option<i64> = None;
        let mut day_before: Option<i64> = None;
        for row in rows {
            let (hanzi, dir, rating, state, at, ms, scheduled, deck_id) = row?;
            if key.as_ref().is_none_or(|(h, d)| *h != hanzi || *d != dir) {
                key = Some((hanzi, dir.clone()));
                interval_before = None;
                day_before = None;
            }
            let day = local_day(at, off);
            let review = state == scheduler::REVIEW;
            let mature = interval_before.is_some_and(|s| s as f64 >= MATURE_DAYS * DAY as f64);
            answers.push(Answer {
                day,
                hour: (((at + off).rem_euclid(DAY)) / 3600) as usize,
                rating: rating.clamp(1, 4) as usize,
                kind: if !review { 0 } else if mature { 2 } else { 1 },
                new: state == 0,
                counts_for_retention: review && day_before != Some(day),
                ms: ms.clamp(0, ANSWER_CAP_MS),
                direction: dir,
                deck: deck_id,
            });
            interval_before = Some(scheduled);
            day_before = Some(day);
        }
    }

    // ---------- periods ----------
    let first_answer_day = answers.iter().map(|a| a.day).min();
    let from = match days {
        Some(n) => today - n + 1,
        None => first_answer_day.unwrap_or(today),
    };
    let previous_from = days.map(|n| from - n);
    let mut previous = Sum::default();
    let (mut period_days, mut previous_days, mut all_days) = (HashSet::new(), HashSet::new(), HashSet::new());
    let table_from = [("today", today), ("yesterday", today - 1), ("week", today - 6), ("month", today - 29), ("year", today - 364), ("all", i64::MIN)];
    st.retention_table = table_from.iter().map(|(name, _)| RetentionRow { range: name.to_string(), ..Default::default() }).collect();
    let mut by_dir: HashMap<String, DirStat> = HashMap::new();
    let mut by_group: HashMap<i64, DeckStat> = HashMap::new();
    let mut by_day: HashMap<i64, DayStat> = HashMap::new();
    for a in &answers {
        add(&mut st.all_time, a);
        all_days.insert(a.day);
        let d = by_day.entry(a.day).or_default();
        d.reviews += 1;
        d.correct += (a.rating > 1) as i64;
        d.new_cards += a.new as i64;
        d.time_ms += a.ms;
        if a.day == today {
            add(&mut st.today, a);
        }
        if a.counts_for_retention && a.kind > 0 {
            for (row, (name, start)) in st.retention_table.iter_mut().zip(table_from) {
                let inside = if name == "yesterday" { a.day == start } else { a.day >= start };
                if inside {
                    let cell = if a.kind == 2 { &mut row.mature } else { &mut row.young };
                    cell[0] += (a.rating > 1) as i64;
                    cell[1] += 1;
                }
            }
        }
        if a.day >= from {
            add(&mut st.period, a);
            period_days.insert(a.day);
            st.buttons[a.kind][a.rating - 1] += 1;
            st.kind_time_ms[a.kind] += a.ms;
            let hour = &mut st.hours[a.hour.min(23)];
            hour.reviews += 1;
            hour.correct += (a.rating > 1) as i64;
            // day 0 of the epoch was a Thursday
            let weekday = &mut st.weekdays[(a.day + 3).rem_euclid(7) as usize];
            weekday.reviews += 1;
            weekday.correct += (a.rating > 1) as i64;
            let dir = by_dir.entry(a.direction.clone()).or_insert_with(|| DirStat { direction: a.direction.clone(), ..Default::default() });
            dir.reviews += 1;
            dir.correct += (a.rating > 1) as i64;
            dir.time_ms += a.ms;
            if let Some(g) = group_of(a.deck) {
                let deck = by_group.entry(g).or_default();
                deck.reviews += 1;
                deck.correct += (a.rating > 1) as i64;
                deck.time_ms += a.ms;
            }
        } else if previous_from.is_some_and(|p| a.day >= p) {
            add(&mut previous, a);
            previous_days.insert(a.day);
        }
    }

    // ---------- reading and tutor time (whole-app figures: left out when a deck or direction is chosen) ----------
    let mut other_days: Vec<i64> = Vec::new();
    {
        let mut stmt = conn.prepare("SELECT day, kind, SUM(seconds) FROM study_time GROUP BY day, kind ORDER BY day")?;
        for row in stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)))? {
            let (day, kind, seconds) = row?;
            other_days.push(day);
            if scoped {
                continue;
            }
            let ms = seconds * 1000;
            let put = |reading: &mut i64, tutor: &mut i64| if kind == "reading" { *reading += ms } else { *tutor += ms };
            let d = by_day.entry(day).or_default();
            put(&mut d.reading_ms, &mut d.tutor_ms);
            put(&mut st.all_time.reading_ms, &mut st.all_time.tutor_ms);
            all_days.insert(day);
            if day == today {
                put(&mut st.today.reading_ms, &mut st.today.tutor_ms);
            }
            if day >= from {
                put(&mut st.period.reading_ms, &mut st.period.tutor_ms);
                period_days.insert(day);
            } else if previous_from.is_some_and(|p| day >= p) {
                put(&mut previous.reading_ms, &mut previous.tutor_ms);
                previous_days.insert(day);
            }
        }
    }
    st.period.days_studied = period_days.len() as i64;
    st.all_time.days_studied = all_days.len() as i64;
    st.today.days_studied = all_days.contains(&today) as i64;
    previous.days_studied = previous_days.len() as i64;
    st.previous = previous_from.map(|_| previous);

    // one entry per day
    let first_listed = from.min(today - 370).max(today - 3659);
    st.days = (first_listed..=today)
        .map(|d| {
            let mut s = by_day.remove(&d).unwrap_or_default();
            s.date = date_label(d);
            s
        })
        .collect();

    // ---------- streaks: any study on a day, whatever the filters ----------
    {
        let mut stmt = conn.prepare("SELECT DISTINCT (reviewed_at + ?1) / 86400 FROM review_log")?;
        let mut study_days: BTreeSet<i64> = stmt.query_map([off], |r| r.get(0))?.collect::<Result<_, _>>()?;
        study_days.extend(other_days);
        let (mut run, mut prev) = (0, None);
        for &d in &study_days {
            run = if prev == Some(d - 1) { run + 1 } else { 1 };
            st.best_streak = st.best_streak.max(run);
            prev = Some(d);
        }
        st.streak = match prev {
            Some(last) if last == today || last == today - 1 => run,
            _ => 0,
        };
    }

    // ---------- breakdowns ----------
    let mut dirs: Vec<DirStat> = by_dir.into_values().collect();
    dirs.sort_by(|a, b| b.reviews.cmp(&a.reviews).then(a.direction.cmp(&b.direction)));
    st.by_direction = dirs;

    st.hsk = HSK_LEVELS.iter().map(|l| HskStat { level: l.to_string(), ..Default::default() }).collect();
    for w in words.values() {
        if let Some(i) = w.hsk {
            st.hsk[i].words += 1;
            st.hsk[i].studied += w.studied as i64;
            st.hsk[i].mature += w.mature as i64;
        }
        for g in &w.groups {
            let deck = by_group.entry(*g).or_default();
            deck.words += 1;
            deck.studied += w.studied as i64;
            deck.mature += w.mature as i64;
        }
    }
    st.hsk.retain(|h| h.words > 0);
    let mut groups: Vec<DeckStat> = by_group
        .into_iter()
        .map(|(id, mut d)| {
            d.deck_id = id;
            d.name = deck_name.get(&id).cloned().unwrap_or_default();
            d
        })
        .collect();
    groups.sort_by(|a, b| decks::natural_cmp(&a.name, &b.name));
    st.by_deck = groups;

    // ---------- the words forgotten most ----------
    {
        let mut stmt = conn.prepare(&format!(
            "SELECT MIN(c.id), c.hanzi, c.pinyin, c.meaning, p.direction, MAX(p.lapses) AS lapses
             FROM progress p JOIN cards c ON c.id = p.card_id
             WHERE p.lapses > 0 AND {card_in} AND (?2 IS NULL OR p.direction = ?2)
             GROUP BY c.hanzi, p.direction
             ORDER BY lapses DESC, MAX(p.difficulty) DESC LIMIT 10"
        ))?;
        let mut p = deck_param();
        p.push(direction.map(|d| Value::Text(d.to_string())).unwrap_or(Value::Null));
        st.hardest = stmt
            .query_map(params_from_iter(p), |r| {
                Ok(HardCard {
                    card_id: r.get(0)?,
                    hanzi: r.get(1)?,
                    pinyin: r.get(2)?,
                    meaning: r.get(3)?,
                    direction: r.get(4)?,
                    lapses: r.get(5)?,
                })
            })?
            .collect::<Result<_, _>>()?;
    }
    Ok(st)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::study;

    fn all(days: i64) -> StatsFilter {
        StatsFilter { days: Some(days), ..Default::default() }
    }

    #[test]
    fn stats_after_reviews() {
        let mut conn = crate::db::open_in_memory().unwrap();
        let ids: Vec<i64> = crate::cards::list_cards(&conn, None, "").unwrap().iter().map(|r| r.card.id).collect();
        let t = now();
        study::answer_at(&mut conn, ids[0], "recognition", 4, 3000, t - DAY).unwrap();
        study::answer_at(&mut conn, ids[1], "recognition", 1, 2000, t).unwrap();
        study::answer_at(&mut conn, ids[1], "recognition", 3, 900_000, t).unwrap();

        let s = get_stats(&conn, &all(30)).unwrap();
        assert_eq!((s.total_cards, s.total_words, s.unstudied_cards), (12, 12, 10));
        assert_eq!((s.all_time.reviews, s.today.reviews, s.period.reviews), (3, 2, 3));
        assert_eq!(s.period.new_words, 2);
        assert_eq!(s.period.time_ms, 3000 + 2000 + ANSWER_CAP_MS, "a card left open counts for two minutes at most");
        assert_eq!(s.period.days_studied, 2);
        assert_eq!(s.days.len(), 371);
        assert_eq!(s.days.last().unwrap().reviews, 2);
        assert_eq!(s.streak, 2);
        assert_eq!(s.forecast.iter().sum::<i64>(), 2);
        assert_eq!(s.buttons[0], [1, 0, 1, 1], "all three were answers to words being learned");
        assert_eq!(s.hours.iter().map(|h| h.reviews).sum::<i64>(), 3);
        assert_eq!(s.weekdays.iter().map(|h| h.reviews).sum::<i64>(), 3);
        assert_eq!(s.by_direction.len(), 1);
        assert_eq!(s.studied_items, 2);
        assert!(s.previous.is_some_and(|p| p.reviews == 0));
        assert_eq!(s.by_deck.len(), 1, "the one top-level deck");
        assert_eq!((s.by_deck[0].words, s.by_deck[0].studied, s.by_deck[0].reviews), (12, 2, 3));

        // one day back only: yesterday's answer falls into the period before
        let s = get_stats(&conn, &all(1)).unwrap();
        assert_eq!((s.period.reviews, s.previous.unwrap().reviews), (2, 1));
        // all time, and a direction nothing was studied in
        let s = get_stats(&conn, &StatsFilter::default()).unwrap();
        assert_eq!((s.period.reviews, s.previous.is_none()), (3, true));
        let s = get_stats(&conn, &StatsFilter { direction: Some("recall".into()), ..Default::default() }).unwrap();
        assert_eq!((s.period.reviews, s.studied_items, s.unstudied_cards), (0, 0, 12));
        // what is due counts only the directions being studied
        conn.execute("UPDATE progress SET due = 0", []).unwrap();
        let due = |dirs: &[&str]| {
            let f = StatsFilter { due_directions: Some(dirs.iter().map(|d| d.to_string()).collect()), ..Default::default() };
            get_stats(&conn, &f).unwrap().due_now
        };
        assert_eq!((due(&[]), due(&["recognition"]), due(&["recall"])), (2, 2, 0));
    }

    #[test]
    fn decks_retention_and_other_time() {
        let mut conn = crate::db::open_in_memory().unwrap();
        let class = decks::create_deck(&conn, "Class", "", None).unwrap();
        let nine = decks::create_deck(&conn, "Class 9", "", Some(class)).unwrap();
        let other = decks::create_deck(&conn, "Other", "", None).unwrap();
        let add = |deck: i64, hanzi: &str| {
            let input = crate::models::CardInput {
                deck_id: deck,
                hanzi: hanzi.into(),
                pinyin: String::new(),
                meaning: "x".into(),
                notes: String::new(),
                tags: vec![],
            };
            crate::cards::create_card(&conn, &input).unwrap().id
        };
        let a = add(nine, "图书馆");
        let b = add(other, "图书馆"); // the same word in another deck
        let c = add(other, "银行");
        let t = now();
        // learned 40 days ago, reviewed since: by the last review its interval is long
        study::answer_at(&mut conn, a, "recognition", 4, 1000, t - 40 * DAY).unwrap();
        study::answer_at(&mut conn, a, "recognition", 4, 1000, t - 30 * DAY).unwrap();
        study::answer_at(&mut conn, b, "recognition", 3, 1000, t - DAY).unwrap();
        study::answer_at(&mut conn, b, "recognition", 1, 1000, t - DAY + 60).unwrap(); // same day: not counted again
        study::answer_at(&mut conn, c, "recognition", 3, 1000, t).unwrap();

        let s = get_stats(&conn, &all(7)).unwrap();
        let week = s.retention_table.iter().find(|r| r.range == "week").unwrap();
        assert_eq!(week.young[1] + week.mature[1], 1, "one review of a learned word counts, the day's first");
        assert_eq!(week.young[0] + week.mature[0], 1);
        assert_eq!((s.period.retention_of, s.period.retained), (1, 1));

        // the Class deck: the word's reviews count there though they were answered on its copy
        let s = get_stats(&conn, &StatsFilter { deck_id: Some(class), ..Default::default() }).unwrap();
        assert_eq!((s.total_words, s.all_time.reviews), (1, 4));
        assert_eq!(s.by_deck.iter().map(|d| (d.name.as_str(), d.words, d.studied)).collect::<Vec<_>>(), [("Class 9", 1, 1)]);
        assert!(!s.other_time_included);

        log_time(&conn, "reading", 90).unwrap();
        log_time(&conn, "reading", 30).unwrap();
        log_time(&conn, "tutor", 5000).unwrap(); // capped
        assert!(log_time(&conn, "napping", 60).is_err());
        let s = get_stats(&conn, &all(7)).unwrap();
        assert_eq!((s.today.reading_ms, s.today.tutor_ms), (120_000, 300_000));
        assert_eq!(s.days.last().unwrap().reading_ms, 120_000);
        let s = get_stats(&conn, &StatsFilter { deck_id: Some(class), ..Default::default() }).unwrap();
        assert_eq!(s.today.reading_ms, 0, "not when a deck is chosen");
    }
}
