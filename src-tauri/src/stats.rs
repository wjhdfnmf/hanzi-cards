//! Study statistics.

use crate::db::{local_day, local_offset, now, DAY};
use crate::error::AppResult;
use rusqlite::Connection;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayStat {
    pub date: String,
    pub reviews: i64,
    pub correct: i64,
    pub new_cards: i64,
    pub time_ms: i64,
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub total_cards: i64,
    pub unstudied_cards: i64,
    pub learning: i64,
    pub young: i64,
    pub mature: i64,
    pub due_now: i64,
    pub total_reviews: i64,
    pub total_time_ms: i64,
    pub today_reviews: i64,
    pub today_correct: i64,
    pub today_time_ms: i64,
    /// Share of review-state answers not rated "again" over the last 30 days.
    pub retention: Option<f64>,
    pub streak: i64,
    pub best_streak: i64,
    pub days: Vec<DayStat>,
    /// Due counts for today (index 0, includes overdue) through the next 29 days.
    pub forecast: Vec<i64>,
    pub hardest: Vec<HardCard>,
}

fn date_label(day: i64) -> String {
    chrono::DateTime::from_timestamp(day * DAY, 0)
        .map(|d| d.date_naive().format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

fn count(conn: &Connection, sql: &str, p: impl rusqlite::Params) -> AppResult<i64> {
    Ok(conn.query_row(sql, p, |r| r.get(0))?)
}

pub fn get_stats(conn: &Connection, days: i64) -> AppResult<Stats> {
    let t = now();
    let off = local_offset();
    let today = local_day(t, off);
    let days = days.clamp(7, 3660);

    let total_cards = count(conn, "SELECT COUNT(*) FROM cards", [])?;
    // Progress is counted by word and direction: copies of a word in several decks share
    // their progress (one row each), and are one thing to study.
    const STUDIED: &str = "SELECT DISTINCT c.hanzi, p.direction FROM progress p JOIN cards c ON c.id = p.card_id";
    let studied = |cond: &str| format!("SELECT COUNT(*) FROM ({STUDIED} WHERE {cond})");
    let unstudied_cards = count(
        conn,
        "SELECT COUNT(DISTINCT c.hanzi) FROM cards c WHERE NOT EXISTS (SELECT 1 FROM progress p WHERE p.card_id = c.id)",
        [],
    )?;
    let learning = count(conn, &studied("p.state IN (1, 3)"), [])?;
    let young = count(conn, &studied("p.state = 2 AND p.stability < 21"), [])?;
    let mature = count(conn, &studied("p.state = 2 AND p.stability >= 21"), [])?;
    let due_now = count(conn, &studied("p.due <= ?1"), [t])?;

    let (total_reviews, total_time_ms): (i64, i64) = conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(duration_ms), 0) FROM review_log",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;

    // Per-day buckets for the requested window.
    let first_day = today - days + 1;
    let mut by_day: HashMap<i64, DayStat> = HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT (reviewed_at + ?1) / 86400 AS day, COUNT(*), SUM(rating > 1),
                    SUM(state_before = 0), SUM(duration_ms)
             FROM review_log WHERE reviewed_at >= ?2 GROUP BY day",
        )?;
        let rows = stmt.query_map([off, first_day * DAY - off], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                DayStat {
                    date: String::new(),
                    reviews: r.get(1)?,
                    correct: r.get(2)?,
                    new_cards: r.get(3)?,
                    time_ms: r.get(4)?,
                },
            ))
        })?;
        for row in rows {
            let (d, s) = row?;
            by_day.insert(d, s);
        }
    }
    let day_stats: Vec<DayStat> = (first_day..=today)
        .map(|d| {
            let mut s = by_day.remove(&d).unwrap_or(DayStat {
                date: String::new(),
                reviews: 0,
                correct: 0,
                new_cards: 0,
                time_ms: 0,
            });
            s.date = date_label(d);
            s
        })
        .collect();
    let today_stat = day_stats.last().cloned();

    // Streaks over all distinct study days.
    let mut stmt = conn.prepare(
        "SELECT DISTINCT (reviewed_at + ?1) / 86400 AS day FROM review_log ORDER BY day",
    )?;
    let study_days: Vec<i64> = stmt
        .query_map([off], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    let mut best_streak = 0;
    let mut run = 0;
    let mut prev: Option<i64> = None;
    for &d in &study_days {
        run = if prev == Some(d - 1) { run + 1 } else { 1 };
        best_streak = best_streak.max(run);
        prev = Some(d);
    }
    let streak = match prev {
        Some(last) if last == today || last == today - 1 => run,
        _ => 0,
    };

    let (ret_total, ret_ok): (i64, i64) = conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(rating > 1), 0) FROM review_log
         WHERE state_before = 2 AND reviewed_at >= ?1",
        [t - 30 * DAY],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let retention = (ret_total > 0).then(|| ret_ok as f64 / ret_total as f64);

    let mut forecast = vec![0i64; 30];
    {
        let mut stmt = conn.prepare(
            "SELECT day, COUNT(*) FROM (
               SELECT DISTINCT c.hanzi, p.direction, (p.due + ?1) / 86400 - ?2 AS day
               FROM progress p JOIN cards c ON c.id = p.card_id)
             GROUP BY day",
        )?;
        let rows = stmt.query_map([off, today], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?;
        for row in rows {
            let (d, n) = row?;
            let idx = d.max(0);
            if idx < 30 {
                forecast[idx as usize] += n;
            }
        }
    }

    let mut stmt = conn.prepare(
        "SELECT MIN(c.id), c.hanzi, c.pinyin, c.meaning, p.direction, MAX(p.lapses) AS lapses
         FROM progress p JOIN cards c ON c.id = p.card_id
         WHERE p.lapses > 0 GROUP BY c.hanzi, p.direction
         ORDER BY lapses DESC, MAX(p.difficulty) DESC LIMIT 10",
    )?;
    let hardest = stmt
        .query_map([], |r| {
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

    Ok(Stats {
        total_cards,
        unstudied_cards,
        learning,
        young,
        mature,
        due_now,
        total_reviews,
        total_time_ms,
        today_reviews: today_stat.as_ref().map(|s| s.reviews).unwrap_or(0),
        today_correct: today_stat.as_ref().map(|s| s.correct).unwrap_or(0),
        today_time_ms: today_stat.as_ref().map(|s| s.time_ms).unwrap_or(0),
        retention,
        streak,
        best_streak,
        days: day_stats,
        forecast,
        hardest,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::study;

    #[test]
    fn stats_after_reviews() {
        let mut conn = crate::db::open_in_memory().unwrap();
        let ids: Vec<i64> = crate::cards::list_cards(&conn, None, "")
            .unwrap()
            .iter()
            .map(|r| r.card.id)
            .collect();
        let t = now();
        study::answer_at(&mut conn, ids[0], "recognition", 4, 3000, t - DAY).unwrap();
        study::answer_at(&mut conn, ids[1], "recognition", 1, 2000, t).unwrap();
        study::answer_at(&mut conn, ids[1], "recognition", 3, 2000, t).unwrap();

        let s = get_stats(&conn, 30).unwrap();
        assert_eq!(s.total_cards, 12);
        assert_eq!(s.unstudied_cards, 10);
        assert_eq!(s.total_reviews, 3);
        assert_eq!(s.today_reviews, 2);
        assert_eq!(s.days.len(), 30);
        assert_eq!(s.streak, 2);
        assert_eq!(s.forecast.iter().sum::<i64>(), 2);
    }
}
