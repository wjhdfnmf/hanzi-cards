//! Settings › Speech › "Calibrate tone detection": the learner records a sentence and the four
//! tones of "ma"; each recording is analysed here (pitch track, the syllable's shape, the tone
//! it sounds like) for the charts, and the recordings together become the pitch profile that
//! "Use pitch to tell tones apart" needs (see tones::PitchProfile).
//!
//! Calibration keeps no state on this side: the page keeps each recording's pitch track
//! (100 numbers a second) and sends the earlier ones back with each new recording.

use crate::tones::{self, Frame, PitchProfile, Voice};
use serde::Serialize;

/// A speaker's range in semitones above 100 Hz and in Hz.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceInfo {
    pub median_st: f32,
    pub spread_st: f32,
    pub median_hz: f32,
    /// Bottom and top of the range used for tones (median ∓ spread/2: Chao's 1 and 5).
    pub low_hz: f32,
    pub high_hz: f32,
}

impl From<Voice> for VoiceInfo {
    fn from(v: Voice) -> Self {
        VoiceInfo {
            median_st: v.median,
            spread_st: v.spread,
            median_hz: tones::hz(v.median),
            low_hz: tones::hz(v.median - v.spread / 2.0),
            high_hz: tones::hz(v.median + v.spread / 2.0),
        }
    }
}

/// The saved profile, for the range chart.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileInfo {
    pub recordings: u32,
    /// Whether "Use pitch to tell tones apart" can use it yet.
    pub ready: bool,
    pub bins: Vec<f32>,
    pub voice: Option<VoiceInfo>,
    pub params: tones::Params,
}

pub fn info(profile: &PitchProfile) -> ProfileInfo {
    ProfileInfo {
        recordings: profile.recordings,
        ready: profile.voice().is_some(),
        bins: profile.bins().to_vec(),
        voice: profile.voice_so_far().map(VoiceInfo::from),
        params: tones::params(),
    }
}

/// One calibration recording, analysed.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Analysis {
    /// Pitch per 10 ms frame (semitones above 100 Hz), None where unvoiced.
    pub pitch: Vec<Option<f32>>,
    /// Loudness per frame, 0..1 (1 = the loudest frame).
    pub level: Vec<f32>,
    pub voiced_ms: f32,
    /// Mains hum found in the recording and filtered out before tracking (Hz).
    pub hum_hz: Vec<f32>,
    /// The syllable that was judged (frame range), for a one-syllable recording.
    pub segment: Option<(usize, usize)>,
    /// The frames its shape was measured from (frame range): the syllable's middle 80%,
    /// including a creaky ending counted as the bottom of the voice.
    pub measured: Option<(usize, usize)>,
    /// How many frames after the syllable counted as creak (no pitch, still loud).
    pub creak_frames: usize,
    /// Its shape: 5 points on Chao's scale relative to the voice (-2 .. +2).
    pub contour: Option<[f32; 5]>,
    /// How likely each of tones 1-4 is, from the pitch alone.
    pub probs: Option<[f32; 4]>,
    pub heard: Option<u8>,
    /// How to say the asked tone more clearly, if its shape is off.
    pub advice: Vec<Advice>,
    /// How long the syllable sounded (frames, `tones::syllable_length`), for a one-syllable
    /// recording, and the speaker's usual length it was compared with.
    pub length_frames: Option<f32>,
    pub typical_length: f32,
    /// The range the shape was measured against (this recording and the earlier ones).
    pub voice: Option<VoiceInfo>,
    /// The profile so far, including this recording.
    pub profile: ProfileInfo,
}

/// An earlier recording of the same calibration, as the page keeps it: its pitch track, the
/// tone that was asked for (None: the sentence) and the syllable's length.
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Earlier {
    pub pitch: Vec<Option<f32>>,
    #[serde(default)]
    pub tone: Option<u8>,
    #[serde(default)]
    pub length: Option<f32>,
}

/// The profile a calibration's recordings make, from scratch.
pub fn build(recordings: &[Earlier]) -> PitchProfile {
    let mut p = PitchProfile::parse("");
    for r in recordings {
        p.add_pitches(&r.pitch);
        if let (Some(_), Some(len)) = (r.tone, r.length) {
            p.add_length(len.round() as usize);
        }
    }
    p
}

/// Analyse a recording (16 kHz mono). `tone` is the tone that was asked for when the
/// recording is one syllable; None for a sentence (only its range is used). `saved_typical`
/// is the usual syllable length from the saved profile, used until this calibration has its
/// own (from two words).
pub fn analyze(pcm: &[f32], tone: Option<u8>, earlier: &[Earlier], saved_typical: Option<f32>) -> Analysis {
    let (frames, hum) = tones::pitch_track_hum(pcm);
    let mut profile = build(earlier);
    profile.add(&frames);
    let mut lengths: Vec<f32> = earlier.iter().filter(|e| e.tone.is_some()).filter_map(|e| e.length).collect();
    lengths.sort_by(f32::total_cmp);
    let typical = if lengths.len() >= 2 { lengths[lengths.len() / 2] } else { saved_typical.unwrap_or(tones::TYPICAL_LENGTH) };
    // where this speaker's 1st tone sat (Hz), the height a 4th tone starts from
    let t1_hz = earlier.iter().rev().find(|e| e.tone == Some(1)).and_then(|e| {
        let mut v: Vec<f32> = e.pitch.iter().flatten().copied().collect();
        v.sort_by(f32::total_cmp);
        v.get(v.len() / 2).map(|&st| tones::hz(st))
    });
    let voice = profile.voice_so_far().or_else(|| Voice::from_frames(&frames));
    let loud = frames.iter().map(|f| f.rms).fold(0f32, f32::max).max(1e-9);
    let p = tones::params();
    let mut out = Analysis {
        pitch: frames.iter().map(|f| f.st).collect(),
        level: frames.iter().map(|f| f.rms / loud).collect(),
        voiced_ms: frames.iter().filter(|f| f.st.is_some()).count() as f32 * p.hop_ms,
        hum_hz: hum,
        segment: None,
        measured: None,
        creak_frames: 0,
        contour: None,
        probs: None,
        heard: None,
        advice: Vec::new(),
        length_frames: None,
        typical_length: typical,
        voice: voice.map(VoiceInfo::from),
        profile: info(&profile),
    };
    if let (Some(tone), Some(voice)) = (tone, voice) {
        judge(&frames, voice, typical, &mut out);
        if let Some(c) = out.contour {
            out.advice = advise(tone, &c, voice, t1_hz);
        }
    }
    out
}

/// A pointer for saying a tone more clearly: `key` names it (the page has the words), with
/// where the voice should be and where it was.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Advice {
    pub key: &'static str,
    pub target_hz: f32,
    pub actual_hz: f32,
}

/// Compare a syllable's shape (Chao scale relative to the voice: -2 bottom .. +2 top) with
/// what the asked tone should do. The thresholds sit about halfway between the tone's shape
/// and where learners commonly land instead (a 4th tone starting mid-voice, a 3rd that
/// doesn't get low, a 2nd that barely rises, a 1st below the top of the voice). `t1_hz`: where
/// the speaker's own 1st tone sat, if known: a 4th tone starts from about there.
pub fn advise(tone: u8, c: &[f32; 5], voice: Voice, t1_hz: Option<f32>) -> Vec<Advice> {
    let hz = |x: f32| tones::hz(voice.median + x * voice.spread / 4.0);
    let a = |key, target: f32, actual: f32| Advice { key, target_hz: hz(target), actual_hz: hz(actual) };
    let mean = c.iter().sum::<f32>() / 5.0;
    let (lo, hi) = c.iter().fold((f32::MAX, f32::MIN), |(lo, hi), &x| (lo.min(x), hi.max(x)));
    let mut out = Vec::new();
    match tone {
        1 => {
            if mean < 1.0 {
                out.push(a("t1Higher", 1.6, mean));
            }
            if hi - lo > 1.2 {
                out.push(a("t1Level", 1.6, mean));
            }
        }
        2 => {
            if c[4] - lo < 1.5 {
                out.push(a("t2Rise", 1.8, c[4]));
            }
        }
        3 => {
            if lo > -1.4 {
                out.push(a("t3Lower", -2.0, lo));
            }
        }
        4 => {
            if c[0] < 1.0 {
                let mut adv = a("t4Higher", 1.8, c[0]);
                if let Some(t1) = t1_hz.filter(|&t1| t1 > adv.actual_hz) {
                    adv.key = "t4HigherT1";
                    adv.target_hz = t1;
                }
                out.push(adv);
            }
            if c[0] - lo < 2.0 {
                out.push(a("t4Fall", -1.4, lo));
            }
        }
        _ => {}
    }
    out
}

fn judge(frames: &[Frame], voice: Voice, typical_length: f32, out: &mut Analysis) {
    let Some(&seg) = tones::syllables(frames, 1).first() else { return };
    out.segment = Some(seg);
    out.measured = tones::measured_range(frames, seg);
    let voiced = frames[seg.0..seg.1].iter().filter(|f| f.st.is_some()).count();
    out.creak_frames = tones::creak_tail(frames, seg.1, voiced);
    out.length_frames = Some(tones::syllable_length(frames, seg) as f32);
    out.contour = tones::contour(frames, seg, voice);
    if let Some(Some(lp)) = tones::tone_logprobs(frames, 1, voice, Some(typical_length)).first() {
        let probs = lp.map(f32::exp);
        out.heard = (0..4).max_by(|&a, &b| probs[a].total_cmp(&probs[b])).map(|k| k as u8 + 1);
        out.probs = Some(probs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sung syllable: pitch gliding from `from` to `to` Hz over `secs`, with harmonics.
    fn glide(from: f32, to: f32, secs: f32) -> Vec<f32> {
        let n = (secs * tones::SAMPLE_RATE as f32) as usize;
        let mut phase = 0f32;
        let mut out = vec![0f32; tones::SAMPLE_RATE / 5]; // 200 ms of silence first
        for i in 0..n {
            let f = from + (to - from) * i as f32 / n as f32;
            phase += 2.0 * std::f32::consts::PI * f / tones::SAMPLE_RATE as f32;
            let env = (i.min(n - i) as f32 / 800.0).min(1.0);
            out.push(env * 0.3 * (phase.sin() + 0.5 * (2.0 * phase).sin() + 0.25 * (3.0 * phase).sin()));
        }
        out.extend(vec![0f32; tones::SAMPLE_RATE / 5]);
        out
    }

    #[test]
    fn calibration_hears_the_four_tones() {
        // a sentence-like recording first: the voice moving over its whole range
        let mut sentence = glide(110.0, 220.0, 0.6);
        sentence.extend(glide(220.0, 110.0, 0.6));
        sentence.extend(glide(150.0, 150.0, 0.6));
        let first = analyze(&sentence, None, &[], None);
        assert!(first.contour.is_none(), "a sentence isn't judged");
        let v = first.voice.as_ref().expect("a range after one recording");
        assert!(v.low_hz > 100.0 && v.high_hz < 240.0, "range {v:?}");
        let mut kept = vec![Earlier { pitch: first.pitch, ..Default::default() }];
        for (tone, pcm) in [
            (1, glide(205.0, 205.0, 0.5)),
            (2, glide(140.0, 210.0, 0.5)),
            (4, glide(215.0, 115.0, 0.4)),
        ] {
            let a = analyze(&pcm, Some(tone), &kept, None);
            assert_eq!(a.heard, Some(tone), "tone {tone}: {:?} {:?}", a.contour, a.probs);
            let total: f32 = a.probs.unwrap().iter().sum();
            assert!((total - 1.0).abs() < 1e-3);
            assert!(a.segment.is_some() && a.voiced_ms > 300.0);
            let ((s, e), (m0, m1)) = (a.segment.unwrap(), a.measured.unwrap());
            assert!(s <= m0 && m0 < m1 && m1 <= e + a.creak_frames, "measured {m0}..{m1} in {s}..{e}");
            kept.push(Earlier { pitch: a.pitch, tone: Some(tone), length: a.length_frames });
        }
        let p = build(&kept);
        assert_eq!(p.recordings, 4);
        assert!(!info(&p).ready, "needs {} recordings", tones::PROFILE_MIN_RECORDINGS);
        kept.push(Earlier { pitch: analyze(&glide(130.0, 120.0, 0.5), Some(3), &kept, None).pitch, ..Default::default() });
        assert!(info(&build(&kept)).ready);
    }

    /// Check the tracker on saved calibration recordings (Settings saves them in
    /// <app data>/calibration-recordings): how much of the speech has a pitch, and every break
    /// (a run of frames with no pitch while the voice is still loud).
    /// CALIB_DIR=<folder> cargo test --lib calibration_recordings -- --ignored --nocapture
    #[test]
    #[ignore]
    fn calibration_recordings() {
        let Ok(dir) = std::env::var("CALIB_DIR") else { return };
        let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "wav")).collect();
        files.sort();
        let mut earlier: Vec<Earlier> = Vec::new();
        let (mut last_t1, mut last_t3): (Option<Vec<tones::Frame>>, Option<Vec<tones::Frame>>) = (None, None);
        for f in files {
            let bytes = std::fs::read(&f).unwrap();
            let pcm: Vec<f32> = bytes[44..].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).collect();
            let name = f.file_name().unwrap().to_string_lossy().to_string();
            let tone = name.split("tone").nth(1).and_then(|x| x[..1].parse::<u8>().ok());
            let (frames, hum) = tones::pitch_track_hum(&pcm);
            println!("  hum removed: {hum:?}");
            let loud = frames.iter().map(|f| f.rms).fold(0f32, f32::max);
            let peak = pcm.iter().fold(0f32, |m, v| m.max(v.abs()));
            // speech: from the first to the last frame louder than 8% of the loudest
            let speaking: Vec<usize> = (0..frames.len()).filter(|&i| frames[i].rms >= loud * 0.08).collect();
            let (first, last) = (*speaking.first().unwrap_or(&0), *speaking.last().unwrap_or(&0));
            let loud_frames = speaking.len();
            let pitched = frames.iter().filter(|f| f.st.is_some()).count();
            println!(
                "\n{name}: {:.2} s, peak {:.2}, speech {:.2}-{:.2} s, pitched {pitched}/{loud_frames} loud frames ({:.0}%)",
                pcm.len() as f32 / 16000.0,
                peak,
                first as f32 / 100.0,
                last as f32 / 100.0,
                100.0 * pitched as f32 / loud_frames.max(1) as f32
            );
            // breaks: unpitched runs inside the speech where it's still at least 15% as loud
            let mut i = first;
            while i <= last {
                if frames[i].st.is_none() && frames[i].rms >= loud * 0.15 {
                    let s = i;
                    while i <= last && frames[i].st.is_none() && frames[i].rms >= loud * 0.15 {
                        i += 1;
                    }
                    let mean = frames[s..i].iter().map(|f| f.rms).sum::<f32>() / (i - s) as f32 / loud;
                    println!("  break {:.2}-{:.2} s ({} ms, loudness {:.0}%)", s as f32 / 100.0, i as f32 / 100.0, (i - s) * 10, mean * 100.0);
                } else {
                    i += 1;
                }
            }
            // CALIB_FRAMES=1: every frame of the speech: raw pitch (before cleaning), kept
            // pitch, loudness
            if std::env::var_os("CALIB_FRAMES").is_some() {
                let raw = tones::pitch_raw(&tones::remove_hum(&pcm).0);
                for i in first.saturating_sub(3)..(last + 4).min(frames.len()) {
                    let hz = |s: Option<f32>| s.map_or("   .".into(), |s| format!("{:4.0}", tones::hz(s)));
                    println!("    {:.2}s raw {} kept {} loud {:3.0}%", i as f32 / 100.0, hz(raw[i].st), hz(frames[i].st), 100.0 * frames[i].rms / loud);
                }
            }
            // the pitch every 50 ms over the speech, in Hz ('.' = none)
            let line: Vec<String> = (first..=last).step_by(5).map(|i| frames[i].st.map_or(".".into(), |st| format!("{:.0}", tones::hz(st)))).collect();
            println!("  Hz: {}", line.join(" "));
            let a = analyze(&pcm, tone, &earlier, None);
            if let Some(v) = &a.voice {
                println!("  voice so far: {:.0} Hz ({:.0}-{:.0}), spread {:.1} st", v.median_hz, v.low_hz, v.high_hz, v.spread_st);
            }
            if tone.is_some() {
                println!("  asked tone {tone:?}, heard {:?}, probs {:?}, contour {:?}, measured {:?}, creak {}", a.heard, a.probs.map(|p| p.map(|x| (x * 100.0).round())), a.contour.map(|c| c.map(|x| (x * 10.0).round() / 10.0)), a.measured, a.creak_frames);
                for adv in &a.advice {
                    println!("  advice {}: aim {:.0} Hz, was {:.0} Hz", adv.key, adv.target_hz, adv.actual_hz);
                }
            }
            // the same, with the range anchored on this calibration's tone 1 and tone 3
            match tone {
                Some(1) => last_t1 = Some(frames.clone()),
                Some(3) => last_t3 = Some(frames.clone()),
                _ => {}
            }
            if let (Some(t), Some(v)) = (tone, last_t1.as_ref().zip(last_t3.as_ref()).and_then(|(a, b)| tones::anchor_voice(a, b))) {
                if let Some(Some(lp)) = tones::tone_logprobs(&frames, 1, v, Some(a.typical_length)).first() {
                    let p = lp.map(|x| (x.exp() * 100.0).round());
                    let heard = (0..4).max_by(|&a, &b| p[a].total_cmp(&p[b])).unwrap() + 1;
                    let c = tones::syllables(&frames, 1).first().and_then(|&r| tones::contour(&frames, r, v));
                    println!("  ANCHORED asked {t} heard {heard} probs {p:?} contour {:?} (range {:.0}-{:.0} Hz)", c.map(|c| c.map(|x| (x * 10.0).round() / 10.0)), tones::hz(v.median - v.spread / 2.0), tones::hz(v.median + v.spread / 2.0));
                }
            }
            earlier.push(Earlier { pitch: a.pitch, tone, length: a.length_frames });
        }
    }

    /// A low voice with 120 Hz mains hum under it (a raw microphone near wiring): the hum is
    /// found and removed, the pauses don't get its pitch, and the voice's range is the voice's.
    #[test]
    fn mains_hum_is_removed() {
        let mut pcm = vec![0f32; 8000];
        pcm.extend(glide(140.0, 90.0, 0.5));
        pcm.extend(vec![0f32; 4000]);
        pcm.extend(glide(95.0, 145.0, 0.5));
        pcm.extend(vec![0f32; 8000]);
        let hum = |i: usize| 0.03 * (2.0 * std::f32::consts::PI * 120.0 * i as f32 / 16000.0).sin();
        let noisy: Vec<f32> = pcm.iter().enumerate().map(|(i, v)| v + hum(i)).collect();
        assert_eq!(tones::find_hum(&pcm), Vec::<f32>::new(), "no hum in the clean voice");
        let found = tones::find_hum(&noisy);
        assert!(found.len() == 1 && (found[0] - 120.0).abs() < 0.5, "found {found:?}");
        let a = analyze(&noisy, None, &[], None);
        // the silences (the first and last 0.5 s) have no pitch
        assert!(a.pitch[..40].iter().chain(&a.pitch[a.pitch.len() - 40..]).all(|p| p.is_none()), "hum tracked as pitch");
        let v = a.voice.unwrap();
        assert!(v.low_hz < 105.0 && v.high_hz > 125.0, "voice range {v:?}");
    }

    #[test]
    fn advice_for_a_mid_voice_fourth_tone() {
        let v = Voice { median: 3.3, spread: 7.5 }; // about 121 Hz, 97-150
        // one learner's 4th tone: starts in the middle, drops into creak
        let adv = advise(4, &[0.4, 0.3, -2.0, -2.0, -2.0], v, None);
        assert_eq!(adv.iter().map(|a| a.key).collect::<Vec<_>>(), ["t4Higher"]);
        assert!(adv[0].target_hz > 145.0 && adv[0].actual_hz < 130.0, "{adv:?}");
        // the shapes themselves need no advice
        for (t, c) in tones::params().templates {
            assert!(advise(t, &c, v, None).is_empty(), "tone {t} {c:?}: {:?}", advise(t, &c, v, None));
        }
        assert_eq!(advise(3, &[0.1, -0.3, 0.2, 0.7, -0.4], v, None)[0].key, "t3Lower");
        assert_eq!(advise(2, &[0.0, -0.2, 0.0, 0.3, 0.5], v, None)[0].key, "t2Rise");
    }

    #[test]
    fn silence_gives_no_judgement() {
        let a = analyze(&vec![0f32; 16000], Some(1), &[], None);
        assert!(a.heard.is_none() && a.voice.is_none() && a.voiced_ms == 0.0);
        assert_eq!(a.profile.recordings, 0);
    }
}
