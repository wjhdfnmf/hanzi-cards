//! Picking the right word among ones that differ only in tone (妈 麻 马 骂), from the
//! pitch of the recording (tones.rs) together with how likely Whisper finds each word.
//!
//! Whisper alone writes the word it expects more than the tone it hears: on six native
//! speakers it picks the right one of four tones about half the time, the pitch tracker about
//! 90% (docs/tone-recognition.md). Used for spoken answers when the "pitch_tones" setting is
//! on: when Whisper writes the answer or a same-syllable word, the candidates are re-scored
//! as Whisper's log-probability + PITCH_WEIGHT × the pitch's log-probability.
//!
//! Measured on Tone Perfect: Catherine Ryu, Mandarin Tone Perception & Production Team, and
//! Michigan State University Libraries. Tone Perfect: Multimodal Database for Mandarin
//! Chinese. Accessed 1 January 2022. https://tone.lib.msu.edu/

use crate::error::AppResult;
use crate::pinyin;
use crate::speech;
use crate::tones::{self, Frame, Voice};
use rusqlite::Connection;

/// How much the pitch counts against Whisper's score. Tuned with `tone_select_bench` on Tone
/// Perfect (720 clips, right word of four): Whisper only 43% (small) / 57% (large-v3-turbo),
/// weight 1 → 90%, 2 → 92%, 4 → 92%, pitch only 91.5%. 2 keeps Whisper as a tie-breaker.
pub const PITCH_WEIGHT: f32 = 2.0;

/// Same-syllable words to consider besides the answer (most common first), plus Whisper's
/// own word when it isn't among them (`add_heard`).
pub const MAX_VARIANTS: usize = 7;

/// Words that could have been meant: the answer and the same syllables in other tones.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub text: String,
    /// Tones as said (sandhi applied), one per character; 5 = neutral, not judged.
    pub tones: Vec<u8>,
}

fn digits(numbered: &str) -> Vec<u8> {
    numbered.chars().filter_map(|c| c.to_digit(10)).map(|d| d as u8).collect()
}

/// The card's answer first, then up to `max` words with the same syllables and other tones.
/// Empty when the answer's tones aren't known for every character.
pub fn candidates(conn: &Connection, hanzi: &str, card_pinyin: &str, max: usize) -> AppResult<Vec<Candidate>> {
    let Some(answer) = speech::answer_forms(hanzi).into_iter().next() else { return Ok(vec![]) };
    let n = answer.chars().count();
    let tones = digits(&pinyin::marks_to_numbered(card_pinyin));
    if tones.len() != n {
        return Ok(vec![]);
    }
    let mut out = vec![Candidate { tones: pinyin::spoken_tones(&answer, &tones), text: answer }];
    for (text, num) in speech::tone_variants(conn, hanzi, card_pinyin, max)? {
        let t = digits(&num);
        if t.len() == n && text.chars().count() == n {
            out.push(Candidate { tones: pinyin::spoken_tones(&text, &t), text });
        }
    }
    Ok(out)
}

/// Add Whisper's word when it has the answer's syllables but isn't a candidate yet (a less
/// common word, like 鼠 shǔ for 书 shū), so the pitch can still choose.
pub fn add_heard(conn: &Connection, cands: &mut Vec<Candidate>, heard: &str) -> AppResult<()> {
    let Some(answer) = cands.first() else { return Ok(()) };
    if cands.iter().any(|c| c.text == heard) || heard.chars().count() != answer.text.chars().count() {
        return Ok(());
    }
    let row: Option<(String, String)> = conn
        .query_row(
            "SELECT pinyin_num, pinyin_key FROM dictionary WHERE simplified = ?1 ORDER BY score DESC LIMIT 1",
            [heard],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok();
    let answer_key = conn
        .query_row("SELECT pinyin_key FROM dictionary WHERE simplified = ?1 ORDER BY score DESC LIMIT 1", [&answer.text], |r| r.get::<_, String>(0))
        .ok();
    if let (Some((num, key)), Some(answer_key)) = (row, answer_key) {
        let t = digits(&num);
        if key == answer_key && t.len() == answer.tones.len() {
            cands.push(Candidate { tones: pinyin::spoken_tones(heard, &t), text: heard.to_string() });
        }
    }
    Ok(())
}

/// For each candidate, the summed log-probability of its tones from the pitch (neutral
/// syllables and syllables without pitch count 0). None when no syllable could be judged or
/// every candidate gets the same score.
pub fn pitch_scores(frames: &[Frame], voice: Voice, typical_length: Option<f32>, cands: &[Candidate]) -> Option<Vec<f32>> {
    let n = cands.first()?.tones.len();
    let probs = tones::tone_logprobs(frames, n, voice, typical_length);
    if probs.iter().all(Option::is_none) {
        return None;
    }
    let scores: Vec<f32> = cands
        .iter()
        .map(|c| {
            c.tones
                .iter()
                .zip(&probs)
                .map(|(&t, p)| match (t, p) {
                    (1..=4, Some(p)) => p[t as usize - 1],
                    _ => 0.0,
                })
                .sum()
        })
        .collect();
    let same = scores.iter().all(|s| (s - scores[0]).abs() < 1e-6);
    (!same).then_some(scores)
}

/// The index of the best candidate: Whisper's score plus the weighted pitch score.
pub fn choose(whisper: &[f32], pitch: Option<&[f32]>, weight: f32) -> usize {
    (0..whisper.len())
        .max_by(|&a, &b| {
            let s = |i: usize| whisper[i] + pitch.map(|p| weight * p[i]).unwrap_or(0.0);
            s(a).total_cmp(&s(b))
        })
        .unwrap_or(0)
}

/// The word to use instead of Whisper's transcript `heard` (simplified, characters only),
/// or None to keep it: when `heard` is one of the candidates and the learner's pitch range is
/// known, the candidate with the best Whisper + pitch score. Adds this recording to the
/// profile either way.
pub fn select(
    app_data: &std::path::Path,
    model: &str,
    pcm: &[f32],
    heard: &str,
    cands: &[Candidate],
    profile: &mut tones::PitchProfile,
    trim: Option<(f32, f32)>,
) -> AppResult<Option<String>> {
    let frames = tones::pitch_track(pcm);
    // the range and usual syllable length before this recording
    let voice = profile.voice();
    let typical = profile.typical_length().unwrap_or(tones::TYPICAL_LENGTH);
    profile.add(&frames);
    if cands.first().is_some_and(|c| c.tones.len() == 1) {
        if let Some(&seg) = tones::syllables(&frames, 1).first() {
            profile.add_length(tones::syllable_length(&frames, seg));
        }
    }
    let (Some(voice), true) = (voice, cands.len() > 1 && cands.iter().any(|c| c.text == heard)) else { return Ok(None) };
    let Some(pitch) = pitch_scores(&frames, voice, Some(typical), cands) else { return Ok(None) };
    // no prompt or word bias here: they would favour the answer, which is what's being checked
    let opts = crate::whisper::Options { chinese_only: true, trim, ..Default::default() };
    let texts: Vec<String> = cands.iter().map(|c| c.text.clone()).collect();
    let scores = crate::whisper::score(app_data, model, pcm, &opts, &texts)?;
    Ok(Some(cands[choose(&scores, Some(&pitch), PITCH_WEIGHT)].text.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sandhi() {
        assert_eq!(pinyin::spoken_tones("你好", &[3, 3]), vec![2, 3]);
        assert_eq!(pinyin::spoken_tones("不是", &[4, 4]), vec![2, 4]);
        assert_eq!(pinyin::spoken_tones("不好", &[4, 3]), vec![4, 3]);
        assert_eq!(pinyin::spoken_tones("一起", &[1, 3]), vec![4, 3]);
        assert_eq!(pinyin::spoken_tones("一样", &[1, 4]), vec![2, 4]);
        assert_eq!(pinyin::spoken_tones("第一", &[4, 1]), vec![4, 1]);
        assert_eq!(pinyin::spoken_tones("妈妈", &[1, 5]), vec![1, 5]);
    }

    #[test]
    fn choose_weighs_pitch() {
        let whisper = [-1.0, -3.0];
        assert_eq!(choose(&whisper, None, 1.0), 0);
        assert_eq!(choose(&whisper, Some(&[-5.0, -0.1]), 1.0), 1);
        assert_eq!(choose(&whisper, Some(&[-5.0, -0.1]), 0.2), 0);
    }

    #[test]
    fn profile_round_trip() {
        let mut p = tones::PitchProfile::parse("");
        assert!(p.voice().is_none());
        let frames: Vec<Frame> = (0..200).map(|i| Frame { st: Some(10.0 + (i % 20) as f32 * 0.5), rms: 0.5 }).collect();
        for _ in 0..tones::PROFILE_MIN_RECORDINGS {
            p.add(&frames);
        }
        let q = tones::PitchProfile::parse(&p.serialize());
        let v = q.voice().unwrap();
        assert_eq!(q.recordings, tones::PROFILE_MIN_RECORDINGS);
        assert!((v.median - 15.0).abs() < 1.0, "{v:?}");
    }

    fn read_wav(path: &std::path::Path) -> Vec<f32> {
        let b = std::fs::read(path).unwrap();
        b[44..].chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0).collect()
    }

    /// The app's path end to end: for a card (跑 pǎo, …), Tone Perfect clips of its syllable in
    /// all four tones, as a learner who meant the card's word but said that tone. Whisper
    /// transcribes with a word-list prompt and the card word favoured, like a drill; then
    /// `select` picks among the dictionary's same-syllable words. Counts how often the final
    /// word has the tone that was actually said.
    /// WHISPER_DIR=<app data> TP_DIR=../tone-data/tp/all cargo test --features gpu --lib tone_select_e2e -- --ignored --nocapture
    #[test]
    #[ignore]
    fn tone_select_e2e() {
        let (Ok(app), Ok(tp)) = (std::env::var("WHISPER_DIR"), std::env::var("TP_DIR")) else { return };
        let app = std::path::Path::new(&app);
        let tp = std::path::Path::new(&tp);
        let model = std::env::var("WHISPER_MODEL").unwrap_or_else(|_| "small".into());
        // read-only: never write to the learner's real database from a test
        let conn = Connection::open_with_flags(app.join("hanzi-cards.db"), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let meta: Vec<(usize, String, u8, String)> = std::fs::read_to_string(tp.join("meta.tsv"))
            .unwrap()
            .lines()
            .map(|l| {
                let f: Vec<&str> = l.split('\t').collect();
                (f[0].parse().unwrap(), f[1].to_string(), f[2].parse().unwrap(), f[3].to_string())
            })
            .collect();
        let cards = [("跑", "pǎo", "pao"), ("同", "tóng", "tong"), ("影", "yǐng", "ying"), ("书", "shū", "shu"), ("位", "wèi", "wei"), ("题", "tí", "ti"), ("去", "qù", "qu"), ("想", "xiǎng", "xiang"), ("臭", "chòu", "chou"), ("气", "qì", "qi")];
        let trim = if crate::whisper::using_gpu() { None } else { Some((2.0, 10.0)) };
        let mut speakers: Vec<&str> = meta.iter().map(|m| m.3.as_str()).collect();
        speakers.sort();
        speakers.dedup();
        // [said tone]: (whisper's word had that tone, final word had it, clips)
        let mut tally = [(0u32, 0u32, 0u32); 5];
        let mut changed = Vec::new();
        let diag_weights = [1.0f32, 2.0, 4.0, 8.0, 1000.0];
        let (mut by_weight, mut applied, mut outside, mut outside_seen) = (vec![0u32; diag_weights.len()], 0u32, 0u32, Vec::new());
        for spk in &speakers {
            // the learner's range from 30 earlier answers (other syllables)
            let mut profile = tones::PitchProfile::parse("");
            for m in meta.iter().filter(|m| m.3 == *spk && !cards.iter().any(|c| c.2 == m.1)).step_by(37).take(30) {
                profile.add(&tones::pitch_track(&read_wav(&tp.join("clean").join(format!("{}.wav", m.0)))));
            }
            for (hanzi, py, syl) in cards {
                let base = candidates(&conn, hanzi, py, MAX_VARIANTS).unwrap();
                for m in meta.iter().filter(|m| m.3 == *spk && m.1 == syl) {
                    let mut cands = base.clone();
                    let said = m.2;
                    if !cands.iter().any(|c| c.tones == [said]) {
                        continue; // no word with that tone to pick
                    }
                    let pcm = read_wav(&tp.join("clean").join(format!("{}.wav", m.0)));
                    let opts = crate::whisper::Options {
                        prompt: format!("你好，{hanzi}，学生，老师。"),
                        bias_words: vec![hanzi.to_string()],
                        bias: 1.0,
                        chinese_only: true,
                        trim,
                        ..Default::default()
                    };
                    let text = crate::whisper::transcribe(app, &model, &pcm, "zh", &opts).unwrap();
                    let heard: String = speech::to_simplified(&conn, &text).unwrap().chars().filter(|c| pinyin::is_hanzi(*c)).collect();
                    add_heard(&conn, &mut cands, &heard).unwrap();
                    // Whisper's word's tone, also when it isn't a candidate
                    let heard_tone = cands.iter().find(|c| c.text == heard).and_then(|c| c.tones.first().copied()).or_else(|| {
                        conn.query_row("SELECT pinyin_num FROM dictionary WHERE simplified = ?1 ORDER BY score DESC LIMIT 1", [&heard], |r| r.get::<_, String>(0))
                            .ok()
                            .and_then(|n| digits(&n).first().copied())
                    });
                    let tone_of = |w: &str| if w == heard { heard_tone } else { cands.iter().find(|c| c.text == w).and_then(|c| c.tones.first().copied()) };
                    let mut p = profile.clone();
                    let chosen = select(app, &model, &pcm, &heard, &cands, &mut p, trim).unwrap();
                    let fin = chosen.clone().unwrap_or(heard.clone());
                    // diagnostics: other weights, and how often Whisper's word isn't a candidate
                    if !cands.iter().any(|c| c.text == heard) {
                        outside += 1;
                        if outside_seen.len() < 12 {
                            outside_seen.push(format!("{syl}{said}:{heard}"));
                        }
                    } else if let Some(ps) = pitch_scores(&tones::pitch_track(&pcm), profile.voice().unwrap(), profile.typical_length(), &cands) {
                        let texts: Vec<String> = cands.iter().map(|c| c.text.clone()).collect();
                        let ws = crate::whisper::score(app, &model, &pcm, &crate::whisper::Options { chinese_only: true, trim, ..Default::default() }, &texts).unwrap();
                        for (k, w) in diag_weights.iter().enumerate() {
                            by_weight[k] += (cands[choose(&ws, Some(&ps), *w)].tones == [said]) as u32;
                        }
                        applied += 1;
                    }
                    let t = &mut tally[said as usize];
                    t.0 += (tone_of(&heard) == Some(said)) as u32;
                    t.1 += (tone_of(&fin) == Some(said)) as u32;
                    t.2 += 1;
                    if fin != heard && changed.len() < 25 {
                        changed.push(format!("{spk} {syl}{said}: {heard}→{fin}"));
                    }
                }
            }
        }
        let sum = tally.iter().fold((0, 0, 0), |a, t| (a.0 + t.0, a.1 + t.1, a.2 + t.2));
        for (t, (w, f, n)) in tally.iter().enumerate().skip(1) {
            println!("said tone {t}: Whisper's word {w}/{n}, with pitch {f}/{n}");
        }
        println!("Whisper's word not a candidate: {outside} ({})", outside_seen.join(" "));
        println!(
            "where pitch applied ({applied}): {}",
            diag_weights.iter().zip(&by_weight).map(|(w, r)| format!("w{w} {r}/{applied}")).collect::<Vec<_>>().join(" · ")
        );
        println!("{model} e2e: word with the tone said: Whisper {}/{} → with pitch {}/{}", sum.0, sum.2, sum.1, sum.2);
        println!("changed: {}", changed.join("  "));
    }

    /// Whisper alone vs pitch alone vs both, on Tone Perfect's 40-clip folders
    /// (tone-data/tp/whisper/<speaker>-g<k>: ten syllables × four tones, characters in
    /// list.tsv). Each speaker's range comes from a profile built from all their clips there.
    /// WHISPER_DIR=<app data> TP_WHISPER=../tone-data/tp/whisper cargo test --features gpu --lib tone_select_bench -- --ignored --nocapture
    #[test]
    #[ignore]
    fn tone_select_bench() {
        let (Ok(app), Ok(root)) = (std::env::var("WHISPER_DIR"), std::env::var("TP_WHISPER")) else { return };
        let app = std::path::Path::new(&app);
        let model = std::env::var("WHISPER_MODEL").unwrap_or_else(|_| "small".into());
        let set = std::env::var("BENCH_SET").unwrap_or_else(|_| "clean".into());
        let weights = [0.0f32, 0.25, 0.5, 1.0, 2.0, 4.0, 1000.0];
        let mut dirs: Vec<std::path::PathBuf> = std::fs::read_dir(&root).unwrap().map(|e| e.unwrap().path()).collect();
        dirs.sort();
        let speaker = |d: &std::path::Path| d.file_name().unwrap().to_string_lossy().split('-').next().unwrap().to_string();
        let mut right = vec![0u32; weights.len()];
        let mut total = 0u32;
        let mut speakers: Vec<String> = dirs.iter().map(|d| speaker(d)).collect();
        speakers.dedup();
        let opts = crate::whisper::Options { trim: if crate::whisper::using_gpu() { None } else { Some((2.0, 10.0)) }, ..Default::default() };
        for spk in &speakers {
            let mine: Vec<&std::path::PathBuf> = dirs.iter().filter(|d| speaker(d) == *spk).collect();
            let mut clips = Vec::new(); // (pcm, frames, candidates)
            for d in &mine {
                let list: Vec<String> = std::fs::read_to_string(d.join("list.tsv")).unwrap().lines().map(|l| l.split('\t').next().unwrap().to_string()).collect();
                for (i, w) in list.iter().enumerate() {
                    let pcm = read_wav(&d.join(&set).join(format!("{i}.wav")));
                    let frames = tones::pitch_track(&pcm);
                    let set4 = &list[i / 4 * 4..i / 4 * 4 + 4];
                    // the right word first, then the other three tones
                    let mut cands = vec![Candidate { text: w.clone(), tones: vec![(i % 4) as u8 + 1] }];
                    for (k, o) in set4.iter().enumerate() {
                        if o != w {
                            cands.push(Candidate { text: o.clone(), tones: vec![k as u8 + 1] });
                        }
                    }
                    clips.push((pcm, frames, cands));
                }
            }
            let mut profile = tones::PitchProfile::parse("");
            for (_, f, _) in &clips {
                profile.add(f);
            }
            let voice = profile.voice().unwrap();
            let mut spk_right = vec![0u32; weights.len()];
            for (pcm, frames, cands) in &clips {
                let texts: Vec<String> = cands.iter().map(|c| c.text.clone()).collect();
                let ws = crate::whisper::score(app, &model, pcm, &opts, &texts).unwrap();
                let ps = pitch_scores(frames, voice, Some(tones::TYPICAL_LENGTH), cands);
                for (k, &w) in weights.iter().enumerate() {
                    let ok = choose(&ws, ps.as_deref(), w) == 0;
                    right[k] += ok as u32;
                    spk_right[k] += ok as u32;
                }
                total += 1;
            }
            println!("{spk} ({:.0}±{:.0} st): {}", voice.median, voice.spread, weights.iter().zip(&spk_right).map(|(w, r)| format!("w{w}:{r}/{}", clips.len())).collect::<Vec<_>>().join(" "));
        }
        println!(
            "{model} {set} right word of 4 (w0 = Whisper only, w1000 = pitch only): {}",
            weights.iter().zip(&right).map(|(w, r)| format!("w{w} {:.1}%", 100.0 * *r as f32 / total as f32)).collect::<Vec<_>>().join(" · ")
        );
    }
}
