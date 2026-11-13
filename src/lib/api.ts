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
  /** Other cards this row stands for: the same characters, pinyin and meaning (a folded list only). */
  copies: CardCopy[];
}

export interface CardCopy {
  id: number;
  deckId: number;
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
  /** This answer made the word a leech (forgotten many times); it was tagged "leech". */
  leech: boolean;
  /** A leech remembered at review several times in a row: the "leech" tag came off. */
  unleeched: boolean;
}

export interface DayStat {
  date: string;
  reviews: number;
  correct: number;
  newCards: number;
  /** Answering flashcards. */
  timeMs: number;
  readingMs: number;
  tutorMs: number;
}

/** What the Statistics page is looking at. */
export interface StatsFilter {
  /** A deck with its subdecks; null = everything. */
  deckId: number | null;
  /** The last N days including today; null = all time. */
  days: number | null;
  direction: Direction | null;
}

/** Totals over a stretch of days. */
export interface StatSum {
  reviews: number;
  correct: number;
  newWords: number;
  timeMs: number;
  readingMs: number;
  tutorMs: number;
  daysStudied: number;
  /** True retention: reviews of words already learned (the first of each day), and how many were remembered. */
  retained: number;
  retentionOf: number;
}

export interface Stats {
  totalCards: number;
  totalWords: number;
  /** Words never studied. */
  unstudiedCards: number;
  learning: number;
  young: number;
  mature: number;
  dueNow: number;
  dueTomorrow: number;
  leeches: number;
  streak: number;
  bestStreak: number;
  targetRetention: number;
  today: StatSum;
  period: StatSum;
  /** The stretch of the same length before the period (null for all time). */
  previous: StatSum | null;
  allTime: StatSum;
  periodDays: number | null;
  /** One entry per day, oldest first: at least the last 371 days, and the whole period. */
  days: DayStat[];
  retentionTable: {
    range: "today" | "yesterday" | "week" | "month" | "year" | "all";
    /** [remembered, reviews] */
    young: [number, number];
    mature: [number, number];
  }[];
  /** Again, Hard, Good, Easy for words being learned, young words and mature words. */
  buttons: [number, number, number, number][];
  kindTimeMs: number[];
  hours: { reviews: number; correct: number }[];
  /** Monday first. */
  weekdays: { reviews: number; correct: number }[];
  byDirection: { direction: Direction; reviews: number; correct: number; timeMs: number }[];
  byDeck: {
    deckId: number;
    name: string;
    words: number;
    studied: number;
    mature: number;
    reviews: number;
    correct: number;
    timeMs: number;
  }[];
  hsk: { level: string; words: number; studied: number; mature: number }[];
  stability: number[];
  difficulty: number[];
  avgRetrievability: number | null;
  knownEstimate: number;
  studiedItems: number;
  dailyLoad: number;
  forecast: number[];
  hardest: { cardId: number; hanzi: string; pinyin: string; meaning: string; direction: Direction; lapses: number }[];
  /** Reading and tutor time are in the totals (not when a deck or direction is chosen). */
  otherTimeIncluded: boolean;
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
  /** Why: "gpu" | "gpu_small" | "cpu" | "cpu_modest" | "cpu_weak". */
  recommendedReason: string;
  /** What the suggestion is based on (memory in MB, when known). */
  cores: number;
  ramMb: number | null;
  gpuMemoryMb: number | null;
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
  length: "tiny" | "short" | "normal" | "long";
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
  /** Reading database. */
  stories: number;
  collections: number;
}

/** Which database: cards & decks, or reading (stories). */
export type DbKind = "cards" | "reading";

export interface DataFile {
  name: string;
  path: string;
  sizeBytes: number;
  /** Unix seconds. */
  modified: number;
  /** null when the file isn't a readable Hanzi Cards database. */
  contents: DataContents | null;
  /** For a backup the app made by itself: daily, before an update, or the database replaced by a switch. */
  auto: "daily" | "version" | "switch" | null;
}

export interface DataInfo {
  current: DataFile;
  folder: string;
  /** This database's backups folder (backups/decks or backups/reading). */
  backupFolder: string;
  /** The learner's own backups, newest first. The app never deletes these. */
  backups: DataFile[];
  /** The app's own (in "auto" inside the backups folder), newest first; it cleans these up. */
  autoBackups: DataFile[];
  autoFolder: string;
  /** How many automatic backups of each sort are kept. */
  keep: { daily: number; weekly: number; version: number; switch: number };
}

export interface ImportResult {
  imported: number;
  /** Already in the same deck, or rows without Chinese characters. */
  skipped: number;
  /** Words already in other decks that weren't imported again. */
  skippedOther: number;
}

/** A collection of stories; collections nest like decks. */
export interface StoryFolder {
  id: number;
  parentId: number | null;
  name: string;
  /** Names from the top, e.g. "Graded readers › Level 1". */
  path: string;
  depth: number;
  childCount: number;
  description: string;
  createdAt: number;
  /** Stories directly in this collection. */
  storyCount: number;
  /** Including all subcollections. */
  totalStories: number;
}

/** A story without its text, for lists. */
export interface StoryInfo {
  id: number;
  folderId: number | null;
  title: string;
  /** Chinese characters in the text. */
  chars: number;
  preview: string;
  createdAt: number;
  updatedAt: number;
}

export interface Story {
  id: number;
  folderId: number | null;
  title: string;
  body: string;
  /** Key vocabulary, one "word – pinyin – meaning" per line ("" for none). */
  vocab: string;
  position: number;
  createdAt: number;
  updatedAt: number;
}

/** What importing a folder of stories added. */
export interface StoryExport {
  stories: number;
  collections: number;
  /** The file or folder that was written. */
  path: string;
}

export interface StoryImport {
  collections: number;
  stories: number;
  skipped: number;
}

/** A free font the app can download for the Reading page. */
export interface FontFile {
  key: string;
  sizeBytes: number;
  downloaded: boolean;
}

/** A database that couldn't be opened when the app started (it's damaged). */
export interface StartupProblem {
  kind: DbKind;
  /** "damaged" (backups are offered), "in_use" (another program has it) or "error". */
  reason: "damaged" | "in_use" | "error";
  message: string;
}

/** What a study session with a selection would hold, as numbers only. */
export interface SessionCounts {
  matching: number;
  dueCount: number;
  newRemainingToday: number;
}

export const api = {
  /** Set when a database couldn't be opened at startup; the app then shows the recovery page. */
  startupProblem: () => invoke<StartupProblem | null>("startup_problem"),
  /** Try to open that database again; true when it is open now. */
  startupRetry: () => invoke<boolean>("startup_retry"),
  getSettings: () => invoke<Record<string, string>>("get_settings"),
  setSetting: (key: string, value: string) => invoke<void>("set_setting", { key, value }),

  listDecks: () => invoke<Deck[]>("list_decks"),
  createDeck: (name: string, description = "", parentId: number | null = null) =>
    invoke<number>("create_deck", { name, description, parentId }),
  updateDeck: (id: number, name: string, description: string, parentId: number | null) =>
    invoke<void>("update_deck", { id, name, description, parentId }),
  deleteDeck: (id: number) => invoke<void>("delete_deck", { id }),

  /** `merge`: a word that several decks hold with the same pinyin and meaning is one row. */
  listCards: (deckId: number | null, search = "", merge = false) =>
    invoke<CardRow[]>("list_cards", { deckId, search, merge }),
  createCard: (input: CardInput) => invoke<Card>("create_card", { input }),
  /** `copies`: cards that were the same word; they get the new characters, pinyin and meaning too. */
  updateCard: (id: number, input: CardInput, copies: number[] = []) =>
    invoke<Card>("update_card", { id, input, copies }),
  deleteCards: (ids: number[]) => invoke<void>("delete_cards", { ids }),
  moveCards: (ids: number[], deckId: number) => invoke<void>("move_cards", { ids, deckId }),
  listTags: () => invoke<string[]>("list_tags"),
  /** Remove these tags from every card; returns how many cards changed. */
  removeTags: (tags: string[]) => invoke<number>("remove_tags", { tags }),
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
  /** Like buildSession, without the cards (for the count on the Study page). */
  sessionCounts: (filter: StudyFilter) => invoke<SessionCounts>("session_counts", { filter }),
  previewIntervals: (cardId: number, direction: Direction) =>
    invoke<IntervalPreview[]>("preview_intervals", { cardId, direction }),
  answerCard: (cardId: number, direction: Direction, rating: number, durationMs: number) =>
    invoke<AnswerResult>("answer_card", { cardId, direction, rating, durationMs }),
  getStats: (filter: StatsFilter) => invoke<Stats>("get_stats", { filter }),
  /** Time spent on the Reading or AI Tutor page, reported a little at a time. */
  logStudyTime: (kind: "reading" | "tutor", seconds: number) => invoke<void>("log_study_time", { kind, seconds }),

  dataInfo: (kind: DbKind) => invoke<DataInfo>("data_info", { kind }),
  /** Returns the backup's file name. */
  dataBackup: (kind: DbKind) => invoke<string>("data_backup", { kind }),
  dataSaveCopy: (kind: DbKind, path: string) => invoke<void>("data_save_copy", { kind, path }),
  /** Delete one backup file (the learner's own or an automatic one). */
  dataDeleteBackup: (kind: DbKind, path: string) => invoke<void>("data_delete_backup", { kind, path }),
  dataInspect: (kind: DbKind, path: string) => invoke<DataFile>("data_inspect", { kind, path }),
  /** Switch to another database file; returns the name of the backup of the replaced one. */
  dataUseFile: (kind: DbKind, path: string) => invoke<string>("data_use_file", { kind, path }),
  /** Recovery only: set a damaged database's files aside and start an empty one; returns the
   * name of the folder they were moved to. */
  dataStartFresh: (kind: DbKind) => invoke<string>("data_start_fresh", { kind }),
  /** Open the app-data folder (both databases and their backups). */
  dataShowInFolder: () => invoke<void>("data_show_in_folder"),

  dictStatus: () => invoke<DictStatus>("dict_status"),
  dictSearch: (query: string, limit = 60) => invoke<DictEntry[]>("dict_search", { query, limit }),
  dictLookup: (hanzi: string) => invoke<DictEntry[]>("dict_lookup", { hanzi }),
  dictCandidates: (query: string, limit = 9) => invoke<Candidate[]>("dict_candidates", { query, limit }),
  dictSegment: (text: string, prefer: number[] = []) => invoke<Segment[]>("dict_segment", { text, prefer }),
  dictImportFile: (path: string) => invoke<number>("dict_import_file", { path }),
  dictDownload: () => invoke<number>("dict_download"),

  listStoryFolders: () => invoke<StoryFolder[]>("list_story_folders"),
  createStoryFolder: (name: string, description = "", parentId: number | null = null) =>
    invoke<number>("create_story_folder", { name, description, parentId }),
  updateStoryFolder: (id: number, name: string, description: string, parentId: number | null) =>
    invoke<void>("update_story_folder", { id, name, description, parentId }),
  deleteStoryFolder: (id: number) => invoke<void>("delete_story_folder", { id }),
  /** null = every story; a collection includes its subcollections. */
  listStories: (folderId: number | null) => invoke<StoryInfo[]>("list_stories", { folderId }),
  getStory: (id: number) => invoke<Story>("get_story", { id }),
  createStory: (folderId: number | null, title: string, body: string) =>
    invoke<number>("create_story", { folderId, title, body }),
  updateStory: (id: number, folderId: number | null, title: string, body: string) =>
    invoke<void>("update_story", { id, folderId, title, body }),
  deleteStory: (id: number) => invoke<void>("delete_story", { id }),
  /** Text files, each one story titled by its file name; returns how many were added. */
  importStories: (folderId: number | null, paths: string[]) => invoke<number>("import_stories", { folderId, paths }),
  /** The story's text split into dictionary words. */
  storySegments: (id: number) => invoke<Segment[]>("story_segments", { id }),
  storyTotal: () => invoke<number>("story_total"),
  /** A folder of story files as a collection; subfolders become subcollections. */
  importStoryFolder: (folderId: number | null, path: string) =>
    invoke<StoryImport>("import_story_folder", { folderId, path }),
  /** Write one story as a text file (the format the import reads). */
  exportStory: (id: number, path: string) => invoke<StoryExport>("export_story", { id, path }),
  /** Write a collection (null = every story) as a new folder of story files inside `path`. */
  exportStoryFolder: (folderId: number | null, path: string) =>
    invoke<StoryExport>("export_story_folder", { folderId, path }),
  fontsList: () => invoke<FontFile[]>("fonts_list"),
  /** Progress: "font-download" events. */
  fontDownload: (key: string) => invoke<void>("font_download", { key }),
  fontDelete: (key: string) => invoke<void>("font_delete", { key }),
  /** A downloaded font file's bytes. */
  fontData: (key: string) => invoke<ArrayBuffer>("font_data", { key }),
};

export function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}
