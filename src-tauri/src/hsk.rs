//! HSK levels for tagging cards: "HSK1" … "HSK6", "HSK7-9". HSK 3.0 levels from the 2026
//! syllabus; a word that isn't in it takes its level from the 2021 HSK 3.0 lists.
//!
//! The word list (`data/hsk.tsv`) comes from github.com/drkameleon/complete-hsk-vocabulary
//! (MIT licence, reproduced at the top of the file).

use crate::error::AppResult;
use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::sync::OnceLock;

fn levels() -> &'static HashMap<&'static str, u8> {
    static MAP: OnceLock<HashMap<&'static str, u8>> = OnceLock::new();
    MAP.get_or_init(|| {
        include_str!("../data/hsk.tsv")
            .lines()
            .filter(|l| !l.starts_with('#'))
            .filter_map(|l| {
                // word, 2026 level, 2021 level (either may be empty)
                let mut f = l.split('\t');
                let w = f.next()?;
                let y2026: Option<u8> = f.next().and_then(|v| v.trim().parse().ok());
                let y2021: Option<u8> = f.next().and_then(|v| v.trim().parse().ok());
                Some((w, y2026.or(y2021)?))
            })
            .collect()
    })
}

/// The HSK tag for a card's characters. A word on the list gets its level; a combination
/// that isn't (你呢, 烤鸭, 您贵姓) is split into list words and gets the highest of their
/// levels; if part of it isn't on any list, there's no tag. Answer variants ("你好 / 您好",
/// optional parts in brackets) and an added 儿 are tried too; the lowest level wins.
pub fn tag_for(hanzi: &str) -> Option<String> {
    let mut forms = crate::speech::answer_forms(hanzi);
    forms.push(hanzi.trim().to_string());
    let extra: Vec<String> = forms.iter().filter_map(|f| f.strip_suffix('儿').map(String::from)).collect();
    forms.extend(extra);
    let level = forms.iter().filter_map(|f| level_of(f)).min()?;
    Some(if level >= 7 { "HSK7-9".to_string() } else { format!("HSK{level}") })
}

/// Level of a word, or of a combination as the highest level of its parts. The parts are
/// list words covering every character, as few as possible (longer words first), then the
/// split with the lowest top level. Punctuation and spaces are ignored.
fn level_of(text: &str) -> Option<u8> {
    let map = levels();
    if let Some(&lv) = map.get(text) {
        return Some(lv);
    }
    let cs: Vec<char> = text.chars().filter(|c| crate::pinyin::is_hanzi(*c)).collect();
    if cs.is_empty() {
        return None;
    }
    // best[i] = (parts, top level) for the first i characters
    let mut best: Vec<Option<(usize, u8)>> = vec![None; cs.len() + 1];
    best[0] = Some((0, 0));
    for end in 1..=cs.len() {
        for start in end.saturating_sub(8)..end {
            let Some((parts, top)) = best[start] else { continue };
            let word: String = cs[start..end].iter().collect();
            let Some(&lv) = map.get(word.as_str()) else { continue };
            let cand = (parts + 1, top.max(lv));
            if best[end].is_none_or(|b| cand < b) {
                best[end] = Some(cand);
            }
        }
    }
    best[cs.len()].map(|(_, top)| top)
}

/// Whether new cards get their HSK tag (Settings; on unless turned off).
pub fn enabled(conn: &Connection) -> bool {
    crate::db::get_setting(conn, "hsk_autotag").ok().flatten().as_deref() != Some("false")
}

/// `tags` (comma-separated) with the HSK tag added, if there is one and it's missing.
pub fn with_tag(tags: &str, hanzi: &str) -> String {
    let Some(tag) = tag_for(hanzi) else { return tags.to_string() };
    let mut list = crate::cards::split_tags(tags);
    // one level per card: a card that already has one (ours or the learner's) keeps it
    if list.iter().any(|t| is_level_tag(t)) {
        return tags.to_string();
    }
    list.push(tag);
    list.sort();
    list.join(",")
}

/// "HSK1" … "HSK6", "HSK7-9" (any case).
pub fn is_level_tag(tag: &str) -> bool {
    let t = tag.trim().to_ascii_uppercase();
    matches!(t.strip_prefix("HSK"), Some("1" | "2" | "3" | "4" | "5" | "6" | "7-9"))
}

/// For new cards: `with_tag` when auto-tagging is on.
pub fn tags_for_new(conn: &Connection, tags: &str, hanzi: &str) -> String {
    if enabled(conn) { with_tag(tags, hanzi) } else { tags.to_string() }
}

/// Add HSK tags to every existing card that lacks one. Returns how many were tagged.
pub fn tag_all(conn: &Connection) -> AppResult<usize> {
    let rows: Vec<(i64, String, String)> = {
        let mut stmt = conn.prepare("SELECT id, hanzi, tags FROM cards")?;
        let v = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<Result<_, _>>()?;
        v
    };
    let mut n = 0;
    for (id, hanzi, tags) in rows {
        let new = with_tag(&tags, &hanzi);
        if new != tags {
            conn.execute("UPDATE cards SET tags = ?2 WHERE id = ?1", params![id, new])?;
            n += 1;
        }
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_and_variants() {
        assert_eq!(tag_for("学生").as_deref(), Some("HSK1"));
        assert_eq!(tag_for("爱好").as_deref(), Some("HSK2"));
        assert_eq!(tag_for(" 一点儿 ").as_deref(), Some("HSK1"));
        // combinations: the highest level of the list words they're made of
        assert_eq!(tag_for("你好").as_deref(), Some("HSK1"), "你 + 好");
        assert_eq!(tag_for("你呢").as_deref(), Some("HSK1"));
        assert_eq!(tag_for("学生爱好").as_deref(), Some("HSK2"), "学生 (1) + 爱好 (2)");
        assert_eq!(tag_for("工作日学生").as_deref(), Some("HSK3"), "工作日 (3) + 学生 (1)");
        assert_eq!(tag_for("你好！").as_deref(), Some("HSK1"), "punctuation ignored");
        assert_eq!(tag_for("学生龘"), None, "龘 isn't on any list");
        // not in the 2026 syllabus: the 2021 level fills the gap
        assert!(tag_for("英语").is_some(), "英语 is in the 2021 lists");
        assert_eq!(tag_for("没问题").as_deref(), Some("HSK1"), "没 (2021: 1) + 问题 (1)");
        assert_eq!(with_tag("food", "学生"), "HSK1,food");
        assert_eq!(with_tag("HSK1", "学生"), "HSK1", "no duplicate");
        assert_eq!(with_tag("HSK2,food", "学生"), "HSK2,food", "one level per card");
        assert!(is_level_tag("hsk7-9") && !is_level_tag("HSK") && !is_level_tag("HSK10"));
        assert_eq!(with_tag("", "学生龘"), "");
    }

    #[test]
    fn tags_existing_and_new_cards() {
        let conn = crate::db::open_in_memory().unwrap();
        let deck: i64 = conn.query_row("SELECT id FROM decks LIMIT 1", [], |r| r.get(0)).unwrap();
        // existing cards: tag_all adds missing tags and keeps the learner's own
        conn.execute("INSERT INTO cards (deck_id, hanzi, tags, created_at, updated_at) VALUES (?1, '学生', 'class', 0, 0)", [deck]).unwrap();
        assert!(tag_all(&conn).unwrap() >= 1);
        let tags: String = conn.query_row("SELECT tags FROM cards WHERE hanzi = '学生'", [], |r| r.get(0)).unwrap();
        assert_eq!(tags, "HSK1,class");
        assert_eq!(tag_all(&conn).unwrap(), 0, "nothing left to tag");
        let input = crate::models::CardInput {
            deck_id: deck,
            hanzi: "爱好".into(),
            pinyin: "ài hào".into(),
            meaning: "hobby".into(),
            notes: String::new(),
            tags: vec![],
        };
        assert_eq!(crate::cards::create_card(&conn, &input).unwrap().tags, vec!["HSK2"]);
        crate::db::set_setting(&conn, "hsk_autotag", "false").unwrap();
        assert!(crate::cards::create_card(&conn, &CardInputExt::with(&input, "工作")).unwrap().tags.is_empty());
    }

    struct CardInputExt;
    impl CardInputExt {
        fn with(c: &crate::models::CardInput, hanzi: &str) -> crate::models::CardInput {
            crate::models::CardInput { hanzi: hanzi.into(), ..c.clone() }
        }
    }
}
