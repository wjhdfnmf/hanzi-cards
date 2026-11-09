use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Deck {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub created_at: i64,
    pub card_count: i64,
    pub due_count: i64,
    pub new_count: i64,
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
    /// Any of "recognition", "recall", "listening".
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DictStatus {
    pub entries: i64,
    pub imported_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub imported: i64,
    pub skipped: i64,
}
