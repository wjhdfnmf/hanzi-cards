//! Two snapshots into one, keeping the latest of both.
//!
//! The rules, for what each device did since they last agreed:
//!
//! - **Decks, cards, collections, stories**: of two versions of the same thing, the one
//!   edited last.
//! - **Deleted things stay deleted**, unless they were edited after the deletion: then the
//!   edit wins and the thing is back. Deleting a deck or collection takes what was in it,
//!   except that something edited or added in it afterwards brings it (and the rest of what
//!   that device still had in it) back.
//! - **Answers**: all of them, from everywhere. An answer that was taken back stays taken back.
//! - **A word's schedule**: from wherever it was reviewed last, for every copy of the word.
//! - **Reading and tutor time**: each device's own total for a day, the larger figure of the two
//!   (a device only ever adds to its own).
//! - **Statistics that were reset** stay reset: answers from before the reset, and study
//!   time from days before it, are dropped wherever they come from.
//! - **Progress that was reset** stays reset: schedules from reviews before it are dropped.
//! - **The same thing made separately on two devices** (the starter deck, a file imported
//!   on both) becomes one thing: same place, same content, one id from then on.
//!
//! Merging is a plain function of its two inputs: no clock, no database. The same inputs give
//! the same output in either order, and merging the result again changes nothing, which is
//! what lets any number of devices sync in any order and end up identical.

use crate::snapshot::*;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// Ids known to be the same thing. The smallest id of a group stands for all of it.
#[derive(Default)]
struct Same {
    parent: BTreeMap<String, String>,
}

impl Same {
    fn find(&self, uid: &str) -> String {
        let mut at = uid;
        while let Some(p) = self.parent.get(at) {
            if p == at {
                break;
            }
            at = p;
        }
        at.to_string()
    }

    /// (each group's root is its smallest id: the larger root always goes under the smaller)
    fn union(&mut self, a: &str, b: &str) -> bool {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra == rb {
            return false;
        }
        let (small, big) = if ra < rb { (ra, rb) } else { (rb, ra) };
        self.parent.insert(big, small);
        true
    }

    fn pairs(&self) -> Vec<(String, String)> {
        self.parent.keys().map(|k| (k.clone(), self.find(k))).filter(|(a, b)| a != b).collect()
    }
}

/// Of two versions of a row, the one changed last; equal times are settled by content, so
/// that both sides settle them the same way.
fn later<T: Serialize>(a: T, b: T, at: impl Fn(&T) -> i64) -> T {
    let (ta, tb) = (at(&a), at(&b));
    if ta != tb {
        return if ta > tb { a } else { b };
    }
    let text = |x: &T| serde_json::to_string(x).unwrap_or_default();
    if text(&a) >= text(&b) {
        a
    } else {
        b
    }
}

fn put<T: Serialize>(map: &mut BTreeMap<String, T>, uid: String, row: T, at: impl Fn(&T) -> i64) {
    let row = match map.remove(&uid) {
        Some(old) => later(old, row, at),
        None => row,
    };
    map.insert(uid, row);
}

/// A deck or a collection: something with a name inside a parent.
trait Node {
    fn uid(&self) -> &str;
    fn parent(&self) -> Option<&str>;
    fn name(&self) -> &str;
}
impl Node for Deck {
    fn uid(&self) -> &str {
        &self.uid
    }
    fn parent(&self) -> Option<&str> {
        self.parent.as_deref()
    }
    fn name(&self) -> &str {
        &self.name
    }
}
impl Node for Folder {
    fn uid(&self) -> &str {
        &self.uid
    }
    fn parent(&self) -> Option<&str> {
        self.parent.as_deref()
    }
    fn name(&self) -> &str {
        &self.name
    }
}

/// Pair up things that only one side has each and that have the same key, in id order.
fn pair_up<K: Ord>(a: BTreeMap<String, K>, b: BTreeMap<String, K>, same: &mut Same) -> bool {
    fn group<'m, K: Ord>(mine: &'m BTreeMap<String, K>, theirs: &BTreeMap<String, K>) -> BTreeMap<&'m K, Vec<&'m String>> {
        let mut by_key: BTreeMap<&K, Vec<&String>> = BTreeMap::new();
        for (uid, key) in mine {
            if !theirs.contains_key(uid) {
                by_key.entry(key).or_default().push(uid);
            }
        }
        by_key
    }
    let (only_a, only_b) = (group(&a, &b), group(&b, &a));
    let mut pairs: Vec<(String, String)> = Vec::new();
    for (key, ours) in &only_a {
        if let Some(theirs) = only_b.get(key) {
            pairs.extend(ours.iter().zip(theirs.iter()).map(|(x, y)| ((*x).clone(), (*y).clone())));
        }
    }
    let mut changed = false;
    for (x, y) in pairs {
        changed |= same.union(&x, &y);
    }
    changed
}

/// Decks (or collections) made separately on both sides with the same name in the same
/// place are one. From the top down: joining two parents can make their children match.
fn unify_nodes<T: Node>(a: &[T], b: &[T], same: &mut Same) {
    loop {
        let keys = |rows: &[T], same: &Same| -> BTreeMap<String, (String, String)> {
            rows.iter().map(|r| (same.find(r.uid()), (r.parent().map(|p| same.find(p)).unwrap_or_default(), r.name().to_string()))).collect()
        };
        let (ka, kb) = (keys(a, same), keys(b, same));
        if !pair_up(ka, kb, same) {
            break;
        }
    }
}

fn unify_items<T, K: Ord>(a: &[T], b: &[T], same: &mut Same, uid: impl Fn(&T) -> &str, key: impl Fn(&T, &Same) -> K) {
    let keys = |rows: &[T], same: &Same| -> BTreeMap<String, K> { rows.iter().map(|r| (same.find(uid(r)), key(r, same))).collect() };
    let (ka, kb) = (keys(a, same), keys(b, same));
    pair_up(ka, kb, same);
}

/// Parents that aren't there become "top level", and a loop of parents (two devices each
/// moved one deck into the other) is opened at its smallest id.
fn fix_parents(parents: &mut BTreeMap<String, Option<String>>) {
    let known: BTreeSet<String> = parents.keys().cloned().collect();
    for p in parents.values_mut() {
        if p.as_ref().is_some_and(|x| !known.contains(x)) {
            *p = None;
        }
    }
    loop {
        let mut cut: Option<String> = None;
        'search: for start in parents.keys() {
            let mut path = vec![start.clone()];
            let mut at = start;
            while let Some(Some(p)) = parents.get(at) {
                if let Some(i) = path.iter().position(|x| x == p) {
                    cut = path[i..].iter().min().cloned();
                    break 'search;
                }
                path.push(p.clone());
                at = p;
            }
        }
        match cut {
            Some(c) => parents.insert(c, None),
            None => break,
        };
    }
}

/// Which containers (decks, collections) and which of the things in them are still there.
struct Tree<'a> {
    parents: &'a BTreeMap<String, Option<String>>,
    /// When each container was last changed, and when it was deleted (if it was).
    changed: BTreeMap<String, i64>,
    deleted: &'a BTreeMap<String, i64>,
}

impl Tree<'_> {
    fn deleted_above(&self, start: Option<&String>, dead: &BTreeSet<String>) -> Vec<String> {
        let mut out = Vec::new();
        let mut at = start;
        while let Some(u) = at {
            if dead.contains(u) {
                out.push(u.clone());
            }
            at = self.parents.get(u).and_then(|p| p.as_ref());
        }
        out
    }

    /// `items`: id -> (its container, when it was last changed), for the ones not deleted
    /// themselves. Returns the containers that are alive.
    fn alive(&self, items: &BTreeMap<String, (Option<String>, i64)>) -> BTreeSet<String> {
        // deleted, and not changed since
        let mut dead: BTreeSet<String> = self.changed.iter().filter(|(u, t)| self.deleted.get(*u).is_some_and(|d| d >= *t)).map(|(u, _)| u.clone()).collect();
        // ...unless something in it (at any depth) was changed after every deletion above it
        loop {
            let mut back: Vec<String> = Vec::new();
            let mut consider = |start: Option<&String>, changed: i64| {
                let above = self.deleted_above(start, &dead);
                if !above.is_empty() && above.iter().all(|c| self.deleted[c] < changed) {
                    back.extend(above);
                }
            };
            for (u, t) in &self.changed {
                if !dead.contains(u) {
                    consider(self.parents[u].as_ref(), *t);
                }
            }
            for (container, t) in items.values() {
                consider(container.as_ref(), *t);
            }
            if back.is_empty() {
                break;
            }
            for u in back {
                dead.remove(&u);
            }
        }
        self.changed.keys().filter(|u| !dead.contains(*u) && self.deleted_above(self.parents[*u].as_ref(), &dead).is_empty()).cloned().collect()
    }
}

/// One snapshot with every id standing for itself and nothing deleted left in it:
/// `merge(s, nothing)`. What a device holds after a sync is always in this form.
pub fn tidy(s: &Snapshot) -> Snapshot {
    merge(s, &Snapshot::default())
}

pub fn merge(a: &Snapshot, b: &Snapshot) -> Snapshot {
    // ---- which ids are the same thing ----
    let mut same = Same::default();
    for (old, new) in a.aliases.iter().chain(&b.aliases) {
        same.union(old, new);
    }
    unify_nodes(&a.decks, &b.decks, &mut same);
    unify_nodes(&a.folders, &b.folders, &mut same);
    unify_items(&a.cards, &b.cards, &mut same, |c| &c.uid, |c, same| (same.find(&c.deck), c.hanzi.clone(), c.pinyin.clone(), c.meaning.clone()));
    unify_items(&a.stories, &b.stories, &mut same, |s| &s.uid, |s, same| {
        (s.folder.as_ref().map(|f| same.find(f)).unwrap_or_default(), s.title.clone(), s.body.clone())
    });
    // The same text in the same collection is one story, whatever it is called (a story that
    // was given a new title in a copy of the library, and so a new id, is still that story).
    // Unlike the pairing above, this also joins two that one device already holds. A story
    // that was deleted is left out of it: deleting one copy must not take the other.
    {
        let deleted: BTreeSet<String> = a.deleted.iter().chain(&b.deleted).filter(|t| t.kind == "story").map(|t| same.find(&t.uid)).collect();
        let mut first: BTreeMap<(String, &str), String> = BTreeMap::new();
        for s in a.stories.iter().chain(&b.stories) {
            let uid = same.find(&s.uid);
            if s.body.trim().is_empty() || deleted.contains(&uid) {
                continue;
            }
            let key = (s.folder.as_ref().map(|f| same.find(f)).unwrap_or_default(), s.body.as_str());
            match first.get(&key) {
                Some(other) => {
                    let other = other.clone();
                    same.union(&other, &uid);
                }
                None => {
                    first.insert(key, uid);
                }
            }
        }
    }
    let id = |uid: &str| same.find(uid);

    // ---- deletions, the latest of each ----
    let mut deleted: BTreeMap<(String, String), i64> = BTreeMap::new();
    for t in a.deleted.iter().chain(&b.deleted) {
        let at = deleted.entry((t.kind.clone(), id(&t.uid))).or_insert(t.at);
        *at = (*at).max(t.at);
    }
    let deleted_of = |kind: &str| -> BTreeMap<String, i64> { deleted.iter().filter(|((k, _), _)| k == kind).map(|((_, u), at)| (u.clone(), *at)).collect() };
    let (deck_gone, card_gone, folder_gone, story_gone, log_gone) = (deleted_of("deck"), deleted_of("card"), deleted_of("folder"), deleted_of("story"), deleted_of("log"));

    // ---- rows: the version changed last ----
    let mut decks: BTreeMap<String, Deck> = BTreeMap::new();
    for d in a.decks.iter().chain(&b.decks) {
        let d = Deck { uid: id(&d.uid), parent: d.parent.as_deref().map(id), ..d.clone() };
        put(&mut decks, d.uid.clone(), d, |d| d.updated_at);
    }
    let mut cards: BTreeMap<String, Card> = BTreeMap::new();
    for c in a.cards.iter().chain(&b.cards) {
        let c = Card { uid: id(&c.uid), deck: id(&c.deck), ..c.clone() };
        put(&mut cards, c.uid.clone(), c, |c| c.updated_at);
    }
    let mut folders: BTreeMap<String, Folder> = BTreeMap::new();
    for f in a.folders.iter().chain(&b.folders) {
        let f = Folder { uid: id(&f.uid), parent: f.parent.as_deref().map(id), ..f.clone() };
        put(&mut folders, f.uid.clone(), f, |f| f.updated_at);
    }
    let mut stories: BTreeMap<String, Story> = BTreeMap::new();
    for s in a.stories.iter().chain(&b.stories) {
        let s = Story { uid: id(&s.uid), folder: s.folder.as_deref().map(id), ..s.clone() };
        put(&mut stories, s.uid.clone(), s, |s| s.updated_at);
    }

    // ---- decks and cards: what is still there ----
    let mut parents: BTreeMap<String, Option<String>> = decks.iter().map(|(u, d)| (u.clone(), d.parent.clone())).collect();
    fix_parents(&mut parents);
    for (u, d) in decks.iter_mut() {
        d.parent = parents[u].clone();
    }
    let kept = |gone: &BTreeMap<String, i64>, uid: &str, changed: i64| gone.get(uid).is_none_or(|at| *at < changed);
    cards.retain(|u, c| kept(&card_gone, u, c.updated_at) && decks.contains_key(&c.deck));
    let alive = Tree { parents: &parents, changed: decks.iter().map(|(u, d)| (u.clone(), d.updated_at)).collect(), deleted: &deck_gone }
        .alive(&cards.iter().map(|(u, c)| (u.clone(), (Some(c.deck.clone()), c.updated_at))).collect());
    decks.retain(|u, _| alive.contains(u));
    cards.retain(|_, c| alive.contains(&c.deck));

    // ---- collections and stories ----
    let mut parents: BTreeMap<String, Option<String>> = folders.iter().map(|(u, f)| (u.clone(), f.parent.clone())).collect();
    fix_parents(&mut parents);
    for (u, f) in folders.iter_mut() {
        f.parent = parents[u].clone();
    }
    stories.retain(|u, s| kept(&story_gone, u, s.updated_at));
    for s in stories.values_mut() {
        // (a story needn't be in a collection)
        if s.folder.as_ref().is_some_and(|f| !folders.contains_key(f)) {
            s.folder = None;
        }
    }
    let alive = Tree { parents: &parents, changed: folders.iter().map(|(u, f)| (u.clone(), f.updated_at)).collect(), deleted: &folder_gone }
        .alive(&stories.iter().map(|(u, s)| (u.clone(), (s.folder.clone(), s.updated_at))).collect());
    folders.retain(|u, _| alive.contains(u));
    stories.retain(|_, s| s.folder.as_ref().is_none_or(|f| alive.contains(f)));

    // a deletion is remembered for as long as the thing isn't there
    deleted.retain(|(kind, uid), _| match kind.as_str() {
        "deck" => !decks.contains_key(uid),
        "card" => !cards.contains_key(uid),
        "folder" => !folders.contains_key(uid),
        "story" => !stories.contains_key(uid),
        _ => true,
    });

    // ---- the statistics were started over: nothing from before that comes back ----
    let reset = deleted.get(&("reset".to_string(), "statistics".to_string())).copied();
    let progress_reset = deleted.get(&("reset".to_string(), "progress".to_string())).copied();

    // ---- answers: all of them, except the ones taken back ----
    let mut log: BTreeMap<String, Review> = BTreeMap::new();
    for r in a.log.iter().chain(&b.log) {
        let r = Review { uid: id(&r.uid), card: id(&r.card), ..r.clone() };
        if cards.contains_key(&r.card) && !log_gone.contains_key(&r.uid) && reset.is_none_or(|at| r.reviewed_at >= at) {
            put(&mut log, r.uid.clone(), r, |r| r.reviewed_at);
        }
    }

    // ---- schedules: from the last review, for every copy of the word ----
    let newer = |x: Progress, y: Progress| {
        // (compared without the card, so copies of a word compare by what they hold)
        let bare = |p: &Progress| serde_json::to_string(&Progress { card: String::new(), ..p.clone() }).unwrap_or_default();
        if (x.last_review, x.reps, bare(&x)) >= (y.last_review, y.reps, bare(&y)) {
            x
        } else {
            y
        }
    };
    let mut by_word: BTreeMap<(String, String), Progress> = BTreeMap::new();
    for p in a.progress.iter().chain(&b.progress) {
        let Some(card) = cards.get(&id(&p.card)) else { continue };
        if progress_reset.is_some_and(|at| p.last_review < at) {
            continue;
        }
        let key = (card.hanzi.clone(), p.direction.clone());
        let p = match by_word.remove(&key) {
            Some(old) => newer(old, p.clone()),
            None => p.clone(),
        };
        by_word.insert(key, p);
    }
    let mut progress: Vec<Progress> = Vec::new();
    for c in cards.values() {
        for ((_, _), p) in by_word.range((c.hanzi.clone(), String::new())..).take_while(|((h, _), _)| *h == c.hanzi) {
            progress.push(Progress { card: c.uid.clone(), ..p.clone() });
        }
    }
    progress.sort_by(|x, y| (&x.card, &x.direction).cmp(&(&y.card, &y.direction)));

    // ---- reading and tutor time: each device's own total for a day ----
    let mut time: BTreeMap<(i64, String, String), i64> = BTreeMap::new();
    // (days are counted from 1970; the day of the reset itself is kept: time since counts)
    for t in a.time.iter().chain(&b.time).filter(|t| reset.is_none_or(|at| t.day >= at / 86_400)) {
        let s = time.entry((t.day, t.kind.clone(), t.device.clone())).or_insert(0);
        *s = (*s).max(t.seconds);
    }

    Snapshot {
        format: a.format.max(b.format),
        cards_schema: a.cards_schema.max(b.cards_schema),
        stories_schema: a.stories_schema.max(b.stories_schema),
        decks: decks.into_values().collect(),
        cards: cards.into_values().collect(),
        progress,
        log: log.into_values().collect(),
        time: time.into_iter().map(|((day, kind, device), seconds)| Time { day, kind, device, seconds }).collect(),
        folders: folders.into_values().collect(),
        stories: stories.into_values().collect(),
        deleted: deleted.into_iter().map(|((kind, uid), at)| Tomb { kind, uid, at }).collect(),
        aliases: same.pairs(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deck(uid: &str, parent: Option<&str>, name: &str, at: i64) -> Deck {
        Deck { uid: uid.into(), parent: parent.map(String::from), name: name.into(), created_at: at, updated_at: at, ..Default::default() }
    }
    fn card(uid: &str, deck: &str, hanzi: &str, meaning: &str, at: i64) -> Card {
        Card { uid: uid.into(), deck: deck.into(), hanzi: hanzi.into(), meaning: meaning.into(), created_at: at, updated_at: at, ..Default::default() }
    }
    fn tomb(kind: &str, uid: &str, at: i64) -> Tomb {
        Tomb { kind: kind.into(), uid: uid.into(), at }
    }
    fn prog(card: &str, dir: &str, last: i64, reps: i64) -> Progress {
        Progress { card: card.into(), direction: dir.into(), last_review: last, due: last + 100, reps, state: 2, ..Default::default() }
    }
    fn review(uid: &str, card: &str, at: i64) -> Review {
        Review { uid: uid.into(), card: card.into(), direction: "recognition".into(), rating: 3, reviewed_at: at, ..Default::default() }
    }
    fn base() -> Snapshot {
        Snapshot { format: 1, decks: vec![deck("d1", None, "HSK", 10)], cards: vec![card("c1", "d1", "你好", "hello", 10), card("c2", "d1", "水", "water", 10)], ..Default::default() }
    }
    fn both_ways(a: &Snapshot, b: &Snapshot) -> Snapshot {
        let m = merge(a, b);
        assert_eq!(m, merge(b, a), "the order of merging must not matter");
        assert_eq!(merge(&m, &m), m, "merging the result again must change nothing");
        assert_eq!(merge(&m, a), m, "nor merging either side into it again");
        assert_eq!(merge(&m, b), m);
        m
    }

    #[test]
    fn the_later_edit_wins() {
        let (mut a, mut b) = (base(), base());
        a.cards[0].meaning = "hi".into();
        a.cards[0].updated_at = 20;
        b.cards[0].meaning = "hello there".into();
        b.cards[0].updated_at = 30;
        b.cards[1].notes = "a note".into();
        b.cards[1].updated_at = 15;
        let m = both_ways(&a, &b);
        assert_eq!(m.cards[0].meaning, "hello there");
        assert_eq!(m.cards[1].notes, "a note");
    }

    #[test]
    fn new_things_from_both_sides_are_kept() {
        let (mut a, mut b) = (base(), base());
        a.cards.push(card("ca", "d1", "猫", "cat", 20));
        b.decks.push(deck("d2", Some("d1"), "Animals", 21));
        b.cards.push(card("cb", "d2", "狗", "dog", 22));
        let m = both_ways(&a, &b);
        assert_eq!(m.cards.len(), 4);
        assert_eq!(m.decks.len(), 2);
    }

    #[test]
    fn deleted_stays_deleted_unless_edited_afterwards() {
        let (mut a, mut b) = (base(), base());
        // a deletes both cards at 20; b edited one before that and the other after
        a.cards.clear();
        a.deleted = vec![tomb("card", "c1", 20), tomb("card", "c2", 20)];
        b.cards[0].updated_at = 15;
        b.cards[1].updated_at = 25;
        b.cards[1].meaning = "water (drink)".into();
        let m = both_ways(&a, &b);
        assert_eq!(m.cards.iter().map(|c| c.uid.as_str()).collect::<Vec<_>>(), ["c2"]);
        assert_eq!(m.deleted, vec![tomb("card", "c1", 20)], "the deletion that was overruled is forgotten");
        // a third device that never heard of the deletion doesn't bring the card back
        assert_eq!(both_ways(&m, &base()).cards.len(), 1);
    }

    #[test]
    fn a_deleted_deck_takes_its_cards_unless_one_was_changed_since() {
        let (mut a, b) = (base(), base());
        a.decks.clear();
        a.cards.clear();
        a.deleted = vec![tomb("deck", "d1", 20)]; // (the cards went with it: no deletions of their own)
        let m = both_ways(&a, &b);
        assert!(m.decks.is_empty() && m.cards.is_empty());
        assert_eq!(m.deleted.len(), 1);

        // b added a card to the deck after a deleted it: the deck is back, with what b had in it
        let mut b2 = base();
        b2.cards.push(card("c3", "d1", "茶", "tea", 30));
        let m = both_ways(&a, &b2);
        assert_eq!(m.decks.len(), 1);
        assert_eq!(m.cards.len(), 3);
        assert!(m.deleted.is_empty());
    }

    #[test]
    fn a_deck_inside_a_deleted_deck() {
        let mut a = base();
        a.decks.push(deck("d2", Some("d1"), "Sub", 11));
        a.cards.push(card("c3", "d2", "茶", "tea", 12));
        let mut b = a.clone();
        // b deletes the outer deck at 20
        b.decks.clear();
        b.cards.clear();
        b.deleted = vec![tomb("deck", "d1", 20), tomb("deck", "d2", 20)];
        assert!(both_ways(&a, &b).decks.is_empty());
        // a card deep inside was edited at 30: both decks come back
        a.cards[2].updated_at = 30;
        let m = both_ways(&a, &b);
        assert_eq!(m.decks.len(), 2);
        assert_eq!(m.cards.len(), 3);
    }

    #[test]
    fn answers_are_all_kept_and_the_last_review_sets_the_schedule() {
        let (mut a, mut b) = (base(), base());
        a.log = vec![review("r1", "c1", 100)];
        a.progress = vec![prog("c1", "recognition", 100, 1)];
        b.log = vec![review("r2", "c1", 200), review("r3", "c2", 150)];
        b.progress = vec![prog("c1", "recognition", 200, 1), prog("c2", "recognition", 150, 1)];
        let m = both_ways(&a, &b);
        assert_eq!(m.log.len(), 3);
        assert_eq!(m.progress.iter().find(|p| p.card == "c1").unwrap().last_review, 200);
        assert_eq!(m.progress.len(), 2);
    }

    #[test]
    fn an_answer_taken_back_stays_taken_back() {
        let (mut a, mut b) = (base(), base());
        a.log = vec![review("r1", "c1", 100)];
        b.deleted = vec![tomb("log", "r1", 101)];
        let m = both_ways(&a, &b);
        assert!(m.log.is_empty());
        assert_eq!(m.deleted.len(), 1);
    }

    #[test]
    fn progress_that_was_reset_stays_reset() {
        // a still has schedules from before b started every card over; b has studied c2 since
        let mut a = base();
        a.progress = vec![prog("c1", "recognition", 100, 3), prog("c2", "recognition", 120, 2)];
        let mut b = base();
        b.deleted = vec![tomb("reset", "progress", 200)];
        b.progress = vec![prog("c2", "recognition", 250, 1)];
        let m = both_ways(&a, &b);
        assert_eq!(m.progress.iter().map(|p| (p.card.as_str(), p.last_review)).collect::<Vec<_>>(), [("c2", 250)]);
        assert_eq!(m.deleted, b.deleted, "the reset is remembered");
    }

    #[test]
    fn statistics_that_were_reset_stay_reset() {
        let t = |day, seconds| Time { day, kind: "reading".into(), device: "A".into(), seconds };
        let mut a = base();
        a.log = vec![review("r1", "c1", 86_400 * 10), review("r2", "c1", 86_400 * 20)];
        a.progress = vec![prog("c1", "recognition", 86_400 * 20, 2)];
        a.time = vec![t(10, 600), t(20, 300)];
        // b started its statistics over on day 15, and has studied since
        let mut b = base();
        b.deleted = vec![tomb("reset", "statistics", 86_400 * 15 + 100)];
        b.log = vec![review("r3", "c2", 86_400 * 16)];
        b.time = vec![t(16, 60)];
        let m = both_ways(&a, &b);
        assert_eq!(m.log.iter().map(|r| r.uid.as_str()).collect::<Vec<_>>(), ["r2", "r3"], "answers from before the reset are gone");
        assert_eq!(m.time, vec![t(16, 60), t(20, 300)]);
        assert_eq!(m.progress.len(), 1, "what the scheduler remembers stays");
        assert_eq!(m.deleted, b.deleted, "the reset is remembered");
        // a third device with the old history doesn't bring it back
        assert_eq!(both_ways(&m, &a), m);
    }

    #[test]
    fn copies_of_a_word_share_the_latest_schedule() {
        let mut a = base();
        a.decks.push(deck("d2", None, "Other", 11));
        a.cards.push(card("c9", "d2", "你好", "hello (copy)", 12));
        let mut b = base();
        b.progress = vec![prog("c1", "recognition", 300, 4)];
        b.log = vec![review("r1", "c1", 300)];
        let m = both_ways(&a, &b);
        let of = |card: &str| m.progress.iter().find(|p| p.card == card).map(|p| p.last_review);
        assert_eq!(of("c1"), Some(300));
        assert_eq!(of("c9"), Some(300), "the copy in the other deck takes it up");
        assert_eq!(of("c2"), None);
    }

    #[test]
    fn reading_time_is_each_devices_own_total() {
        let t = |device: &str, seconds| Time { day: 20000, kind: "reading".into(), device: device.into(), seconds };
        let (mut a, mut b) = (base(), base());
        a.time = vec![t("A", 600), t("B", 100)];
        b.time = vec![t("B", 250)];
        let m = both_ways(&a, &b);
        assert_eq!(m.time, vec![t("A", 600), t("B", 250)]);
    }

    #[test]
    fn the_same_thing_made_on_two_devices_becomes_one() {
        // two installs each made the starter deck, with ids of their own
        let a = Snapshot {
            decks: vec![deck("da", None, "Getting Started", 10)],
            cards: vec![card("ca1", "da", "你好", "hello", 10), card("ca2", "da", "水", "water", 10)],
            progress: vec![prog("ca1", "recognition", 50, 1)],
            log: vec![review("ra", "ca1", 50)],
            ..Default::default()
        };
        let b = Snapshot {
            decks: vec![deck("db", None, "Getting Started", 12)],
            cards: vec![card("cb1", "db", "你好", "hello", 12), card("cb2", "db", "水", "water", 12), card("cb3", "db", "茶", "tea", 13)],
            log: vec![review("rb", "cb2", 60)],
            progress: vec![prog("cb2", "recognition", 60, 1)],
            ..Default::default()
        };
        let m = both_ways(&a, &b);
        assert_eq!(m.decks.len(), 1);
        assert_eq!(m.cards.len(), 3);
        assert_eq!(m.log.len(), 2);
        assert!(m.log.iter().all(|r| m.cards.iter().any(|c| c.uid == r.card)));
        assert_eq!(m.progress.len(), 2);
        assert_eq!(m.aliases.len(), 3);
        // a device still holding the ids that were given up, with an edit made since, lands
        // on the one card, not a second copy
        let mut late = b.clone();
        late.cards[0].meaning = "hello!".into();
        late.cards[0].updated_at = 99;
        let m2 = both_ways(&m, &late);
        assert_eq!(m2.cards.len(), 3);
        assert_eq!(m2.cards.iter().find(|c| c.hanzi == "你好").unwrap().meaning, "hello!");
    }

    /// A library whose stories were given new titles in a copy (and so new ids), synced
    /// with a box that still has them under the old titles: one of each, with the new title.
    #[test]
    fn a_retitled_copy_of_a_story_is_the_same_story() {
        let story = |uid: &str, folder: Option<&str>, title: &str, body: &str, at: i64| Story {
            uid: uid.into(),
            folder: folder.map(String::from),
            title: title.into(),
            body: body.into(),
            created_at: 10,
            updated_at: at,
            ..Default::default()
        };
        let folders = vec![Folder { uid: "f1".into(), name: "Beginner".into(), created_at: 1, updated_at: 1, ..Default::default() }];
        let old = Snapshot {
            folders: folders.clone(),
            stories: vec![story("o1", Some("f1"), "Fable: 狼来了 – The Boy Who Cried Wolf", "从前，有一个小男孩……", 10), story("o2", None, "Song: 橄榄树", "不要问我从哪里来", 10), story("o3", Some("f1"), "Only here", "别的故事", 10)],
            ..Default::default()
        };
        let new = Snapshot {
            folders,
            stories: vec![story("n1", Some("f1"), "狼来了", "从前，有一个小男孩……", 50), story("n2", None, "橄榄树", "不要问我从哪里来", 50), story("n9", Some("f1"), "橄榄树", "不要问我从哪里来", 50)],
            ..Default::default()
        };
        let m = both_ways(&old, &new);
        let titles: Vec<&str> = m.stories.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(m.stories.len(), 4, "{titles:?}");
        assert!(titles.contains(&"狼来了") && !titles.iter().any(|t| t.starts_with("Fable")), "{titles:?}");
        assert_eq!(titles.iter().filter(|t| **t == "橄榄树").count(), 2, "the same text in two collections stays two stories");
        assert!(titles.contains(&"Only here"));
        // a device that already took both sets in (each story twice) is put right by its next sync
        let both = Snapshot { folders: old.folders.clone(), stories: old.stories.iter().chain(&new.stories).cloned().collect(), ..Default::default() };
        assert_eq!(tidy(&both).stories.len(), 4);
        assert_eq!(both_ways(&both, &m).stories, m.stories);
        // deleting the copy with the old title by hand doesn't take the other with it
        let mut by_hand = both.clone();
        by_hand.stories.retain(|s| s.uid != "o1");
        by_hand.deleted = vec![tomb("story", "o1", 99)];
        let m2 = both_ways(&by_hand, &both);
        assert!(m2.stories.iter().any(|s| s.title == "狼来了"), "the retitled story is still there");
        assert!(!m2.stories.iter().any(|s| s.title.starts_with("Fable")));
    }

    #[test]
    fn two_decks_moved_into_each_other_dont_make_a_loop() {
        let mut a = Snapshot { decks: vec![deck("d1", None, "One", 10), deck("d2", None, "Two", 10)], ..Default::default() };
        let mut b = a.clone();
        a.decks[0].parent = Some("d2".into());
        a.decks[0].updated_at = 20;
        b.decks[1].parent = Some("d1".into());
        b.decks[1].updated_at = 21;
        let m = both_ways(&a, &b);
        assert_eq!(m.decks.len(), 2);
        assert_eq!(m.decks.iter().filter(|d| d.parent.is_none()).count(), 1);
    }

    #[test]
    fn stories_and_collections() {
        let story = |uid: &str, folder: Option<&str>, title: &str, at: i64| Story {
            uid: uid.into(),
            folder: folder.map(String::from),
            title: title.into(),
            body: format!("从前……{title}"),
            created_at: at,
            updated_at: at,
            ..Default::default()
        };
        let folder = |uid: &str, name: &str, at: i64| Folder { uid: uid.into(), name: name.into(), created_at: at, updated_at: at, ..Default::default() };
        let a = Snapshot { folders: vec![folder("f1", "Level 1", 10)], stories: vec![story("s1", Some("f1"), "月亮", 10), story("s0", None, "Loose", 10)], ..Default::default() };
        let mut b = a.clone();
        b.stories[0].bookmarked = true;
        b.stories[0].updated_at = 20;
        b.stories.push(story("s2", Some("f1"), "小猫", 21));
        let mut c = a.clone();
        c.folders.clear();
        c.stories.retain(|s| s.uid == "s0");
        c.deleted = vec![tomb("folder", "f1", 15), tomb("story", "s1", 15)];
        let m = both_ways(&a, &b);
        assert!(m.stories.iter().find(|s| s.uid == "s1").unwrap().bookmarked);
        assert_eq!(m.stories.len(), 3);
        // c deleted the collection at 15; b's changes at 20 and 21 bring it back
        let m = both_ways(&m, &c);
        assert_eq!(m.folders.len(), 1);
        assert_eq!(m.stories.len(), 3);
        // against the untouched original, c's deletion stands (the loose story stays)
        let m = both_ways(&a, &c);
        assert!(m.folders.is_empty());
        assert_eq!(m.stories.iter().map(|s| s.uid.as_str()).collect::<Vec<_>>(), ["s0"]);
    }

    /// Three devices do random things and sync in a random order through one box. Once each
    /// has synced twice more, all three hold exactly what the box holds.
    #[test]
    fn devices_end_up_identical() {
        struct Rng(u64);
        impl Rng {
            fn next(&mut self, n: u64) -> u64 {
                self.0 ^= self.0 << 13;
                self.0 ^= self.0 >> 7;
                self.0 ^= self.0 << 17;
                self.0 % n
            }
        }
        for seed in 1..=60u64 {
            let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
            let start = tidy(&base());
            let mut devices = vec![start.clone(), start.clone(), start.clone()];
            let mut boxed = start;
            let mut clock = 100i64;
            let mut made = 0u32;
            for _ in 0..120 {
                clock += 1;
                let who = rng.next(3) as usize;
                let d = &mut devices[who];
                match rng.next(10) {
                    0 => {
                        made += 1;
                        let parent = (!d.decks.is_empty() && rng.next(2) == 0).then(|| d.decks[rng.next(d.decks.len() as u64) as usize].uid.clone());
                        // (sometimes the same name on two devices: they should become one deck)
                        let name = format!("Deck {}", rng.next(4));
                        d.decks.push(Deck { uid: format!("d{who}-{made}"), parent, name, created_at: clock, updated_at: clock, ..Default::default() });
                    }
                    1 | 2 if !d.decks.is_empty() => {
                        made += 1;
                        let deck = d.decks[rng.next(d.decks.len() as u64) as usize].uid.clone();
                        let hanzi = ["你好", "水", "茶", "猫", "狗"][rng.next(5) as usize];
                        d.cards.push(card(&format!("c{who}-{made}"), &deck, hanzi, "m", clock));
                    }
                    3 | 4 if !d.cards.is_empty() => {
                        let i = rng.next(d.cards.len() as u64) as usize;
                        d.cards[i].meaning = format!("edit {clock}");
                        d.cards[i].updated_at = clock;
                    }
                    5 if !d.cards.is_empty() => {
                        let i = rng.next(d.cards.len() as u64) as usize;
                        let c = d.cards.remove(i);
                        d.progress.retain(|p| p.card != c.uid);
                        d.log.retain(|r| r.card != c.uid);
                        d.deleted.push(tomb("card", &c.uid, clock));
                    }
                    6 if !d.decks.is_empty() => {
                        // a deck goes with everything in it, as the app deletes one
                        let i = rng.next(d.decks.len() as u64) as usize;
                        let mut gone = vec![d.decks[i].uid.clone()];
                        loop {
                            let more: Vec<String> = d.decks.iter().filter(|x| x.parent.as_ref().is_some_and(|p| gone.contains(p)) && !gone.contains(&x.uid)).map(|x| x.uid.clone()).collect();
                            if more.is_empty() {
                                break;
                            }
                            gone.extend(more);
                        }
                        let cards: Vec<String> = d.cards.iter().filter(|c| gone.contains(&c.deck)).map(|c| c.uid.clone()).collect();
                        d.decks.retain(|x| !gone.contains(&x.uid));
                        d.cards.retain(|c| !cards.contains(&c.uid));
                        d.progress.retain(|p| !cards.contains(&p.card));
                        d.log.retain(|r| !cards.contains(&r.card));
                        d.deleted.extend(gone.iter().map(|u| tomb("deck", u, clock)));
                    }
                    7 | 8 if !d.cards.is_empty() => {
                        made += 1;
                        let c = d.cards[rng.next(d.cards.len() as u64) as usize].clone();
                        d.log.push(review(&format!("r{who}-{made}"), &c.uid, clock));
                        // (the app gives every copy of the word the new schedule)
                        let copies: Vec<String> = d.cards.iter().filter(|x| x.hanzi == c.hanzi).map(|x| x.uid.clone()).collect();
                        d.progress.retain(|p| !(copies.contains(&p.card) && p.direction == "recognition"));
                        d.progress.extend(copies.iter().map(|u| prog(u, "recognition", clock, made as i64)));
                    }
                    _ => {
                        let m = merge(&devices[who], &boxed);
                        devices[who] = m.clone();
                        boxed = m;
                    }
                }
            }
            for _ in 0..2 {
                for d in devices.iter_mut() {
                    let m = merge(d, &boxed);
                    *d = m.clone();
                    boxed = m;
                }
            }
            for (i, d) in devices.iter().enumerate() {
                assert_eq!(*d, boxed, "seed {seed}: device {i} differs from the box");
            }
            // and what they hold makes sense
            for c in &boxed.cards {
                assert!(boxed.decks.iter().any(|d| d.uid == c.deck), "seed {seed}: a card without its deck");
            }
            for d in &boxed.decks {
                assert!(d.parent.as_ref().is_none_or(|p| boxed.decks.iter().any(|x| x.uid == *p)), "seed {seed}: a deck without its parent");
            }
            for r in &boxed.log {
                assert!(boxed.cards.iter().any(|c| c.uid == r.card), "seed {seed}: an answer without its card");
            }
            let mut ids: Vec<&String> = boxed.cards.iter().map(|c| &c.uid).collect();
            ids.dedup();
            assert_eq!(ids.len(), boxed.cards.len());
        }
    }
}
