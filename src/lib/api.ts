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

/** A speaker's pitch range (semitones above 100 Hz, and Hz). */
export interface PitchVoice {
  medianSt: number;
  spreadSt: number;
  medianHz: number;
  /** Bottom and top of the range used for tones (Chao's levels 1 and 5). */
  lowHz: number;
  highHz: number;
}

/** The tone detector's settings (from tones.rs). */
export interface PitchParams {
  hopMs: number;
  windowMs: number;
  f0MinHz: number;
  f0MaxHz: number;
  yinFirst: number;
  yinGlobal: number;
  lowPassHz: number;
  shapeWeight: number;
  /** Weight of how far a syllable falls and rises. */
  fallWeight: number;
  creak: number;
  profileDecay: number;
  minRecordings: number;
  profileLowSt: number;
  profileBinSt: number;
  /** [tone, five points on Chao's scale relative to the speaker, -2..+2] */
  templates: [number, number[]][];
}

/** The learner's pitch profile (what "Use pitch to tell tones apart" uses). */
export interface PitchProfileInfo {
  recordings: number;
  ready: boolean;
  /** Histogram of voiced frames in half-semitone bins from params.profileLowSt. */
  bins: number[];
  voice: PitchVoice | null;
  params: PitchParams;
}

/** An earlier recording of the same calibration: its pitch track, the tone asked for (null:
 * the sentence) and the syllable's length. */
export interface PitchEarlier {
  pitch: (number | null)[];
  tone: number | null;
  length: number | null;
}

/** One calibration recording, analysed. */
export interface PitchAnalysis {
  /** Pitch per 10 ms frame (semitones above 100 Hz), null where unvoiced. */
  pitch: (number | null)[];
  /** Loudness per frame, 0..1. */
  level: number[];
  voicedMs: number;
  /** Mains hum found and filtered out before tracking (Hz). */
  humHz: number[];
  segment: [number, number] | null;
  /** The frames the shape was measured from: the syllable's middle 80% plus a creaky ending. */
  measured: [number, number] | null;
  /** Frames after the syllable counted as creak (no pitch, still loud: the bottom of the voice). */
  creakFrames: number;
  contour: number[] | null;
  /** Tones 1-4. */
  probs: number[] | null;
  heard: number | null;
  /** How to say the asked tone more clearly (keys: t1Higher, t1Level, t2Rise, t3Lower,
   * t4Higher, t4Fall), with where the voice should be and where it was. */
  advice: { key: string; targetHz: number; actualHz: number }[];
  /** How long the syllable sounded (10 ms frames) and the speaker's usual length. */
  lengthFrames: number | null;
  typicalLength: number;
  voice: PitchVoice | null;
  profile: PitchProfileInfo;
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
  /** The Whisper model to suggest for this computer. */
  recommended: string;
  /** NVIDIA's libraries (GPU builds only). */
  libs: { nvidia: boolean; installed: boolean; downloaded: boolean; downloadMb: number } | null;
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

/** What a cards database file holds. */
export interface DataContents {
  cards: number;
  decks: number;
  reviews: number;
  /** Unix seconds. */
  lastReview: number | null;
}

export interface DataFile {
  name: string;
  path: string;
  sizeBytes: number;
  /** Unix seconds. */
  modified: number;
  /** null when the file isn't a readable Hanzi Cards database. */
  contents: DataContents | null;
}

export interface DataInfo {
  current: DataFile;
  folder: string;
  /** Newest first. */
  backups: DataFile[];
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
  /** Whether the app can make speech itself, so it can go to a chosen output device. */
  ttsAvailable: () => invoke<boolean>("tts_available"),
  /** Speech for `text` as a WAV file (Windows' speech engine). */
  ttsWav: (text: string, voice: string, rate: number) => invoke<ArrayBuffer>("tts_wav", { text, voice, rate }),
  pitchProfileInfo: () => invoke<PitchProfileInfo>("pitch_profile_info"),
  /** `tone`: the tone asked for (one syllable), or null for a sentence. `earlier`: the
   * calibration's earlier recordings' pitch tracks. Saves nothing. */
  pitchAnalyze: (pcm: Float32Array, tone: number | null, earlier: PitchEarlier[]) =>
    invoke<PitchAnalysis>("pitch_analyze", { pcm: Array.from(pcm), tone, earlier }),
  pitchCalibrationSave: (recordings: PitchEarlier[]) =>
    invoke<PitchProfileInfo>("pitch_calibration_save", { recordings }),
  pitchProfileReset: () => invoke<PitchProfileInfo>("pitch_profile_reset"),
  /** In debug mode only: keep a calibration recording as a WAV in the app-data folder and
   * return its path (null when not in debug mode: nothing is saved). */
  pitchSaveRecording: (pcm: Float32Array, label: string) =>
    invoke<string | null>("pitch_save_recording", { pcm: Array.from(pcm), label }),
  /** `prompt`: words or a previous line the speech probably contains or answers. */
  /** "Say the meaning": judge an English answer against the card's meaning. */
  speechCheckEnglish: (cardId: number, pcm: Float32Array) =>
    invoke<SpokenAnswer>("speech_check_english", { cardId, pcm: Array.from(pcm) }),
  speechTranscribe: (pcm: Float32Array, prompt: string | null = null) =>
    invoke<string>("speech_transcribe", { pcm: Array.from(pcm), prompt }),
  /** Load Whisper ahead of the first spoken answer (fire and forget). */
  speechWarmup: () => invoke<void>("speech_warmup"),
  whisperDeviceInfo: () => invoke<WhisperDeviceInfo>("whisper_device_info"),
  /** Download NVIDIA's GPU libraries (progress: "gpu-download" events). */
  gpuLibsDownload: () => invoke<void>("gpu_libs_download"),
  gpuLibsRemove: () => invoke<void>("gpu_libs_remove"),
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

  dataInfo: () => invoke<DataInfo>("data_info"),
  /** Returns the backup's file name. */
  dataBackup: () => invoke<string>("data_backup"),
  dataSaveCopy: (path: string) => invoke<void>("data_save_copy", { path }),
  dataInspect: (path: string) => invoke<DataFile>("data_inspect", { path }),
  /** Switch to another database file; returns the name of the backup of the replaced one. */
  dataUseFile: (path: string) => invoke<string>("data_use_file", { path }),
  dataShowInFolder: () => invoke<void>("data_show_in_folder"),

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
