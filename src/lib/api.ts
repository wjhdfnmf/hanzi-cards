// Typed wrappers around the Rust commands (src-tauri/src/commands.rs).
import { invoke } from "@tauri-apps/api/core";

export type Direction = "recognition" | "recall" | "listening" | "speaking" | "read_aloud" | "say_meaning";
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

/** Whether spoken answers can be checked on this machine, and with which engine. */
export interface SpeechStatus {
  available: boolean;
  reason: string | null;
  /** "windows" | "whisper" */
  engine: string | null;
  windows: boolean;
  whisperModels: string[];
  whisperModel: string | null;
}

/** Where Whisper runs. */
export interface WhisperDeviceInfo {
  gpuBuild: boolean;
  gpu: string | null;
  gpuProblem: string | null;
  /** "gpu" | "cpu" */
  using: string;
}

export interface WhisperModel {
  /** Too slow without a graphics card. */
  gpuRecommended: boolean;
  name: string;
  sizeMb: number;
  installed: boolean;
}

/** One spoken attempt, judged against the card. */
export interface SpokenAnswer {
  /** "heard" | "silence" | "unclear" | "cancelled" */
  status: string;
  heard: string;
  heardPinyin: string;
  confidence: string;
  correct: boolean;
  /** Suggested rating 1-4. */
  suggested: number;
  /** "tone" when the syllables were right but a tone was wrong. */
  note: string;
}

/** A word in an import file that is already in other decks. */
export interface KeyStatus {
  set: boolean;
  /** Last four characters of the stored key. */
  hint: string | null;
}

/** How the tutor behaves in one conversation. */
export interface TutorOptions {
  level: "hsk1" | "hsk2" | "hsk3";
  length: "short" | "normal" | "long";
  corrections: "off" | "gentle" | "every";
  english: "never" | "asked" | "hints";
  newWords: number;
  lead: "tutor" | "learner";
  onlyMyWords: boolean;
  /** "" = the model chosen in Settings */
  model: string;
  notes: string;
}

export interface AiConversationCost {
  /** null for a call outside a conversation */
  sessionId: number | null;
  startedAt: number;
  endedAt: number;
  model: string;
  calls: number;
  inputTokens: number;
  outputTokens: number;
  cost: number;
}

/** What the AI tutor has cost (US dollars, estimated from token counts). */
export interface AiSpending {
  today: number;
  last7Days: number;
  thisMonth: number;
  total: number;
  conversations: number;
  recent: AiConversationCost[];
}

export interface TutorGrade {
  hanzi: string;
  correct: boolean;
  cardId: number | null;
}

export interface TutorSuggestion {
  hanzi: string;
  pinyin: string;
  meaning: string;
  /** Already a card in some deck. */
  exists: boolean;
}

export type TutorEvent =
  | { kind: "text"; text: string }
  | { kind: "grade"; grade: TutorGrade }
  | { kind: "suggestion"; suggestion: TutorSuggestion };

export interface TutorTurn {
  text: string;
  /** "end_turn", "refusal", "max_tokens", "cancelled", ... */
  stopReason: string;
  /** Session cost so far, US dollars. */
  cost: number;
}

export interface TutorSummary {
  grades: TutorGrade[];
  suggestions: TutorSuggestion[];
  cost: number;
  model: string;
  turns: number;
}

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
  /** Add HSK level tags to all cards that lack one; returns how many changed. */
  hskTagAll: () => invoke<number>("hsk_tag_all"),
  cardsByHanzi: (hanzi: string) => invoke<{ card: Card; deckName: string }[]>("cards_by_hanzi", { hanzi }),
  speechStatus: () => invoke<SpeechStatus>("speech_status"),
  speechListen: (cardId: number, others: string[]) => invoke<SpokenAnswer>("speech_listen", { cardId, others }),
  speechCancel: () => invoke<void>("speech_cancel"),
  speechCheckAudio: (cardId: number, others: string[], pcm: Float32Array) =>
    invoke<SpokenAnswer>("speech_check_audio", { cardId, others, pcm: Array.from(pcm) }),
  /** `prompt`: words or a previous line the speech probably contains or answers. */
  /** "Say the meaning": judge an English answer against the card's meaning. */
  speechCheckEnglish: (cardId: number, pcm: Float32Array) =>
    invoke<SpokenAnswer>("speech_check_english", { cardId, pcm: Array.from(pcm) }),
  speechTranscribe: (pcm: Float32Array, prompt: string | null = null) =>
    invoke<string>("speech_transcribe", { pcm: Array.from(pcm), prompt }),
  /** Load Whisper ahead of the first spoken answer (fire and forget). */
  speechWarmup: () => invoke<void>("speech_warmup"),
  whisperDeviceInfo: () => invoke<WhisperDeviceInfo>("whisper_device_info"),
  whisperModels: () => invoke<WhisperModel[]>("whisper_models"),
  whisperDownload: (name: string) => invoke<void>("whisper_download", { name }),
  whisperDelete: (name: string) => invoke<void>("whisper_delete", { name }),
  aiKeyStatus: () => invoke<KeyStatus>("ai_key_status"),
  aiKeySet: (key: string) => invoke<void>("ai_key_set", { key }),
  aiKeyClear: () => invoke<void>("ai_key_clear"),
  tutorStart: (deckIds: number[], scenario: string, options: TutorOptions) =>
    invoke<{ sessionId: number; words: number }>("tutor_start", { deckIds, scenario, options }),
  tutorSend: (sessionId: number, text: string | null) => invoke<TutorTurn>("tutor_send", { sessionId, text }),
  tutorTranscribe: (sessionId: number, pcm: Float32Array) =>
    invoke<string>("tutor_transcribe", { sessionId, pcm: Array.from(pcm) }),
  tutorCancel: (sessionId: number) => invoke<void>("tutor_cancel", { sessionId }),
  tutorEnd: (sessionId: number) => invoke<TutorSummary>("tutor_end", { sessionId }),
  tutorTranslate: (text: string, sessionId: number | null = null) => invoke<string>("tutor_translate", { text, sessionId }),
  aiSpending: () => invoke<AiSpending>("ai_spending"),
  aiSpendingExport: (path: string) => invoke<void>("ai_spending_export", { path }),
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
