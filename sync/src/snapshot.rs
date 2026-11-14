//! What travels between devices: every deck, card, review, story and so on, as plain rows
//! that name each other by permanent ids (`uid`) instead of a database's own row numbers.
//!
//! The format only ever grows: a newer app adds fields with defaults, so it can still read
//! what an older one wrote. An older app refuses what a newer one wrote (see `check`).

use crate::Error;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};

/// The format this build writes.
pub const FORMAT: u32 = 2;

/// The most a snapshot may unpack to: a guard against a small file that expands without end.
pub const MAX_UNPACKED: u64 = 512 * 1024 * 1024;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Snapshot {
    pub format: u32,
    /// The cards and reading databases' versions on the device that wrote this.
    pub cards_schema: i64,
    pub stories_schema: i64,
    pub decks: Vec<Deck>,
    pub cards: Vec<Card>,
    pub progress: Vec<Progress>,
    pub log: Vec<Review>,
    pub time: Vec<Time>,
    pub folders: Vec<Folder>,
    pub stories: Vec<Story>,
    /// Things that were deleted, and when: so that they stay deleted everywhere.
    pub deleted: Vec<Tomb>,
    /// Ids that turned out to be the same thing made on two devices: (the id given up, the id kept).
    pub aliases: Vec<(String, String)>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Deck {
    pub uid: String,
    pub parent: Option<String>,
    pub name: String,
    pub description: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Card {
    pub uid: String,
    pub deck: String,
    pub hanzi: String,
    pub pinyin: String,
    pub meaning: String,
    pub notes: String,
    pub tags: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// The scheduler's memory of one study direction of one card.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Progress {
    pub card: String,
    pub direction: String,
    pub state: i64,
    pub stability: f64,
    pub difficulty: f64,
    pub due: i64,
    pub last_review: i64,
    pub reps: i64,
    pub lapses: i64,
    pub step: i64,
}

/// One answer.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Review {
    pub uid: String,
    pub card: String,
    pub direction: String,
    pub rating: i64,
    pub state_before: i64,
    pub reviewed_at: i64,
    pub duration_ms: i64,
    pub scheduled_secs: i64,
}

/// Reading or tutor time on one day on one device.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Time {
    pub day: i64,
    pub kind: String,
    pub device: String,
    pub seconds: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Folder {
    pub uid: String,
    pub parent: Option<String>,
    pub name: String,
    pub description: String,
    pub position: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Story {
    pub uid: String,
    pub folder: Option<String>,
    pub title: String,
    pub body: String,
    pub vocab: String,
    pub position: i64,
    pub bookmarked: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

/// "deck" | "card" | "log" | "folder" | "story", or ("reset", "statistics"): the statistics were
/// started over at that time, and answers and study time from before it are dropped
/// (format 2).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Tomb {
    pub kind: String,
    pub uid: String,
    pub at: i64,
}

impl Snapshot {
    /// Can a device whose databases are at these versions take this snapshot in? Not when it
    /// was written by a newer app: that device must be updated first. (An older snapshot is
    /// fine: this app knows everything in it.)
    pub fn check(&self, cards_schema: i64, stories_schema: i64) -> Result<(), Error> {
        if self.format > FORMAT || self.cards_schema > cards_schema || self.stories_schema > stories_schema {
            return Err(Error::Newer);
        }
        Ok(())
    }

    /// As bytes: JSON, compressed.
    pub fn pack(&self) -> Result<Vec<u8>, Error> {
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        serde_json::to_writer(&mut gz, self).map_err(|e| Error::Corrupt(e.to_string()))?;
        gz.flush()?;
        Ok(gz.finish()?)
    }

    pub fn unpack(bytes: &[u8]) -> Result<Snapshot, Error> {
        let mut json = Vec::new();
        flate2::read::GzDecoder::new(bytes).take(MAX_UNPACKED + 1).read_to_end(&mut json).map_err(|e| Error::Corrupt(e.to_string()))?;
        if json.len() as u64 > MAX_UNPACKED {
            return Err(Error::TooLarge);
        }
        serde_json::from_slice(&json).map_err(|e| Error::Corrupt(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_and_unpacks() {
        let s = Snapshot {
            format: FORMAT,
            cards: vec![Card { uid: "a".into(), deck: "d".into(), hanzi: "你好".into(), ..Default::default() }],
            ..Default::default()
        };
        assert_eq!(Snapshot::unpack(&s.pack().unwrap()).unwrap(), s);
        assert!(matches!(Snapshot::unpack(b"not a snapshot"), Err(Error::Corrupt(_))));
    }

    #[test]
    fn reads_what_an_older_app_wrote_and_refuses_a_newer_one() {
        // fields this build knows nothing of are ignored, missing ones take their defaults
        let old: Snapshot = serde_json::from_str(r#"{"format":1,"cards":[{"uid":"a","hanzi":"x","later_field":3}]}"#).unwrap();
        assert_eq!(old.cards[0].hanzi, "x");
        assert!(old.check(13, 4).is_ok());
        let newer = Snapshot { format: FORMAT + 1, ..Default::default() };
        assert!(matches!(newer.check(13, 4), Err(Error::Newer)));
        let newer_db = Snapshot { format: FORMAT, cards_schema: 14, ..Default::default() };
        assert!(matches!(newer_db.check(13, 4), Err(Error::Newer)));
    }
}
