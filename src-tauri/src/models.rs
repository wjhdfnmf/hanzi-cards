use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Deck {
    pub id: i64,
    /// The deck this one is inside (`None` = top level).
    pub parent_id: Option<i64>,
    pub name: String,
    /// Names from the top, e.g. "Class › Class 9".
    pub path: String,
    /// 0 for top-level decks.
    pub depth: i64,
    pub child_count: i64,
    pub description: String,
    pub created_at: i64,
    /// Counts for cards directly in this deck.
    pub card_count: i64,
    pub due_count: i64,
    pub new_count: i64,
    /// Counts including all subdecks.
    pub total_cards: i64,
    pub total_due: i64,
    pub total_new: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Card {
    pub id: i64,
    pub deck_id: i64,
    pub hanzi: String,
    pub pinyin: String,
    pub meaning: String,
    pub notes: String,
    pub tags: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CardInput {
    pub deck_id: i64,
    pub hanzi: String,
    pub pinyin: String,
    pub meaning: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// A card plus a summary of its study progress across directions (for list views).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardRow {
    #[serde(flatten)]
    pub card: Card,
    /// "new" | "learning" | "review"
    pub status: String,
    pub next_due: Option<i64>,
    pub reps: i64,
    pub lapses: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub state: i64,
    pub stability: f64,
    pub difficulty: f64,
    pub due: i64,
    pub last_review: i64,
    pub reps: i64,
    pub lapses: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudyFilter {
    /// Empty = all decks.
    #[serde(default)]
    pub deck_ids: Vec<i64>,
    /// Empty = any tag. Otherwise a card must have at least one of these tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Any of "recognition", "recall", "listening", "speaking", "read_aloud", "say_meaning".
    pub directions: Vec<String>,
    /// "due" | "new" | "all" | "difficult"
    pub mode: String,
    /// 0 = no limit.
    #[serde(default)]
    pub limit: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudyItem {
    pub card: Card,
    pub direction: String,
    pub progress: Option<Progress>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub items: Vec<StudyItem>,
    pub due_count: i64,
    pub new_count: i64,
    pub new_remaining_today: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntervalPreview {
    pub secs: i64,
    /// Set when the outcome is a review interval measured in days.
    pub days: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerResult {
    pub due: i64,
    pub state: i64,
    pub interval_secs: i64,
    pub days: Option<i64>,
    /// This answer made the word a leech (forgotten many times): it was tagged "leech".
    pub leech: bool,
    /// A leech remembered at review several times in a row: its "leech" tag was removed.
    pub unleeched: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DictEntry {
    pub id: i64,
    pub traditional: String,
    pub simplified: String,
    pub pinyin: String,
    pub pinyin_num: String,
    pub definitions: Vec<String>,
    pub in_cards: bool,
}

/// A dictionary word offered while typing pinyin in the characters field.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub entry: DictEntry,
    /// How many characters of the typed pinyin this word uses up (from the start).
    pub consumed: usize,
    /// Ids of the dictionary entries it is made of (several for a composed phrase,
    /// whose `entry` then has id -1).
    pub parts: Vec<i64>,
}

/// A piece of a character string: a dictionary word, or text with no entry.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub text: String,
    pub entry: Option<DictEntry>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DictStatus {
    pub entries: i64,
    pub imported_at: Option<i64>,
}

/// What a Pleco file contains, shown before importing it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlecoPreview {
    pub cards: i64,
    pub categories: Vec<PlecoCategory>,
    /// Cards with no category.
    pub uncategorized: i64,
    /// Whether the dictionary is installed (for pinyin correction).
    pub dictionary: bool,
    /// Duplicates, if the defaults are used for cards without a category.
    pub duplicates: ImportPreview,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlecoCategory {
    /// Pleco's category path, e.g. "Class/Class 9".
    pub name: String,
    /// The deck it goes into, e.g. "Class 9".
    pub deck: String,
    /// The deck already exists (cards are added to it).
    pub exists: bool,
    pub cards: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlecoImportResult {
    pub imported: i64,
    /// Already in the deck, or not a Chinese headword.
    pub skipped: i64,
    /// Words already in other decks that the user chose not to import again.
    pub skipped_other: i64,
    pub decks: i64,
    pub decks_created: i64,
    pub pinyin_corrected: i64,
    pub definitions_added: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub imported: i64,
    /// Already in the same deck, or rows without Chinese characters.
    pub skipped: i64,
    /// Words already in other decks that the user chose not to import again.
    pub skipped_other: i64,
}

/// A word in an import file that already exists in other decks.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateWord {
    pub hanzi: String,
    /// Deck paths it is already in (or will be, earlier in the same file).
    pub decks: Vec<String>,
}

/// What importing a file would do, so the user can be asked about duplicates.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub rows: i64,
    /// Cards that would be added with no duplicates anywhere.
    pub fresh: i64,
    /// Already in the same deck: always skipped.
    pub same_deck: i64,
    /// Already in another deck: imported only if the user agrees.
    pub other_decks: Vec<DuplicateWord>,
    /// Rows without Chinese characters.
    pub invalid: i64,
}
