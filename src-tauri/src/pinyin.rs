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

fn is_vowel(c: char) -> bool {
    matches!(c, 'a' | 'e' | 'i' | 'o' | 'u' | 'v')
}

/// Split one pinyin word into syllables, returning index ranges.
///
/// `letters` are lower-case base letters (ü written as `v`); `marked[i]` says letter `i`
/// carried a tone mark, so a second marked vowel starts a new syllable ("nǚér").
/// Follows pinyin spelling rules: `n`/`ng` end a syllable unless a vowel follows,
/// `er` is a syllable, and a trailing `r` after a syllable is erhua ("zhèr" = zhè + r).
pub fn split_syllables(letters: &[char], marked: &[bool]) -> Vec<(usize, usize)> {
    let n = letters.len();
    let vowel_at = |k: usize| k < n && is_vowel(letters[k]);
    let mut out = Vec::new();
    let mut i = 0;
    while i < n {
        let start = i;
        // erhua: a lone r after a syllable
        if letters[i] == 'r' && i > 0 && !vowel_at(i + 1) {
            out.push((i, i + 1));
            i += 1;
            continue;
        }
        if i + 1 < n && matches!(letters[i], 'z' | 'c' | 's') && letters[i + 1] == 'h' {
            i += 2;
        } else if !is_vowel(letters[i]) {
            i += 1;
        }
        let vowels_start = i;
        let mut seen_mark = false;
        while i < n && is_vowel(letters[i]) {
            if marked.get(i).copied().unwrap_or(false) {
                if seen_mark {
                    break;
                }
                seen_mark = true;
            }
            i += 1;
        }
        if i == vowels_start {
            // no vowel: syllabic n / ng / m
            if i < n && letters[i] == 'g' && i > 0 && letters[i - 1] == 'n' {
                i += 1;
            }
            out.push((start, i));
            continue;
        }
        if i + 1 < n && letters[i] == 'n' && letters[i + 1] == 'g' && !vowel_at(i + 2) {
            i += 2;
        } else if i < n && letters[i] == 'n' && !vowel_at(i + 1) {
            i += 1;
        } else if i < n
            && letters[i] == 'r'
            && vowels_start == start
            && letters[vowels_start..i] == ['e']
            && !vowel_at(i + 1)
        {
            i += 1; // er
        }
        out.push((start, i));
    }
    out
}

/// Lower-case base letter, whether it carried a tone mark, and its tone (0 = none).
fn letter_info(c: char) -> (char, u8) {
    let (b, tone) = base_char(c);
    let b = match b {
        'ü' | 'Ü' => 'v',
        other => other.to_ascii_lowercase(),
    };
    (b, tone)
}

/// Tone-marked pinyin to numbered pinyin, e.g. for Pleco: "nǐ hǎo" -> "ni3 hao3",
/// "zhèr" -> "zhe4r5", "nǚ'ér" -> "nü3er2". Unmarked syllables get 5 (neutral tone).
/// Text that isn't pinyin (spaces, brackets) is kept.
pub fn marks_to_numbered(input: &str) -> String {
    let mut out = String::with_capacity(input.len() + 8);
    let chars: Vec<char> = input.chars().collect();
    let is_letter = |c: char| c.is_alphabetic() && !has_hanzi(&c.to_string());
    let mut i = 0;
    while i < chars.len() {
        if !is_letter(chars[i]) {
            if chars[i] != '\'' && chars[i] != '’' {
                out.push(chars[i]);
            }
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && is_letter(chars[i]) {
            i += 1;
        }
        let word = &chars[start..i];
        let info: Vec<(char, u8)> = word.iter().map(|c| letter_info(*c)).collect();
        let letters: Vec<char> = info.iter().map(|(b, _)| *b).collect();
        let marked: Vec<bool> = info.iter().map(|(_, t)| *t > 0).collect();
        for (a, b) in split_syllables(&letters, &marked) {
            let tone = info[a..b].iter().map(|(_, t)| *t).find(|t| *t > 0).unwrap_or(5);
            for c in &word[a..b] {
                let (base, _) = base_char(*c);
                out.push(base);
            }
            out.push(char::from(b'0' + tone));
        }
    }
    out
}

/// True for a CJK ideograph.
pub fn is_hanzi(c: char) -> bool {
    matches!(c as u32,
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x20000..=0x2EBEF | 0x3007)
}

/// True when the string contains CJK ideographs.
pub fn has_hanzi(s: &str) -> bool {
    s.chars().any(is_hanzi)
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

    fn split(word: &str) -> Vec<String> {
        let letters: Vec<char> = word.chars().collect();
        let marked = vec![false; letters.len()];
        split_syllables(&letters, &marked)
            .into_iter()
            .map(|(a, b)| letters[a..b].iter().collect())
            .collect()
    }

    #[test]
    fn splits_syllables() {
        assert_eq!(split("nihao"), ["ni", "hao"]);
        assert_eq!(split("zhongguo"), ["zhong", "guo"]);
        assert_eq!(split("xian"), ["xian"]);
        assert_eq!(split("shengri"), ["sheng", "ri"]);
        assert_eq!(split("zher"), ["zhe", "r"]);
        assert_eq!(split("yidianr"), ["yi", "dian", "r"]);
        assert_eq!(split("erzi"), ["er", "zi"]);
        assert_eq!(split("nvhair"), ["nv", "hai", "r"]);
        assert_eq!(split("chuang"), ["chuang"]);
    }

    #[test]
    fn converts_marks_to_numbers() {
        assert_eq!(marks_to_numbered("nǐ hǎo"), "ni3 hao3");
        assert_eq!(marks_to_numbered("nǐhǎo"), "ni3hao3");
        assert_eq!(marks_to_numbered("zhèr"), "zhe4r5");
        assert_eq!(marks_to_numbered("xué sheng"), "xue2 sheng5");
        assert_eq!(marks_to_numbered("nǚ'ér"), "nü3er2");
        assert_eq!(marks_to_numbered("nǚér"), "nü3er2");
        assert_eq!(marks_to_numbered("Běi jīng"), "Bei3 jing1");
        assert_eq!(marks_to_numbered("diǎn (zhōng)"), "dian3 (zhong1)");
        assert_eq!(marks_to_numbered("yī diǎnr"), "yi1 dian3r5");
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
