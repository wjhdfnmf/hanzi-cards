//! A record of what the AI tutor has cost: one row per batch of Claude API calls, with
//! token counts and the estimated price, so spending can be reviewed in Settings.

use crate::ai::Usage;
use crate::error::AppResult;
use rusqlite::{params, Connection};
use serde::Serialize;

pub fn record(conn: &Connection, kind: &str, model: &str, session: Option<u64>, u: &Usage) -> AppResult<()> {
    conn.execute(
        "INSERT INTO ai_usage (at, kind, model, session_id, input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, cost)
         VALUES (strftime('%s','now'), ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            kind,
            model,
            session.map(|s| s as i64),
            u.input_tokens as i64,
            u.output_tokens as i64,
            u.cache_read_tokens as i64,
            u.cache_write_tokens as i64,
            u.cost(model)
        ],
    )?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    /// The tutor session (None for one-off calls such as translations outside a session).
    pub session_id: Option<i64>,
    pub started_at: i64,
    pub ended_at: i64,
    pub model: String,
    pub calls: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Spending {
    pub today: f64,
    pub last7_days: f64,
    pub this_month: f64,
    pub total: f64,
    pub conversations: i64,
    /// Most recent first.
    pub recent: Vec<Conversation>,
}

pub fn summary(conn: &Connection, limit: i64) -> AppResult<Spending> {
    let sum = |cond: &str| -> AppResult<f64> {
        Ok(conn.query_row(&format!("SELECT COALESCE(SUM(cost), 0) FROM ai_usage WHERE {cond}"), [], |r| r.get(0))?)
    };
    let today = sum("date(at, 'unixepoch', 'localtime') = date('now', 'localtime')")?;
    let last7_days = sum("at >= strftime('%s','now') - 7 * 86400")?;
    let this_month = sum("strftime('%Y-%m', at, 'unixepoch', 'localtime') = strftime('%Y-%m', 'now', 'localtime')")?;
    let total = sum("1")?;
    let conversations: i64 =
        conn.query_row("SELECT COUNT(DISTINCT session_id) FROM ai_usage WHERE session_id IS NOT NULL", [], |r| r.get(0))?;
    // One line per conversation; calls outside a conversation are one line each.
    let mut stmt = conn.prepare(
        "SELECT session_id, MIN(at), MAX(at), MAX(model), COUNT(*), SUM(input_tokens + cache_read_tokens + cache_write_tokens),
                SUM(output_tokens), SUM(cost)
         FROM ai_usage GROUP BY COALESCE(session_id, -id) ORDER BY MAX(at) DESC, MAX(id) DESC LIMIT ?1",
    )?;
    let recent = stmt
        .query_map([limit], |r| {
            Ok(Conversation {
                session_id: r.get(0)?,
                started_at: r.get(1)?,
                ended_at: r.get(2)?,
                model: r.get(3)?,
                calls: r.get(4)?,
                input_tokens: r.get(5)?,
                output_tokens: r.get(6)?,
                cost: r.get(7)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Spending { today, last7_days, this_month, total, conversations, recent })
}

/// Every row, as CSV (for a spreadsheet).
pub fn export_csv(conn: &Connection) -> AppResult<String> {
    let mut out = String::from("time,kind,model,conversation,input_tokens,output_tokens,cache_read_tokens,cache_write_tokens,cost_usd\n");
    let mut stmt = conn.prepare(
        "SELECT datetime(at, 'unixepoch', 'localtime'), kind, model, COALESCE(session_id, ''), input_tokens, output_tokens,
                cache_read_tokens, cache_write_tokens, cost FROM ai_usage ORDER BY at, id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(format!(
            "{},{},{},{},{},{},{},{},{:.6}\n",
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, rusqlite::types::Value>(3).map(|v| match v {
                rusqlite::types::Value::Integer(i) => i.to_string(),
                _ => String::new(),
            })?,
            r.get::<_, i64>(4)?,
            r.get::<_, i64>(5)?,
            r.get::<_, i64>(6)?,
            r.get::<_, i64>(7)?,
            r.get::<_, f64>(8)?
        ))
    })?;
    for row in rows {
        out.push_str(&row?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_and_sums() {
        let conn = crate::db::open_in_memory().unwrap();
        let u = Usage { input_tokens: 10_000, output_tokens: 1_000, cache_read_tokens: 0, cache_write_tokens: 0 };
        record(&conn, "tutor", "claude-opus-5-5", Some(7), &u).unwrap();
        record(&conn, "tutor", "claude-opus-5-5", Some(7), &u).unwrap();
        record(&conn, "translate", "claude-opus-5-5", Some(8), &u).unwrap();
        let s = summary(&conn, 10).unwrap();
        let one = u.cost("claude-opus-5-5");
        assert!((s.total - 3.0 * one).abs() < 1e-12);
        assert!((s.today - s.total).abs() < 1e-12);
        assert_eq!(s.conversations, 2);
        assert_eq!(s.recent.len(), 2);
        let seven = s.recent.iter().find(|c| c.session_id == Some(7)).unwrap();
        assert_eq!(seven.calls, 2);
        assert!(export_csv(&conn).unwrap().lines().count() == 4);
    }
}
