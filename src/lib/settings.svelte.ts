import { api } from "./api";
import { setLang } from "./i18n.svelte";
import { isCustomTheme } from "./themes";

export const settings = $state({
  loaded: false,
  language: "en",
  theme: "light",
  desiredRetention: 0.9,
  newPerDay: 20,
  /** "2" (default): Wrong / Right (Again / Good); "4": Again / Hard / Good / Easy. */
  answerButtons: "2",
  /** Once one direction of a word is answered, leave its other directions for the next session. */
  oneDirectionPerSession: true,
  // Settings › Studying › Advanced (the scheduler reads the same keys)
  /** Correct recalls a new word needs on its first day. */
  newRecalls: 3,
  /** Correct recalls a forgotten word needs before it's back on the normal spacing. */
  relearnRecalls: 1,
  /** Minutes before a word missed while learning comes back. */
  againMinutes: 1,
  /** Minutes before the next recall after the 1st, 2nd … correct one ("5 15"; the last repeats). */
  learnGaps: "5 15",
  /** Minutes before a forgotten review word first comes back. */
  relearnMinutes: 10,
  /** In a session, a missed word comes back at least this many cards later. */
  minGapCards: 5,
  /** Spread review dates a little. */
  fuzz: true,
  /** Longest review interval in days. */
  maxIntervalDays: 36500,
  /** Lapses that make a word a leech (0 = off). */
  leechLapses: 8,
  /** Order new words are introduced: "hsk" (HSK1 first, then deck order), "deck", "random". */
  newOrder: "hsk",
  toneColors: true,
  /** Tag new cards with their HSK level (2026 syllabus). */
  hskAutotag: true,
  speechRate: 0.9,
  autoplayAudio: false,
  voice: "",
  /** Where speech plays: "" = the system's default output, else a device id (with its name,
   * to find it again if the id changes). */
  audioOutput: "",
  audioOutputLabel: "",
  /** "auto" | "windows" | "whisper" */
  speechEngine: "auto",
  whisperModel: "base",
  /** "auto" (GPU if available) | "cpu" */
  whisperDevice: "auto",
  /** Re-check Whisper's word against same-syllable words in other tones by pitch. */
  pitchTones: false,
  /** Keep the spoken answer (in memory, until the next card) so it can be played back. */
  replayAnswers: true,
  tutorModel: "claude-opus-5-5",
  /** Keep calibration recordings for checking the tone detector (always on in dev builds). */
  debugMode: false,
  /** Reading page: the story's typeface (a key of READING_FONTS) and text size in px. */
  readingFont: "sans",
  readingSize: 24,
  /** Space between lines, as a multiple of the text size. */
  readingLineHeight: 1.9,
  /** Colour the story's characters by tone (when tone colours are on). */
  readingToneColors: false,
  /** How the Reading page looks: "plain" (a list and a title), or a kind of book cover, with a
   * banner on the open story: "books" (illustrated), "cloth", "thread", "poster". */
  readingStyle: "plain",
});

const KEYS: Record<string, keyof typeof settings> = {
  language: "language",
  theme: "theme",
  desired_retention: "desiredRetention",
  new_per_day: "newPerDay",
  answer_buttons: "answerButtons",
  one_direction_per_session: "oneDirectionPerSession",
  new_recalls: "newRecalls",
  relearn_recalls: "relearnRecalls",
  again_minutes: "againMinutes",
  learn_gaps: "learnGaps",
  relearn_minutes: "relearnMinutes",
  min_gap_cards: "minGapCards",
  fuzz: "fuzz",
  max_interval_days: "maxIntervalDays",
  leech_lapses: "leechLapses",
  new_order: "newOrder",
  tone_colors: "toneColors",
  hsk_autotag: "hskAutotag",
  speech_rate: "speechRate",
  autoplay_audio: "autoplayAudio",
  voice: "voice",
  audio_output: "audioOutput",
  audio_output_label: "audioOutputLabel",
  speech_engine: "speechEngine",
  whisper_model: "whisperModel",
  whisper_device: "whisperDevice",
  pitch_tones: "pitchTones",
  replay_answers: "replayAnswers",
  tutor_model: "tutorModel",
  debug_mode: "debugMode",
  reading_font: "readingFont",
  reading_size: "readingSize",
  reading_line_height: "readingLineHeight",
  reading_tone_colors: "readingToneColors",
  reading_style: "readingStyle",
};

function parse(field: keyof typeof settings, raw: string): unknown {
  const current = settings[field];
  if (typeof current === "boolean") return raw === "true";
  if (typeof current === "number") {
    const n = Number(raw);
    return Number.isFinite(n) ? n : current;
  }
  return raw;
}

/** "system" follows the OS (no data-theme); "light", "dark" or a custom theme's key is set on
 * <html data-theme>, where app.css defines its colours. An unknown value falls back to system. */
export function applyTheme() {
  const root = document.documentElement;
  const t = settings.theme;
  if (t === "light" || t === "dark" || isCustomTheme(t)) root.dataset.theme = t;
  else delete root.dataset.theme;
}

export async function loadSettings() {
  const raw = await api.getSettings();
  for (const [key, field] of Object.entries(KEYS)) {
    if (raw[key] !== undefined) (settings as Record<string, unknown>)[field] = parse(field, raw[key]);
  }
  setLang(settings.language);
  applyTheme();
  settings.loaded = true;
}

export async function saveSetting<K extends keyof typeof settings>(field: K, value: (typeof settings)[K]) {
  settings[field] = value;
  const key = Object.keys(KEYS).find((k) => KEYS[k] === field);
  if (key) await api.setSetting(key, String(value));
  if (field === "language") setLang(String(value));
  if (field === "theme") applyTheme();
}
