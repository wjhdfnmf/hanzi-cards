import { api } from "./api";
import { setLang } from "./i18n.svelte";

export const settings = $state({
  loaded: false,
  language: "en",
  theme: "system",
  desiredRetention: 0.9,
  newPerDay: 20,
  toneColors: true,
  speechRate: 0.9,
  autoplayAudio: false,
  voice: "",
});

const KEYS: Record<string, keyof typeof settings> = {
  language: "language",
  theme: "theme",
  desired_retention: "desiredRetention",
  new_per_day: "newPerDay",
  tone_colors: "toneColors",
  speech_rate: "speechRate",
  autoplay_audio: "autoplayAudio",
  voice: "voice",
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
