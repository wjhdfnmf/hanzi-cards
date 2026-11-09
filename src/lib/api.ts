// Typed wrappers around the Rust commands (src-tauri/src/commands.rs).
import { invoke } from "@tauri-apps/api/core";

export type Direction = "recognition" | "recall" | "listening";
export type StudyMode = "due" | "new" | "all" | "difficult";

export interface Deck {
  id: number;
  /** The deck this one is inside (null = top level). */
  parentId: number | null;
  name: string;
  /** Names from the top, e.g. "Class › Class 9". */
  path: string;
  depth: number;
  childCount: number;
  description: string;
  createdAt: number;
  /** Cards directly in this deck. */
  cardCount: number;
  dueCount: number;
  newCount: number;
  /** Including all subdecks. */
  totalCards: number;
  totalDue: number;
  totalNew: number;
}

export interface Card {
  id: number;
  deckId: number;
  hanzi: string;
  pinyin: string;
  meaning: string;
  notes: string;
  tags: string[];
  createdAt: number;
  updatedAt: number;
}

export interface CardInput {
  deckId: number;
  hanzi: string;
  pinyin: string;
  meaning: string;
  notes: string;
  tags: string[];
}

export interface CardRow extends Card {
  status: "new" | "learning" | "review";
  nextDue: number | null;
  reps: number;
  lapses: number;
}

export interface Progress {
  state: number;
  stability: number;
  difficulty: number;
  due: number;
  lastReview: number;
  reps: number;
  lapses: number;
}

export interface StudyFilter {
  deckIds: number[];
  tags: string[];
  directions: Direction[];
  mode: StudyMode;
  limit: number;
}

export interface StudyItem {
  card: Card;
  direction: Direction;
  progress: Progress | null;
}

export interface SessionInfo {
  items: StudyItem[];
  dueCount: number;
  newCount: number;
  newRemainingToday: number;
}

export interface IntervalPreview {
  secs: number;
  days: number | null;
}

export interface AnswerResult {
  due: number;
  state: number;
  intervalSecs: number;
  days: number | null;
}

export interface DayStat {
  date: string;
  reviews: number;
  correct: number;
  newCards: number;
  timeMs: number;
}

export interface Stats {
  totalCards: number;
  unstudiedCards: number;
  learning: number;
  young: number;
  mature: number;
  dueNow: number;
  totalReviews: number;
  totalTimeMs: number;
  todayReviews: number;
  todayCorrect: number;
  todayTimeMs: number;
  retention: number | null;
  streak: number;
  bestStreak: number;
  days: DayStat[];
  forecast: number[];
  hardest: { cardId: number; hanzi: string; pinyin: string; meaning: string; direction: Direction; lapses: number }[];
}

export interface DictEntry {
  id: number;
  traditional: string;
  simplified: string;
  pinyin: string;
  pinyinNum: string;
  definitions: string[];
  inCards: boolean;
}

/**
 * A word offered while typing pinyin; `consumed` = how many typed characters it uses.
 * A composed phrase has `entry.id` -1 and lists its words' entry ids in `parts`.
 */
export interface Candidate {
  entry: DictEntry;
  consumed: number;
  parts: number[];
}

/** A dictionary word within a character string (`entry` is null when none matches). */
export interface Segment {
  text: string;
  entry: DictEntry | null;
}

/** A word in an import file that is already in other decks. */
export interface DuplicateWord {
  hanzi: string;
  decks: string[];
}

/** What importing a file would do. */
export interface ImportPreview {
  rows: number;
  fresh: number;
  sameDeck: number;
  otherDecks: DuplicateWord[];
  invalid: number;
}

/** What a Pleco flashcard file contains, before importing it. */
export interface PlecoPreview {
  cards: number;
  categories: { name: string; deck: string; exists: boolean; cards: number }[];
  uncategorized: number;
  dictionary: boolean;
  duplicates: ImportPreview;
}

export interface PlecoImportResult {
  imported: number;
  skipped: number;
  skippedOther: number;
  decks: number;
  decksCreated: number;
  pinyinCorrected: number;
  definitionsAdded: number;
}

export interface DictStatus {
  entries: number;
  importedAt: number | null;
}

export interface ImportResult {
  imported: number;
  /** Already in the same deck, or rows without Chinese characters. */
  skipped: number;
  /** Words already in other decks that weren't imported again. */
  skippedOther: number;
}

export const api = {
  getSettings: () => invoke<Record<string, string>>("get_settings"),
  setSetting: (key: string, value: string) => invoke<void>("set_setting", { key, value }),

  listDecks: () => invoke<Deck[]>("list_decks"),
  createDeck: (name: string, description = "", parentId: number | null = null) =>
    invoke<number>("create_deck", { name, description, parentId }),
  updateDeck: (id: number, name: string, description: string, parentId: number | null) =>
    invoke<void>("update_deck", { id, name, description, parentId }),
  deleteDeck: (id: number) => invoke<void>("delete_deck", { id }),

  listCards: (deckId: number | null, search = "") => invoke<CardRow[]>("list_cards", { deckId, search }),
  createCard: (input: CardInput) => invoke<Card>("create_card", { input }),
  updateCard: (id: number, input: CardInput) => invoke<Card>("update_card", { id, input }),
  deleteCards: (ids: number[]) => invoke<void>("delete_cards", { ids }),
  moveCards: (ids: number[], deckId: number) => invoke<void>("move_cards", { ids, deckId }),
  listTags: () => invoke<string[]>("list_tags"),
  cardsByHanzi: (hanzi: string) => invoke<{ card: Card; deckName: string }[]>("cards_by_hanzi", { hanzi }),
  importPreview: (deckId: number, path: string) => invoke<ImportPreview>("import_preview", { deckId, path }),
  importCards: (deckId: number, path: string, includeOtherDecks = false) =>
    invoke<ImportResult>("import_cards", { deckId, path, includeOtherDecks }),
  exportCards: (deckId: number | null, path: string) => invoke<number>("export_cards", { deckId, path }),
  plecoPreview: (path: string, defaultDeck: number | null) =>
    invoke<PlecoPreview>("pleco_preview", { path, defaultDeck }),
  importPleco: (path: string, defaultDeck: number | null, useDictionary: boolean, includeOtherDecks: boolean) =>
    invoke<PlecoImportResult>("import_pleco", { path, defaultDeck, useDictionary, includeOtherDecks }),
  exportPleco: (deckId: number | null, path: string) => invoke<number>("export_pleco", { deckId, path }),

  buildSession: (filter: StudyFilter) => invoke<SessionInfo>("build_session", { filter }),
  previewIntervals: (cardId: number, direction: Direction) =>
    invoke<IntervalPreview[]>("preview_intervals", { cardId, direction }),
  answerCard: (cardId: number, direction: Direction, rating: number, durationMs: number) =>
    invoke<AnswerResult>("answer_card", { cardId, direction, rating, durationMs }),
  getStats: (days: number) => invoke<Stats>("get_stats", { days }),

  dictStatus: () => invoke<DictStatus>("dict_status"),
  dictSearch: (query: string, limit = 60) => invoke<DictEntry[]>("dict_search", { query, limit }),
  dictLookup: (hanzi: string) => invoke<DictEntry[]>("dict_lookup", { hanzi }),
  dictCandidates: (query: string, limit = 9) => invoke<Candidate[]>("dict_candidates", { query, limit }),
  dictSegment: (text: string, prefer: number[] = []) => invoke<Segment[]>("dict_segment", { text, prefer }),
  dictImportFile: (path: string) => invoke<number>("dict_import_file", { path }),
  dictDownload: () => invoke<number>("dict_download"),
};

export function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}
