//! FSRS-5 spaced-repetition scheduler.
//!
//! Implements the published FSRS-5 memory model (stability, difficulty,
//! retrievability) with the default parameters, plus short learning steps for
//! new and forgotten cards. Review intervals are aligned to local day starts so
//! a card due "in 1 day" becomes due at midnight rather than 24h later.

use crate::db::DAY;

/// Default FSRS-5 parameters (trained on a large public review dataset).
pub const W: [f64; 19] = [
    0.40255, 1.18385, 3.173, 15.69105, 7.1949, 0.5345, 1.4604, 0.0046, 1.54575, 0.1192, 1.01925,
    1.9395, 0.11, 0.29605, 2.2698, 0.2315, 2.9898, 0.51655, 0.6621,
];
const DECAY: f64 = -0.5;
const FACTOR: f64 = 19.0 / 81.0;
const MAX_DAYS: f64 = 36_500.0;

pub const LEARNING: i64 = 1;
pub const REVIEW: i64 = 2;
pub const RELEARNING: i64 = 3;

#[derive(Debug, Clone, PartialEq)]
pub struct Memory {
    pub state: i64,
    pub stability: f64,
    pub difficulty: f64,
    pub due: i64,
    pub last_review: i64,
    pub reps: i64,
    pub lapses: i64,
}

#[derive(Debug, Clone)]
pub struct Outcome {
    pub mem: Memory,
    /// Review interval in days, or `None` for a short learning step.
    pub days: Option<i64>,
}

pub fn retrievability(elapsed_days: f64, stability: f64) -> f64 {
    (1.0 + FACTOR * elapsed_days / stability).powf(DECAY)
}

fn init_stability(g: u8) -> f64 {
    W[(g - 1) as usize].max(0.1)
}

fn init_difficulty(g: u8) -> f64 {
    (W[4] - (W[5] * (g as f64 - 1.0)).exp() + 1.0).clamp(1.0, 10.0)
}

fn next_difficulty(d: f64, g: u8) -> f64 {
    let delta = -W[6] * (g as f64 - 3.0);
    let damped = d + delta * (10.0 - d) / 9.0;
    let reverted = W[7] * init_difficulty(4) + (1.0 - W[7]) * damped;
    reverted.clamp(1.0, 10.0)
}

fn recall_stability(d: f64, s: f64, r: f64, g: u8) -> f64 {
    let hard = if g == 2 { W[15] } else { 1.0 };
    let easy = if g == 4 { W[16] } else { 1.0 };
    s * (W[8].exp() * (11.0 - d) * s.powf(-W[9]) * ((W[10] * (1.0 - r)).exp() - 1.0) * hard * easy
        + 1.0)
}

fn forget_stability(d: f64, s: f64, r: f64) -> f64 {
    let sf = W[11] * d.powf(-W[12]) * ((s + 1.0).powf(W[13]) - 1.0) * (W[14] * (1.0 - r)).exp();
    sf.min(s)
}

/// Same-day review (FSRS-5 short-term stability).
fn short_term_stability(s: f64, g: u8) -> f64 {
    let next = s * (W[17] * (g as f64 - 3.0 + W[18])).exp();
    if g >= 3 {
        next.max(s)
    } else {
        next
    }
}

fn interval_days(stability: f64, retention: f64) -> i64 {
    let ivl = stability / FACTOR * (retention.powf(1.0 / DECAY) - 1.0);
    ivl.round().clamp(1.0, MAX_DAYS) as i64
}

fn clamp_s(s: f64) -> f64 {
    s.clamp(0.01, MAX_DAYS)
}

fn step(state: i64, secs: i64, s: f64, d: f64, now: i64, reps: i64, lapses: i64) -> Outcome {
    Outcome {
        mem: Memory {
            state,
            stability: clamp_s(s),
            difficulty: d,
            due: now + secs,
            last_review: now,
            reps,
            lapses,
        },
        days: None,
    }
}

fn review(s: f64, d: f64, now: i64, retention: f64, reps: i64, lapses: i64) -> Outcome {
    let s = clamp_s(s);
    Outcome {
        mem: Memory {
            state: REVIEW,
            stability: s,
            difficulty: d,
            due: now, // replaced in schedule_all once days are final
            last_review: now,
            reps,
            lapses,
        },
        days: Some(interval_days(s, retention)),
    }
}

fn schedule_one(mem: Option<&Memory>, g: u8, now: i64, retention: f64) -> Outcome {
    const MIN: i64 = 60;
    match mem {
        None => {
            let s = init_stability(g);
            let d = init_difficulty(g);
            match g {
                1 => step(LEARNING, MIN, s, d, now, 1, 0),
                2 => step(LEARNING, 6 * MIN, s, d, now, 1, 0),
                3 => step(LEARNING, 10 * MIN, s, d, now, 1, 0),
                _ => review(s, d, now, retention, 1, 0),
            }
        }
        Some(m) => {
            let elapsed = (now - m.last_review).max(0) as f64 / DAY as f64;
            let same_day = elapsed < 1.0;
            let r = retrievability(elapsed, m.stability);
            let d = next_difficulty(m.difficulty, g);
            let reps = m.reps + 1;
            let s = if same_day {
                short_term_stability(m.stability, g)
            } else if g == 1 {
                forget_stability(m.difficulty, m.stability, r)
            } else {
                recall_stability(m.difficulty, m.stability, r, g)
            };

            if m.state == REVIEW {
                if g == 1 {
                    step(RELEARNING, 10 * MIN, s, d, now, reps, m.lapses + 1)
                } else {
                    review(s, d, now, retention, reps, m.lapses)
                }
            } else {
                // learning or relearning
                match g {
                    1 => step(m.state, MIN, s, d, now, reps, m.lapses),
                    2 => step(m.state, 6 * MIN, s, d, now, reps, m.lapses),
                    _ => review(s, d, now, retention, reps, m.lapses),
                }
            }
        }
    }
}

/// Compute the outcome of each rating (1 = again, 2 = hard, 3 = good, 4 = easy).
/// `day_start` is the UTC timestamp of the current local midnight.
pub fn schedule_all(mem: Option<&Memory>, now: i64, day_start: i64, retention: f64) -> Vec<Outcome> {
    let mut out: Vec<Outcome> = (1..=4u8)
        .map(|g| schedule_one(mem, g, now, retention))
        .collect();

    // Keep review intervals ordered: hard <= good < easy.
    if let (Some(h), Some(g)) = (out[1].days, out[2].days) {
        if h > g {
            out[1].days = Some(g);
        }
    }
    if let (Some(h), Some(g)) = (out[1].days, out[2].days) {
        if g <= h {
            out[2].days = Some(h + 1);
        }
    }
    if let (Some(g), Some(e)) = (out[2].days, out[3].days) {
        if e <= g {
            out[3].days = Some(g + 1);
        }
    }
    for o in out.iter_mut() {
        if let Some(days) = o.days {
            o.mem.due = day_start + days * DAY;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000;

    #[test]
    fn new_card_outcomes() {
        let out = schedule_all(None, NOW, NOW - 3600, 0.9);
        assert_eq!(out[0].mem.state, LEARNING);
        assert_eq!(out[0].mem.due, NOW + 60);
        assert_eq!(out[2].mem.due, NOW + 600);
        assert_eq!(out[3].mem.state, REVIEW);
        assert_eq!(out[3].days, Some(16)); // S0(easy) = 15.69 days at 90% retention
    }

    #[test]
    fn graduating_and_growing() {
        let first = schedule_all(None, NOW, NOW, 0.9)[2].mem.clone();
        let later = NOW + 600;
        let grad = schedule_all(Some(&first), later, NOW, 0.9);
        assert_eq!(grad[2].mem.state, REVIEW);
        let good_days = grad[2].days.unwrap();
        assert!(good_days >= 3, "good after learning step: {good_days}");

        // Review it on time: interval should grow substantially.
        let mem = grad[2].mem.clone();
        let t = mem.due;
        let next = schedule_all(Some(&mem), t, t, 0.9);
        let (h, g, e) = (next[1].days.unwrap(), next[2].days.unwrap(), next[3].days.unwrap());
        assert!(h <= g && g < e, "ordering {h} {g} {e}");
        assert!(g > good_days * 2, "interval should grow: {good_days} -> {g}");

        // Forgetting moves to relearning and counts a lapse.
        assert_eq!(next[0].mem.state, RELEARNING);
        assert_eq!(next[0].mem.lapses, 1);
        assert!(next[0].mem.stability < mem.stability);
    }

    #[test]
    fn difficulty_moves_with_rating() {
        let d = init_difficulty(3);
        assert!(next_difficulty(d, 1) > d);
        assert!(next_difficulty(d, 4) < d);
    }
}
