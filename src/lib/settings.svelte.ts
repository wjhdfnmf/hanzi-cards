import { api } from "./api";
import { setLang } from "./i18n.svelte";

export const settings = $state({
  loaded: false,
  language: "en",
  theme: "system",
  desiredRetention: 0.9,
  newPerDay: 20,
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
});

const KEYS: Record<string, keyof typeof settings> = {
  language: "language",
  theme: "theme",
  desired_retention: "desiredRetention",
  new_per_day: "newPerDay",
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

export function applyTheme() {
  const root = document.documentElement;
  if (settings.theme === "light" || settings.theme === "dark") root.dataset.theme = settings.theme;
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
