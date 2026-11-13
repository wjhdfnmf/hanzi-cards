//! What the app accepts from outside: text typed or imported, files chosen, settings saved.
//!
//! Everything a learner types is stored and shown as plain text (the page never builds HTML
//! from it, and every query passes it as a parameter), so these checks are about keeping the
//! data well-formed and bounded: no invisible control characters, no single field or file
//! large enough to make the app crawl, no setting the app doesn't know.

use crate::error::{AppError, AppResult};
use std::path::Path;

// Characters (not bytes). Generous: the longest real values are a fraction of these.
pub const MAX_HANZI: usize = 200;
pub const MAX_PINYIN: usize = 400;
pub const MAX_MEANING: usize = 2_000;
pub const MAX_NOTES: usize = 10_000;
pub const MAX_TAGS: usize = 500;
pub const MAX_NAME: usize = 200;
pub const MAX_DESCRIPTION: usize = 2_000;
pub const MAX_STORY: usize = 1_000_000;
/// A message to the tutor, or a custom scene for it.
pub const MAX_MESSAGE: usize = 2_000;
pub const MAX_QUERY: usize = 200;

// Files, in bytes.
pub const MAX_TEXT_FILE: u64 = 50 * 1024 * 1024; // CSV / TSV
pub const MAX_XML_FILE: u64 = 100 * 1024 * 1024; // Pleco
pub const MAX_STORY_FILE: u64 = 5 * 1024 * 1024;
pub const MAX_DICTIONARY: u64 = 200 * 1024 * 1024; // CC-CEDICT, unpacked

/// A recording handed over for recognition: a minute at 16 kHz (the page stops at 20 s).
pub const MAX_PCM: usize = 60 * 16_000;

/// Text as it is stored: without control characters (invisible, and they break exports).
/// With `lines`, line breaks and tabs are kept.
pub fn clean(s: &str, lines: bool) -> String {
    s.chars().filter(|c| !c.is_control() || (lines && matches!(c, '\n' | '\t'))).collect()
}

/// `s` cut to `max` characters (for imports, where one long field shouldn't lose the row).
pub fn clip(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((at, _)) => s[..at].to_string(),
        None => s.to_string(),
    }
}

/// An error when `s` is longer than `max` characters (for what the learner types).
pub fn fits(what: &str, s: &str, max: usize) -> AppResult<()> {
    if s.chars().nth(max).is_some() {
        return Err(AppError::msg(format!("{what} is too long (at most {max} characters).")));
    }
    Ok(())
}

/// A file's bytes, refused when it is larger than `max` (nothing is read then).
pub fn read_file(path: &Path, max: u64) -> AppResult<Vec<u8>> {
    let size = std::fs::metadata(path)?.len();
    if size > max {
        return Err(AppError::msg(format!(
            "{} is too large to import ({} MB; at most {} MB).",
            path.display(),
            size / 1_048_576,
            max / 1_048_576
        )));
    }
    Ok(std::fs::read(path)?)
}

pub fn check_pcm(pcm: &[f32]) -> AppResult<()> {
    if pcm.len() > MAX_PCM {
        return Err(AppError::msg("That recording is too long (at most a minute)."));
    }
    // (a sample that isn't a number would turn everything computed from the recording into
    // not-a-number too: the pitch, and every score Whisper gives)
    if pcm.iter().any(|v| !v.is_finite()) {
        return Err(AppError::msg("That recording is damaged. Please try again."));
    }
    Ok(())
}

/// The settings the page may save. The app's own records in the same table (its version, the
/// pitch profile) aren't among them.
const SETTINGS: [&str; 34] = [
    "language", "theme", "desired_retention", "new_per_day", "answer_buttons", "one_direction_per_session",
    "new_recalls", "relearn_recalls", "again_minutes", "learn_gaps", "relearn_minutes", "min_gap_cards", "fuzz",
    "max_interval_days", "leech_lapses", "new_order", "tone_colors", "hsk_autotag", "speech_rate", "autoplay_audio",
    "voice", "audio_output", "audio_output_label", "speech_engine", "whisper_model", "whisper_device", "pitch_tones",
    "replay_answers", "tutor_model", "debug_mode", "reading_font", "reading_size", "reading_line_height",
    "reading_tone_colors",
];

pub fn check_setting(key: &str, value: &str) -> AppResult<()> {
    if !SETTINGS.contains(&key) {
        return Err(AppError::msg(format!("Unknown setting: {key}")));
    }
    fits("That setting", value, 500)?;
    if value.chars().any(char::is_control) {
        return Err(AppError::msg("A setting can't contain control characters."));
    }
    Ok(())
}

/// `%`, `_` and `\` in text that goes into a LIKE pattern, so they match themselves (the
/// pattern is used with `ESCAPE '\'`).
pub fn like_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Spreadsheet programs run a cell that starts like a formula. Cards are also exchanged
/// between people, so an exported cell that would start one gets an apostrophe in front,
/// which spreadsheets show as plain text: always for `=` and `@`; for `+` and `-` only when
/// the rest has a call or a link in it, so that "-ology" and "+1" stay as they are.
pub fn guard_cell(s: &str) -> std::borrow::Cow<'_, str> {
    let risky = match s.chars().next() {
        Some('=' | '@' | '\t' | '\r') => true,
        Some('+' | '-') => s.contains(['(', '|', '!']),
        _ => false,
    };
    if risky {
        std::borrow::Cow::Owned(format!("'{s}"))
    } else {
        std::borrow::Cow::Borrowed(s)
    }
}

/// Takes off the apostrophe `guard_cell` added (importing a file the app exported).
pub fn unguard_cell(s: &str) -> &str {
    match s.strip_prefix('\'') {
        Some(rest) if matches!(guard_cell(rest), std::borrow::Cow::Owned(_)) => rest,
        _ => s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_and_limits_text() {
        assert_eq!(clean("你\u{0}好\u{7}\r\n", false), "你好");
        assert_eq!(clean("a\tb\nc\u{1b}", true), "a\tb\nc");
        assert_eq!(clip("你好吗", 2), "你好");
        assert_eq!(clip("你好", 5), "你好");
        assert!(fits("It", "你好", 2).is_ok() && fits("It", "你好吗", 2).is_err());
        assert!(check_pcm(&vec![0.0; MAX_PCM]).is_ok() && check_pcm(&vec![0.0; MAX_PCM + 1]).is_err());
        assert!(check_pcm(&[0.1, f32::NAN]).is_err() && check_pcm(&[f32::INFINITY]).is_err());
    }

    #[test]
    fn only_known_settings() {
        assert!(check_setting("theme", "dark").is_ok());
        assert!(check_setting("app_version", "9.9.9").is_err(), "the app's own record");
        assert!(check_setting("pitch_profile", "").is_err());
        assert!(check_setting("theme", &"x".repeat(501)).is_err());
        assert!(check_setting("voice", "a\u{0}b").is_err());
    }

    #[test]
    fn like_patterns_and_spreadsheet_cells() {
        assert_eq!(like_escape("50%_a\\"), "50\\%\\_a\\\\");
        for (cell, guarded) in [
            ("=1+1", "'=1+1"),
            ("@SUM(A1)", "'@SUM(A1)"),
            ("+cmd|' /C calc'!A0", "'+cmd|' /C calc'!A0"),
            ("-2+HYPERLINK(\"x\")", "'-2+HYPERLINK(\"x\")"),
            ("-ology", "-ology"),
            ("+1", "+1"),
            ("to like; to be fond of", "to like; to be fond of"),
            ("'quoted'", "'quoted'"),
        ] {
            assert_eq!(guard_cell(cell), guarded);
            assert_eq!(unguard_cell(&guard_cell(cell)), cell, "round trip");
        }
        assert_eq!(unguard_cell("'tis"), "'tis", "an apostrophe of the learner's own stays");
    }
}
