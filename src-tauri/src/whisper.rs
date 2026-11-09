//! Whisper speech recognition: OpenAI's open model, run locally on the CPU with candle.
//!
//! Works on every platform and transcribes freely (any words), unlike the Windows
//! recognizer, which picks from a phrase list. Models are downloaded on request from
//! Hugging Face (openai/whisper-*, Apache-2.0) into the app's data folder.

use crate::error::{AppError, AppResult};
use candle_core::{Device, Tensor, D};
use candle_nn::VarBuilder;
use candle_transformers::models::whisper::{self as m, audio, Config};
use serde::Serialize;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// (name, Hugging Face repo, approximate download size in MB)
pub const MODELS: [(&str, &str, u64); 3] = [
    ("tiny", "openai/whisper-tiny", 151),
    ("base", "openai/whisper-base", 290),
    ("small", "openai/whisper-small", 967),
];
const FILES: [&str; 3] = ["config.json", "tokenizer.json", "model.safetensors"];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WhisperModel {
    pub name: String,
    pub size_mb: u64,
    pub installed: bool,
}

pub fn models_dir(app_data: &Path) -> PathBuf {
    app_data.join("whisper")
}

fn model_dir(app_data: &Path, name: &str) -> AppResult<PathBuf> {
    if !MODELS.iter().any(|(n, _, _)| *n == name) {
        return Err(AppError::msg(format!("Unknown Whisper model: {name}")));
    }
    Ok(models_dir(app_data).join(name))
}

pub fn is_installed(app_data: &Path, name: &str) -> bool {
    model_dir(app_data, name).is_ok_and(|d| FILES.iter().all(|f| d.join(f).is_file()))
}

pub fn list(app_data: &Path) -> Vec<WhisperModel> {
    MODELS
        .iter()
        .map(|(name, _, mb)| WhisperModel { name: name.to_string(), size_mb: *mb, installed: is_installed(app_data, name) })
        .collect()
}

/// Download a model's files, reporting (bytes so far, total bytes) as it goes.
/// Files are written to `*.part` and renamed when complete.
pub fn download(app_data: &Path, name: &str, mut progress: impl FnMut(u64, u64)) -> AppResult<()> {
    let dir = model_dir(app_data, name)?;
    std::fs::create_dir_all(&dir)?;
    let repo = MODELS.iter().find(|(n, _, _)| *n == name).map(|(_, r, _)| *r).unwrap_or_default();
    // Sizes first, for an overall progress figure.
    let mut sizes = Vec::new();
    for f in FILES {
        let url = format!("https://huggingface.co/{repo}/resolve/main/{f}");
        let resp = ureq::head(&url).call().map_err(|e| AppError::msg(format!("Download failed: {e}")))?;
        sizes.push(resp.header("content-length").and_then(|v| v.parse::<u64>().ok()).unwrap_or(0));
    }
    let total: u64 = sizes.iter().sum();
    let mut done = 0u64;
    for f in FILES {
        let target = dir.join(f);
        let url = format!("https://huggingface.co/{repo}/resolve/main/{f}");
        let resp = ureq::get(&url).call().map_err(|e| AppError::msg(format!("Download failed: {e}")))?;
        let part = dir.join(format!("{f}.part"));
        let mut out = std::fs::File::create(&part)?;
        let mut reader = resp.into_reader();
        let mut buf = vec![0u8; 1 << 16];
        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n])?;
            done += n as u64;
            progress(done, total.max(done));
        }
        out.flush()?;
        drop(out);
        std::fs::rename(&part, &target)?;
    }
    Ok(())
}

pub fn delete(app_data: &Path, name: &str) -> AppResult<()> {
    let dir = model_dir(app_data, name)?;
    unload_if(name);
    if dir.exists() {
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

// ---------- tokenizer (GPT-2 byte-level BPE, decode only) ----------

struct Tokenizer {
    /// token id -> bytes
    bytes: Vec<Vec<u8>>,
    special: HashMap<String, u32>,
    /// For encoding prompts: token text (GPT-2 byte characters) -> id, and BPE merge ranks.
    vocab: HashMap<String, u32>,
    ranks: HashMap<(String, String), usize>,
    byte_chars: Vec<char>,
}

/// GPT-2's reversible byte <-> printable character table.
fn byte_decoder() -> HashMap<char, u8> {
    let mut bs: Vec<u32> = (b'!' as u32..=b'~' as u32).chain(0xA1..=0xAC).chain(0xAE..=0xFF).collect();
    let mut cs = bs.clone();
    let mut n = 0;
    for b in 0..256u32 {
        if !bs.contains(&b) {
            bs.push(b);
            cs.push(256 + n);
            n += 1;
        }
    }
    bs.into_iter()
        .zip(cs)
        .filter_map(|(b, c)| char::from_u32(c).map(|c| (c, b as u8)))
        .collect()
}

impl Tokenizer {
    fn load(path: &Path) -> AppResult<Self> {
        let json: serde_json::Value = serde_json::from_slice(&std::fs::read(path)?)
            .map_err(|e| AppError::msg(format!("Bad tokenizer file: {e}")))?;
        let decoder = byte_decoder();
        let mut byte_chars = vec![' '; 256];
        for (c, b) in &decoder {
            byte_chars[*b as usize] = *c;
        }
        let mut vocab = HashMap::new();
        let mut ranks = HashMap::new();
        for (i, m) in json["model"]["merges"].as_array().into_iter().flatten().enumerate() {
            // "a b" (older files) or ["a", "b"]
            let pair = match m {
                serde_json::Value::String(t) => t.split_once(' ').map(|(a, b)| (a.to_string(), b.to_string())),
                serde_json::Value::Array(v) => match (v.first().and_then(|x| x.as_str()), v.get(1).and_then(|x| x.as_str())) {
                    (Some(a), Some(b)) => Some((a.to_string(), b.to_string())),
                    _ => None,
                },
                _ => None,
            };
            if let Some(pair) = pair {
                ranks.entry(pair).or_insert(i);
            }
        }
        let mut bytes: Vec<Vec<u8>> = Vec::new();
        let mut put = |id: usize, b: Vec<u8>| {
            if bytes.len() <= id {
                bytes.resize(id + 1, Vec::new());
            }
            bytes[id] = b;
        };
        if let Some(json_vocab) = json["model"]["vocab"].as_object() {
            for (tok, id) in json_vocab {
                let id = id.as_u64().unwrap_or(0) as usize;
                vocab.insert(tok.clone(), id as u32);
                put(id, tok.chars().filter_map(|c| decoder.get(&c).copied()).collect());
            }
        }
        let mut special = HashMap::new();
        for t in json["added_tokens"].as_array().into_iter().flatten() {
            if let (Some(content), Some(id)) = (t["content"].as_str(), t["id"].as_u64()) {
                special.insert(content.to_string(), id as u32);
            }
        }
        Ok(Tokenizer { bytes, special, vocab, ranks, byte_chars })
    }

    /// Token ids for ordinary text (GPT-2 byte-level BPE).
    fn encode(&self, text: &str) -> Vec<u32> {
        let mut out = Vec::new();
        for piece in pretokenize(text) {
            let mut syms: Vec<String> = piece.bytes().map(|b| self.byte_chars[b as usize].to_string()).collect();
            loop {
                let best = syms
                    .windows(2)
                    .enumerate()
                    .filter_map(|(i, w)| self.ranks.get(&(w[0].clone(), w[1].clone())).map(|r| (*r, i)))
                    .min();
                let Some((_, i)) = best else { break };
                let merged = format!("{}{}", syms[i], syms[i + 1]);
                syms[i] = merged;
                syms.remove(i + 1);
            }
            out.extend(syms.iter().filter_map(|t| self.vocab.get(t).copied()));
        }
        out
    }

    fn token(&self, s: &str) -> AppResult<u32> {
        self.special.get(s).copied().ok_or_else(|| AppError::msg(format!("Tokenizer has no {s}")))
    }

    /// Text of ordinary tokens (special tokens are skipped).
    fn decode(&self, ids: &[u32], first_special: u32) -> String {
        let mut out = Vec::new();
        for &id in ids {
            if id < first_special {
                if let Some(b) = self.bytes.get(id as usize) {
                    out.extend_from_slice(b);
                }
            }
        }
        String::from_utf8_lossy(&out).into_owned()
    }
}

/// GPT-2's pre-tokenizer: `'s|'t|'re|'ve|'m|'ll|'d| ?\p{L}+| ?\p{N}+| ?[^\s\p{L}\p{N}]+|\s+(?!\S)|\s+`
fn pretokenize(text: &str) -> Vec<String> {
    #[derive(PartialEq)]
    enum K {
        L,
        N,
        O,
        S,
    }
    let kind = |c: char| {
        if c.is_alphabetic() {
            K::L
        } else if c.is_numeric() {
            K::N
        } else if c.is_whitespace() {
            K::S
        } else {
            K::O
        }
    };
    let cs: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < cs.len() {
        if cs[i] == '\'' {
            let rest: String = cs[i + 1..cs.len().min(i + 3)].iter().collect();
            if let Some(c) = ["re", "ve", "ll", "s", "t", "m", "d"].iter().find(|c| rest.starts_with(*c)) {
                out.push(format!("'{c}"));
                i += 1 + c.len();
                continue;
            }
        }
        let start = i;
        if cs[i] == ' ' && i + 1 < cs.len() && kind(cs[i + 1]) != K::S {
            i += 1;
        }
        let k = kind(cs[i]);
        if k == K::S {
            let mut j = i;
            while j < cs.len() && kind(cs[j]) == K::S {
                j += 1;
            }
            // leave the last space to start the next word
            if j < cs.len() && j - i > 1 {
                j -= 1;
            }
            out.push(cs[i..j].iter().collect());
            i = j;
            continue;
        }
        i += 1; // the run's first character always belongs to it
        while i < cs.len() && kind(cs[i]) == k && !(k == K::O && cs[i] == '\'') {
            i += 1;
        }
        out.push(cs[start..i].iter().collect());
    }
    out
}

// ---------- mel filterbank (librosa.filters.mel, Slaney scale and norm) ----------

fn hz_to_mel(f: f64) -> f64 {
    let f_sp = 200.0 / 3.0;
    let (min_log_hz, min_log_mel, logstep) = (1000.0, 1000.0 / f_sp, (6.4f64).ln() / 27.0);
    if f >= min_log_hz {
        min_log_mel + (f / min_log_hz).ln() / logstep
    } else {
        f / f_sp
    }
}

fn mel_to_hz(m: f64) -> f64 {
    let f_sp = 200.0 / 3.0;
    let (min_log_hz, min_log_mel, logstep) = (1000.0, 1000.0 / f_sp, (6.4f64).ln() / 27.0);
    if m >= min_log_mel {
        min_log_hz * (logstep * (m - min_log_mel)).exp()
    } else {
        f_sp * m
    }
}

/// Mel filters, `n_mels` rows of `n_fft / 2 + 1` weights.
pub fn mel_filters(sample_rate: usize, n_fft: usize, n_mels: usize) -> Vec<f32> {
    let n_freqs = n_fft / 2 + 1;
    let fft_freqs: Vec<f64> = (0..n_freqs).map(|i| i as f64 * sample_rate as f64 / n_fft as f64).collect();
    let (lo, hi) = (hz_to_mel(0.0), hz_to_mel(sample_rate as f64 / 2.0));
    let mel_f: Vec<f64> = (0..n_mels + 2)
        .map(|i| mel_to_hz(lo + (hi - lo) * i as f64 / (n_mels + 1) as f64))
        .collect();
    let mut out = vec![0f32; n_mels * n_freqs];
    for i in 0..n_mels {
        let (f0, f1, f2) = (mel_f[i], mel_f[i + 1], mel_f[i + 2]);
        let enorm = 2.0 / (f2 - f0);
        for (j, &f) in fft_freqs.iter().enumerate() {
            let lower = (f - f0) / (f1 - f0);
            let upper = (f2 - f) / (f2 - f1);
            out[i * n_freqs + j] = (lower.min(upper).max(0.0) * enorm) as f32;
        }
    }
    out
}

// ---------- model ----------

struct Engine {
    name: String,
    encoder: crate::whisper_net::Encoder,
    decoder: crate::whisper_net::Decoder,
    config: Config,
    tokenizer: Tokenizer,
    filters: Vec<f32>,
    suppress: Tensor,
    /// `suppress` plus every token with Latin letters or digits (for Chinese-only answers).
    suppress_latin: Tensor,
    device: Device,
}

/// The loaded model, kept between answers (loading takes a moment).
static ENGINE: Mutex<Option<Engine>> = Mutex::new(None);

fn err(e: candle_core::Error) -> AppError {
    AppError::msg(format!("Whisper failed: {e}"))
}

fn unload_if(name: &str) {
    let mut guard = ENGINE.lock().unwrap_or_else(|e| e.into_inner());
    if guard.as_ref().is_some_and(|e| e.name == name) {
        *guard = None;
    }
}

fn load(app_data: &Path, name: &str) -> AppResult<Engine> {
    if !is_installed(app_data, name) {
        return Err(AppError::msg(format!(
            "The Whisper {name} model isn't downloaded yet. Download it in Settings › Speech recognition."
        )));
    }
    let dir = model_dir(app_data, name)?;
    let config: Config = serde_json::from_slice(&std::fs::read(dir.join("config.json"))?)
        .map_err(|e| AppError::msg(format!("Bad model config: {e}")))?;
    let tokenizer = Tokenizer::load(&dir.join("tokenizer.json"))?;
    let device = Device::Cpu;
    // SAFETY: the file is only read while mapped; we never modify it while loaded.
    let vb = unsafe { VarBuilder::from_mmaped_safetensors(&[dir.join("model.safetensors")], m::DTYPE, &device) }
        .map_err(err)?;
    let encoder = crate::whisper_net::Encoder::load(vb.pp("model.encoder"), &config).map_err(err)?;
    let decoder = crate::whisper_net::Decoder::load(vb.pp("model.decoder"), &config).map_err(err)?;
    let no_timestamps = tokenizer.token(m::NO_TIMESTAMPS_TOKEN)?;
    let suppress: Vec<f32> = (0..config.vocab_size as u32)
        .map(|i| if config.suppress_tokens.contains(&i) || i == no_timestamps { f32::NEG_INFINITY } else { 0.0 })
        .collect();
    let first_special = tokenizer.special.values().copied().min().unwrap_or(u32::MAX);
    let latin: Vec<f32> = suppress
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let has_latin = (i as u32) < first_special
                && tokenizer.bytes.get(i).is_some_and(|b| {
                    // ASCII letters/digits, or full-width ones (０-９, Ａ-Ｚ, ａ-ｚ)
                    b.iter().any(|c| c.is_ascii_alphanumeric())
                        || String::from_utf8_lossy(b).chars().any(|c| matches!(c, '\u{FF10}'..='\u{FF19}' | '\u{FF21}'..='\u{FF3A}' | '\u{FF41}'..='\u{FF5A}'))
                });
            if has_latin { f32::NEG_INFINITY } else { *v }
        })
        .collect();
    let suppress = Tensor::new(suppress.as_slice(), &device).map_err(err)?;
    let suppress_latin = Tensor::new(latin.as_slice(), &device).map_err(err)?;
    let filters = mel_filters(m::SAMPLE_RATE, m::N_FFT, config.num_mel_bins);
    Ok(Engine { name: name.to_string(), encoder, decoder, config, tokenizer, filters, suppress, suppress_latin, device })
}

/// Load the model and run it once on silence, so the first real answer is quick.
pub fn warm_up(app_data: &Path, name: &str) -> AppResult<()> {
    let silence = vec![0f32; m::SAMPLE_RATE];
    transcribe(app_data, name, &silence, "zh", &Options { trim: Some((2.0, 10.0)), ..Default::default() }).map(|_| ())
}

/// How to decode.
#[derive(Debug, Clone)]
pub struct Options {
    /// Text the speech is likely to contain or follow (expected words, the previous line of a
    /// conversation). Whisper treats it as what came before, which steers it toward those
    /// words and toward Simplified characters.
    pub prompt: String,
    /// Beam width (1 = greedy).
    pub beams: usize,
    /// Words the speaker probably says (their cards). While decoding, a token that
    /// continues one of these words gets `bias` added to its log-probability, so a card
    /// word wins when the audio is close to it.
    pub bias_words: Vec<String>,
    pub bias: f32,
    /// Never write Latin letters or digits (a learner answering in Mandarin).
    pub chinese_only: bool,
    /// Encode only the speech plus `(margin, minimum)` seconds instead of a padded 30 s
    /// window: much less work for short answers. None = the full window.
    pub trim: Option<(f32, f32)>,
}

impl Default for Options {
    fn default() -> Self {
        Options { prompt: String::new(), beams: 1, bias_words: Vec::new(), bias: 0.0, chinese_only: false, trim: None }
    }
}

/// Byte prefixes of the favoured words (tokens may split a character into bytes).
struct WordBias {
    prefixes: std::collections::HashSet<Vec<u8>>,
    max_len: usize,
    bonus: f32,
}

impl WordBias {
    fn new(words: &[String], bonus: f32) -> Option<Self> {
        let mut prefixes = std::collections::HashSet::new();
        let mut max_len = 0;
        for w in words {
            let b = w.trim().as_bytes();
            max_len = max_len.max(b.len());
            for i in 1..=b.len() {
                prefixes.insert(b[..i].to_vec());
            }
        }
        (bonus > 0.0 && !prefixes.is_empty()).then_some(WordBias { prefixes, max_len, bonus })
    }

    /// Whether `token` continues a favoured word, given the text so far (`tail`).
    fn favours(&self, tail: &[u8], token: &[u8]) -> bool {
        if token.is_empty() || token.len() > self.max_len {
            return false;
        }
        (0..=tail.len().min(self.max_len - token.len())).any(|k| {
            let mut v = tail[tail.len() - k..].to_vec();
            v.extend_from_slice(token);
            self.prefixes.contains(&v)
        })
    }
}

/// Transcribe up to 30 s of 16 kHz mono audio. `language` is a Whisper language code
/// ("zh" for Mandarin). Returns the text as Whisper wrote it.
pub fn transcribe(app_data: &Path, name: &str, pcm: &[f32], language: &str, opts: &Options) -> AppResult<String> {
    let mut guard = ENGINE.lock().unwrap_or_else(|e| e.into_inner());
    if guard.as_ref().is_none_or(|e| e.name != name) {
        *guard = None; // free the old model first
        *guard = Some(load(app_data, name)?);
    }
    let e = guard.as_mut().expect("loaded above");
    run(e, pcm, language, opts).map_err(err)
}

/// How likely each candidate text is to be what was said: the sum of its tokens'
/// log-probabilities (and of ending there), given the audio. Higher is likelier. Used to
/// check tones: the answer against the same syllables with other tones.
#[allow(dead_code)] // tone-checking prototype (docs/tone-recognition.md)
pub fn score(app_data: &Path, name: &str, pcm: &[f32], opts: &Options, candidates: &[String]) -> AppResult<Vec<f32>> {
    let mut guard = ENGINE.lock().unwrap_or_else(|e| e.into_inner());
    if guard.as_ref().is_none_or(|e| e.name != name) {
        *guard = None;
        *guard = Some(load(app_data, name)?);
    }
    let e = guard.as_mut().expect("loaded above");
    score_run(e, pcm, opts, candidates).map_err(err)
}

#[allow(dead_code)]
fn score_run(e: &mut Engine, pcm: &[f32], opts: &Options, candidates: &[String]) -> candle_core::Result<Vec<f32>> {
    let (features, start, eot) = prepare(e, pcm, "zh", opts)?;
    let mut out = Vec::with_capacity(candidates.len());
    for cand in candidates {
        let mut state = e.decoder.start(&features)?;
        let mut feed = Tensor::new(start.as_slice(), &e.device)?.unsqueeze(0)?;
        let mut total = 0f32;
        for t in e.tokenizer.encode(cand).into_iter().chain([eot]) {
            let logits = e.decoder.step(&feed, &mut state)?;
            let logp = candle_nn::ops::log_softmax(&logits, D::Minus1)?.squeeze(0)?.to_vec1::<f32>()?;
            total += logp[t as usize];
            feed = Tensor::new(&[t], &e.device)?.unsqueeze(0)?;
        }
        out.push(total);
    }
    Ok(out)
}

/// Mel, encoder and the decoder's start tokens: (audio features, start tokens, end token).
fn prepare(e: &mut Engine, pcm: &[f32], language: &str, opts: &Options) -> candle_core::Result<(Tensor, Vec<u32>, u32)> {
    let pcm = &pcm[..pcm.len().min(m::N_SAMPLES)];
    let mel = audio::pcm_to_mel(&e.config, pcm, &e.filters);
    let n_mels = e.config.num_mel_bins;
    let frames = mel.len() / n_mels;
    let keep = match opts.trim {
        Some((margin, min)) => {
            let speech = pcm.len() / m::HOP_LENGTH;
            let want = speech + (margin * 100.0) as usize;
            want.max((min * 100.0) as usize).div_ceil(2) * 2
        }
        None => m::N_FRAMES,
    };
    let mel = Tensor::from_vec(mel, (1, n_mels, frames), &e.device)?.narrow(2, 0, keep.min(frames).min(m::N_FRAMES))?;
    let tok = |s: &str| e.tokenizer.special.get(s).copied();
    let (Some(sot), Some(eot), Some(transcribe), Some(no_ts)) =
        (tok(m::SOT_TOKEN), tok(m::EOT_TOKEN), tok(m::TRANSCRIBE_TOKEN), tok(m::NO_TIMESTAMPS_TOKEN))
    else {
        return Err(candle_core::Error::Msg("tokenizer is missing Whisper's special tokens".into()));
    };
    let lang = tok(&format!("<|{language}|>")).ok_or_else(|| candle_core::Error::Msg(format!("unknown language {language}")))?;
    let mut start = Vec::new();
    let prompt = opts.prompt.trim();
    if !prompt.is_empty() {
        if let Some(prev) = tok("<|startofprev|>") {
            let ids = e.tokenizer.encode(prompt);
            let keep = ids.len().min(e.config.max_target_positions / 2 - 1);
            start.push(prev);
            start.extend_from_slice(&ids[ids.len() - keep..]);
        }
    }
    start.extend([sot, lang, transcribe, no_ts]);
    Ok((e.encoder.forward(&mel)?, start, eot))
}

fn run(e: &mut Engine, pcm: &[f32], language: &str, opts: &Options) -> candle_core::Result<String> {
    let pcm = &pcm[..pcm.len().min(m::N_SAMPLES)];
    let mel = audio::pcm_to_mel(&e.config, pcm, &e.filters);
    let n_mels = e.config.num_mel_bins;
    let frames = mel.len() / n_mels;
    // Mel frames are 10 ms; the encoder halves them, so keep an even number.
    let keep = match opts.trim {
        Some((margin, min)) => {
            let speech = pcm.len() / m::HOP_LENGTH;
            let want = speech + (margin * 100.0) as usize;
            want.max((min * 100.0) as usize).div_ceil(2) * 2
        }
        None => m::N_FRAMES,
    };
    let mel = Tensor::from_vec(mel, (1, n_mels, frames), &e.device)?.narrow(2, 0, keep.min(frames).min(m::N_FRAMES))?;

    let tok = |s: &str| e.tokenizer.special.get(s).copied();
    let (Some(sot), Some(eot), Some(transcribe), Some(no_ts)) =
        (tok(m::SOT_TOKEN), tok(m::EOT_TOKEN), tok(m::TRANSCRIBE_TOKEN), tok(m::NO_TIMESTAMPS_TOKEN))
    else {
        return Err(candle_core::Error::Msg("tokenizer is missing Whisper's special tokens".into()));
    };
    let lang = tok(&format!("<|{language}|>"))
        .ok_or_else(|| candle_core::Error::Msg(format!("unknown language {language}")))?;

    let t_enc = std::time::Instant::now();
    let audio_features = e.encoder.forward(&mel)?;
    let enc_ms = t_enc.elapsed().as_millis();
    let t_dec = std::time::Instant::now();

    // <|startofprev|> prompt, then the usual start: language, transcribe, no timestamps.
    let mut start = Vec::new();
    let prompt = opts.prompt.trim();
    if !prompt.is_empty() {
        if let Some(prev) = tok("<|startofprev|>") {
            let ids = e.tokenizer.encode(prompt);
            let keep = ids.len().min(e.config.max_target_positions / 2 - 1);
            start.push(prev);
            start.extend_from_slice(&ids[ids.len() - keep..]);
        }
    }
    start.extend([sot, lang, transcribe, no_ts]);
    let prompt_len = start.len();
    // About 10 tokens per second of audio is plenty for speech; more means it is looping.
    let max_new = (e.config.max_target_positions - prompt_len).min(96).min(12 + pcm.len() / 1600);
    // Whisper's "suppress blank": the first token can't be the end or a lone space.
    let blank: Vec<u32> = e.tokenizer.encode(" ").into_iter().chain([eot]).collect();

    let beams = opts.beams.max(1);
    let bias = WordBias::new(&opts.bias_words, opts.bias);
    let features = if beams > 1 { audio_features.repeat((beams, 1, 1))? } else { audio_features };
    let mut state = e.decoder.start(&features)?;
    // The first step feeds the whole start (prompt included) to every row; after that,
    // each row only needs its newest token.
    let mut feed = Tensor::new(start.as_slice(), &e.device)?.unsqueeze(0)?.repeat((beams, 1))?;
    // (tokens, sum of log-probabilities)
    let mut live: Vec<(Vec<u32>, f32)> = vec![(start.clone(), 0.0)];
    let mut done: Vec<(Vec<u32>, f32)> = Vec::new();
    for step in 0..max_new {
        let logits = e.decoder.step(&feed, &mut state)?;
        let logits = logits.broadcast_add(if opts.chinese_only { &e.suppress_latin } else { &e.suppress })?;
        let logp = candle_nn::ops::log_softmax(&logits, D::Minus1)?.to_vec2::<f32>()?;

        let mut cands: Vec<(f32, usize, u32)> = Vec::new();
        for (b, (toks, score)) in live.iter().enumerate() {
            let row = &logp[b];
            let mut best: Vec<(f32, u32)> = row
                .iter()
                .enumerate()
                .filter(|(t, _)| !(step == 0 && blank.contains(&(*t as u32))))
                .map(|(t, lp)| (*lp, t as u32))
                .filter(|(lp, _)| lp.is_finite())
                .collect();
            // Look a little further down the list when favouring card words.
            let pool = if bias.is_some() { 40 } else { beams + 1 }.min(best.len());
            best.select_nth_unstable_by(pool.saturating_sub(1), |a, b| b.0.total_cmp(&a.0));
            best.truncate(pool);
            if let Some(bias) = &bias {
                let mut text: Vec<u8> = Vec::new();
                for &t in &toks[prompt_len..] {
                    if let Some(b) = e.tokenizer.bytes.get(t as usize) {
                        text.extend_from_slice(b);
                    }
                }
                let tail = &text[text.len().saturating_sub(bias.max_len)..];
                for c in best.iter_mut() {
                    if c.1 != eot && e.tokenizer.bytes.get(c.1 as usize).is_some_and(|b| bias.favours(tail, b)) {
                        c.0 += bias.bonus;
                    }
                }
                best.sort_by(|a, b| b.0.total_cmp(&a.0));
            }
            let k = (beams + 1).min(best.len());
            for (lp, t) in best.into_iter().take(k) {
                cands.push((score + lp, b, t));
            }
        }
        cands.sort_by(|a, b| b.0.total_cmp(&a.0));
        // (tokens, score, the row it grew from)
        let mut next: Vec<(Vec<u32>, f32, usize)> = Vec::new();
        for (score, b, t) in cands {
            let mut toks = live[b].0.clone();
            if t == eot {
                if done.len() < beams {
                    done.push((toks, score));
                }
            } else {
                toks.push(t);
                next.push((toks, score, b));
            }
            if next.len() == beams {
                break;
            }
        }
        if done.len() >= beams || next.is_empty() {
            break;
        }
        // Rows for the next step (padded to the beam count by repeating the last one).
        let pick = |r: usize| &next[r.min(next.len() - 1)];
        if beams > 1 {
            let parents: Vec<u32> = (0..beams).map(|r| pick(r).2 as u32).collect();
            crate::whisper_net::Decoder::reorder(&mut state, &parents)?;
        }
        let last: Vec<u32> = (0..beams).map(|r| *pick(r).0.last().expect("has a token")).collect();
        feed = Tensor::from_vec(last, (beams, 1), &e.device)?;
        live = next.into_iter().map(|(t, s, _)| (t, s)).collect();
        if step == max_new - 1 {
            done.extend(live.drain(..));
        }
    }
    if done.is_empty() {
        done = live;
    }
    // Rank finished beams by average log-probability (no length bias).
    let best = done
        .iter()
        .max_by(|a, b| {
            let la = (a.0.len() - prompt_len).max(1) as f32;
            let lb = (b.0.len() - prompt_len).max(1) as f32;
            (a.1 / la).total_cmp(&(b.1 / lb))
        })
        .map(|d| d.0.clone())
        .unwrap_or_default();
    if std::env::var_os("WHISPER_TIMING").is_some() {
        eprintln!("whisper timing: encoder {enc_ms} ms, decoder {} ms, {} tokens", t_dec.elapsed().as_millis(), best.len().saturating_sub(prompt_len));
    }
    Ok(collapse_repeats(e.tokenizer.decode(&best[prompt_len.min(best.len())..], eot).trim()))
}

/// Whisper sometimes loops on unclear audio ("祝福祝福祝福…"): keep one copy of any
/// 1-6 character unit repeated more than three times in a row (one character: more than four).
fn collapse_repeats(text: &str) -> String {
    let mut cs: Vec<char> = text.chars().collect();
    for unit in 1..=6 {
        let mut i = 0;
        while i + unit <= cs.len() {
            let mut n = 1;
            while i + (n + 1) * unit <= cs.len() && cs[i + n * unit..i + (n + 1) * unit] == cs[i..i + unit] {
                n += 1;
            }
            if n > if unit == 1 { 4 } else { 3 } {
                cs.drain(i + unit..i + n * unit);
            }
            i += 1;
        }
    }
    cs.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mel_filters_look_right() {
        let f = mel_filters(16000, 400, 80);
        assert_eq!(f.len(), 80 * 201);
        // every filter has some weight, none negative
        for i in 0..80 {
            let row = &f[i * 201..(i + 1) * 201];
            assert!(row.iter().all(|w| *w >= 0.0));
            assert!(row.iter().any(|w| *w > 0.0), "filter {i} empty");
        }
        // Slaney scale is linear below 1 kHz: 15 mels = 1000 Hz
        assert!((mel_to_hz(hz_to_mel(440.0)) - 440.0).abs() < 1e-6);
        assert!((hz_to_mel(1000.0) - 15.0).abs() < 1e-9);
    }

    #[test]
    fn favours_card_word_continuations() {
        let b = WordBias::new(&["医生".to_string(), "米饭".to_string()], 1.0).unwrap();
        let yi = "医".as_bytes();
        assert!(b.favours(b"", &yi[..2]), "first bytes of a word");
        assert!(b.favours(&yi[..2], &yi[2..]), "rest of a split character");
        assert!(b.favours("我是医".as_bytes(), "生".as_bytes()), "second character after other text");
        assert!(!b.favours("我是".as_bytes(), "生".as_bytes()), "not mid-word without its start");
        assert!(!b.favours(b"", "你".as_bytes()));
    }

    #[test]
    fn collapses_loops_but_not_words() {
        assert_eq!(collapse_repeats("祝福祝福祝福祝福祝福"), "祝福");
        assert_eq!(collapse_repeats("哈哈哈，谢谢谢谢"), "哈哈哈，谢谢谢谢");
        assert_eq!(collapse_repeats("我想我想我想我想喝水"), "我想喝水");
    }

    #[test]
    fn pretokenizes_like_gpt2() {
        assert_eq!(pretokenize("Hello world's 123!!  ok"), ["Hello", " world", "'s", " 123", "!!", " ", " ok"]);
        assert_eq!(pretokenize("我想喝咖啡，你呢？"), ["我想喝咖啡", "，", "你呢", "？"]);
        assert_eq!(pretokenize("'x"), ["'", "x"]);
    }

    #[test]
    fn byte_decoder_round_trips_utf8() {
        let d = byte_decoder();
        assert_eq!(d.len(), 256);
        assert_eq!(d[&'Ġ'], b' ', "GPT-2 writes a space as Ġ");
    }

    /// Needs a downloaded model and a 16 kHz mono 16-bit WAV:
    /// WHISPER_DIR=<app data> WHISPER_WAV=clip.wav cargo test --lib whisper -- --ignored --nocapture
    #[test]
    #[ignore]
    fn transcribes_a_wav() {
        let (Ok(dir), Ok(wav)) = (std::env::var("WHISPER_DIR"), std::env::var("WHISPER_WAV")) else { return };
        let bytes = std::fs::read(wav).unwrap();
        // 44-byte RIFF header, then 16-bit little-endian samples
        let pcm: Vec<f32> =
            bytes[44..].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).collect();
        let name = std::env::var("WHISPER_MODEL").unwrap_or_else(|_| "base".into());
        let t0 = std::time::Instant::now();
        let text = transcribe(Path::new(&dir), &name, &pcm, "zh", &Options::default()).unwrap();
        println!("TRANSCRIPT\t{text}\t{:.1}s", t0.elapsed().as_secs_f32());
        let t1 = std::time::Instant::now();
        let again = transcribe(Path::new(&dir), &name, &pcm, "zh", &Options::default()).unwrap();
        println!("SECOND RUN\t{again}\t{:.1}s", t1.elapsed().as_secs_f32());
    }

    /// Does Whisper hear tones? For each clip in TONES_DIR (list.tsv + clean/noisy wavs),
    /// score the right word against the same syllables with other tones and count how often
    /// the right one wins.
    /// WHISPER_DIR=<app data> TONES_DIR=... cargo test --lib whisper_tone_bench -- --ignored --nocapture
    #[test]
    #[ignore]
    fn whisper_tone_bench() {
        let (Ok(dir), Ok(tones)) = (std::env::var("WHISPER_DIR"), std::env::var("TONES_DIR")) else { return };
        let conn = rusqlite::Connection::open(Path::new(&dir).join("hanzi-cards.db")).unwrap();
        let name = std::env::var("WHISPER_MODEL").unwrap_or_else(|_| "small".into());
        let tones = Path::new(&tones);
        let list: Vec<String> = std::fs::read_to_string(tones.join("list.tsv")).unwrap().lines().map(|l| l.split('\t').next().unwrap().to_string()).collect();
        // the four tones of each single-syllable set are each other's distractors
        let sets: Vec<Vec<String>> = list[..40].chunks(4).map(|c| c.to_vec()).collect();
        for set in std::env::var("BENCH_SETS").unwrap_or_else(|_| "clean,noisy".into()).split(',') {
            let (mut mono, mut di) = ((0, 0), (0, 0));
            let mut misses = Vec::new();
            let t0 = std::time::Instant::now();
            for (i, word) in list.iter().enumerate() {
                let mut cands = vec![word.clone()];
                if i < 40 {
                    cands.extend(sets[i / 4].iter().filter(|w| *w != word).cloned());
                } else {
                    let py: String = conn
                        .query_row("SELECT pinyin FROM dictionary WHERE simplified = ?1 ORDER BY score DESC LIMIT 1", [word], |r| r.get(0))
                        .unwrap_or_default();
                    cands.extend(crate::speech::tone_distractors(&conn, word, &py, 6).unwrap_or_default());
                }
                if cands.len() < 2 {
                    continue;
                }
                let bytes = std::fs::read(tones.join(set).join(format!("{i}.wav"))).unwrap();
                let pcm: Vec<f32> = bytes[44..].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).collect();
                let opts = Options { trim: Some((2.0, 10.0)), ..Default::default() };
                let scores = score(Path::new(&dir), &name, &pcm, &opts, &cands).unwrap();
                let best = (0..cands.len()).max_by(|&a, &b| scores[a].total_cmp(&scores[b])).unwrap();
                let ok = best == 0;
                let tally = if i < 40 { &mut mono } else { &mut di };
                tally.0 += ok as u32;
                tally.1 += 1;
                if !ok && misses.len() < 12 {
                    misses.push(format!("{word}→{}", cands[best]));
                }
            }
            println!(
                "{name} {set:5} right tone wins: single syllables {}/{} (vs 3 others), 2-syllable words {}/{} (vs up to 6)  {:.2}s/clip  misses: {}",
                mono.0, mono.1, di.0, di.1, t0.elapsed().as_secs_f32() / list.len() as f32, misses.join(" ")
            );
        }
    }

    /// Accuracy benchmark over a folder of clips (list.txt: one expected text per line;
    /// clean/N.wav, noisy/N.wav). Prints the character error rate per decoding setup.
    /// WHISPER_DIR=<app data> WHISPER_BENCH=<folder> cargo test --release --lib whisper_bench -- --ignored --nocapture
    #[test]
    #[ignore]
    fn whisper_bench() {
        let (Ok(dir), Ok(bench)) = (std::env::var("WHISPER_DIR"), std::env::var("WHISPER_BENCH")) else { return };
        let conn = rusqlite::Connection::open(Path::new(&dir).join("hanzi-cards.db")).unwrap();
        let name = std::env::var("WHISPER_MODEL").unwrap_or_else(|_| "base".into());
        let bench = Path::new(&bench);

        let expected: Vec<String> =
            std::fs::read_to_string(bench.join("list.txt")).unwrap().lines().map(|l| l.trim().to_string()).collect();
        // Lines 0-39 are card words, 40-49 sentences, 50+ words that are not on any card.
        let words: Vec<&str> = expected.iter().take(40).map(|s| s.as_str()).collect();
        let comma_prompt = format!("{}。", words.join("，"));
        {
            let mut g = ENGINE.lock().unwrap();
            *g = Some(load(Path::new(&dir), &name).unwrap());
            let t = &g.as_ref().unwrap().tokenizer;
            let ids = t.encode(&comma_prompt);
            assert_eq!(t.decode(&ids, u32::MAX), comma_prompt, "prompt round-trips");
        }
        // Only the characters matter for judging answers.
        let strip = |s: &str| s.chars().filter(|c| crate::pinyin::is_hanzi(*c)).collect::<Vec<char>>();
        let dist = |a: &[char], b: &[char]| {
            let mut prev: Vec<usize> = (0..=b.len()).collect();
            for (i, ca) in a.iter().enumerate() {
                let mut cur = vec![i + 1; b.len() + 1];
                for (j, cb) in b.iter().enumerate() {
                    cur[j + 1] = (prev[j] + (ca != cb) as usize).min(prev[j + 1] + 1).min(cur[j] + 1);
                }
                prev = cur;
            }
            prev[b.len()]
        };
        // All the learner's card words, as the app uses them.
        let cards: Vec<String> = {
            let mut st = conn.prepare("SELECT DISTINCT hanzi FROM cards").unwrap();
            let v: Vec<String> = st.query_map([], |r| r.get(0)).unwrap().map(|r| r.unwrap()).collect();
            v
        };
        println!("{} card words", cards.len());
        let only: Vec<String> = std::env::var("BENCH_SETUPS").unwrap_or_default().split(',').filter(|s| !s.is_empty()).map(String::from).collect();
        let setups: Vec<(&str, &str, f32, bool)> = vec![
            ("greedy", "", 0.0, false),
            ("zh", "", 0.0, true),
            ("zh+bias1", "", 1.0, true),
            ("zh+bias2", "", 2.0, true),
            ("zh+prompt+bias1", comma_prompt.as_str(), 1.0, true),
            ("zh+prompt+bias2", comma_prompt.as_str(), 2.0, true),
        ];
        let setups: Vec<_> = setups.into_iter().filter(|s| only.is_empty() || only.iter().any(|o| o == s.0)).collect();
        let sets = std::env::var("BENCH_SETS").unwrap_or_else(|_| "clean,noisy".into());
        let trims: Vec<(String, Option<(f32, f32)>)> = std::env::var("BENCH_TRIMS")
            .unwrap_or_else(|_| "full".into())
            .split(',')
            .map(|t| {
                let v: Vec<f32> = t.split(':').filter_map(|x| x.parse().ok()).collect();
                (t.to_string(), if v.len() == 2 { Some((v[0], v[1])) } else { None })
            })
            .collect();
        for set in sets.split(',') {
          for (trim_label, trim) in &trims {
            for &(label, prompt, bias, chinese_only) in &setups {
                let opts = Options { prompt: prompt.to_string(), bias, bias_words: cards.clone(), chinese_only, trim: *trim, ..Default::default() };
                let (mut errs, mut total) = (0, 0);
                let mut groups = [0usize; 3];
                let t0 = std::time::Instant::now();
                let mut wrong = Vec::new();
                for (i, want) in expected.iter().enumerate() {
                    let bytes = std::fs::read(bench.join(set).join(format!("{i}.wav"))).unwrap();
                    let pcm: Vec<f32> =
                        bytes[44..].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).collect();
                    let raw = transcribe(Path::new(&dir), &name, &pcm, "zh", &opts).unwrap();
                    let got = crate::speech::to_simplified(&conn, &raw).unwrap();
                    let (w, g) = (strip(want), strip(&got));
                    let d = dist(&w, &g);
                    errs += d;
                    total += w.len();
                    if d == 0 {
                        groups[if i < 40 { 0 } else if i < 50 { 1 } else { 2 }] += 1;
                    } else if wrong.len() < 10 && (i >= 50 || wrong.len() < 6) {
                        wrong.push(format!("{want}->{got}"));
                    }
                }
                println!(
                    "{name} {set:5} {label:16} {trim_label:6} CER {:5.1}%  exact: card words {}/40, sentences {}/10, other words {}/{}  {:.2}s/clip  e.g. {}",
                    100.0 * errs as f32 / total as f32,
                    groups[0],
                    groups[1],
                    groups[2],
                    expected.len().saturating_sub(50),
                    t0.elapsed().as_secs_f32() / expected.len() as f32,
                    wrong.join(" ")
                );
            }
          }
        }
    }
}
