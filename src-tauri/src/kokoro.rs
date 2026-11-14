//! The built-in voice: Kokoro (Chinese + English, Apache-2.0), run on the processor by
//! sherpa-onnx. It is the backup for computers with no Chinese system voice, and a choice
//! everywhere. Nothing of it ships with the app: the model (147 MB to download, 200 MB on
//! disk) is fetched in Settings › Audio and kept in `<app data>/voices`.
//!
//! Measured on this project's test set (docs/local-tts.md): every sentence right, single
//! words less sure than the Windows voices, about real time on the processor.

use crate::error::{AppError, AppResult};
use sherpa_onnx::{GenerationConfig, OfflineTts, OfflineTtsConfig, OfflineTtsKokoroModelConfig, OfflineTtsModelConfig};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const NAME: &str = "kokoro-int8-multi-lang-v1_1";
const URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/kokoro-int8-multi-lang-v1_1.tar.bz2";
/// The archive as published (checked 2026-11-13): anything else is thrown away.
const SHA256: &str = "259c9c8bc906d6078e4e0570421885dabb68b539c2f970fcbb890a58eeaf3e9c";
const DOWNLOAD_BYTES: u64 = 147_052_523;
/// The speaker used (of the model's 103): a clear Mandarin woman's voice, the one measured.
const SPEAKER: i32 = 3;

pub fn dir(app_data: &Path) -> PathBuf {
    app_data.join("voices").join(NAME)
}

pub fn installed(app_data: &Path) -> bool {
    let d = dir(app_data);
    ["model.int8.onnx", "voices.bin", "tokens.txt", "lexicon-zh.txt"].iter().all(|f| d.join(f).is_file())
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub installed: bool,
    pub download_mb: u64,
}

pub fn status(app_data: &Path) -> Status {
    Status { installed: installed(app_data), download_mb: DOWNLOAD_BYTES / 1_000_000 }
}

/// Download the voice, check it, and unpack it; reports (bytes so far, total).
pub fn download(app_data: &Path, mut progress: impl FnMut(u64, u64)) -> AppResult<()> {
    let voices = app_data.join("voices");
    std::fs::create_dir_all(&voices)?;
    let part = voices.join("download.part");
    let resp = crate::net::get(URL).call().map_err(|e| AppError::msg(format!("Couldn't download the voice: {e}")))?;
    let total = resp.header("Content-Length").and_then(|s| s.parse::<u64>().ok()).unwrap_or(DOWNLOAD_BYTES);
    let mut reader = resp.into_reader();
    let mut file = std::fs::File::create(&part)?;
    let mut hasher = <sha2::Sha256 as sha2::Digest>::new();
    let mut buf = vec![0u8; 1 << 16];
    let mut done = 0u64;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        sha2::Digest::update(&mut hasher, &buf[..n]);
        file.write_all(&buf[..n])?;
        done += n as u64;
        progress(done, total.max(done));
    }
    file.sync_all()?;
    drop(file);
    let got: String = sha2::Digest::finalize(hasher).iter().map(|b| format!("{b:02x}")).collect();
    if got != SHA256 {
        let _ = std::fs::remove_file(&part);
        return Err(AppError::msg("The download was cut short or isn't the file expected. Please try again."));
    }
    // unpack beside the final place, then move it in whole: a half-unpacked voice never counts
    let staging = voices.join("unpacking");
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)?;
    let unpacked = (|| -> std::io::Result<()> {
        let mut archive = tar::Archive::new(bzip2::read::BzDecoder::new(std::fs::File::open(&part)?));
        for entry in archive.entries()? {
            // (`unpack_in` refuses any path that would leave the folder)
            entry?.unpack_in(&staging)?;
        }
        Ok(())
    })();
    let _ = std::fs::remove_file(&part);
    if let Err(e) = unpacked {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(AppError::msg(format!("Couldn't unpack the voice: {e}")));
    }
    let target = dir(app_data);
    let _ = std::fs::remove_dir_all(&target);
    std::fs::rename(staging.join(NAME), &target)?;
    let _ = std::fs::remove_dir_all(&staging);
    if !installed(app_data) {
        return Err(AppError::msg("The voice's files aren't all there after unpacking."));
    }
    Ok(())
}

/// Remove the downloaded voice.
pub fn remove(app_data: &Path) -> AppResult<()> {
    *ENGINE.lock().unwrap_or_else(|e| e.into_inner()) = None; // its files are open while loaded
    let d = dir(app_data);
    if d.exists() {
        std::fs::remove_dir_all(&d)?;
    }
    Ok(())
}

/// The loaded model (about a second to load, so it is kept). sherpa-onnx's handle is a C
/// object that may move between threads as long as only one uses it at a time: the mutex.
struct Engine(OfflineTts);
unsafe impl Send for Engine {}
static ENGINE: Mutex<Option<Engine>> = Mutex::new(None);

fn load(d: &Path) -> Result<OfflineTts, String> {
    let f = |name: &str| Some(d.join(name).to_string_lossy().replace('\\', "/"));
    // the model's own rules for reading dates, numbers and phone numbers
    let rules = ["date-zh.fst", "number-zh.fst", "phone-zh.fst"]
        .iter()
        .filter(|n| d.join(n).is_file())
        .filter_map(|n| f(n))
        .collect::<Vec<_>>()
        .join(",");
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 4) as i32;
    let config = OfflineTtsConfig {
        model: OfflineTtsModelConfig {
            kokoro: OfflineTtsKokoroModelConfig {
                model: f("model.int8.onnx"),
                voices: f("voices.bin"),
                tokens: f("tokens.txt"),
                lexicon: Some(format!("{},{}", f("lexicon-us-en.txt").unwrap(), f("lexicon-zh.txt").unwrap())),
                data_dir: f("espeak-ng-data"),
                dict_dir: f("dict"),
                length_scale: 1.0,
                ..Default::default()
            },
            num_threads: threads,
            ..Default::default()
        },
        rule_fsts: Some(rules).filter(|s| !s.is_empty()),
        ..Default::default()
    };
    OfflineTts::create(&config).ok_or_else(|| "The built-in voice couldn't be loaded. Removing it and downloading it again (Settings › Audio) may help.".to_string())
}

/// What is handed to the model: a lone word gets a full stop. The model is trained on
/// sentences and clips or mis-tones a bare word; with the stop it scored 41 of 60 card words
/// instead of 34.
fn prepared(text: &str) -> String {
    let t = text.trim();
    let ends = t.chars().last().is_some_and(|c| "。！？!?.…；;，,、".contains(c));
    if ends || t.chars().count() > 12 {
        t.to_string()
    } else {
        format!("{t}。")
    }
}

/// Say `text` at `rate` (1 = normal); returns a WAV file (16-bit, the model's 24 kHz).
pub fn synthesize(app_data: &Path, text: &str, rate: f64) -> Result<Vec<u8>, String> {
    if text.trim().is_empty() {
        return Err("Nothing to say.".into());
    }
    if !installed(app_data) {
        return Err("The built-in voice isn't installed (Settings › Audio).".into());
    }
    let mut guard = ENGINE.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        *guard = Some(Engine(load(&dir(app_data))?));
    }
    let tts = &guard.as_ref().expect("just loaded").0;
    let config = GenerationConfig { sid: SPEAKER, speed: rate.clamp(0.5, 2.0) as f32, ..Default::default() };
    let audio = tts
        .generate_with_config(&prepared(text), &config, None::<fn(&[f32], f32) -> bool>)
        .ok_or_else(|| "The built-in voice couldn't say that.".to_string())?;
    Ok(wav(audio.samples(), audio.sample_rate() as u32))
}

/// A 16-bit mono WAV, at a steady loudness (the model's clips vary).
fn wav(samples: &[f32], rate: u32) -> Vec<u8> {
    let peak = samples.iter().fold(0f32, |m, x| m.max(x.abs()));
    let gain = if peak > 1e-4 { (0.9 / peak).min(8.0) } else { 1.0 };
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + samples.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for v in samples {
        out.extend_from_slice(&((v * gain).clamp(-1.0, 1.0) * 32767.0).round().to_le_bytes_i16());
    }
    out
}

trait I16Bytes {
    fn to_le_bytes_i16(self) -> [u8; 2];
}
impl I16Bytes for f32 {
    fn to_le_bytes_i16(self) -> [u8; 2] {
        (self as i16).to_le_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lone_word_gets_a_full_stop() {
        assert_eq!(prepared(" 图书馆 "), "图书馆。");
        assert_eq!(prepared("你好吗？"), "你好吗？");
        assert_eq!(prepared("我今天去图书馆学习，然后回家吃饭"), "我今天去图书馆学习，然后回家吃饭");
    }

    #[test]
    fn wav_has_a_header_and_steady_loudness() {
        let w = wav(&[0.0, 0.3, -0.3, 0.15], 24000);
        assert_eq!(&w[..4], b"RIFF");
        assert_eq!(w.len(), 44 + 8);
        let peak = i16::from_le_bytes([w[46], w[47]]);
        assert!((29000..=29600).contains(&peak), "{peak}");
    }

    /// The real download (147 MB): KOKORO_DL=<empty folder> cargo test -- --ignored downloads_and_unpacks
    #[test]
    #[ignore]
    fn downloads_and_unpacks() {
        let app_data = PathBuf::from(std::env::var("KOKORO_DL").expect("KOKORO_DL"));
        assert!(!installed(&app_data));
        let mut last = (0, 0);
        download(&app_data, |got, total| last = (got, total)).unwrap();
        assert_eq!(last.0, DOWNLOAD_BYTES);
        assert!(installed(&app_data));
        assert!(!app_data.join("voices").join("download.part").exists() && !app_data.join("voices").join("unpacking").exists());
        assert!(synthesize(&app_data, "你好", 1.0).unwrap().len() > 10_000);
        remove(&app_data).unwrap();
        assert!(!installed(&app_data));
    }

    /// Needs the voice on disk: KOKORO_DIR=<folder holding voices/…> cargo test -- --ignored says_a_word
    #[test]
    #[ignore]
    fn says_a_word() {
        let app_data = PathBuf::from(std::env::var("KOKORO_DIR").expect("KOKORO_DIR"));
        assert!(installed(&app_data));
        let t = std::time::Instant::now();
        let word = synthesize(&app_data, "图书馆", 1.0).unwrap();
        let first = t.elapsed();
        let t = std::time::Instant::now();
        let sentence = synthesize(&app_data, "我今天去图书馆学习，Kokoro can read English too。", 1.0).unwrap();
        println!("word {} bytes in {:?} (with loading); sentence {} bytes in {:?}", word.len(), first, sentence.len(), t.elapsed());
        assert!(word.len() > 44 + 24000 / 2, "at least a quarter of a second of sound");
        if let Ok(out) = std::env::var("KOKORO_OUT") {
            std::fs::write(Path::new(&out).join("kokoro-word.wav"), &word).unwrap();
            std::fs::write(Path::new(&out).join("kokoro-sentence.wav"), &sentence).unwrap();
        }
    }
}
