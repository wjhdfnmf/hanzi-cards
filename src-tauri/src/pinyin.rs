//! Pinyin helpers: numbered pinyin ("ni3 hao3") to tone marks ("nǐ hǎo"),
//! and normalised keys used for searching.

const TABLE: [(char, [char; 4]); 12] = [
    ('a', ['ā', 'á', 'ǎ', 'à']),
    ('e', ['ē', 'é', 'ě', 'è']),
    ('i', ['ī', 'í', 'ǐ', 'ì']),
    ('o', ['ō', 'ó', 'ǒ', 'ò']),
    ('u', ['ū', 'ú', 'ǔ', 'ù']),
    ('ü', ['ǖ', 'ǘ', 'ǚ', 'ǜ']),
    ('A', ['Ā', 'Á', 'Ǎ', 'À']),
    ('E', ['Ē', 'É', 'Ě', 'È']),
    ('I', ['Ī', 'Í', 'Ǐ', 'Ì']),
    ('O', ['Ō', 'Ó', 'Ǒ', 'Ò']),
    ('U', ['Ū', 'Ú', 'Ǔ', 'Ù']),
    ('Ü', ['Ǖ', 'Ǘ', 'Ǚ', 'Ǜ']),
];

fn mark(c: char, tone: u8) -> char {
    TABLE
        .iter()
        .find(|(b, _)| *b == c)
        .map(|(_, m)| m[(tone - 1) as usize])
        .unwrap_or(c)
}

/// Returns the unmarked letter and tone (0 when the char carries no tone mark).
fn base_char(c: char) -> (char, u8) {
    for (b, marks) in TABLE.iter() {
        if let Some(i) = marks.iter().position(|m| *m == c) {
            return (*b, i as u8 + 1);
        }
    }
    (c, 0)
}

/// Put a tone mark on one syllable following the standard placement rules:
/// `a` or `e` takes the mark, `ou` marks the `o`, otherwise the last vowel.
pub fn apply_tone(syllable: &str, tone: u8) -> String {
    let s = syllable
        .replace("u:", "ü")
        .replace("U:", "Ü")
        .replace('v', "ü")
        .replace('V', "Ü");
    if !(1..=4).contains(&tone) {
        return s;
    }
    let chars: Vec<char> = s.chars().collect();
    let lower: Vec<char> = chars
        .iter()
        .map(|c| c.to_lowercase().next().unwrap_or(*c))
        .collect();
    let idx = lower
        .iter()
        .position(|&c| c == 'a')
        .or_else(|| lower.iter().position(|&c| c == 'e'))
        .or_else(|| lower.windows(2).position(|w| w[0] == 'o' && w[1] == 'u'))
        .or_else(|| lower.iter().rposition(|c| "aeiouü".contains(*c)));
    match idx {
        Some(i) => {
            let mut out = chars.clone();
            out[i] = mark(chars[i], tone);
            out.into_iter().collect()
        }
        None => s,
    }
}

/// Convert numbered pinyin to tone marks. Works for spaced ("ni3 hao3") and
/// run-together ("ni3hao3") input; text without tone numbers passes through.
pub fn numbered_to_marks(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut buf = String::new();
    for c in input.chars() {
        let is_letter = c.is_ascii_alphabetic() || c == 'ü' || c == 'Ü';
        let is_umlaut_colon = c == ':' && (buf.ends_with('u') || buf.ends_with('U'));
        if is_letter || is_umlaut_colon {
            buf.push(c);
        } else if ('0'..='5').contains(&c) && !buf.is_empty() {
            out.push_str(&apply_tone(&buf, c as u8 - b'0'));
            buf.clear();
        } else {
            out.push_str(&buf);
            buf.clear();
            out.push(c);
        }
    }
    out.push_str(&buf);
    out
}

/// Lower-case letters only, tones removed, ü written as v: "Běi jīng" -> "beijing".
pub fn toneless_key(s: &str) -> String {
    let s = s.replace("u:", "v").replace("U:", "v");
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        let (b, _) = base_char(c);
        let b = if b == 'ü' || b == 'Ü' { 'v' } else { b };
        if b.is_ascii_alphabetic() {
            out.push(b.to_ascii_lowercase());
        }
    }
    out
}

/// Like [`toneless_key`] but keeps tone digits: "Bei3 jing1" -> "bei3jing1".
pub fn toned_key(s: &str) -> String {
    let s = s.replace("u:", "v").replace("U:", "v");
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        let (b, _) = base_char(c);
        let b = if b == 'ü' || b == 'Ü' { 'v' } else { b };
        if b.is_ascii_alphanumeric() {
            out.push(b.to_ascii_lowercase());
        }
    }
    out
}

/// True when the string contains CJK ideographs.
pub fn has_hanzi(s: &str) -> bool {
    s.chars().any(|c| {
        matches!(c as u32,
            0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x20000..=0x2EBEF | 0x3007)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_numbers_to_marks() {
        assert_eq!(numbered_to_marks("ni3 hao3"), "nǐ hǎo");
        assert_eq!(numbered_to_marks("ni3hao3"), "nǐhǎo");
        assert_eq!(numbered_to_marks("lu:4"), "lǜ");
        assert_eq!(numbered_to_marks("nv3"), "nǚ");
        assert_eq!(numbered_to_marks("Bei3 jing1"), "Běi jīng");
        assert_eq!(numbered_to_marks("xiu1 gui4 liu2 zhou1 er2"), "xiū guì liú zhōu ér");
        assert_eq!(numbered_to_marks("xie4 xie5"), "xiè xie");
        assert_eq!(numbered_to_marks("already nǐ"), "already nǐ");
    }

    #[test]
    fn builds_search_keys() {
        assert_eq!(toneless_key("Běi jīng"), "beijing");
        assert_eq!(toneless_key("lu:4 se4"), "lvse");
        assert_eq!(toned_key("Bei3 jing1"), "bei3jing1");
        assert!(has_hanzi("你好"));
        assert!(!has_hanzi("hello"));
    }
}
