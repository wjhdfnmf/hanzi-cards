//! Spoken answers on an iPhone: Apple's own speech recognition (the Speech framework), on the
//! device only: the recording never leaves the phone (as the microphone permission promises).
//!
//! The recording (16 kHz mono, from the page as on every platform) is written to a short-lived
//! WAV file and handed to the recognizer; the words being studied go with it as hints.

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::AnyThread;
use objc2_foundation::{NSArray, NSError, NSLocale, NSOperationQueue, NSString, NSURL};
use objc2_speech::{
    SFSpeechRecognitionResult, SFSpeechRecognizer, SFSpeechRecognizerAuthorizationStatus as Access,
    SFSpeechURLRecognitionRequest,
};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::Duration;

const REFUSED: &str = "Speech recognition is turned off for Hanzi Cards. Turn it on in the iPhone's Settings › Privacy & Security › Speech Recognition.";

/// "No speech detected" (the assistant framework's error 1110): nothing was said.
const NO_SPEECH: isize = 1110;

/// Whether spoken answers can be checked: `None` when they can (or the learner hasn't been
/// asked yet: the question comes with the first answer), else why not.
pub fn unavailable() -> Option<String> {
    match unsafe { SFSpeechRecognizer::authorizationStatus() } {
        Access::Denied | Access::Restricted => Some(REFUSED.into()),
        _ => None,
    }
}

/// Ask for permission when it hasn't been asked yet; true when it's granted.
fn allowed() -> bool {
    match unsafe { SFSpeechRecognizer::authorizationStatus() } {
        Access::Authorized => true,
        Access::NotDetermined => {
            let (tx, rx) = mpsc::channel();
            let answered = RcBlock::new(move |status: Access| {
                let _ = tx.send(status == Access::Authorized);
            });
            unsafe { SFSpeechRecognizer::requestAuthorization(&answered) };
            rx.recv_timeout(Duration::from_secs(300)).unwrap_or(false)
        }
        _ => false,
    }
}

/// What was said in `pcm` (16 kHz mono), in `lang` ("zh-CN", "en-US"). Empty when nothing
/// was said. Blocks until the recognizer is done, so call it off the main thread.
pub fn transcribe(tmp_dir: &Path, pcm: &[f32], lang: &str, hints: &[String]) -> Result<String, String> {
    if !allowed() {
        return Err(REFUSED.into());
    }
    static NEXT: AtomicU64 = AtomicU64::new(0);
    std::fs::create_dir_all(tmp_dir).map_err(|e| e.to_string())?;
    let path = tmp_dir.join(format!("answer-{}.wav", NEXT.fetch_add(1, Ordering::Relaxed)));
    crate::speech::write_wav(&path, pcm).map_err(|e| e.to_string())?;
    let text = recognize(&path, lang, hints);
    let _ = std::fs::remove_file(&path);
    text
}

fn recognize(path: &Path, lang: &str, hints: &[String]) -> Result<String, String> {
    let path = path.to_str().ok_or("The recording's path isn't valid text.")?;
    let locale = NSLocale::initWithLocaleIdentifier(NSLocale::alloc(), &NSString::from_str(lang));
    let recognizer = unsafe { SFSpeechRecognizer::initWithLocale(SFSpeechRecognizer::alloc(), &locale) }
        .ok_or_else(|| format!("This iPhone has no speech recognition for {lang}."))?;
    if !unsafe { recognizer.isAvailable() } {
        return Err("Speech recognition isn't available right now. Try again in a moment.".into());
    }
    if !unsafe { recognizer.supportsOnDeviceRecognition() } {
        let language = if lang.starts_with("zh") { "Chinese (Mandarin)" } else { "English" };
        return Err(format!(
            "This iPhone can't recognise {language} on its own yet. Add a {language} keyboard (Settings › General › Keyboard › Keyboards) and turn on Dictation there; the iPhone then downloads what it needs."
        ));
    }
    // (results come on a queue of their own, not the main thread's, which may be busy)
    unsafe { recognizer.setQueue(&NSOperationQueue::new()) };

    let url = NSURL::fileURLWithPath(&NSString::from_str(path));
    let request = unsafe { SFSpeechURLRecognitionRequest::initWithURL(SFSpeechURLRecognitionRequest::alloc(), &url) };
    unsafe {
        request.setShouldReportPartialResults(false);
        request.setRequiresOnDeviceRecognition(true);
        let words: Vec<Retained<NSString>> = hints.iter().take(100).map(|w| NSString::from_str(w)).collect();
        request.setContextualStrings(&NSArray::from_retained_slice(&words));
    }

    let (tx, rx) = mpsc::channel::<Result<String, String>>();
    let handler = RcBlock::new(move |result: *mut SFSpeechRecognitionResult, error: *mut NSError| {
        // SAFETY: the recognizer passes valid objects or null
        if let Some(e) = unsafe { error.as_ref() } {
            let _ = tx.send(if e.code() == NO_SPEECH { Ok(String::new()) } else { Err(e.localizedDescription().to_string()) });
        } else if let Some(r) = unsafe { result.as_ref() } {
            if unsafe { r.isFinal() } {
                let _ = tx.send(Ok(unsafe { r.bestTranscription().formattedString() }.to_string()));
            }
        }
    });
    let task = unsafe { recognizer.recognitionTaskWithRequest_resultHandler(&request, &handler) };
    match rx.recv_timeout(Duration::from_secs(20)) {
        Ok(text) => text,
        Err(_) => {
            unsafe { task.cancel() };
            Err("Speech recognition took too long. Try again.".into())
        }
    }
}
