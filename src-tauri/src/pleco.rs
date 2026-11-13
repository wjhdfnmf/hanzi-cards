//! Pleco flashcard files (`<plecoflash>` XML): import and export.
//!
//! A Pleco card has a headword (simplified, sometimes traditional too), numbered pinyin,
//! a definition and category assignments. Categories become decks ("Class/Class 9"
//! becomes the deck "Class 9"); a card in several categories goes into each deck.

use crate::cards::{self, DeckRef, Dup};
use crate::db::now;
use crate::decks;
use crate::dictionary;
use crate::error::{AppError, AppResult};
use crate::models::{PlecoCategory, PlecoImportResult, PlecoPreview};
use crate::pinyin;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashSet;
use std::path::Path;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct PlecoCard {
    pub hanzi: String,
    pub traditional: Option<String>,
    pub pron: String,
    pub defn: String,
    pub categories: Vec<String>,
    pub created: Option<i64>,
    pub modified: Option<i64>,
}

// ---------- reading ----------

pub fn read_file(path: &Path) -> AppResult<Vec<PlecoCard>> {
    let bytes = crate::input::read_file(path, crate::input::MAX_XML_FILE)?;
    let text = String::from_utf8(bytes).map_err(|_| AppError::msg("The Pleco file is not UTF-8 text."))?;
    parse(&text)
}

pub fn parse(xml: &str) -> AppResult<Vec<PlecoCard>> {
    let xml = xml.trim_start_matches('\u{feff}');
    let doc = roxmltree::Document::parse(xml)
        .map_err(|e| AppError::msg(format!("This isn't a readable Pleco flashcard file: {e}")))?;
    let root = doc.root_element();
    if !root.has_tag_name("plecoflash") {
        return Err(AppError::msg("This isn't a Pleco flashcard file (no <plecoflash> element)."));
    }
    let text = |n: roxmltree::Node| -> String {
        n.descendants().filter(|d| d.is_text()).filter_map(|d| d.text()).collect::<String>().trim().to_string()
    };
    let mut out = Vec::new();
    for card in root.descendants().filter(|n| n.has_tag_name("card")) {
        let Some(entry) = card.children().find(|n| n.has_tag_name("entry")) else { continue };
        let heads: Vec<roxmltree::Node> = entry.children().filter(|n| n.has_tag_name("headword")).collect();
        let simplified = heads
            .iter()
            .find(|h| h.attribute("charset").is_none_or(|c| c == "sc"))
            .or(heads.first());
        let Some(simplified) = simplified else { continue };
        let hanzi = text(*simplified);
        let traditional = heads
            .iter()
            .find(|h| h.attribute("charset") == Some("tc"))
            .map(|h| text(*h))
            .filter(|t| *t != hanzi && !t.is_empty());
        let child = |name: &str| entry.children().find(|n| n.has_tag_name(name)).map(text).unwrap_or_default();
        out.push(PlecoCard {
            hanzi,
            traditional,
            pron: child("pron"),
            defn: child("defn"),
            categories: card
                .children()
                .filter(|n| n.has_tag_name("catassign"))
                .filter_map(|n| n.attribute("category"))
                .map(|c| c.trim().to_string())
                .filter(|c| !c.is_empty())
                .collect(),
            created: card.attribute("created").and_then(|v| v.parse().ok()),
            modified: card.attribute("modified").and_then(|v| v.parse().ok()),
        });
    }
    Ok(out)
}

// ---------- pinyin ----------

fn starts_like_glide(s: &str) -> bool {
    s.starts_with(['i', 'u', 'v', 'ü'])
}

/// Pleco's numbered pinyin to tone marks: "ni3hao3" -> "nǐ hǎo".
///
/// Cards typed in Pleco with tone marks can export with digits in odd places
/// ("na3i5na3i5" for nǎinai, "dia3n5" for diǎn). Pieces that can't stand alone as a
/// syllable (just consonants, or starting with i/u/ü) are joined back to the one before.
pub fn pron_to_marks(pron: &str) -> String {
    let pron = pron.trim();
    if !pron.chars().any(|c| c.is_ascii_digit()) {
        return pron.to_string();
    }
    let mut out = String::new();
    let chars: Vec<char> = pron.chars().collect();
    let in_word = |c: char| c.is_ascii_alphanumeric() || matches!(c, 'ü' | 'Ü' | ':');
    let mut i = 0;
    while i < chars.len() {
        if !in_word(chars[i]) {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && in_word(chars[i]) {
            i += 1;
        }
        let word: String = chars[start..i].iter().collect::<String>().replace("u:", "ü").replace("U:", "Ü");
        out.push_str(&word_to_marks(&word));
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn word_to_marks(word: &str) -> String {
    // pieces of letters, each followed by its tone digit (0 = none given)
    let mut pieces: Vec<(String, u8)> = Vec::new();
    let mut cur = String::new();
    for c in word.chars() {
        if let Some(d) = c.to_digit(10) {
            if !cur.is_empty() {
                pieces.push((std::mem::take(&mut cur), d as u8));
            }
        } else {
            cur.push(c);
        }
    }
    if !cur.is_empty() {
        pieces.push((cur, 0));
    }

    let mut merged: Vec<(String, u8)> = Vec::new();
    for (letters, tone) in pieces {
        let low = letters.to_lowercase();
        let has_vowel = low.chars().any(|c| "aeiouüv".contains(c));
        let dangling = (!has_vowel && low != "r") || starts_like_glide(&low);
        match merged.last_mut() {
            Some(prev) if dangling => {
                prev.0.push_str(&letters);
                if !(1..=4).contains(&prev.1) && tone != 0 {
                    prev.1 = tone;
                }
            }
            _ => merged.push((letters, tone)),
        }
    }

    let mut sylls: Vec<String> = Vec::new();
    for (letters, tone) in merged {
        let chars: Vec<char> = letters.chars().collect();
        let base: Vec<char> = chars
            .iter()
            .map(|c| match c.to_ascii_lowercase() {
                'ü' | 'Ü' => 'v',
                o => o,
            })
            .collect();
        let ranges = pinyin::split_syllables(&base, &vec![false; base.len()]);
        let last = ranges.len().saturating_sub(1);
        for (k, (a, b)) in ranges.into_iter().enumerate() {
            let syl: String = chars[a..b].iter().collect();
            let t = if k == last && tone > 0 { tone } else { 5 };
            sylls.push(pinyin::apply_tone(&syl, t));
        }
    }
    join_syllables(sylls.iter().map(String::as_str))
}

/// Space-separated syllables, with an erhua "r" kept on the syllable before it.
fn join_syllables<'a>(sylls: impl Iterator<Item = &'a str>) -> String {
    let mut out = String::new();
    for s in sylls {
        if s.eq_ignore_ascii_case("r") && !out.is_empty() && !out.ends_with(' ') {
            out.push_str(s);
        } else {
            if !out.is_empty() && !out.ends_with(['(', ' ']) {
                out.push(' ');
            }
            out.push_str(s);
        }
    }
    out
}

fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace() && !matches!(c, '\'' | '’')).flat_map(char::to_lowercase).collect()
}

/// Pinyin for `hanzi` from the dictionary, using Pleco's pinyin to choose between
/// readings (行 háng / xíng, 差 chà / chā …). `None` if a character isn't in the dictionary.
///
/// Words are split two ways: by dictionary words (longest match), and by following Pleco's
/// letters. Following the letters wins only if it doesn't break a dictionary word apart,
/// so a slip like "zen3ma5yang4" can't turn 怎么样 into 怎 + 么 + 样.
fn dictionary_pinyin(conn: &Connection, hanzi: &str, pleco: &[String]) -> AppResult<Option<String>> {
    let chars: Vec<char> = hanzi.chars().collect();
    let mut by_letters: Option<Segmented> = None;
    let mut by_words: Option<Segmented> = None;
    for alt in pleco.iter().map(String::as_str).chain(std::iter::once("")) {
        let marks = pron_to_marks(alt);
        let guide = Guide {
            bare: pinyin::toneless_key(&marks),
            toned: pinyin::toned_key(&pinyin::marks_to_numbered(&marks)),
            // capitalisation of each letter in `bare` (proper nouns: "Mei3guo2")
            upper: marks
                .chars()
                .filter(|c| !pinyin::toneless_key(&c.to_string()).is_empty())
                .map(char::is_uppercase)
                .collect(),
        };
        let Some(words) = segment_pinyin(conn, &chars, &guide, true)? else {
            return Ok(None);
        };
        if words.matched {
            return Ok(Some(words.text)); // dictionary words and Pleco agree
        }
        let Some(letters) = segment_pinyin(conn, &chars, &guide, false)? else {
            return Ok(None);
        };
        if letters.matched && letters.words <= words.words && by_letters.is_none() {
            by_letters = Some(letters);
        }
        by_words.get_or_insert(words);
    }
    Ok(by_letters.or(by_words).map(|s| s.text))
}

struct Guide {
    bare: String,
    toned: String,
    upper: Vec<bool>,
}

struct Segmented {
    text: String,
    /// Every word's reading matched the guide pinyin.
    matched: bool,
    words: usize,
}

/// Split `chars` into dictionary words and join their pinyin. Readings whose letters follow
/// the guide are preferred (then: whose tones appear in it, whose capitalisation matches).
/// With `longest`, words are always the longest dictionary match; otherwise shorter words
/// are tried when the longest one's reading doesn't fit the guide.
fn segment_pinyin(conn: &Connection, chars: &[char], guide: &Guide, longest: bool) -> AppResult<Option<Segmented>> {
    let mut parts: Vec<String> = Vec::new();
    let mut pos = 0usize;
    let mut matched = !guide.bare.is_empty();
    let mut words = 0;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if !pinyin::is_hanzi(c) {
            parts.push(match c {
                '（' | '(' => "(".into(),
                '）' | ')' => ")".into(),
                c if c.is_whitespace() => String::new(),
                c => c.to_string(),
            });
            i += 1;
            continue;
        }
        let run = chars[i..].iter().take_while(|c| pinyin::is_hanzi(**c)).count();
        let mut fallback: Option<(usize, crate::models::DictEntry)> = None;
        let mut picked: Option<(usize, crate::models::DictEntry)> = None;
        for len in (1..=run.min(8)).rev() {
            let word: String = chars[i..i + len].iter().collect();
            let entries: Vec<_> = dictionary::lookup(conn, &word)?
                .into_iter()
                .filter(|e| e.simplified == word || e.traditional == word)
                .collect();
            if entries.is_empty() {
                continue;
            }
            let rest = &guide.bare[pos.min(guide.bare.len())..];
            let want_upper = guide.upper.get(pos).copied().unwrap_or(false);
            let fit = |e: &crate::models::DictEntry| {
                let tones = guide.toned.contains(&pinyin::toned_key(&e.pinyin_num));
                let case = e.pinyin_num.starts_with(char::is_uppercase) == want_upper;
                u8::from(tones) * 2 + u8::from(case)
            };
            // the best-fitting reading whose letters follow the guide; among equals, the
            // first (most common) one
            let best = entries
                .iter()
                .enumerate()
                .filter(|(_, e)| rest.starts_with(&pinyin::toneless_key(&e.pinyin_num)))
                .max_by_key(|(k, e)| (fit(e), std::cmp::Reverse(*k)))
                .map(|(_, e)| e.clone());
            if let Some(e) = best {
                picked = Some((len, e));
                break;
            }
            if longest {
                fallback = Some((len, entries[0].clone()));
                break;
            }
            fallback.get_or_insert((len, entries[0].clone()));
        }
        let Some((len, entry)) = picked.or_else(|| {
            matched = false;
            fallback
        }) else {
            return Ok(None); // a character with no dictionary entry
        };
        let want_upper = guide.upper.get(pos).copied().unwrap_or(false);
        let text = if guide.bare.is_empty() { entry.pinyin.clone() } else { with_case(&entry.pinyin, want_upper) };
        pos += pinyin::toneless_key(&entry.pinyin_num).len();
        parts.push(text);
        words += 1;
        i += len;
    }
    if pos != guide.bare.len() {
        matched = false;
    }
    let sylls: Vec<&str> = parts.iter().flat_map(|p| p.split_whitespace()).collect();
    Ok(Some(Segmented { text: join_syllables(sylls.into_iter()).replace(" )", ")"), matched, words }))
}

/// Upper- or lower-case the first letter ("Xīng qī rì" -> "xīng qī rì").
fn with_case(s: &str, upper: bool) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if upper => c.to_uppercase().chain(chars).collect(),
        Some(c) => c.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn clean_definition(defn: &str) -> String {
    defn.replace('；', "; ")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("; ")
        .split(' ')
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------- decks ----------
//
// A category path "Class/Class 9" is the nested deck Class › Class 9. Cards without a
// category go into the chosen deck, or a top-level deck called "Pleco".

fn categories_in_order(cards: &[PlecoCard]) -> Vec<String> {
    let mut seen = HashSet::new();
    cards
        .iter()
        .flat_map(|c| c.categories.iter())
        .filter(|c| seen.insert(c.to_string()))
        .cloned()
        .collect()
}

/// Where each of a card's copies goes.
fn targets(conn: &Connection, card: &PlecoCard, default_deck: Option<i64>) -> AppResult<Vec<DeckRef>> {
    if card.categories.is_empty() {
        return Ok(vec![match default_deck {
            Some(id) => DeckRef::Id(id),
            None => cards::deck_ref(conn, None, &["Pleco".to_string()])?,
        }]);
    }
    card.categories
        .iter()
        .map(|c| cards::deck_ref(conn, None, &decks::split_path(c)))
        .collect()
}

fn usable(card: &PlecoCard) -> bool {
    let h = card.hanzi.trim();
    !h.is_empty() && pinyin::has_hanzi(h) && h.chars().count() <= crate::input::MAX_HANZI && !h.chars().any(char::is_control)
}

pub fn preview(conn: &Connection, cards: &[PlecoCard], default_deck: Option<i64>) -> AppResult<PlecoPreview> {
    let mut categories = Vec::new();
    for c in categories_in_order(cards) {
        let path = decks::split_path(&c);
        categories.push(PlecoCategory {
            deck: decks::label(&path),
            exists: decks::find_path(conn, None, &path)?.is_some(),
            cards: cards.iter().filter(|k| k.categories.contains(&c)).count() as i64,
            name: c,
        });
    }
    let mut items = Vec::new();
    for card in cards.iter().filter(|c| usable(c)) {
        for t in targets(conn, card, default_deck)? {
            items.push((t, card.hanzi.trim()));
        }
    }
    let invalid = cards.iter().filter(|c| !usable(c)).count() as i64;
    Ok(PlecoPreview {
        cards: cards.len() as i64,
        categories,
        uncategorized: cards.iter().filter(|c| c.categories.is_empty()).count() as i64,
        dictionary: dictionary::status(conn)?.entries > 0,
        duplicates: cards::DupChecker::load(conn)?.preview(items.into_iter(), invalid),
    })
}

// ---------- import ----------

pub struct ImportOptions {
    /// Deck for cards without a category; `None` = a top-level deck called "Pleco".
    pub default_deck: Option<i64>,
    /// Correct pinyin and fill in missing definitions from the dictionary.
    pub use_dictionary: bool,
    /// Also import words that are already in other decks (they are studied once).
    pub include_other_decks: bool,
}

pub fn import(conn: &mut Connection, cards: &[PlecoCard], opts: &ImportOptions) -> AppResult<PlecoImportResult> {
    let use_dict = opts.use_dictionary && dictionary::status(conn)?.entries > 0;
    let tx = conn.transaction()?;
    let mut result = PlecoImportResult::default();
    let mut dups = cards::DupChecker::load(&tx)?;
    let mut used_decks: HashSet<i64> = HashSet::new();
    let ts = now();
    for (n, card) in cards.iter().enumerate() {
        if !usable(card) {
            result.skipped += 1;
            continue;
        }
        let hanzi = card.hanzi.trim();
        // decide which copies to add before doing any (slow) pinyin work
        let mut to_add = Vec::new();
        for target in targets(&tx, card, opts.default_deck)? {
            match dups.check(&target, hanzi) {
                Dup::SameDeck => result.skipped += 1,
                Dup::OtherDecks(_) if !opts.include_other_decks => result.skipped_other += 1,
                _ => {
                    let (deck, made) = cards::resolve_deck(&tx, &target, "Imported from Pleco")?;
                    result.decks_created += made as i64;
                    dups.add(DeckRef::Id(deck), hanzi);
                    to_add.push(deck);
                }
            }
        }
        if to_add.is_empty() {
            continue;
        }

        let alts: Vec<String> =
            card.pron.split(';').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
        let pleco_pinyin = alts.first().map(|a| pron_to_marks(a)).unwrap_or_default();
        let mut pinyin_text = crate::input::clip(&crate::input::clean(&pleco_pinyin, false), crate::input::MAX_PINYIN);
        let mut meaning = crate::input::clip(&crate::input::clean(&clean_definition(&card.defn), true), crate::input::MAX_MEANING);
        if use_dict {
            if let Some(p) = dictionary_pinyin(&tx, hanzi, &alts)? {
                if !pleco_pinyin.is_empty() && compact(&p) != compact(&pleco_pinyin) {
                    result.pinyin_corrected += 1;
                }
                pinyin_text = p;
            }
            if meaning.is_empty() {
                if let Some(e) = dictionary::lookup(&tx, hanzi)?.into_iter().next() {
                    meaning = e
                        .definitions
                        .iter()
                        .filter(|d| !d.starts_with("CL:"))
                        .take(3)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join("; ");
                    result.definitions_added += 1;
                }
            }
        }

        let created = card.created.unwrap_or(ts + n as i64);
        for deck in to_add {
            tx.execute(
                "INSERT INTO cards (deck_id, hanzi, pinyin, meaning, notes, tags, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, '', ?7, ?5, ?6)",
                params![deck, hanzi, pinyin_text, meaning, created, card.modified.unwrap_or(created), crate::hsk::tags_for_new(&tx, "", &hanzi)],
            )?;
            used_decks.insert(deck);
            result.imported += 1;
        }
    }
    result.decks = used_decks.len() as i64;
    if result.imported > 0 {
        crate::study::share_progress(&tx, None)?; // words already studied in other decks
    }
    tx.commit()?;
    Ok(result)
}

// ---------- export ----------

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

/// Write cards (one deck, or all) as a Pleco flashcard file; decks become categories.
pub fn export(conn: &Connection, deck_id: Option<i64>, path: &Path) -> AppResult<i64> {
    let text = export_xml(conn, deck_id)?;
    std::fs::write(path, text.0)?;
    Ok(text.1)
}

pub fn export_xml(conn: &Connection, deck_id: Option<i64>) -> AppResult<(String, i64)> {
    // cards of the deck and its subdecks, oldest first
    let rows: Vec<_> = cards::list_cards(conn, deck_id, "")?.into_iter().rev().map(|r| r.card).collect();
    let paths = decks::paths(conn)?;
    let sep = decks::PATH_FILE_SEP.to_string();
    let category = |deck: i64| paths.get(&deck).map(|p| p.join(&sep)).unwrap_or_default();
    let has_dict = dictionary::status(conn)?.entries > 0;
    let mut trad_stmt = conn.prepare(
        "SELECT traditional FROM dictionary WHERE simplified = ?1 ORDER BY score DESC LIMIT 1",
    )?;

    let mut categories: Vec<String> = Vec::new();
    for c in &rows {
        let cat = category(c.deck_id);
        if !categories.contains(&cat) {
            categories.push(cat);
        }
    }
    let mut x = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    x.push_str(&format!(
        "<plecoflash formatversion=\"2\" creator=\"Hanzi Cards\" generator=\"Hanzi Cards {}\" platform=\"{}\" created=\"{}\">\n<categories>",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        now()
    ));
    for c in &categories {
        x.push_str(&format!("<category name=\"{}\"/>", escape(c)));
    }
    x.push_str("</categories>\n<cards>\n");
    for c in &rows {
        let trad: Option<String> = if has_dict {
            trad_stmt.query_row([&c.hanzi], |r| r.get(0)).optional()?.filter(|t: &String| *t != c.hanzi)
        } else {
            None
        };
        x.push_str(&format!(
            "<card language=\"chinese\" created=\"{}\" modified=\"{}\"><entry>",
            c.created_at, c.updated_at
        ));
        x.push_str(&format!("<headword charset=\"sc\">{}</headword>", escape(&c.hanzi)));
        x.push_str(&format!(
            "<headword charset=\"tc\">{}</headword>",
            escape(trad.as_deref().unwrap_or(&c.hanzi))
        ));
        if !c.pinyin.trim().is_empty() {
            x.push_str(&format!(
                "<pron type=\"hypy\" tones=\"numbers\">{}</pron>",
                escape(&pinyin::marks_to_numbered(c.pinyin.trim()))
            ));
        }
        if !c.meaning.trim().is_empty() {
            x.push_str(&format!("<defn>{}</defn>", escape(c.meaning.trim())));
        }
        x.push_str(&format!("</entry><catassign category=\"{}\"/></card>\n", escape(&category(c.deck_id))));
    }
    x.push_str("</cards>\n</plecoflash>\n");
    Ok((x, rows.len() as i64))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?><plecoflash formatversion="2" creator="Pleco User" generator="Pleco 2.0 Flashcard Exporter" platform="iPhone OS" created="1794198212"><categories><category name="Numbers"/><category name="HSK1-1"/><category name="Class/Class 9"/></categories><cards>
<card language="chinese" created="1780456431" modified="1780631905"><entry><headword>我</headword><pron type="hypy" tones="numbers">wo3</pron><defn>I；me</defn></entry><catassign category="HSK1-1"/></card>
<card language="chinese" created="1780458112" modified="1780459494"><entry><headword charset="sc">一</headword><headword charset="tc">一</headword><pron type="hypy" tones="numbers">yi1</pron><defn>one; 1</defn></entry><catassign category="Numbers"/></card>
<card language="chinese" created="1782194236" modified="1782194236"><entry><headword>奶奶</headword><pron type="hypy" tones="numbers">na3i5na3i5</pron><defn>Dad's Grandmother</defn></entry><catassign category="Class/Class 9"/></card>
<card language="chinese" created="1788832357" modified="1788832367"><entry><headword>那儿</headword><pron type="hypy" tones="numbers">na4 er2; na4r5</pron><defn>there &amp; then</defn></entry><catassign category="Class/Class 9"/><catassign category="HSK1-1"/></card>
<card language="chinese" created="1790524281" modified="1790524281"><entry><headword>差</headword><pron type="hypy" tones="numbers">cha4</pron><defn>to be short of</defn></entry></card>
<card language="chinese" created="1790524283" modified="1790524283"><entry><headword>你怎么样</headword><pron type="hypy" tones="numbers">ni3zen3ma5yang4</pron><defn>How are you?</defn></entry><catassign category="Class/Class 9"/></card>
<card language="chinese" created="1790524282" modified="1790524282"><entry><headword>学生</headword><pron type="hypy" tones="numbers">xue2sheng1</pron></entry><catassign category="HSK1-1"/></card>
</cards></plecoflash>"#;

    const DICT: &str = "我 我 [wo3] /I/me/my/
奶奶 奶奶 [nai3 nai5] /grandma/
那兒 那儿 [na4 r5] /there/
差 差 [cha1] /difference/
差 差 [cha4] /to differ from/short of/
學生 学生 [xue2 sheng5] /student/schoolchild/
一 一 [yi1] /one/
你 你 [ni3] /you/
怎麼樣 怎么样 [zen3 me5 yang4] /how?/how about?/
怎 怎 [zen3] /how/
麼 么 [me5] /suffix/
嗎 么 [ma5] /variant of 嗎/
樣 样 [yang4] /manner/
那 那 [na4] /that/
兒 儿 [er2] /son/
";

    #[test]
    fn converts_pleco_pinyin() {
        assert_eq!(pron_to_marks("ni3hao3"), "nǐ hǎo");
        assert_eq!(pron_to_marks("na3i5na3i5"), "nǎi nǎi");
        assert_eq!(pron_to_marks("dia3n5 (zhong1)"), "diǎn (zhōng)");
        assert_eq!(pron_to_marks("yi4dia3n5r5"), "yì diǎnr");
        assert_eq!(pron_to_marks("na4r5"), "nàr");
        assert_eq!(pron_to_marks("xi1an1"), "xī ān");
        assert_eq!(pron_to_marks("Mei3guo2"), "Měi guó");
        assert_eq!(pron_to_marks("lu:4"), "lǜ");
        assert_eq!(pron_to_marks("wai4 gong1"), "wài gōng");
        assert_eq!(pron_to_marks("nǐ hǎo"), "nǐ hǎo");
    }

    #[test]
    fn parses_cards_and_categories() {
        let cards = parse(SAMPLE).unwrap();
        assert_eq!(cards.len(), 7);
        assert_eq!(cards[0].defn, "I；me");
        assert_eq!(cards[3].defn, "there & then");
        assert_eq!(cards[3].categories, ["Class/Class 9", "HSK1-1"]);
        assert!(cards[4].categories.is_empty());
        assert_eq!(cards[1].traditional, None, "same as simplified");
        assert!(parse("<notpleco/>").is_err());
    }

    #[test]
    fn imports_with_dictionary_corrections() {
        let mut conn = crate::db::open_in_memory().unwrap();
        dictionary::import(&mut conn, DICT).unwrap();
        let cards = parse(SAMPLE).unwrap();

        let p = preview(&conn, &cards, None).unwrap();
        assert_eq!(p.cards, 7);
        assert_eq!(p.uncategorized, 1);
        let names: Vec<&str> = p.categories.iter().map(|c| c.deck.as_str()).collect();
        assert_eq!(names, ["HSK1-1", "Numbers", "Class › Class 9"]);
        let dup_words: Vec<&str> = p.duplicates.other_decks.iter().map(|w| w.hanzi.as_str()).collect();
        assert_eq!(dup_words, ["我", "那儿"], "我 is in the seed deck; 那儿 is in two categories");

        // Without other-deck duplicates: 我 is skipped, 那儿 goes into its first category only.
        let mut skip_conn = crate::db::open_in_memory().unwrap();
        dictionary::import(&mut skip_conn, DICT).unwrap();
        let opts = ImportOptions { default_deck: None, use_dictionary: true, include_other_decks: false };
        let r = import(&mut skip_conn, &cards, &opts).unwrap();
        assert_eq!((r.imported, r.skipped_other), (6, 2));

        let opts = ImportOptions { default_deck: None, use_dictionary: true, include_other_decks: true };
        let r = import(&mut conn, &cards, &opts).unwrap();
        assert_eq!(r.imported, 8, "那儿 goes into two decks");
        assert_eq!(r.decks_created, 5, "HSK1-1, Numbers, Class, Class › Class 9, and 'Pleco'");
        let nested: Option<i64> = crate::decks::find_path(&conn, None, &decks::split_path("Class/Class 9")).unwrap();
        assert!(nested.is_some(), "categories become nested decks");
        assert_eq!(r.definitions_added, 1, "学生 had no definition");

        let card = |deck: &str, hanzi: &str| -> (String, String) {
            conn.query_row(
                "SELECT c.pinyin, c.meaning FROM cards c JOIN decks d ON d.id = c.deck_id WHERE d.name = ?1 AND c.hanzi = ?2",
                [deck, hanzi],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap()
        };
        assert_eq!(card("HSK1-1", "我"), ("wǒ".into(), "I; me".into()));
        assert_eq!(card("Class 9", "奶奶").0, "nǎi nai");
        assert_eq!(card("Class 9", "那儿").0, "nàr", "the alternative that matches the word wins");
        assert_eq!(card("Pleco", "差").0, "chà", "Pleco's reading picks between chā and chà");
        assert_eq!(card("HSK1-1", "学生"), ("xué sheng".into(), "student; schoolchild".into()));
        assert_eq!(card("Class 9", "你怎么样").0, "nǐ zěn me yàng", "a Pleco slip doesn't split 怎么样");

        // Importing again adds nothing.
        let again = import(&mut conn, &cards, &opts).unwrap();
        assert_eq!((again.imported, again.skipped, again.decks_created), (0, 8, 0));
        assert_eq!((again.pinyin_corrected, again.definitions_added), (0, 0), "nothing added, nothing corrected");
    }

    #[test]
    fn imports_without_dictionary() {
        let mut conn = crate::db::open_in_memory().unwrap();
        let cards = parse(SAMPLE).unwrap();
        let opts = ImportOptions { default_deck: Some(1), use_dictionary: true, include_other_decks: true };
        let r = import(&mut conn, &cards, &opts).unwrap();
        assert_eq!(r.pinyin_corrected, 0);
        let pinyin: String = conn
            .query_row("SELECT pinyin FROM cards WHERE hanzi = '奶奶'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(pinyin, "nǎi nǎi", "Pleco's own tones, tidied");
        let deck: String = conn
            .query_row("SELECT d.name FROM cards c JOIN decks d ON d.id = c.deck_id WHERE c.hanzi = '差'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(deck, "Getting Started", "uncategorised cards go to the chosen deck");
    }

    #[test]
    fn exports_and_reimports() {
        let mut conn = crate::db::open_in_memory().unwrap();
        dictionary::import(&mut conn, DICT).unwrap();
        let top = decks::create_deck(&conn, "Family & friends", "", None).unwrap();
        let deck = decks::create_deck(&conn, "Lesson 1", "", Some(top)).unwrap();
        for (h, p, m) in [("奶奶", "nǎi nai", "grandma <dad's mom>"), ("那儿", "nàr", "there"), ("学生", "xué sheng", "student")] {
            conn.execute(
                "INSERT INTO cards (deck_id, hanzi, pinyin, meaning, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 1, 2)",
                params![deck, h, p, m],
            )
            .unwrap();
        }
        let (xml, n) = export_xml(&conn, Some(top)).unwrap();
        assert_eq!(n, 3, "a deck exports with its subdecks");
        assert!(xml.contains("<category name=\"Family &amp; friends/Lesson 1\"/>"));
        assert!(xml.contains("<pron type=\"hypy\" tones=\"numbers\">nai3 nai5</pron>"));
        assert!(xml.contains("<pron type=\"hypy\" tones=\"numbers\">na4r5</pron>"));
        assert!(xml.contains("<headword charset=\"tc\">那兒</headword>"));

        let back = parse(&xml).unwrap();
        assert_eq!(back.len(), 3);
        assert_eq!(back[0].defn, "grandma <dad's mom>");
        assert_eq!(pron_to_marks(&back[0].pron), "nǎi nai");
        assert_eq!(pron_to_marks(&back[1].pron), "nàr");
        assert_eq!(back[1].traditional.as_deref(), Some("那兒"));
    }

    /// Dry run on a real export, in memory:
    /// PLECO_FILE=flash.xml [PLECO_DICT_DB=<a copy of dictionary.db>] cargo test -- --ignored real_file --nocapture
    #[test]
    #[ignore]
    fn real_file() {
        let Ok(path) = std::env::var("PLECO_FILE") else { return };
        let cards = read_file(Path::new(&path)).unwrap();
        let mut conn = crate::db::open_in_memory().unwrap();
        if let Ok(db) = std::env::var("PLECO_DICT_DB") {
            conn.execute("ATTACH DATABASE ?1 AS src", [format!("file:{db}?mode=ro")]).unwrap();
            conn.execute_batch("INSERT INTO dictionary SELECT * FROM src.dictionary; DETACH DATABASE src;").unwrap();
        }
        println!("{:?}", preview(&conn, &cards, None).unwrap().categories);
        let opts = ImportOptions { default_deck: None, use_dictionary: true, include_other_decks: false };
        println!("{:?}", preview(&conn, &cards, None).unwrap().duplicates);
        let r = import(&mut conn, &cards, &opts).unwrap();
        println!("{r:?}");
        let mut stmt = conn
            .prepare("SELECT d.name, c.hanzi, c.pinyin, c.meaning FROM cards c JOIN decks d ON d.id = c.deck_id WHERE d.id > 1 ORDER BY c.id")
            .unwrap();
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?))).unwrap();
        for row in rows {
            let (deck, hanzi, pinyin, meaning) = row.unwrap();
            println!("ROW\t{deck}\t{hanzi}\t{pinyin}\t{meaning}");
        }
    }
}
