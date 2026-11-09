//! Tone checking from pitch (prototype, not wired into the app yet).
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
#![allow(dead_code)] // prototype: exercised by tests, not used by the app yet

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

/// YIN pitch tracking (de Cheveigné & Kawahara 2002), cleaned up.
pub fn pitch_track(pcm: &[f32]) -> Vec<Frame> {
    let mut frames = pitch_raw(pcm);
    clean(&mut frames);
    frames
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

fn pitch_raw(pcm: &[f32]) -> Vec<Frame> {
    let cutoff = std::env::var("YIN_LOWPASS").ok().and_then(|v| v.parse().ok()).unwrap_or(1000.0f32);
    let filtered = if cutoff > 0.0 { low_pass(pcm, cutoff) } else { pcm.to_vec() };
    let (first, global) = yin_threshold();
    let pcm = &filtered[..];
    let tau_min = (SAMPLE_RATE as f32 / F0_MAX) as usize;
    let tau_max = (SAMPLE_RATE as f32 / F0_MIN) as usize;
    let mut frames = Vec::new();
    let mut d = vec![0f32; tau_max + 1];
    let mut start = 0;
    while start + WIN + tau_max <= pcm.len() {
        let x = &pcm[start..start + WIN + tau_max];
        let rms = (x[..WIN].iter().map(|v| v * v).sum::<f32>() / WIN as f32).sqrt();
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
    for f in frames.iter_mut() {
        if let Some(s) = f.st.as_mut() {
            if *s - med > 8.0 {
                *s -= 12.0;
            } else if med - *s > 8.0 {
                *s += 12.0;
            }
            if (*s - med).abs() > 10.0 {
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

/// Split the voiced frames into `n` syllables: (start, end) frame ranges.
pub fn syllables(frames: &[Frame], n: usize) -> Vec<(usize, usize)> {
    // voiced runs, bridging gaps of up to 3 frames
    let mut runs: Vec<(usize, usize)> = Vec::new();
    for (i, f) in frames.iter().enumerate() {
        if f.st.is_none() {
            continue;
        }
        match runs.last_mut() {
            Some(r) if i <= r.1 + 3 => r.1 = i + 1,
            _ => runs.push((i, i + 1)),
        }
    }
    if runs.is_empty() || n == 0 {
        return runs;
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
    runs
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

/// Five points of a syllable's pitch on Chao's scale relative to the speaker: about -2 (their
/// lowest) to +2 (their highest).
pub fn contour(frames: &[Frame], (s, e): (usize, usize), voice: Voice) -> Option<[f32; 5]> {
    let v: Vec<f32> = frames[s..e].iter().filter_map(|f| f.st).collect();
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

const TEMPLATES: [(u8, [f32; 5]); 5] = [
    (1, [1.6, 1.6, 1.6, 1.6, 1.6]),     // 55
    (2, [-0.3, -0.3, 0.3, 1.1, 1.8]),   // 35
    (3, [-0.6, -1.4, -2.0, -1.5, -0.6]), // 214 (citation form)
    (3, [-0.8, -1.4, -1.9, -2.0, -2.0]), // 21 (half third, in phrases)
    (4, [1.8, 1.4, 0.5, -0.5, -1.4]),   // 51
];

/// The nearest tone (1-4) for a contour, with the distance to it and to the runner-up.
pub fn classify(c: &[f32; 5], shape_weight: f32) -> (u8, f32, f32) {
    let mean = c.iter().sum::<f32>() / 5.0;
    let mut best: Vec<(u8, f32)> = Vec::new();
    for (tone, t) in TEMPLATES {
        let tmean = t.iter().sum::<f32>() / 5.0;
        // level (where in the voice) + shape (how it moves), shape counted more
        let level = (mean - tmean).powi(2);
        let shape: f32 = c.iter().zip(t.iter()).map(|(x, y)| ((x - mean) - (y - tmean)).powi(2)).sum();
        let d = level + shape_weight * shape;
        match best.iter_mut().find(|b| b.0 == tone) {
            Some(b) => b.1 = b.1.min(d),
            None => best.push((tone, d)),
        }
    }
    best.sort_by(|a, b| a.1.total_cmp(&b.1));
    (best[0].0, best[0].1, best[1].1)
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
}
