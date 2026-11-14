//! Tone checking from pitch. Used for spoken answers by tone_check.rs ("pitch_tones" setting).
//!
//! 1. Track the pitch (F0) of the recording with YIN, 10 ms frames.
//! 2. Split the voiced part into the expected number of syllables (at loudness dips).
//! 3. Describe each syllable's pitch as 5 points in the speaker's own range and pick the
//!    nearest of the four tone shapes (Chao's 55, 35, 214/21, 51).
//!
//! The speaker's range (median and spread of their pitch in semitones) comes from earlier
//! recordings; a learner's voice can't be judged high or low from one syllable alone.
//!
//! See docs/tone-recognition.md for what was measured and the plan.
#![allow(dead_code)] // some helpers (classify, the tuning knobs) are only used by the benches

pub const SAMPLE_RATE: usize = 16000;
const HOP: usize = 160; // 10 ms
const WIN: usize = 640; // 40 ms
const F0_MIN: f32 = 70.0;
const F0_MAX: f32 = 500.0;

#[derive(Debug, Clone, Copy)]
pub struct Frame {
    /// Pitch in semitones above 100 Hz, if voiced.
    pub st: Option<f32>,
    pub rms: f32,
}

fn semitones(f0: f32) -> f32 {
    12.0 * (f0 / 100.0).log2()
}

/// YIN pitch tracking (de Cheveigné & Kawahara 2002), cleaned up. Mains hum is removed first.
pub fn pitch_track(pcm: &[f32]) -> Vec<Frame> {
    pitch_track_hum(pcm).0
}

/// `pitch_track`, also returning the hum frequencies it removed.
pub fn pitch_track_hum(pcm: &[f32]) -> (Vec<Frame>, Vec<f32>) {
    let (pcm, hum) = remove_hum(pcm);
    let mut frames = pitch_raw(&pcm);
    clean(&mut frames);
    (frames, hum)
}

/// Power of `x` (Hann-windowed) at frequency `f` (Goertzel).
fn power_at(x: &[f32], f: f32) -> f32 {
    let n = x.len() as f32;
    let w = 2.0 * (2.0 * std::f32::consts::PI * f / SAMPLE_RATE as f32).cos();
    let (mut s1, mut s2) = (0f32, 0f32);
    for (i, &v) in x.iter().enumerate() {
        let hann = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / n).cos();
        let s0 = v * hann + w * s1 - s2;
        (s2, s1) = (s1, s0);
    }
    (s1 * s1 + s2 * s2 - w * s1 * s2).max(0.0)
}

/// Mains hum: a microphone near mains wiring picks up a steady 50 or 60 Hz and its
/// harmonics. At 100/120 Hz it sits right in a low voice and the pitch tracker locks onto it
/// in pauses and loses the voice where the two disagree. (Measured on one learner's raw
/// microphone: a 120 Hz tone with a third of the energy below 400 Hz in every quiet stretch.)
///
/// Finds the hum's harmonics in the quietest half second and notches them out (3 Hz wide;
/// a voice glides through so quickly that it barely loses anything). Returns the filtered
/// samples and the frequencies removed. `HUM=0` turns it off.
pub fn remove_hum(pcm: &[f32]) -> (Vec<f32>, Vec<f32>) {
    let found = find_hum(pcm);
    if found.is_empty() {
        return (pcm.to_vec(), found);
    }
    let mut out = pcm.to_vec();
    for &f in &found {
        let w0 = 2.0 * std::f32::consts::PI * f / SAMPLE_RATE as f32;
        let q = f / 3.0;
        let alpha = w0.sin() / (2.0 * q);
        let cos = w0.cos();
        let a0 = 1.0 + alpha;
        let (b0, b1, b2, a1, a2) = (1.0 / a0, -2.0 * cos / a0, 1.0 / a0, -2.0 * cos / a0, (1.0 - alpha) / a0);
        let (mut x1, mut x2, mut y1, mut y2) = (0f32, 0f32, 0f32, 0f32);
        for v in out.iter_mut() {
            let y = b0 * *v + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2;
            (x2, x1, y2, y1) = (x1, *v, y1, y);
            *v = y;
        }
    }
    (out, found)
}

/// The hum's harmonics (Hz) in the quietest half second of the recording, or none.
pub fn find_hum(pcm: &[f32]) -> Vec<f32> {
    if std::env::var("HUM").is_ok_and(|v| v == "0") {
        return Vec::new();
    }
    const LEN: usize = SAMPLE_RATE / 2;
    const STEP: usize = SAMPLE_RATE / 20;
    if pcm.len() < LEN + STEP {
        return Vec::new();
    }
    let energy = |s: usize| pcm[s..s + LEN].iter().map(|v| v * v).sum::<f32>();
    let quiet = (0..=(pcm.len() - LEN) / STEP).map(|k| k * STEP).min_by(|&a, &b| energy(a).total_cmp(&energy(b))).unwrap();
    let seg = &pcm[quiet..quiet + LEN];
    // the quiet stretch must really be quieter than the speech, or it's voice, not hum
    let loudest = (0..=(pcm.len() - LEN) / STEP).map(|k| energy(k * STEP)).fold(0f32, f32::max);
    if energy(quiet) > loudest * 0.25 {
        return Vec::new();
    }
    // the band the pitch tracker cares about, in 2 Hz steps
    let band: Vec<(f32, f32)> = (20..=250).map(|k| (k as f32 * 2.0, power_at(seg, k as f32 * 2.0))).collect();
    let total: f32 = band.iter().map(|b| b.1).sum();
    if total <= 0.0 {
        return Vec::new();
    }
    // try both mains frequencies; keep the series with more hum power
    let mut best: (f32, Vec<f32>) = (0.0, Vec::new());
    for mains in [50.0f32, 60.0] {
        let (mut sum, mut found) = (0f32, Vec::new());
        for k in 1..=8 {
            // the exact peak near k × mains: mains drifts by at most about 0.2%
            let reach = (k as f32 * mains * 0.002).max(0.1);
            let (f, p) = (-5..=5)
                .map(|d| k as f32 * mains + d as f32 * reach / 5.0)
                .map(|f| (f, power_at(seg, f)))
                .fold((0.0, 0.0), |a, b| if b.1 > a.1 { b } else { a });
            // a hum harmonic: a real share of the band and far above its neighbourhood
            let mut around: Vec<f32> = [-12.0, -9.0, -6.0, 6.0, 9.0, 12.0].iter().map(|d| power_at(seg, f + d)).collect();
            around.sort_by(f32::total_cmp);
            if p >= total * 0.02 && p > around[2] * 20.0 {
                found.push(f);
                sum += p;
            }
        }
        if sum > best.0 {
            best = (sum, found);
        }
    }
    best.1
}

/// Second-order Butterworth low-pass (RBJ biquad): keeps the fundamental, drops the
/// upper harmonics and hiss that confuse the period search.
fn low_pass(pcm: &[f32], cutoff: f32) -> Vec<f32> {
    let w0 = 2.0 * std::f32::consts::PI * cutoff / SAMPLE_RATE as f32;
    let alpha = w0.sin() / (2.0 * std::f32::consts::FRAC_1_SQRT_2);
    let cos = w0.cos();
    let a0 = 1.0 + alpha;
    let (b0, b1, b2) = ((1.0 - cos) / 2.0 / a0, (1.0 - cos) / a0, (1.0 - cos) / 2.0 / a0);
    let (a1, a2) = (-2.0 * cos / a0, (1.0 - alpha) / a0);
    let (mut x1, mut x2, mut y1, mut y2) = (0f32, 0f32, 0f32, 0f32);
    pcm.iter()
        .map(|&x| {
            let y = b0 * x + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2;
            (x2, x1, y2, y1) = (x1, x, y1, y);
            y
        })
        .collect()
}

fn yin_threshold() -> (f32, f32) {
    let first = std::env::var("YIN_FIRST").ok().and_then(|v| v.parse().ok()).unwrap_or(0.25);
    let global = std::env::var("YIN_GLOBAL").ok().and_then(|v| v.parse().ok()).unwrap_or(0.45);
    (first, global)
}

pub(crate) fn pitch_raw(pcm: &[f32]) -> Vec<Frame> {
    let cutoff = std::env::var("YIN_LOWPASS").ok().and_then(|v| v.parse().ok()).unwrap_or(1000.0f32);
    let filtered = if cutoff > 0.0 { low_pass(pcm, cutoff) } else { pcm.to_vec() };
    let (first, global) = yin_threshold();
    let pcm = &filtered[..];
    let tau_min = (SAMPLE_RATE as f32 / F0_MAX) as usize;
    let tau_max = (SAMPLE_RATE as f32 / F0_MIN) as usize;
    let mut frames = Vec::new();
    let mut d = vec![0f32; tau_max + 1];
    // loudness first: quiet frames are dropped by clean() anyway, so skip their pitch search
    let rms_at = |start: usize| (pcm[start..start + WIN].iter().map(|v| v * v).sum::<f32>() / WIN as f32).sqrt();
    let loudest = (0..).map(|k| k * HOP).take_while(|s| s + WIN + tau_max <= pcm.len()).map(rms_at).fold(0f32, f32::max);
    let mut start = 0;
    while start + WIN + tau_max <= pcm.len() {
        let x = &pcm[start..start + WIN + tau_max];
        let rms = rms_at(start);
        if rms < loudest * 0.08 {
            frames.push(Frame { st: None, rms });
            start += HOP;
            continue;
        }
        for (tau, dt) in d.iter_mut().enumerate().skip(1) {
            let mut s = 0f32;
            for j in 0..WIN {
                let diff = x[j] - x[j + tau];
                s += diff * diff;
            }
            *dt = s;
        }
        // cumulative mean normalised difference
        let mut cmnd = vec![1f32; tau_max + 1];
        let mut running = 0f32;
        for tau in 1..=tau_max {
            running += d[tau];
            cmnd[tau] = if running > 0.0 { d[tau] * tau as f32 / running } else { 1.0 };
        }
        let mut pick = None;
        let mut tau = tau_min;
        while tau <= tau_max {
            if cmnd[tau] < first {
                while tau < tau_max && cmnd[tau + 1] < cmnd[tau] {
                    tau += 1;
                }
                pick = Some(tau);
                break;
            }
            tau += 1;
        }
        if pick.is_none() {
            let (best, val) = (tau_min..=tau_max).map(|t| (t, cmnd[t])).fold((0, f32::MAX), |a, b| if b.1 < a.1 { b } else { a });
            if val < global {
                pick = Some(best);
            }
        }
        let st = pick.map(|t| {
            // parabolic interpolation around the minimum
            let t_f = if t > 1 && t < tau_max {
                let (a, b, c) = (cmnd[t - 1], cmnd[t], cmnd[t + 1]);
                let den = a - 2.0 * b + c;
                if den.abs() > 1e-9 { t as f32 + 0.5 * (a - c) / den } else { t as f32 }
            } else {
                t as f32
            };
            semitones(SAMPLE_RATE as f32 / t_f)
        });
        frames.push(Frame { st, rms });
        start += HOP;
    }
    frames
}

/// Drop quiet frames, fix octave jumps, median-smooth, remove blips shorter than 50 ms.
fn clean(frames: &mut [Frame]) {
    let loud = frames.iter().map(|f| f.rms).fold(0f32, f32::max);
    for f in frames.iter_mut() {
        if f.rms < loud * 0.08 {
            f.st = None;
        }
    }
    let mut voiced: Vec<f32> = frames.iter().filter_map(|f| f.st).collect();
    if voiced.is_empty() {
        return;
    }
    voiced.sort_by(f32::total_cmp);
    let med = voiced[voiced.len() / 2];
    let env = |k: &str, d: f32| std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
    let (jump, drop) = (env("OCT_JUMP", 8.0), env("OCT_DROP", 10.0));
    for f in frames.iter_mut() {
        if let Some(s) = f.st.as_mut() {
            if *s - med > jump {
                *s -= 12.0;
            } else if med - *s > jump {
                *s += 12.0;
            }
            if (*s - med).abs() > drop {
                f.st = None;
            }
        }
    }
    let orig: Vec<Option<f32>> = frames.iter().map(|f| f.st).collect();
    for i in 0..frames.len() {
        if orig[i].is_none() {
            continue;
        }
        let mut w: Vec<f32> = (i.saturating_sub(2)..(i + 3).min(orig.len())).filter_map(|j| orig[j]).collect();
        w.sort_by(f32::total_cmp);
        frames[i].st = Some(w[w.len() / 2]);
    }
    let mut i = 0;
    while i < frames.len() {
        if frames[i].st.is_some() {
            let s = i;
            while i < frames.len() && frames[i].st.is_some() {
                i += 1;
            }
            if i - s < 5 {
                for f in &mut frames[s..i] {
                    f.st = None;
                }
            }
        } else {
            i += 1;
        }
    }
}

/// Split the voice into `n` syllables: (start, end) frame ranges, each from its first to its
/// last pitched frame (a creaky ending is added by `creak_tail`), or the whole stretch for a
/// syllable that is all creak.
///
/// Syllables are found from pitched frames *and* creak (loud, no pitch): a low syllable said
/// creakily can have almost no pitch, and then a word's other syllable was judged in its
/// place. One native speaker (2026-11-10), syllables in two-syllable words right: 52 of 85
/// (3 more got no judgement) → 59 of 88. Tone Perfect: 89.7% → 90.8% clean, 87.4% → 91.3% noisy.
pub fn syllables(frames: &[Frame], n: usize) -> Vec<(usize, usize)> {
    let creak = creak_level(frames);
    let sounding = |f: &Frame| f.st.is_some() || f.rms >= creak;
    // sounding runs, bridging gaps of up to 3 frames
    let mut runs: Vec<(usize, usize)> = Vec::new();
    for (i, f) in frames.iter().enumerate() {
        if !sounding(f) {
            continue;
        }
        match runs.last_mut() {
            Some(r) if i <= r.1 + 3 => r.1 = i + 1,
            _ => runs.push((i, i + 1)),
        }
    }
    // a run with no pitch is a syllable only if it lasts (else a click or a consonant)
    let pitched = |(s, e): (usize, usize)| frames[s..e].iter().filter(|f| f.st.is_some()).count();
    runs.retain(|&r| pitched(r) >= 1 || r.1 - r.0 >= MIN_CREAK_SYLLABLE);
    if runs.is_empty() || n == 0 {
        return runs.into_iter().map(|r| trim_to_pitch(frames, r)).collect();
    }
    // too many: merge across the smallest gaps
    while runs.len() > n {
        let k = (0..runs.len() - 1).min_by_key(|&k| runs[k + 1].0 - runs[k].1).unwrap();
        let next = runs.remove(k + 1);
        runs[k].1 = next.1;
    }
    // too few: split the longest run at its deepest loudness dip (or pitch jump)
    while runs.len() < n {
        let k = (0..runs.len()).max_by_key(|&k| runs[k].1 - runs[k].0).unwrap();
        let (s, e) = runs[k];
        if e - s < 10 {
            break;
        }
        let lo = s + (e - s) / 5;
        let hi = e - (e - s) / 5;
        let smooth = |i: usize| (i.saturating_sub(2)..(i + 3).min(frames.len())).map(|j| frames[j].rms).sum::<f32>();
        let cut = (lo..hi).min_by(|&a, &b| smooth(a).total_cmp(&smooth(b))).unwrap();
        runs[k] = (s, cut);
        runs.insert(k + 1, (cut, e));
    }
    runs.into_iter().map(|r| trim_to_pitch(frames, r)).collect()
}

/// A syllable with no pitch at all counts if it has this many frames of creak.
const MIN_CREAK_SYLLABLE: usize = 8;

fn trim_to_pitch(frames: &[Frame], (s, e): (usize, usize)) -> (usize, usize) {
    match ((s..e).find(|&i| frames[i].st.is_some()), (s..e).rev().find(|&i| frames[i].st.is_some())) {
        (Some(a), Some(b)) => (a, b + 1),
        _ => (s, e),
    }
}

/// How loud a frame with no pitch must be to count as creak (still voice): 10% of the
/// loudest frame (`TONE_CREAK`), and 3× the background (the quietest tenth of the recording),
/// so room noise doesn't pass for voice. Infinite when creak is turned off.
pub fn creak_level(frames: &[Frame]) -> f32 {
    let creak = std::env::var("TONE_CREAK").ok().and_then(|x| x.parse::<f32>().ok()).unwrap_or(0.1);
    if creak <= 0.0 || frames.is_empty() {
        return f32::INFINITY;
    }
    let loud = frames.iter().map(|f| f.rms).fold(0f32, f32::max);
    let mut r: Vec<f32> = frames.iter().map(|f| f.rms).collect();
    r.sort_by(f32::total_cmp);
    let noise = r[r.len() / 10];
    (loud * creak).max(noise * 3.0)
}

/// A speaker's pitch range: median and spread (10th-90th percentile), in semitones.
#[derive(Debug, Clone, Copy)]
pub struct Voice {
    pub median: f32,
    pub spread: f32,
}

impl Voice {
    pub fn from_frames<'a>(all: impl IntoIterator<Item = &'a Frame>) -> Option<Voice> {
        let mut v: Vec<f32> = all.into_iter().filter_map(|f| f.st).collect();
        if v.len() < 20 {
            return None;
        }
        v.sort_by(f32::total_cmp);
        let p = |q: f32| v[((v.len() - 1) as f32 * q) as usize];
        Some(Voice { median: p(0.5), spread: (p(0.9) - p(0.1)).max(4.0) })
    }
}

/// A range anchored on the speaker's own tones: their tone 1's height is where the tone 1
/// shape sits (+1.6) and the bottom of their tone 3 is the floor (-2). `t1` and `t3` are
/// recordings of one syllable each (calibration's mā and mǎ). A learner's speaking range
/// (emphasis, laughter, creak) is often wider than the band they use for tones, and then
/// every tone looks squashed into the middle.
///
/// Not used by the app: on Tone Perfect it changed little (89.7% → 89.3%), and on one
/// learner it often failed (the bottom of their 3rd tone was creak, with no pitch to anchor
/// on) or turned their 3rd tones into 4ths. Kept for the bench ("anchored").
pub fn anchor_voice(t1: &[Frame], t3: &[Frame]) -> Option<Voice> {
    let pitches = |f: &[Frame]| -> Vec<f32> {
        let Some(&(s, e)) = syllables(f, 1).first() else { return Vec::new() };
        let mut v: Vec<f32> = f[s..e].iter().filter_map(|x| x.st).collect();
        v.sort_by(f32::total_cmp);
        v
    };
    let (a, b) = (pitches(t1), pitches(t3));
    if a.len() < 10 || b.len() < 5 {
        return None;
    }
    let high = a[a.len() / 2];
    let low = b[b.len() / 10];
    let unit = (high - low) / 3.6;
    if unit * 4.0 < 4.0 {
        return None; // too little difference to anchor on
    }
    Some(Voice { median: low + 2.0 * unit, spread: unit * 4.0 })
}

/// Five points of a syllable's pitch on Chao's scale relative to the speaker: about -2 (their
/// lowest) to +2 (their highest).
/// A fall often ends in creak: still loud, but with no pitch the tracker can follow. Count
/// that tail as the bottom of the speaker's range instead of dropping it, or a 4th tone
/// that loses its second half looks like a level 1st tone. Returns how many frames after the
/// syllable (which has `voiced` pitched frames) count as creak.
/// Tone Perfect, 6 speakers: 4th tones right 49% → 75%, all tones 83% → 90% (0 turns it off)
pub fn creak_tail(frames: &[Frame], e: usize, voiced: usize) -> usize {
    let level = creak_level(frames);
    let tail = frames[e..].iter().take_while(|f| f.st.is_none() && f.rms >= level).count();
    if tail >= 4 && tail * 4 >= voiced {
        tail
    } else {
        0
    }
}

/// What `contour` measures for a syllable, frame by frame: Some(pitch), or None for creak (the
/// bottom of the voice). That is its pitched frames; creak inside it (4+ frames: a 3rd tone
/// that dips into creak and comes back up); a creaky tail (`creak_tail`); and for a syllable
/// that is all creak, the creak.
fn measured(frames: &[Frame], (s, e): (usize, usize)) -> Vec<(usize, Option<f32>)> {
    let level = creak_level(frames);
    let creaky = |i: usize| frames[i].st.is_none() && frames[i].rms >= level;
    let voiced = frames[s..e].iter().filter(|f| f.st.is_some()).count();
    if voiced == 0 {
        let c: Vec<_> = (s..e).filter(|&i| creaky(i)).map(|i| (i, None)).collect();
        return if c.len() >= MIN_CREAK_SYLLABLE { c } else { Vec::new() };
    }
    let mut out = Vec::new();
    let mut i = s;
    while i < e {
        if let Some(st) = frames[i].st {
            out.push((i, Some(st)));
            i += 1;
            continue;
        }
        let j = (i..e).find(|&j| frames[j].st.is_some()).unwrap_or(e);
        if j - i >= 4 && (i..j).all(creaky) {
            out.extend((i..j).map(|k| (k, None)));
        }
        i = j;
    }
    let tail = creak_tail(frames, e, voiced);
    out.extend((e..e + tail).map(|k| (k, None)));
    out
}

/// The frames `contour` measures for a syllable: its pitched frames plus any creak,
/// without the first and last 10%. (first, last + 1), for showing which part was judged.
pub fn measured_range(frames: &[Frame], seg: (usize, usize)) -> Option<(usize, usize)> {
    let m = measured(frames, seg);
    if m.len() < 5 {
        return None;
    }
    let a = m.len() / 10;
    Some((m[a].0, m[m.len() - 1 - a].0 + 1))
}

pub fn contour(frames: &[Frame], seg: (usize, usize), voice: Voice) -> Option<[f32; 5]> {
    let floor = voice.median - voice.spread / 2.0;
    let v: Vec<f32> = measured(frames, seg).into_iter().map(|(_, st)| st.unwrap_or(floor)).collect();
    if v.len() < 5 {
        return None;
    }
    // onsets and offsets wobble: use the middle 80%
    let a = v.len() / 10;
    let v = &v[a..v.len() - a];
    let mut out = [0f32; 5];
    for (i, q) in [0.1f32, 0.3, 0.5, 0.7, 0.9].iter().enumerate() {
        let x = q * (v.len() - 1) as f32;
        let (i0, f) = (x.floor() as usize, x.fract());
        let y = v[i0] * (1.0 - f) + v[(i0 + 1).min(v.len() - 1)] * f;
        out[i] = (y - voice.median) / (voice.spread / 4.0);
    }
    Some(out)
}

/// Index in TEMPLATES of the 3rd tone's full dip and rise.
const DIPPING_THIRD: usize = 2;
const TEMPLATES: [(u8, [f32; 5]); 5] = [
    (1, [1.6, 1.6, 1.6, 1.6, 1.6]),     // 55
    (2, [-0.3, -0.3, 0.3, 1.1, 1.8]),   // 35
    (3, [-0.6, -1.4, -2.0, -1.5, -0.6]), // 214 (citation form)
    (3, [-0.8, -1.4, -1.9, -2.0, -2.0]), // 21 (half third, in phrases)
    (4, [1.8, 1.4, 0.5, -0.5, -1.4]),   // 51
];

/// The biggest fall and the biggest rise within a contour (earlier point to later point).
fn fall_rise(c: &[f32; 5]) -> (f32, f32) {
    let (mut fall, mut rise) = (0f32, 0f32);
    for i in 0..5 {
        for j in i + 1..5 {
            fall = fall.max(c[i] - c[j]);
            rise = rise.max(c[j] - c[i]);
        }
    }
    (fall, rise)
}

/// Weight of how far a contour falls and rises, whatever its timing (`FALL_WEIGHT` to
/// experiment). The point-by-point shape alone misses a 4th tone that drops fast and early
/// (into creak) from the middle of the voice: point by point it looks like the low half
/// third (21), but it falls about twice as far. Speed of the fall is the main cue listeners
/// use (Smith & Burnham 2012), and 3rd and 4th tones are the pair most often confused.
/// Tone Perfect, 6 speakers (FALL_WEIGHTS=0,0.25,0.5,1,2,4): best at 1, all tones
/// 89.7% → 90.3% clean and 87.4% → 89.8% with noise; 4th tones 75% → 78% and 80% → 86%.
pub fn fall_weight() -> f32 {
    std::env::var("FALL_WEIGHT").ok().and_then(|v| v.parse().ok()).unwrap_or(FALL_WEIGHT)
}
const FALL_WEIGHT: f32 = 1.0;

/// Distance from a contour to each tone's shape (index 0 = tone 1): level (where in the
/// voice) + `shape_weight` × shape (how it moves) + fall weight × (how far it falls and rises).
pub fn distances(c: &[f32; 5], shape_weight: f32) -> [f32; 4] {
    distances_with(c, shape_weight, fall_weight())
}

pub fn distances_with(c: &[f32; 5], shape_weight: f32, fall_weight: f32) -> [f32; 4] {
    distances_at(c, shape_weight, fall_weight, Place::Alone)
}

/// Where a syllable is: said alone, or in a word of several (before another syllable, or last).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Place {
    Alone,
    BeforeAnother,
    Last,
}

impl Place {
    pub fn of(k: usize, n: usize) -> Place {
        match (n, k + 1 == n) {
            (0 | 1, _) => Place::Alone,
            (_, false) => Place::BeforeAnother,
            (_, true) => Place::Last,
        }
    }
}

/// Weight of the level term for a syllable in a word (`WORD_LEVEL` to experiment), against 1
/// for one said alone. See `distances_at`.
pub fn word_level_weight() -> f32 {
    std::env::var("WORD_LEVEL").ok().and_then(|v| v.parse().ok()).unwrap_or(WORD_LEVEL)
}
const WORD_LEVEL: f32 = 0.5;
/// Whether a 3rd tone before another syllable may be the dipping 214 (`WORD_DIP=1`).
fn word_dip() -> bool {
    std::env::var("WORD_DIP").is_ok_and(|v| v == "1")
}

/// `distances_with` for a syllable at `place`. In a word, pitch drifts down through the phrase and
/// each tone bends toward its neighbours (Xu 1997), so how high a syllable sits says less than
/// how it moves: a 2nd tone after another syllable or last in a word often starts at the bottom
/// of the voice and rises only to the middle. And a 3rd tone before another syllable is the
/// low half third (21), never the full dip and rise (214).
pub fn distances_at(c: &[f32; 5], shape_weight: f32, fall_weight: f32, place: Place) -> [f32; 4] {
    let mean = c.iter().sum::<f32>() / 5.0;
    let (fall, rise) = fall_rise(c);
    let level_weight = if place == Place::Alone { 1.0 } else { word_level_weight() };
    let mut d = [f32::MAX; 4];
    for (i, (tone, t)) in TEMPLATES.into_iter().enumerate() {
        if place == Place::BeforeAnother && i == DIPPING_THIRD && !word_dip() {
            continue;
        }
        let tmean = t.iter().sum::<f32>() / 5.0;
        let level = level_weight * (mean - tmean).powi(2);
        let shape: f32 = c.iter().zip(t.iter()).map(|(x, y)| ((x - mean) - (y - tmean)).powi(2)).sum();
        let (tfall, trise) = fall_rise(&t);
        let movement = (fall - tfall).powi(2) + (rise - trise).powi(2);
        let k = tone as usize - 1;
        d[k] = d[k].min(level + shape_weight * shape + fall_weight * movement);
    }
    d
}

/// How long a syllable sounds, in frames: its pitched part widened to where the voice drops
/// below 10% of the loudest, so a creaky ending (still voice, no pitch) counts.
pub fn syllable_length(frames: &[Frame], (s, e): (usize, usize)) -> usize {
    let loud = frames.iter().map(|f| f.rms).fold(0f32, f32::max);
    let th = loud * 0.1;
    let mut a = s;
    while a > 0 && frames[a - 1].rms >= th {
        a -= 1;
    }
    let mut b = e.min(frames.len());
    while b < frames.len() && frames[b].rms >= th {
        b += 1;
    }
    b - a
}

/// A said-alone syllable's length by tone, as ln(length / the speaker's typical syllable):
/// 4th tones are short and 3rd tones long (Ho 1976; Whalen & Xu 1992). Measured on Tone
/// Perfect (6 speakers, `syllable_length` against each speaker's median).
/// Clean recordings: [-0.04, 0.06, 0.22, -0.48] (sd 0.13-0.22): a 4th tone is about 62% of
/// the speaker's usual length, a 3rd about 125%.
pub const LENGTH_BY_TONE: [f32; 4] = [-0.04, 0.06, 0.22, -0.48];
/// A typical said-alone syllable on Tone Perfect (`syllable_length`, frames), for when the
/// speaker's own isn't known yet.
pub const TYPICAL_LENGTH: f32 = 58.0;

/// Weight of the length cue (`LENGTH_WEIGHT` to experiment). Only for a syllable said on its
/// own: in phrases, lengths follow the rhythm more than the tone.
pub fn length_weight() -> f32 {
    std::env::var("LENGTH_WEIGHT").ok().and_then(|v| v.parse().ok()).unwrap_or(LENGTH_WEIGHT)
}
/// Tone Perfect (LENGTH_WEIGHTS=0..48; with fall weight 1): all tones 90.3% → 91.8% clean and
/// 89.8% → 90.5% noisy at 16 against the speaker's own usual length (91.3% / 90.6% against a
/// fixed one); 4th tones 78% → 81%. Higher weights help clean recordings further but noisy
/// ones less from 24 on.
const LENGTH_WEIGHT: f32 = 16.0;

/// Added distance for each tone from a syllable's length against the speaker's typical one.
pub fn length_distances(length: f32, typical: f32, weight: f32, by_tone: &[f32; 4]) -> [f32; 4] {
    let x = (length.max(1.0) / typical.max(1.0)).ln();
    by_tone.map(|mu| weight * (x - mu).powi(2))
}

// ---------- a classifier learned from recordings, for a syllable said alone ----------

/// What the learned model sees of a syllable: its five contour points, how far each is from
/// their mean (squared), how far it falls and rises, and its length against the speaker's
/// usual (as a logarithm; 0 when that isn't known).
pub fn model_features(c: &[f32; 5], len_ratio_ln: f32) -> [f32; 19] {
    let mean = c.iter().sum::<f32>() / 5.0;
    let (fall, rise) = fall_rise(c);
    let mut x = [0f32; 19];
    x[0] = 1.0;
    for i in 0..5 {
        x[1 + i] = c[i];
        x[6 + i] = (c[i] - mean) * (c[i] - mean);
    }
    x[11..].copy_from_slice(&[mean * mean, fall, rise, fall * fall, rise * rise, c[4] - c[0], len_ratio_ln, len_ratio_ln * len_ratio_ln]);
    x
}

/// Softmax regression weights [tone][feature], fitted to Tone Perfect (six native speakers,
/// 9,839 clean recordings of single syllables) by `tone_model_bench` (MODEL_PRINT=1).
const TONE_MODEL: [[f32; 19]; 4] = [
    [3.3109, 0.1748, 0.5459, 0.6249, 0.6436, 0.1337, 0.0566, 0.1884, 0.4958, 0.1032, -0.8252, -0.1509, -0.5997, -1.2339, 0.0031, -0.0021, -0.0342, 1.2503, -2.8819],
    [0.2538, -0.3869, -0.5384, -0.3677, 0.3294, 0.2703, -0.0264, -0.0359, -0.6061, -0.0578, 0.5700, -0.6697, -0.1944, 0.8498, 0.0327, -0.1026, 0.3909, 0.2044, -4.5188],
    [-2.0222, -0.1504, -0.4084, -0.2660, -0.6866, -0.3087, -0.0540, 0.0783, 0.2429, -0.0518, -0.0931, 0.6232, 0.2107, 0.2702, 0.0245, -0.0101, -0.0783, 4.6498, 4.9288],
    [-1.5425, 0.3624, 0.4010, 0.0088, -0.2864, -0.0954, 0.0238, -0.2308, -0.1326, 0.0064, 0.3483, 0.1974, 0.5834, 0.1139, -0.0602, 0.1147, -0.2783, -6.1044, 2.4718],
];

/// The learned model's log-probability of each tone for a syllable said alone.
pub fn model_logprobs(c: &[f32; 5], len_ratio_ln: f32) -> [f32; 4] {
    let x = model_features(c, len_ratio_ln);
    let s = TONE_MODEL.map(|w| w.iter().zip(&x).map(|(a, b)| a * b).sum::<f32>());
    let m = s.iter().copied().fold(f32::MIN, f32::max);
    let z = m + s.iter().map(|a| (a - m).exp()).sum::<f32>().ln();
    s.map(|a| a - z)
}

/// How much the learned model counts beside the template distances for a syllable said alone
/// (`MODEL_WEIGHT` to experiment; 0 = templates only).
///
/// Measured 2026-11-13, each Tone Perfect speaker judged by a model fitted to the other five:
/// templates and length 91.5%; the model alone 97.1%; blended at 1, 2, 3, 4: 93.4, 94.5,
/// 95.4, 95.9% (4th tones 79% → 90% at 3). On a native speaker recorded at home (speaker1:
/// 80 single syllables), the model alone was worse than the templates (71 against 74 right:
/// it knows only careful studio tones, and takes a low 3rd tone without its rise for a 4th);
/// blended at 3 it was level (74 clean, 68 against 67 with noise). In words the model made
/// things worse at every weight (66 of 88 → 62), so words keep the templates alone.
pub fn model_weight() -> f32 {
    std::env::var("MODEL_WEIGHT").ok().and_then(|v| v.parse().ok()).unwrap_or(MODEL_WEIGHT)
}
const MODEL_WEIGHT: f32 = 3.0;

/// The nearest tone (1-4) for a contour, with the distance to it and to the runner-up.
pub fn classify(c: &[f32; 5], shape_weight: f32) -> (u8, f32, f32) {
    let d = distances(c, shape_weight);
    let mut best: Vec<(u8, f32)> = (1..=4).map(|t| (t, d[t as usize - 1])).collect();
    best.sort_by(|a, b| a.1.total_cmp(&b.1));
    (best[0].0, best[0].1, best[1].1)
}

/// Shape weight used by the app (and the benches' default).
pub const SHAPE_WEIGHT: f32 = 2.0;

/// How sharply distances turn into probabilities (smaller = more confident).
const TONE_TEMPERATURE: f32 = 1.0;

/// For each of `n` expected syllables, the log-probability of tones 1-4 from its pitch, or
/// None for a syllable with too little pitch to judge. A single syllable said alone is also
/// judged by its length against `typical_length` (the speaker's usual, in frames).
pub fn tone_logprobs(frames: &[Frame], n: usize, voice: Voice, typical_length: Option<f32>) -> Vec<Option<[f32; 4]>> {
    let syl = syllables(frames, n);
    (0..n)
        .map(|k| {
            let seg = *syl.get(k)?;
            let c = contour(frames, seg, voice)?;
            let mut d = distances_at(&c, SHAPE_WEIGHT, fall_weight(), Place::of(k, n));
            if n == 1 {
                let length = syllable_length(frames, seg) as f32;
                if let Some(typical) = typical_length {
                    let extra = length_distances(length, typical, length_weight(), &LENGTH_BY_TONE);
                    (0..4).for_each(|t| d[t] += extra[t]);
                }
                // what a model learned from recordings makes of it (see `model_weight`)
                let len_ln = typical_length.map_or(0.0, |typical| (length.max(1.0) / typical.max(1.0)).ln());
                let learned = model_logprobs(&c, len_ln);
                let weight = model_weight();
                (0..4).for_each(|t| d[t] -= weight * learned[t]);
            }
            let d = d.map(|x| -x / TONE_TEMPERATURE);
            let max = d.iter().copied().fold(f32::MIN, f32::max);
            let z = max + d.iter().map(|x| (x - max).exp()).sum::<f32>().ln();
            Some(d.map(|x| x - z))
        })
        .collect()
}

/// The learner's pitch range, built up from their recordings: a histogram of voiced frames
/// in half-semitone bins, older recordings fading out (each new one scales the rest by
/// `PROFILE_DECAY`). Stored as text in the settings ("pitch_profile").
#[derive(Debug, Clone)]
pub struct PitchProfile {
    pub recordings: u32,
    bins: Vec<f32>,
    /// The speaker's usual length of a syllable said alone: (mean of ln(frames), how many),
    /// the mean moving like the range (newer recordings count more).
    length: (f32, u32),
}

const PROFILE_LOW: f32 = -8.0; // semitones above 100 Hz: 63 Hz
const PROFILE_BINS: usize = 80; // up to +32 st: 635 Hz
const PROFILE_DECAY: f32 = 0.97;
/// Recordings needed before the range is trusted.
pub const PROFILE_MIN_RECORDINGS: u32 = 5;

impl PitchProfile {
    pub fn parse(s: &str) -> PitchProfile {
        let mut parts = s.split(';');
        let ok = parts.next() == Some("v1");
        let recordings = parts.next().and_then(|x| x.parse().ok()).filter(|_| ok).unwrap_or(0);
        // (only counts: anything else in a stored value would spoil every sum made from it)
        let count = |x: &str| x.parse::<f32>().ok().filter(|v| v.is_finite() && *v >= 0.0).unwrap_or(0.0);
        let mut bins: Vec<f32> = parts.next().filter(|_| ok).map(|b| b.split(',').map(count).collect()).unwrap_or_default();
        bins.resize(PROFILE_BINS, 0.0);
        // optional (added later): "<mean ln length>,<count>"
        let length = parts
            .next()
            .filter(|_| ok)
            .and_then(|l| l.split_once(','))
            .and_then(|(m, n)| Some((m.parse::<f32>().ok().filter(|m| m.is_finite())?, n.parse().ok()?)))
            .unwrap_or((0.0, 0));
        PitchProfile { recordings, bins, length }
    }

    pub fn serialize(&self) -> String {
        let bins: Vec<String> = self.bins.iter().map(|b| if *b < 0.05 { "0".into() } else { format!("{b:.1}") }).collect();
        format!("v1;{};{};{:.4},{}", self.recordings, bins.join(","), self.length.0, self.length.1)
    }

    /// Note the length (frames, `syllable_length`) of a syllable said alone.
    pub fn add_length(&mut self, frames: usize) {
        if frames < 5 {
            return;
        }
        let x = (frames as f32).ln();
        let (m, n) = self.length;
        // a plain average for the first few, then newer ones count more (like the range)
        let w = (1.0 / (n + 1) as f32).max(1.0 - PROFILE_DECAY);
        self.length = (m + w * (x - m), n + 1);
    }

    /// The speaker's usual syllable length (frames), once 3 have been noted.
    pub fn typical_length(&self) -> Option<f32> {
        (self.length.1 >= 3).then(|| self.length.0.exp())
    }

    /// Add one recording's voiced frames.
    pub fn add(&mut self, frames: &[Frame]) {
        let voiced: Vec<f32> = frames.iter().filter_map(|f| f.st).collect();
        if voiced.len() < 10 {
            return;
        }
        self.bins.iter_mut().for_each(|b| *b *= PROFILE_DECAY);
        for st in voiced {
            let k = ((st - PROFILE_LOW) * 2.0).floor();
            if k >= 0.0 && (k as usize) < PROFILE_BINS {
                self.bins[k as usize] += 1.0;
            }
        }
        self.recordings += 1;
    }

    /// The range, once enough recordings are in.
    pub fn voice(&self) -> Option<Voice> {
        if self.recordings < PROFILE_MIN_RECORDINGS {
            return None;
        }
        self.voice_from(100.0)
    }

    /// The range from whatever is in so far (calibration shows it after the first recording).
    pub fn voice_so_far(&self) -> Option<Voice> {
        self.voice_from(20.0)
    }

    fn voice_from(&self, min_frames: f32) -> Option<Voice> {
        let total: f32 = self.bins.iter().sum();
        if total < min_frames {
            return None;
        }
        let at = |q: f32| {
            let mut acc = 0.0;
            for (k, b) in self.bins.iter().enumerate() {
                acc += b;
                if acc >= q * total {
                    return PROFILE_LOW + (k as f32 + 0.5) / 2.0;
                }
            }
            PROFILE_LOW + PROFILE_BINS as f32 / 2.0
        };
        Some(Voice { median: at(0.5), spread: (at(0.9) - at(0.1)).max(4.0) })
    }

    /// Add a recording given as its pitch per frame (as calibration keeps them).
    pub fn add_pitches(&mut self, st: &[Option<f32>]) {
        let frames: Vec<Frame> = st.iter().map(|&st| Frame { st, rms: 0.0 }).collect();
        self.add(&frames);
    }

    /// The histogram: half-semitone bins starting at `PROFILE_LOW` semitones above 100 Hz.
    pub fn bins(&self) -> &[f32] {
        &self.bins
    }
}

/// The pitch tracker's and classifier's settings, for Settings › calibration to show.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Params {
    pub hop_ms: f32,
    pub window_ms: f32,
    pub f0_min_hz: f32,
    pub f0_max_hz: f32,
    pub yin_first: f32,
    pub yin_global: f32,
    pub low_pass_hz: f32,
    pub shape_weight: f32,
    /// Weight of how far a syllable falls and rises (see `fall_weight`).
    pub fall_weight: f32,
    pub creak: f32,
    pub profile_decay: f32,
    pub min_recordings: u32,
    pub profile_low_st: f32,
    pub profile_bin_st: f32,
    /// Tone shapes on Chao's scale relative to the speaker (-2 lowest .. +2 highest).
    pub templates: Vec<(u8, [f32; 5])>,
}

pub fn params() -> Params {
    let env = |k: &str, d: f32| std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
    let (yin_first, yin_global) = yin_threshold();
    Params {
        hop_ms: HOP as f32 * 1000.0 / SAMPLE_RATE as f32,
        window_ms: WIN as f32 * 1000.0 / SAMPLE_RATE as f32,
        f0_min_hz: F0_MIN,
        f0_max_hz: F0_MAX,
        yin_first,
        yin_global,
        low_pass_hz: env("YIN_LOWPASS", 1000.0),
        shape_weight: SHAPE_WEIGHT,
        fall_weight: fall_weight(),
        creak: env("TONE_CREAK", 0.1),
        profile_decay: PROFILE_DECAY,
        min_recordings: PROFILE_MIN_RECORDINGS,
        profile_low_st: PROFILE_LOW,
        profile_bin_st: 0.5,
        templates: TEMPLATES.to_vec(),
    }
}

/// Semitones above 100 Hz to Hz.
pub fn hz(st: f32) -> f32 {
    100.0 * 2f32.powf(st / 12.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_wav(path: &std::path::Path) -> Vec<f32> {
        let b = std::fs::read(path).unwrap();
        b[44..].chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0).collect()
    }

    #[test]
    fn tracks_a_pure_tone() {
        let pcm: Vec<f32> = (0..SAMPLE_RATE / 2).map(|i| (2.0 * std::f32::consts::PI * 200.0 * i as f32 / SAMPLE_RATE as f32).sin() * 0.5).collect();
        let f = pitch_track(&pcm);
        let st: Vec<f32> = f.iter().filter_map(|f| f.st).collect();
        assert!(st.len() > 20);
        let want = semitones(200.0);
        assert!(st.iter().all(|s| (s - want).abs() < 0.3), "{st:?}");
    }

    #[test]
    fn rising_and_falling_glides() {
        let glide = |f0: f32, f1: f32| -> Vec<f32> {
            let n = SAMPLE_RATE / 2;
            let mut phase = 0f32;
            (0..n)
                .map(|i| {
                    let f = f0 + (f1 - f0) * i as f32 / n as f32;
                    phase += 2.0 * std::f32::consts::PI * f / SAMPLE_RATE as f32;
                    phase.sin() * 0.5
                })
                .collect()
        };
        let voice = Voice { median: semitones(200.0), spread: 10.0 };
        for (pcm, want) in [(glide(170.0, 260.0), 2), (glide(290.0, 150.0), 4), (glide(265.0, 265.0), 1)] {
            let f = pitch_track(&pcm);
            let syl = syllables(&f, 1);
            let c = contour(&f, syl[0], voice).unwrap();
            assert_eq!(classify(&c, 2.0).0, want, "{c:?}");
        }
    }

    /// Print pitch tracks: TONES_DIR=... TONES_DUMP=0,1,2 cargo test --lib tone_dump -- --ignored --nocapture
    #[test]
    #[ignore]
    fn tone_dump() {
        let Ok(dir) = std::env::var("TONES_DIR") else { return };
        for i in std::env::var("TONES_DUMP").unwrap_or_default().split(',').filter_map(|x| x.parse::<usize>().ok()) {
            let pcm = read_wav(&std::path::Path::new(&dir).join("clean").join(format!("{i}.wav")));
            let raw_head: Vec<u8> = std::fs::read(std::path::Path::new(&dir).join("clean").join(format!("{i}.wav"))).unwrap()[..48].to_vec();
            let raw = pitch_raw(&pcm);
            let rawline: String = raw.iter().map(|f| match f.st { Some(s) => format!("{:.0} ", s), None => ". ".into() }).collect();
            println!("raw #{i}: {rawline}");
            let f = pitch_track(&pcm);
            let peak = pcm.iter().fold(0f32, |a, b| a.max(b.abs()));
            let line: String = f.iter().map(|f| match f.st { Some(s) => format!("{:.0} ", s), None => if f.rms > 0.01 { "~ ".into() } else { ". ".into() } }).collect();
            println!("#{i} samples {} peak {peak:.2} header {:?}
{line}", pcm.len(), String::from_utf8_lossy(&raw_head[36..44]));
        }
    }

    /// Tone Perfect (six native speakers, every syllable in four tones). TP_DIR is the `all`
    /// folder from scripts/tone_perfect_prep.py (meta.tsv: N, syllable, tone, speaker, gap).
    /// Compares four ways to get tone templates:
    ///   chao      the fixed shapes (55 35 214/21 51), no data
    ///   calib     the speaker's own mā má mǎ mà only (option B: one calibration set)
    ///   own       the speaker's own average per tone, from their other syllables
    ///   others    the other five speakers' average per tone (a trained model, roughly)
    /// TP_DIR=... cargo test --lib tone_perfect_bench -- --ignored --nocapture
    #[test]
    #[ignore]
    fn tone_perfect_bench() {
        let Ok(dir) = std::env::var("TP_DIR") else { return };
        let dir = std::path::Path::new(&dir);
        let weight: f32 = std::env::var("SHAPE_WEIGHT").ok().and_then(|w| w.parse().ok()).unwrap_or(2.0);
        let calib_syl = std::env::var("CALIB").unwrap_or_else(|_| "ma".into());
        struct Clip {
            syl: String,
            tone: u8,
            spk: String,
            gap: bool,
        }
        let clips: Vec<Clip> = std::fs::read_to_string(dir.join("meta.tsv"))
            .unwrap()
            .lines()
            .map(|l| {
                let f: Vec<&str> = l.split('\t').collect();
                Clip { syl: f[1].into(), tone: f[2].parse().unwrap(), spk: f[3].into(), gap: f[4] == "1" }
            })
            .collect();
        let mut speakers: Vec<String> = clips.iter().map(|c| c.spk.clone()).collect();
        speakers.sort();
        speakers.dedup();

        // distance used by every method: level + weight × shape, like classify()
        let dist = |c: &[f32; 5], t: &[f32; 5]| {
            let (cm, tm) = (c.iter().sum::<f32>() / 5.0, t.iter().sum::<f32>() / 5.0);
            (cm - tm).powi(2) + weight * c.iter().zip(t).map(|(x, y)| ((x - cm) - (y - tm)).powi(2)).sum::<f32>()
        };
        let nearest = |c: &[f32; 5], temps: &[(u8, [f32; 5])]| temps.iter().min_by(|a, b| dist(c, &a.1).total_cmp(&dist(c, &b.1))).map(|t| t.0).unwrap_or(0);
        let mean_by_tone = |cs: &mut dyn Iterator<Item = (u8, [f32; 5])>| -> Vec<(u8, [f32; 5])> {
            let mut sum = [[0f32; 5]; 5];
            let mut n = [0u32; 5];
            for (t, c) in cs {
                n[t as usize] += 1;
                for k in 0..5 {
                    sum[t as usize][k] += c[k];
                }
            }
            (1..5u8).filter(|&t| n[t as usize] > 0).map(|t| (t, sum[t as usize].map(|x| x / n[t as usize] as f32))).collect()
        };

        for set in std::env::var("BENCH_SETS").unwrap_or_else(|_| "clean,noisy".into()).split(',') {
            // pitch tracks in parallel
            let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
            let mut tracks: Vec<Vec<Frame>> = vec![Vec::new(); clips.len()];
            std::thread::scope(|s| {
                for (k, chunk) in tracks.chunks_mut(clips.len().div_ceil(threads)).enumerate() {
                    let base = k * clips.len().div_ceil(threads);
                    s.spawn(move || {
                        for (j, t) in chunk.iter_mut().enumerate() {
                            *t = pitch_track(&read_wav(&dir.join(set).join(format!("{}.wav", base + j))));
                        }
                    });
                }
            });
            // each speaker's range from all their clips, then one contour per clip
            let voices: std::collections::HashMap<&str, Voice> = speakers
                .iter()
                .map(|s| (s.as_str(), Voice::from_frames(clips.iter().zip(&tracks).filter(|(c, _)| &c.spk == s).flat_map(|(_, t)| t.iter())).unwrap()))
                .collect();
            let contours: Vec<Option<[f32; 5]>> = clips
                .iter()
                .zip(&tracks)
                .map(|(c, f)| syllables(f, 1).first().and_then(|&r| contour(f, r, voices[c.spk.as_str()])))
                .collect();

            let chao: Vec<(u8, [f32; 5])> = TEMPLATES.to_vec();
            let methods = ["chao", "calib", "own", "others"];
            // [method][expected tone][heard], heard 0 = no pitch
            let mut conf = vec![[[0u32; 5]; 5]; methods.len()];
            let mut per_spk: std::collections::BTreeMap<(usize, &str), (u32, u32)> = Default::default();
            let mut gaps = vec![(0u32, 0u32); methods.len()];
            for spk in &speakers {
                let own_all: Vec<(usize, u8, [f32; 5])> = clips.iter().enumerate().filter(|(_, c)| &c.spk == spk).filter_map(|(i, c)| contours[i].map(|k| (i, c.tone, k))).collect();
                let calib = mean_by_tone(&mut own_all.iter().filter(|(i, _, _)| clips[*i].syl == calib_syl).map(|&(_, t, k)| (t, k)));
                let others = mean_by_tone(&mut clips.iter().enumerate().filter(|(_, c)| &c.spk != spk).filter_map(|(i, c)| contours[i].map(|k| (c.tone, k))));
                for (i, c) in clips.iter().enumerate().filter(|(_, c)| &c.spk == spk) {
                    if c.syl == calib_syl {
                        continue; // calibration syllable: not tested for any method, so all see the same clips
                    }
                    let own = mean_by_tone(&mut own_all.iter().filter(|(j, _, _)| clips[*j].syl != c.syl).map(|&(_, t, k)| (t, k)));
                    for (m, temps) in [&chao, &calib, &own, &others].into_iter().enumerate() {
                        let got = contours[i].map(|k| nearest(&k, temps)).unwrap_or(0);
                        conf[m][c.tone as usize][got as usize] += 1;
                        let e = per_spk.entry((m, spk.as_str())).or_default();
                        e.0 += (got == c.tone) as u32;
                        e.1 += 1;
                        if c.gap {
                            gaps[m].0 += (got == c.tone) as u32;
                            gaps[m].1 += 1;
                        }
                    }
                }
            }
            if std::env::var_os("TP_DIAG").is_some() {
                // average contour and voiced length per (expected, heard by chao)
                let mut acc: std::collections::BTreeMap<(u8, u8), ([f32; 5], f32, f32, u32)> = Default::default();
                for (i, c) in clips.iter().enumerate() {
                    let Some(k) = contours[i] else { continue };
                    let got = nearest(&k, &chao);
                    let voiced = tracks[i].iter().filter(|f| f.st.is_some()).count() as f32;
                    let loud = tracks[i].iter().filter(|f| f.rms > 0.02).count() as f32;
                    let e = acc.entry((c.tone, got)).or_insert(([0.0; 5], 0.0, 0.0, 0));
                    for j in 0..5 {
                        e.0[j] += k[j];
                    }
                    e.1 += voiced;
                    e.2 += loud;
                    e.3 += 1;
                }
                for ((t, g), (s, v, l, n)) in acc.iter().filter(|(_, e)| e.3 >= 20) {
                    let n = *n as f32;
                    println!("  T{t}→{g} n={n}: contour {:?} voiced {:.0} loud {:.0} frames", s.map(|x| (x / n * 10.0).round() / 10.0), v / n, l / n);
                }
            }
            let nopitch: u32 = (1..5).map(|t| conf[0][t][0]).sum();
            println!("== {set} ({} clips, no pitch found: {nopitch}; speaker ranges: {})", clips.len(), speakers.iter().map(|s| format!("{s} {:.0}±{:.0}st", voices[s.as_str()].median, voices[s.as_str()].spread)).collect::<Vec<_>>().join(", "));
            for (m, name) in methods.iter().enumerate() {
                let ok: u32 = (1..5).map(|t| conf[m][t][t]).sum();
                let n: u32 = (1..5).map(|t| conf[m][t].iter().sum::<u32>()).sum();
                let by_tone: Vec<String> = (1..5).map(|t| format!("T{t} {:.0}%", 100.0 * conf[m][t][t] as f32 / conf[m][t].iter().sum::<u32>().max(1) as f32)).collect();
                let by_spk: Vec<String> = speakers.iter().map(|s| { let e = per_spk[&(m, s.as_str())]; format!("{s} {:.0}%", 100.0 * e.0 as f32 / e.1 as f32) }).collect();
                println!(
                    "{name:7} {:.1}% ({ok}/{n}) | {} | {} | non-words {:.0}%",
                    100.0 * ok as f32 / n as f32,
                    by_tone.join(" "),
                    by_spk.join(" "),
                    100.0 * gaps[m].0 as f32 / gaps[m].1.max(1) as f32
                );
                println!("        expected→heard (rows T1-T4; cols none,1,2,3,4): {:?}", &conf[m][1..]);
            }
            // the app's own scoring (distances(): fixed shapes + fall/rise) at several fall
            // weights (FALL_WEIGHTS=0,0.5,1), with each speaker's range either from all their
            // speech ("range") or anchored on their calibration mā and mǎ ("anchored": tone 1's
            // height is level 5-ish, tone 3's bottom is level 1)
            let anchored: std::collections::HashMap<&str, Voice> = speakers
                .iter()
                .filter_map(|s| {
                    let pick = |tone: u8| clips.iter().zip(&tracks).find(|(c, _)| &c.spk == s && c.syl == calib_syl && c.tone == tone).map(|(_, f)| f.as_slice());
                    let v = anchor_voice(pick(1)?, pick(3)?)?;
                    Some((s.as_str(), v))
                })
                .collect();
            for fw in std::env::var("FALL_WEIGHTS").unwrap_or_default().split(',').filter_map(|w| w.parse::<f32>().ok()) {
                for (label, use_anchor) in [("range", false), ("anchored", true)] {
                    let mut conf = [[0u32; 5]; 5];
                    for (i, c) in clips.iter().enumerate().filter(|(_, c)| c.syl != calib_syl) {
                        let voice = if use_anchor { anchored.get(c.spk.as_str()).copied().unwrap_or(voices[c.spk.as_str()]) } else { voices[c.spk.as_str()] };
                        let k = syllables(&tracks[i], 1).first().and_then(|&r| contour(&tracks[i], r, voice));
                        let got = k
                            .map(|k| {
                                let d = distances_with(&k, weight, fw);
                                (0..4).min_by(|&a, &b| d[a].total_cmp(&d[b])).unwrap() as u8 + 1
                            })
                            .unwrap_or(0);
                        conf[c.tone as usize][got as usize] += 1;
                    }
                    let ok: u32 = (1..5).map(|t| conf[t][t]).sum();
                    let n: u32 = (1..5).map(|t| conf[t].iter().sum::<u32>()).sum();
                    let by_tone: Vec<String> = (1..5).map(|t| format!("T{t} {:.1}%", 100.0 * conf[t][t] as f32 / conf[t].iter().sum::<u32>().max(1) as f32)).collect();
                    println!("fall×{fw:<4} {label:8} {:.1}% ({ok}/{n}) | {} | T4→T3 {} T3→T4 {}", 100.0 * ok as f32 / n as f32, by_tone.join(" "), conf[4][3], conf[3][4]);
                }
            }
            // syllable length (LENGTH_WEIGHTS=0,1,2): against each speaker's median
            // ("speaker") or a fixed typical length ("fixed", the median of everyone)
            let lens: Vec<Option<f32>> = clips.iter().zip(&tracks).map(|(_, f)| syllables(f, 1).first().map(|&r| syllable_length(f, r) as f32)).collect();
            let median = |mut v: Vec<f32>| { v.sort_by(f32::total_cmp); v.get(v.len() / 2).copied().unwrap_or(1.0) };
            let spk_median: std::collections::HashMap<&str, f32> = speakers.iter().map(|s| (s.as_str(), median(clips.iter().zip(&lens).filter(|(c, _)| &c.spk == s).filter_map(|(_, l)| *l).collect()))).collect();
            let all_median = median(lens.iter().flatten().copied().collect());
            let mut by_tone = [0f32; 4];
            let mut sd = [0f32; 4];
            for t in 1..=4u8 {
                let xs: Vec<f32> = clips.iter().zip(&lens).filter(|(c, _)| c.tone == t).filter_map(|(c, l)| l.map(|l| (l / spk_median[c.spk.as_str()]).ln())).collect();
                let m = xs.iter().sum::<f32>() / xs.len().max(1) as f32;
                by_tone[t as usize - 1] = m;
                sd[t as usize - 1] = (xs.iter().map(|x| (x - m).powi(2)).sum::<f32>() / xs.len().max(1) as f32).sqrt();
            }
            println!(
                "length: ln(length / speaker median) by tone {:?}, sd {:?}; medians {} (all {all_median:.0} frames)",
                by_tone.map(|x| (x * 100.0).round() / 100.0),
                sd.map(|x| (x * 100.0).round() / 100.0),
                speakers.iter().map(|s| format!("{s} {:.0}", spk_median[s.as_str()])).collect::<Vec<_>>().join(", ")
            );
            let fw = fall_weight();
            for lw in std::env::var("LENGTH_WEIGHTS").unwrap_or_default().split(',').filter_map(|w| w.parse::<f32>().ok()) {
                for (label, per_speaker) in [("speaker", true), ("fixed", false)] {
                    let mut conf = [[0u32; 5]; 5];
                    for (i, c) in clips.iter().enumerate().filter(|(_, c)| c.syl != calib_syl) {
                        let typical = if per_speaker { spk_median[c.spk.as_str()] } else { all_median };
                        let got = contours[i]
                            .map(|k| {
                                let mut d = distances_with(&k, weight, fw);
                                if let Some(l) = lens[i] {
                                    let extra = length_distances(l, typical, lw, &by_tone);
                                    (0..4).for_each(|t| d[t] += extra[t]);
                                }
                                (0..4).min_by(|&a, &b| d[a].total_cmp(&d[b])).unwrap() as u8 + 1
                            })
                            .unwrap_or(0);
                        conf[c.tone as usize][got as usize] += 1;
                    }
                    let ok: u32 = (1..5).map(|t| conf[t][t]).sum();
                    let n: u32 = (1..5).map(|t| conf[t].iter().sum::<u32>()).sum();
                    let by_t: Vec<String> = (1..5).map(|t| format!("T{t} {:.1}%", 100.0 * conf[t][t] as f32 / conf[t].iter().sum::<u32>().max(1) as f32)).collect();
                    println!("length×{lw:<4} {label:8} {:.1}% ({ok}/{n}) | {} | T4→T1 {} T4→T3 {} T3→T4 {}", 100.0 * ok as f32 / n as f32, by_t.join(" "), conf[4][1], conf[4][3], conf[3][4]);
                }
            }
            if !anchored.is_empty() {
                println!("anchored ranges: {}", speakers.iter().filter_map(|s| anchored.get(s.as_str()).map(|v| format!("{s} {:.0}±{:.0}st", v.median, v.spread))).collect::<Vec<_>>().join(", "));
            }
        }
    }

    /// Tone accuracy on recorded clips. TONES_DIR holds list.tsv ("text<TAB>tones") and
    /// clean/N.wav, noisy/N.wav.
    /// TONES_DIR=... cargo test --release --lib tone_bench -- --ignored --nocapture
    #[test]
    #[ignore]
    fn tone_bench() {
        let Ok(dir) = std::env::var("TONES_DIR") else { return };
        let dir = std::path::Path::new(&dir);
        let list: Vec<(String, Vec<u8>)> = std::fs::read_to_string(dir.join("list.tsv"))
            .unwrap()
            .lines()
            .map(|l| {
                let (t, n) = l.split_once('\t').unwrap();
                (t.to_string(), n.bytes().map(|b| b - b'0').collect())
            })
            .collect();
        let weight: f32 = std::env::var("SHAPE_WEIGHT").ok().and_then(|w| w.parse().ok()).unwrap_or(2.0);
        for set in ["clean", "noisy"] {
            let tracks: Vec<Vec<Frame>> = (0..list.len()).map(|i| pitch_track(&read_wav(&dir.join(set).join(format!("{i}.wav"))))).collect();
            // the speaker's range from all their recordings (in the app: from earlier answers)
            let voice = Voice::from_frames(tracks.iter().flatten()).unwrap();
            let mut conf = [[0u32; 5]; 5]; // [expected][heard], tone 1-4 (index 0 = no pitch)
            let (mut mono, mut mono_ok, mut di, mut di_ok, mut words_ok) = (0, 0, 0, 0, 0);
            let mut misses = Vec::new();
            for (i, (text, tones)) in list.iter().enumerate() {
                let f = &tracks[i];
                let syl = syllables(f, tones.len());
                let mut all_ok = true;
                let mut heard = String::new();
                for (k, &want) in tones.iter().enumerate() {
                    if want == 5 {
                        heard.push('5');
                        continue; // neutral tone: not judged
                    }
                    let c = syl.get(k).and_then(|&r| contour(f, r, voice));
                    if std::env::var_os("TONES_SHAPES").is_some() && set == "clean" && tones.len() == 1 {
                        println!("shape T{want} {text}: {:?}", c.map(|c| c.map(|x| (x * 10.0).round() / 10.0)));
                    }
                    let got = c.map(|c| classify(&c, weight).0).unwrap_or(0);
                    heard.push((b'0' + got) as char);
                    conf[want as usize][got as usize] += 1;
                    let ok = got == want;
                    all_ok &= ok;
                    if tones.len() == 1 {
                        mono += 1;
                        mono_ok += ok as u32;
                    } else {
                        di += 1;
                        di_ok += ok as u32;
                    }
                }
                words_ok += all_ok as u32;
                if !all_ok && misses.len() < 14 {
                    misses.push(format!("{text}:{}→{heard}", tones.iter().map(|t| (b'0' + t) as char).collect::<String>()));
                }
            }
            println!(
                "{set:5} voice {:.1}±{:.1} st | single syllables {mono_ok}/{mono} | syllables in 2-syllable words {di_ok}/{di} | whole items {words_ok}/{}",
                voice.median,
                voice.spread,
                list.len()
            );
            println!("      expected→heard (rows 1-4; columns: none,1,2,3,4): {:?}", &conf[1..]);
            println!("      misses: {}", misses.join("  "));
        }
    }

    /// A speaker's recordings of the speaker sheet, cut by scripts/tone_cut.py, judged the way
    /// the app judges a spoken answer (tone_logprobs: shapes, fall/rise and, for a syllable
    /// said alone, its length against the speaker's usual). SPEAKER_DIR holds meta.tsv and
    /// clean/N.wav, noisy/N.wav. Neutral tones aren't judged; sandhi counts as said.
    /// SPEAKER_DIR=tone-data/<speaker> cargo test --release --lib speaker_bench -- --ignored --nocapture
    #[test]
    #[ignore]
    fn speaker_bench() {
        let Ok(dir) = std::env::var("SPEAKER_DIR") else { return };
        let dir = std::path::Path::new(&dir);
        struct Item {
            n: usize,
            part: String,
            pinyin: String,
            said: Vec<u8>,
            right: Vec<u8>,
        }
        let items: Vec<Item> = std::fs::read_to_string(dir.join("meta.tsv"))
            .unwrap()
            .lines()
            .skip(1)
            .map(|l| {
                let f: Vec<&str> = l.split('\t').collect();
                let digits = |s: &str| s.bytes().map(|b| b - b'0').collect();
                Item { n: f[0].parse().unwrap(), part: f[5].into(), pinyin: f[6].into(), said: digits(f[7]), right: digits(f[8]) }
            })
            .collect();
        for set in ["clean", "noisy"] {
            let tracks: Vec<Vec<Frame>> = items.iter().map(|it| pitch_track(&read_wav(&dir.join(set).join(format!("{}.wav", it.n))))).collect();
            let voice = Voice::from_frames(tracks.iter().flatten()).unwrap();
            // the speaker's usual single-syllable length, as the app learns it from answers
            let mut lens: Vec<f32> = items
                .iter()
                .zip(&tracks)
                .filter(|(it, _)| it.said.len() == 1)
                .filter_map(|(_, f)| syllables(f, 1).first().map(|&s| syllable_length(f, s) as f32))
                .collect();
            lens.sort_by(f32::total_cmp);
            let own_len = lens.get(lens.len() / 2).copied();
            // how much of the speech the tracker gives a pitch to (loud = above 15% of the clip's loudest)
            let (mut loud, mut pitched) = (0usize, 0usize);
            for f in &tracks {
                let top = f.iter().map(|x| x.rms).fold(0f32, f32::max);
                for x in f.iter().filter(|x| x.rms > top * 0.15) {
                    loud += 1;
                    pitched += x.st.is_some() as usize;
                }
            }
            println!("{set}: pitch found in {:.0}% of loud frames", 100.0 * pitched as f32 / loud.max(1) as f32);
            println!(
                "{set}: voice {:.0} Hz ±{:.1} st, usual single-syllable length {} frames (app default {TYPICAL_LENGTH})",
                hz(voice.median),
                voice.spread,
                own_len.map_or("?".into(), |l| format!("{l:.0}"))
            );
            for (label, typical) in [("no length", None), ("default length", Some(TYPICAL_LENGTH as f32)), ("own length", own_len)] {
                let mut by_part: std::collections::BTreeMap<String, (u32, u32, u32)> = Default::default(); // ok, judged, unjudged
                let mut conf = [[0u32; 5]; 5];
                let mut misses = Vec::new();
                let mut false_accepts = (0u32, 0u32);
                for (it, f) in items.iter().zip(&tracks) {
                    // SPEAKER_RUNS=1 prints how each word was split into syllables
                    if set == "clean" && label == "no length" && it.said.len() > 1 && std::env::var_os("SPEAKER_RUNS").is_some() {
                        let runs = syllables(f, 0);
                        let syl = syllables(f, it.said.len());
                        let pitch = |s: usize, e: usize| f[s..e].iter().filter_map(|x| x.st).map(|x| format!("{:.0}", x)).collect::<Vec<_>>().join(",");
                        println!("    RUNS {:12} {:?} -> {:?} | {}", it.pinyin, runs, syl, syl.iter().map(|&(s, e)| format!("[{}]", pitch(s, e))).collect::<Vec<_>>().join(" "));
                    }
                    let probs = tone_logprobs(f, it.said.len(), voice, typical);
                    let mut heard = String::new();
                    let mut all_ok = true;
                    for (k, &want) in it.said.iter().enumerate() {
                        let got = probs.get(k).copied().flatten().map(|p| 1 + (0..4).max_by(|&a, &b| p[a].total_cmp(&p[b])).unwrap() as u8).unwrap_or(0);
                        heard.push((b'0' + got) as char);
                        if want == 5 {
                            continue;
                        }
                        // SPEAKER_SHAPES=2 prints every contour of a tone-2 syllable, and so on
                        if set == "clean" && label == "no length" && std::env::var("SPEAKER_SHAPES").is_ok_and(|t| t == want.to_string()) {
                            let syl = syllables(f, it.said.len());
                            let c = syl.get(k).and_then(|&s| contour(f, s, voice));
                            let pos = if it.said.len() == 1 { "alone".to_string() } else { format!("{}/{}", k + 1, it.said.len()) };
                            println!("    T{want} {:12} {pos:5} heard {got} len {:3} {:?}", it.pinyin, syl.get(k).map_or(0, |s| s.1 - s.0), c.map(|c| c.map(|x| (x * 10.0).round() / 10.0)));
                        }
                        let e = by_part.entry(it.part.clone()).or_default();
                        if got == 0 {
                            e.2 += 1;
                        } else {
                            e.1 += 1;
                            e.0 += (got == want) as u32;
                        }
                        conf[want as usize][got as usize] += 1;
                        all_ok &= got == want;
                        // a wrong tone said on purpose: would the checker pass it as right?
                        if it.right.get(k).is_some_and(|&r| r != want && r != 5) {
                            false_accepts.1 += 1;
                            false_accepts.0 += (got == it.right[k]) as u32;
                        }
                    }
                    if !all_ok {
                        misses.push(format!("{}:{}→{heard}", it.pinyin, it.said.iter().map(|t| (b'0' + t) as char).collect::<String>()));
                    }
                }
                let parts: Vec<String> = by_part.iter().map(|(p, (ok, n, none))| format!("{p} {ok}/{n} {:.0}%{}", 100.0 * *ok as f32 / (*n).max(1) as f32, if *none > 0 { format!(" (+{none} no pitch)") } else { String::new() })).collect();
                let by_t: Vec<String> = (1..5).map(|t| format!("T{t} {}/{}", conf[t][t], conf[t].iter().sum::<u32>())).collect();
                println!("  {label:15} {} | {}", parts.join("  "), by_t.join(" "));
                if false_accepts.1 > 0 {
                    println!("  {:15} wrong tones passed as right: {}/{}", "", false_accepts.0, false_accepts.1);
                }
                if set == "clean" && label == "own length" {
                    println!("  expected→heard (rows T1-T4; columns none,1,2,3,4): {:?}", &conf[1..]);
                    println!("  misses: {}", misses.join("  "));
                }
            }
        }
    }

    // ---------- a learned classifier, measured against the template one ----------

    /// Softmax regression by gradient descent on standardised features; returns weights
    /// [tone][feature] for the raw features (the scaling folded in).
    fn fit_softmax(xs: &[Vec<f32>], ys: &[usize], l2: f32) -> Vec<Vec<f32>> {
        let (n, d) = (xs.len(), xs[0].len());
        let mut mu = vec![0f32; d];
        let mut sd = vec![0f32; d];
        for x in xs {
            for j in 0..d {
                mu[j] += x[j] / n as f32;
            }
        }
        for x in xs {
            for j in 0..d {
                sd[j] += (x[j] - mu[j]).powi(2) / n as f32;
            }
        }
        for j in 0..d {
            sd[j] = sd[j].sqrt().max(1e-6);
        }
        mu[0] = 0.0;
        sd[0] = 1.0; // the constant
        let z: Vec<Vec<f32>> = xs.iter().map(|x| (0..d).map(|j| (x[j] - mu[j]) / sd[j]).collect()).collect();
        let mut w = vec![vec![0f32; d]; 4];
        let mut v = vec![vec![0f32; d]; 4];
        for _ in 0..600 {
            let mut g = vec![vec![0f32; d]; 4];
            for (x, &y) in z.iter().zip(ys) {
                let s: Vec<f32> = w.iter().map(|wk| wk.iter().zip(x).map(|(a, b)| a * b).sum()).collect();
                let m = s.iter().copied().fold(f32::MIN, f32::max);
                let e: Vec<f32> = s.iter().map(|a| (a - m).exp()).collect();
                let t: f32 = e.iter().sum();
                for k in 0..4 {
                    let p = e[k] / t - (k == y) as u8 as f32;
                    for j in 0..d {
                        g[k][j] += p * x[j] / n as f32;
                    }
                }
            }
            for k in 0..4 {
                for j in 0..d {
                    v[k][j] = 0.9 * v[k][j] - 0.5 * (g[k][j] + l2 * w[k][j] * (j > 0) as u8 as f32);
                    w[k][j] += v[k][j];
                }
            }
        }
        // back to raw features
        w.iter()
            .map(|wk| {
                let mut raw: Vec<f32> = (0..d).map(|j| wk[j] / sd[j]).collect();
                raw[0] = wk[0] - (1..d).map(|j| wk[j] * mu[j] / sd[j]).sum::<f32>();
                raw
            })
            .collect()
    }

    /// The learned model and the template distances together: MODEL_ALPHA × the model's
    /// log-probabilities minus the distances (unset = the model alone).
    fn blended_tone(w: &[Vec<f32>], x: &[f32], d: &[f32; 4]) -> usize {
        let Some(alpha) = std::env::var("MODEL_ALPHA").ok().and_then(|v| v.parse::<f32>().ok()) else { return model_tone(w, x) };
        let s: Vec<f32> = (0..4).map(|k| w[k].iter().zip(x).map(|(p, q)| p * q).sum::<f32>()).collect();
        let m = s.iter().copied().fold(f32::MIN, f32::max);
        let z = m + s.iter().map(|a| (a - m).exp()).sum::<f32>().ln();
        (0..4).max_by(|&a, &b| (alpha * (s[a] - z) - d[a]).total_cmp(&(alpha * (s[b] - z) - d[b]))).unwrap()
    }

    fn model_tone(w: &[Vec<f32>], x: &[f32]) -> usize {
        (0..4)
            .max_by(|&a, &b| {
                let s = |k: usize| w[k].iter().zip(x).map(|(p, q)| p * q).sum::<f32>();
                s(a).total_cmp(&s(b))
            })
            .unwrap()
    }

    /// Would a classifier learned from Tone Perfect beat the fixed shapes? Each speaker is
    /// tested with a model trained on the other five; then a model trained on all six is
    /// tried on a learner-like speaker (SPEAKER_DIR), single syllables and words.
    /// TP_DIR=../tone-data/tp/all SPEAKER_DIR=../tone-data/speaker1 cargo test --release --lib tone_model_bench -- --ignored --nocapture
    #[test]
    #[ignore]
    fn tone_model_bench() {
        let Ok(dir) = std::env::var("TP_DIR") else { return };
        let dir = std::path::Path::new(&dir);
        let meta: Vec<(u8, String)> = std::fs::read_to_string(dir.join("meta.tsv"))
            .unwrap()
            .lines()
            .map(|l| {
                let f: Vec<&str> = l.split('\t').collect();
                (f[2].parse().unwrap(), f[3].to_string())
            })
            .collect();
        let mut speakers: Vec<String> = meta.iter().map(|m| m.1.clone()).collect();
        speakers.sort();
        speakers.dedup();
        let l2: f32 = std::env::var("MODEL_L2").ok().and_then(|v| v.parse().ok()).unwrap_or(1e-3);
        let mut final_model: Option<Vec<Vec<f32>>> = None;
        for set in ["clean", "noisy"] {
            let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
            let mut tracks: Vec<Vec<Frame>> = vec![Vec::new(); meta.len()];
            std::thread::scope(|s| {
                for (k, chunk) in tracks.chunks_mut(meta.len().div_ceil(threads)).enumerate() {
                    let base = k * meta.len().div_ceil(threads);
                    s.spawn(move || {
                        for (j, t) in chunk.iter_mut().enumerate() {
                            *t = pitch_track(&read_wav(&dir.join(set).join(format!("{}.wav", base + j))));
                        }
                    });
                }
            });
            // per speaker: the voice from all their clips and their usual syllable length
            struct Row {
                tone: usize,
                spk: usize,
                c: [f32; 5],
                len: f32,
            }
            let mut rows: Vec<Row> = Vec::new();
            for (si, spk) in speakers.iter().enumerate() {
                let mine: Vec<usize> = (0..meta.len()).filter(|&i| &meta[i].1 == spk).collect();
                let voice = Voice::from_frames(mine.iter().flat_map(|&i| tracks[i].iter())).unwrap();
                let mut got: Vec<(usize, [f32; 5], f32)> = Vec::new();
                for &i in &mine {
                    let Some(&seg) = syllables(&tracks[i], 1).first() else { continue };
                    let Some(c) = contour(&tracks[i], seg, voice) else { continue };
                    got.push((meta[i].0 as usize - 1, c, syllable_length(&tracks[i], seg) as f32));
                }
                let mut lens: Vec<f32> = got.iter().map(|g| g.2).collect();
                lens.sort_by(f32::total_cmp);
                let typical = lens[lens.len() / 2];
                rows.extend(got.into_iter().map(|(tone, c, len)| Row { tone, spk: si, c, len: (len / typical).ln() }));
            }
            let feats: Vec<Vec<f32>> = rows.iter().map(|r| model_features(&r.c, r.len).to_vec()).collect();
            // the app's way, on the same clips
            // (the templates and the length cue alone: what the app did before the model)
            let app_d = |r: &Row| {
                let mut d = distances_at(&r.c, SHAPE_WEIGHT, fall_weight(), Place::Alone);
                let extra = length_distances(r.len.exp(), 1.0, length_weight(), &LENGTH_BY_TONE);
                (0..4).for_each(|t| d[t] += extra[t]);
                d
            };
            let app = |r: &Row| {
                let d = app_d(r);
                (0..4).min_by(|&a, &b| d[a].total_cmp(&d[b])).unwrap()
            };
            let mut conf_app = [[0u32; 4]; 4];
            let mut conf_model = [[0u32; 4]; 4];
            let mut per: Vec<(u32, u32, u32)> = vec![(0, 0, 0); speakers.len()];
            for si in 0..speakers.len() {
                let (xs, ys): (Vec<Vec<f32>>, Vec<usize>) = rows.iter().zip(&feats).filter(|(r, _)| r.spk != si).map(|(r, x)| (x.clone(), r.tone)).unzip();
                let w = fit_softmax(&xs, &ys, l2);
                for (r, x) in rows.iter().zip(&feats).filter(|(r, _)| r.spk == si) {
                    let (a, m) = (app(r), blended_tone(&w, x, &app_d(r)));
                    conf_app[r.tone][a] += 1;
                    conf_model[r.tone][m] += 1;
                    per[si].0 += (a == r.tone) as u32;
                    per[si].1 += (m == r.tone) as u32;
                    per[si].2 += 1;
                }
            }
            let total = rows.len() as f32;
            let ok = |c: &[[u32; 4]; 4]| (0..4).map(|t| c[t][t]).sum::<u32>() as f32;
            let by_tone = |c: &[[u32; 4]; 4]| (0..4).map(|t| format!("T{} {:.0}%", t + 1, 100.0 * c[t][t] as f32 / c[t].iter().sum::<u32>() as f32)).collect::<Vec<_>>().join(" ");
            println!("== {set}: {} clips", rows.len());
            println!("  app (fixed shapes + length)   {:.1}% | {} | {:?}", 100.0 * ok(&conf_app) / total, by_tone(&conf_app), conf_app);
            println!("  learned (other five speakers) {:.1}% | {} | {:?}", 100.0 * ok(&conf_model) / total, by_tone(&conf_model), conf_model);
            println!("  by speaker (app → learned): {}", speakers.iter().zip(&per).map(|(s, p)| format!("{s} {:.0}→{:.0}%", 100.0 * p.0 as f32 / p.2 as f32, 100.0 * p.1 as f32 / p.2 as f32)).collect::<Vec<_>>().join("  "));
            if set == "clean" {
                let ys: Vec<usize> = rows.iter().map(|r| r.tone).collect();
                final_model = Some(fit_softmax(&feats, &ys, l2));
            }
        }
        let w = final_model.unwrap();
        if std::env::var_os("MODEL_PRINT").is_some() {
            for wk in &w {
                println!("    [{}],", wk.iter().map(|v| format!("{v:.4}")).collect::<Vec<_>>().join(", "));
            }
        }
        // a learner-like speaker the model has never heard
        let Ok(sdir) = std::env::var("SPEAKER_DIR") else { return };
        let sdir = std::path::Path::new(&sdir);
        let items: Vec<(usize, Vec<u8>)> = std::fs::read_to_string(sdir.join("meta.tsv"))
            .unwrap()
            .lines()
            .skip(1)
            .map(|l| {
                let f: Vec<&str> = l.split('\t').collect();
                (f[0].parse().unwrap(), f[7].bytes().map(|b| b - b'0').collect())
            })
            .collect();
        for set in ["clean", "noisy"] {
            let tracks: Vec<Vec<Frame>> = items.iter().map(|it| pitch_track(&read_wav(&sdir.join(set).join(format!("{}.wav", it.0))))).collect();
            let voice = Voice::from_frames(tracks.iter().flatten()).unwrap();
            let mut lens: Vec<f32> = items.iter().zip(&tracks).filter(|(it, _)| it.1.len() == 1).filter_map(|(_, f)| syllables(f, 1).first().map(|&s| syllable_length(f, s) as f32)).collect();
            lens.sort_by(f32::total_cmp);
            let typical = lens[lens.len() / 2];
            // [single | word][app | learned] = (right, judged)
            let mut score = [[(0u32, 0u32); 2]; 2];
            let mut conf = [[0u32; 4]; 4];
            for (it, f) in items.iter().zip(&tracks) {
                let n = it.1.len();
                let syl = syllables(f, n);
                let probs = tone_logprobs(f, n, voice, Some(typical));
                for (k, &want) in it.1.iter().enumerate() {
                    if want == 5 {
                        continue;
                    }
                    let part = (n > 1) as usize;
                    let app = probs.get(k).copied().flatten().map(|p| (0..4).max_by(|&a, &b| p[a].total_cmp(&p[b])).unwrap());
                    let learned = syl.get(k).and_then(|&seg| contour(f, seg, voice).map(|c| (c, seg))).map(|(c, seg)| {
                        // in a word the length follows the rhythm, not the tone: leave it out (0 = usual)
                        let len = if n == 1 { (syllable_length(f, seg) as f32 / typical).ln() } else { 0.0 };
                        let mut d = distances_at(&c, SHAPE_WEIGHT, fall_weight(), Place::of(k, n));
                        if n == 1 {
                            let extra = length_distances(len.exp(), 1.0, length_weight(), &LENGTH_BY_TONE);
                            (0..4).for_each(|t| d[t] += extra[t]);
                        }
                        blended_tone(&w, &model_features(&c, len), &d)
                    });
                    for (m, got) in [app, learned].into_iter().enumerate() {
                        score[part][m].1 += 1;
                        score[part][m].0 += (got == Some(want as usize - 1)) as u32;
                    }
                    if let (0, Some(g)) = (part, learned) {
                        conf[want as usize - 1][g] += 1;
                    }
                }
            }
            let pct = |s: (u32, u32)| format!("{}/{} {:.0}%", s.0, s.1, 100.0 * s.0 as f32 / s.1.max(1) as f32);
            println!("{} {set}: single syllables app {} → learned {} | in words app {} → learned {} | learned singles {:?}", sdir.display(), pct(score[0][0]), pct(score[0][1]), pct(score[1][0]), pct(score[1][1]), conf);
        }
    }

    /// How much does calibration matter? Each Tone Perfect speaker's range (and usual syllable
    /// length) is taken from only some of their recordings, as calibration would: the four
    /// tones of "ma" alone, then 8, 16 and 40 recordings spread through their list, then all.
    /// Every recording of theirs is then judged the way the app does it.
    /// TP_DIR=../tone-data/tp/all cargo test --release --lib calibration_size_bench -- --ignored --nocapture
    #[test]
    #[ignore]
    fn calibration_size_bench() {
        let Ok(dir) = std::env::var("TP_DIR") else { return };
        let dir = std::path::Path::new(&dir);
        let meta: Vec<(String, u8, String)> = std::fs::read_to_string(dir.join("meta.tsv"))
            .unwrap()
            .lines()
            .map(|l| {
                let f: Vec<&str> = l.split('\t').collect();
                (f[1].to_string(), f[2].parse().unwrap(), f[3].to_string())
            })
            .collect();
        let mut speakers: Vec<String> = meta.iter().map(|m| m.2.clone()).collect();
        speakers.sort();
        speakers.dedup();
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
        let mut tracks: Vec<Vec<Frame>> = vec![Vec::new(); meta.len()];
        std::thread::scope(|s| {
            for (k, chunk) in tracks.chunks_mut(meta.len().div_ceil(threads)).enumerate() {
                let base = k * meta.len().div_ceil(threads);
                s.spawn(move || {
                    for (j, t) in chunk.iter_mut().enumerate() {
                        *t = pitch_track(&read_wav(&dir.join("clean").join(format!("{}.wav", base + j))));
                    }
                });
            }
        });
        let sets: [(&str, usize); 5] = [("ma × 4", 0), ("8", 8), ("16", 16), ("40", 40), ("all", usize::MAX)];
        let mut right = vec![0u32; sets.len()];
        let mut total = 0u32;
        let mut ranges: Vec<Vec<String>> = vec![Vec::new(); sets.len()];
        for spk in &speakers {
            let mine: Vec<usize> = (0..meta.len()).filter(|&i| &meta[i].2 == spk).collect();
            for (k, (_, n)) in sets.iter().enumerate() {
                let chosen: Vec<usize> = match *n {
                    0 => mine.iter().copied().filter(|&i| meta[i].0 == "ma").collect(),
                    usize::MAX => mine.clone(),
                    n => (0..n).map(|j| mine[(j * mine.len() / n + 7) % mine.len()]).collect(),
                };
                let Some(voice) = Voice::from_frames(chosen.iter().flat_map(|&i| tracks[i].iter())) else { continue };
                let mut lens: Vec<f32> = chosen.iter().filter_map(|&i| syllables(&tracks[i], 1).first().map(|&s| syllable_length(&tracks[i], s) as f32)).collect();
                lens.sort_by(f32::total_cmp);
                let typical = lens.get(lens.len() / 2).copied();
                ranges[k].push(format!("{:.0}±{:.0}", voice.median, voice.spread));
                for &i in &mine {
                    let got = tone_logprobs(&tracks[i], 1, voice, typical).first().copied().flatten().map(|p| 1 + (0..4).max_by(|&a, &b| p[a].total_cmp(&p[b])).unwrap() as u8);
                    right[k] += (got == Some(meta[i].1)) as u32;
                    if k == 0 {
                        total += 1;
                    }
                }
            }
        }
        for (k, (name, _)) in sets.iter().enumerate() {
            println!("range from {name:7} recordings: {:.1}% right | ranges (semitones, median±spread) {}", 100.0 * right[k] as f32 / total as f32, ranges[k].join(" "));
        }
    }
}
