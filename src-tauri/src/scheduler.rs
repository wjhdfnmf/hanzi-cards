//! FSRS-6 spaced-repetition scheduler.
//!
//! Implements the published FSRS-6 memory model (stability, difficulty, retrievability, with a
//! trainable forgetting-curve decay) with its default parameters, as in Anki 25.07+ and py-fsrs.
//! Review intervals get a little "fuzz" so cards learned together don't all come due on the
//! same day, and are aligned to local day starts: a card due "in 1 day" is due at midnight.
//!
//! Learning follows "successive relearning" (Rawson & Dunlosky): a new word is recalled
//! correctly 3 times on its first day, at growing gaps, before it graduates; a forgotten word
//! once, then it's back on the normal spacing.

use crate::db::DAY;

/// Default FSRS-6 parameters (py-fsrs / fsrs-rs).
pub const W: [f64; 21] = [
    0.212, 1.2931, 2.3065, 8.2956, 6.4133, 0.8334, 3.0194, 0.001, 1.8722, 0.1666, 0.796, 1.4835,
    0.0614, 0.2629, 1.6483, 0.6014, 1.8729, 0.5425, 0.0912, 0.0658, 0.1542,
];
const MAX_DAYS: f64 = 36_500.0;

fn decay() -> f64 {
    -W[20]
}
/// Chosen so that retrievability is 90% when elapsed time equals stability.
fn factor() -> f64 {
    0.9f64.powf(1.0 / decay()) - 1.0
}

pub const LEARNING: i64 = 1;
pub const REVIEW: i64 = 2;
pub const RELEARNING: i64 = 3;

const MIN: i64 = 60;

/// How studying is scheduled (Settings › Studying › Advanced). `Default` is the
/// research-based setup.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// Target retention: the chance of remembering a word when it comes up.
    pub retention: f64,
    /// Correct recalls a new word needs on its first day before it graduates.
    pub new_recalls: i64,
    /// Correct recalls a forgotten word needs before it's back on the normal spacing.
    pub relearn_recalls: i64,
    /// Seconds before a word missed while learning comes back.
    pub again_secs: i64,
    /// Seconds before the next recall after the 1st, 2nd … correct one (the last repeats).
    pub gaps_secs: Vec<i64>,
    /// Seconds before a forgotten review word first comes back.
    pub relearn_secs: i64,
    /// Spread review intervals a little so words learned together don't all come due together.
    pub fuzz: bool,
    /// Longest review interval, in days.
    pub max_days: i64,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            retention: 0.9,
            new_recalls: 3,
            relearn_recalls: 1,
            again_secs: MIN,
            gaps_secs: vec![5 * MIN, 15 * MIN],
            relearn_secs: 10 * MIN,
            fuzz: true,
            max_days: MAX_DAYS as i64,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Memory {
    pub state: i64,
    pub stability: f64,
    pub difficulty: f64,
    pub due: i64,
    pub last_review: i64,
    pub reps: i64,
    pub lapses: i64,
    /// Correct recalls so far in the current (re)learning phase (0 in review).
    pub step: i64,
}

#[derive(Debug, Clone)]
pub struct Outcome {
    pub mem: Memory,
    /// Review interval in days, or `None` for a short learning step.
    pub days: Option<i64>,
}

pub fn retrievability(elapsed_days: f64, stability: f64) -> f64 {
    (1.0 + factor() * elapsed_days / stability).powf(decay())
}

fn init_stability(g: u8) -> f64 {
    W[(g - 1) as usize].max(0.1)
}

fn init_difficulty(g: u8) -> f64 {
    W[4] - (W[5] * (g as f64 - 1.0)).exp() + 1.0
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
    let long_term = W[11] * d.powf(-W[12]) * ((s + 1.0).powf(W[13]) - 1.0) * (W[14] * (1.0 - r)).exp();
    let short_term = s / (W[17] * W[18]).exp();
    long_term.min(short_term)
}

/// Same-day review (FSRS-6 short-term stability: grows less the more stable a card already is).
fn short_term_stability(s: f64, g: u8) -> f64 {
    let mut inc = (W[17] * (g as f64 - 3.0 + W[18])).exp() * s.powf(-W[19]);
    if g >= 2 {
        inc = inc.max(1.0);
    }
    s * inc
}

fn interval_days(stability: f64, retention: f64, max_days: i64) -> i64 {
    let ivl = stability / factor() * (retention.powf(1.0 / decay()) - 1.0);
    ivl.round().clamp(1.0, max_days.clamp(1, MAX_DAYS as i64) as f64) as i64
}

/// FSRS fuzz: a review interval moves by up to ±15% (2.5–7 days), ±10% (7–20) and ±5% beyond,
/// so cards learned together spread out. `u` in [0, 1) picks the day.
fn fuzz(days: i64, u: f64) -> i64 {
    if (days as f64) < 2.5 {
        return days;
    }
    const RANGES: [(f64, f64, f64); 3] = [(2.5, 7.0, 0.15), (7.0, 20.0, 0.1), (20.0, f64::INFINITY, 0.05)];
    let ivl = days as f64;
    let delta = 1.0 + RANGES.iter().map(|(start, end, f)| f * (ivl.min(*end) - start).max(0.0)).sum::<f64>();
    let lo = ((ivl - delta).round() as i64).max(2);
    let hi = ((ivl + delta).round() as i64).max(lo);
    (lo + (u * (hi - lo + 1) as f64).floor() as i64).min(hi).min(MAX_DAYS as i64)
}

/// A number in [0, 1) from a seed (splitmix64), so a card's fuzz is the same when the buttons
/// are previewed and when it's answered.
fn unit(seed: u64) -> f64 {
    let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 11) as f64 / (1u64 << 53) as f64
}

fn clamp_s(s: f64) -> f64 {
    s.clamp(0.01, MAX_DAYS)
}

#[allow(clippy::too_many_arguments)]
fn step(state: i64, secs: i64, s: f64, d: f64, now: i64, reps: i64, lapses: i64, recalls: i64) -> Outcome {
    Outcome {
        mem: Memory {
            state,
            stability: clamp_s(s),
            difficulty: d.clamp(1.0, 10.0),
            due: now + secs,
            last_review: now,
            reps,
            lapses,
            step: recalls,
        },
        days: None,
    }
}

fn review(s: f64, d: f64, now: i64, cfg: &Config, reps: i64, lapses: i64) -> Outcome {
    let s = clamp_s(s);
    Outcome {
        mem: Memory {
            state: REVIEW,
            stability: s,
            difficulty: d.clamp(1.0, 10.0),
            due: now, // replaced in schedule_all once days are final
            last_review: now,
            reps,
            lapses,
            step: 0,
        },
        days: Some(interval_days(s, cfg.retention, cfg.max_days)),
    }
}

/// A correct recall while learning: graduate after `needed` of them, else wait for the next.
#[allow(clippy::too_many_arguments)]
fn learned(state: i64, recalls: i64, needed: i64, s: f64, d: f64, now: i64, cfg: &Config, reps: i64, lapses: i64) -> Outcome {
    if recalls >= needed {
        review(s, d, now, cfg, reps, lapses)
    } else {
        let i = ((recalls - 1).max(0) as usize).min(cfg.gaps_secs.len().saturating_sub(1));
        let gap = cfg.gaps_secs.get(i).copied().unwrap_or(10 * MIN);
        step(state, gap, s, d, now, reps, lapses, recalls)
    }
}

fn schedule_one(mem: Option<&Memory>, g: u8, now: i64, cfg: &Config) -> Outcome {
    match mem {
        None => {
            let s = init_stability(g);
            let d = init_difficulty(g);
            match g {
                1 => step(LEARNING, cfg.again_secs, s, d, now, 1, 0, 0),
                4 => review(s, d, now, cfg, 1, 0),
                _ => learned(LEARNING, 1, cfg.new_recalls, s, d, now, cfg, 1, 0),
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
                    step(RELEARNING, cfg.relearn_secs, s, d, now, reps, m.lapses + 1, 0)
                } else {
                    review(s, d, now, cfg, reps, m.lapses)
                }
            } else {
                // learning or relearning. Hard is a pass (recalled, with effort) and counts as a
                // correct recall like Good; Again keeps the recalls made so far.
                let needed = if m.state == LEARNING { cfg.new_recalls } else { cfg.relearn_recalls };
                match g {
                    1 => step(m.state, cfg.again_secs, s, d, now, reps, m.lapses, m.step),
                    4 => review(s, d, now, cfg, reps, m.lapses),
                    _ => learned(m.state, m.step + 1, needed, s, d, now, cfg, reps, m.lapses),
                }
            }
        }
    }
}

/// Compute the outcome of each rating (1 = again, 2 = hard, 3 = good, 4 = easy).
/// `day_start` is the UTC timestamp of the current local midnight; `seed` picks this card's
/// fuzz (the same seed gives the same intervals).
pub fn schedule_all(mem: Option<&Memory>, now: i64, day_start: i64, cfg: &Config, seed: u64) -> Vec<Outcome> {
    let mut out: Vec<Outcome> = (1..=4u8).map(|g| schedule_one(mem, g, now, cfg)).collect();
    if cfg.fuzz {
        for (i, o) in out.iter_mut().enumerate() {
            if let Some(days) = o.days {
                o.days = Some(fuzz(days, unit(seed.wrapping_add(i as u64))).min(cfg.max_days.max(1)));
            }
        }
    }

    // Keep review intervals ordered: hard <= good < easy.
    if let (Some(h), Some(g)) = (out[1].days, out[2].days) {
        if h > g {
            out[1].days = Some(g);
        }
    }
    if let (Some(h), Some(g)) = (out[1].days, out[2].days) {
        if g < h {
            out[2].days = Some(h);
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

    fn all(mem: Option<&Memory>, now: i64) -> Vec<Outcome> {
        schedule_all(mem, now, now - 3600, &Config::default(), 7)
    }

    #[test]
    fn retrievability_is_90_percent_at_stability() {
        assert!((retrievability(10.0, 10.0) - 0.9).abs() < 1e-9);
        assert_eq!(interval_days(10.0, 0.9, 36_500), 10);
        assert_eq!(interval_days(10.0, 0.9, 7), 7, "maximum interval");
    }

    #[test]
    fn new_card_outcomes() {
        let out = all(None, NOW);
        assert_eq!(out[0].mem.state, LEARNING);
        assert_eq!(out[0].mem.due, NOW + 60);
        assert_eq!((out[2].mem.state, out[2].mem.step), (LEARNING, 1));
        assert_eq!(out[2].mem.due, NOW + 5 * 60);
        assert_eq!(out[3].mem.state, REVIEW);
        let easy = out[3].days.unwrap();
        assert!((6..=10).contains(&easy), "S0(easy) = 8.3 days, fuzzed ±1.8: {easy}");
    }

    #[test]
    fn new_words_need_three_correct_recalls() {
        let mut t = NOW;
        let mut m = all(None, t)[2].mem.clone(); // 1st correct
        t += 5 * 60;
        m = all(Some(&m), t)[2].mem.clone(); // 2nd
        assert_eq!((m.state, m.step, m.due - t), (LEARNING, 2, 15 * 60));
        // a wrong answer keeps the recalls made so far
        t += 15 * 60;
        let missed = all(Some(&m), t)[0].mem.clone();
        assert_eq!((missed.state, missed.step, missed.due - t), (LEARNING, 2, 60));
        t += 60;
        let third = all(Some(&missed), t);
        assert_eq!(third[2].mem.state, REVIEW, "3rd correct recall graduates");
        assert_eq!(third[1].mem.state, REVIEW, "hard counts as a correct recall");
        assert!(third[2].days.unwrap() >= 1);
    }

    #[test]
    fn graduating_and_growing() {
        let mut t = NOW;
        let mut m = all(None, t)[2].mem.clone();
        for gap in [5 * 60, 15 * 60] {
            t += gap;
            let out = all(Some(&m), t);
            m = out[2].mem.clone();
        }
        assert_eq!(m.state, REVIEW);

        // Review it on time: interval should grow substantially.
        let t = m.due;
        let next = schedule_all(Some(&m), t, t, &Config::default(), 3);
        let (h, g, e) = (next[1].days.unwrap(), next[2].days.unwrap(), next[3].days.unwrap());
        assert!(h <= g && g < e, "ordering {h} {g} {e}");
        assert!(g >= 4, "interval should grow: {g}");

        // Forgetting moves to relearning and counts a lapse; one correct recall graduates.
        assert_eq!(next[0].mem.state, RELEARNING);
        assert_eq!(next[0].mem.lapses, 1);
        assert!(next[0].mem.stability < m.stability);
        let back = schedule_all(Some(&next[0].mem), t + 600, t, &Config::default(), 3);
        assert_eq!(back[2].mem.state, REVIEW);
    }

    #[test]
    fn fuzz_spreads_but_stays_close() {
        assert_eq!(fuzz(2, 0.99), 2, "no fuzz under 2.5 days");
        let seen: std::collections::BTreeSet<i64> = (0..200).map(|i| fuzz(10, unit(i))).collect();
        assert!(seen.len() >= 3, "{seen:?}");
        assert!(seen.iter().all(|d| (8..=12).contains(d)), "{seen:?}");
        assert!((0..200).map(|i| fuzz(100, unit(i))).all(|d| (93..=107).contains(&d)));
        // the same seed: the same intervals (preview matches the answer)
        let m = all(None, NOW)[3].mem.clone();
        let a = schedule_all(Some(&m), m.due, m.due, &Config::default(), 42);
        let b = schedule_all(Some(&m), m.due, m.due, &Config::default(), 42);
        assert_eq!(a[2].days, b[2].days);
        let off = Config { fuzz: false, ..Config::default() };
        let plain: std::collections::BTreeSet<Option<i64>> =
            (0..20).map(|s| schedule_all(Some(&m), m.due, m.due, &off, s)[2].days).collect();
        assert_eq!(plain.len(), 1, "no fuzz: the same interval for every seed");
    }

    #[test]
    fn custom_learning() {
        let cfg = Config { new_recalls: 1, again_secs: 120, ..Config::default() };
        let out = schedule_all(None, NOW, NOW, &cfg, 1);
        assert_eq!(out[0].mem.due, NOW + 120);
        assert_eq!(out[2].mem.state, REVIEW, "one recall is enough");
        let cfg = Config { gaps_secs: vec![60], new_recalls: 4, ..Config::default() };
        let mut m = schedule_all(None, NOW, NOW, &cfg, 1)[2].mem.clone();
        for _ in 0..2 {
            m = schedule_all(Some(&m), m.due, NOW, &cfg, 1)[2].mem.clone();
            assert_eq!((m.state, m.due - m.last_review), (LEARNING, 60), "the last gap repeats");
        }
        assert_eq!(schedule_all(Some(&m), m.due, NOW, &cfg, 1)[2].mem.state, REVIEW);
    }

    #[test]
    fn difficulty_moves_with_rating() {
        let d = init_difficulty(3);
        assert!(next_difficulty(d, 1) > d);
        assert!(next_difficulty(d, 4) < d);
    }
}
