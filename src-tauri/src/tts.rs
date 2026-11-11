//! Text-to-speech made by the app itself, as a WAV file, for when the sound has to go to a
//! chosen output device (Settings › Audio › Output): the webview's own speech always plays
//! on the system's default device, but a WAV played by the page can be sent anywhere.
//!
//! Windows only (its built-in speech engine, the same Microsoft voices the webview lists);
//! elsewhere `available()` is false and the app keeps the webview's speech.

pub fn available() -> bool {
    cfg!(windows)
}

/// Say `text` in the voice whose name is `voice` (as the webview lists it, e.g. "Microsoft
/// Huihui - Chinese (Simplified, PRC)"; the first Chinese voice when it isn't found) at
/// `rate` (1 = normal). Returns a WAV file.
#[cfg(windows)]
pub fn synthesize(text: &str, voice: &str, rate: f64) -> Result<Vec<u8>, String> {
    use windows::core::HSTRING;
    use windows::Media::SpeechSynthesis::SpeechSynthesizer;
    use windows::Storage::Streams::DataReader;
    use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

    // Already-initialised threads return an error we can ignore.
    let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
    let run = || -> windows::core::Result<Vec<u8>> {
        let synth = SpeechSynthesizer::new()?;
        let mut chosen = None;
        let mut chinese = None;
        for v in SpeechSynthesizer::AllVoices()? {
            let name = v.DisplayName()?.to_string();
            // the webview adds " - <language>" to the name Windows uses
            if !name.is_empty() && (voice == name || voice.starts_with(&format!("{name} "))) {
                chosen = Some(v);
                break;
            }
            if chinese.is_none() && v.Language()?.to_string().to_lowercase().starts_with("zh-cn") {
                chinese = Some(v);
            }
        }
        if let Some(v) = chosen.or(chinese) {
            synth.SetVoice(&v)?;
        }
        // Windows takes 0.5-6 (1 = normal), like the webview's rate
        synth.Options()?.SetSpeakingRate(rate.clamp(0.5, 6.0))?;
        let stream = synth.SynthesizeTextToStreamAsync(&HSTRING::from(text))?.join()?;
        let size = stream.Size()? as u32;
        let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0)?)?;
        reader.LoadAsync(size)?.join()?;
        let mut wav = vec![0u8; size as usize];
        reader.ReadBytes(&mut wav)?;
        Ok(wav)
    };
    run().map_err(|e| format!("Couldn't make the speech: {}", e.message()))
}

#[cfg(not(windows))]
pub fn synthesize(_text: &str, _voice: &str, _rate: f64) -> Result<Vec<u8>, String> {
    Err("Choosing an output device needs Windows.".into())
}

#[cfg(all(test, windows))]
mod tests {
    /// cargo test --lib tts_voices -- --ignored --nocapture
    #[test]
    #[ignore]
    fn tts_voices() {
        for v in windows::Media::SpeechSynthesis::SpeechSynthesizer::AllVoices().unwrap() {
            println!("{} | {}", v.DisplayName().unwrap(), v.Language().unwrap());
        }
    }

    #[test]
    fn makes_a_wav() {
        let wav = match super::synthesize("你好", "", 1.0) {
            Ok(w) => w,
            // a Windows without any speech voice installed
            Err(e) if e.contains("voice") => return,
            Err(e) => panic!("{e}"),
        };
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert!(wav.len() > 10_000, "{} bytes for 你好", wav.len());
    }
}
