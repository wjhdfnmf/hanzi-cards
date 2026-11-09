// Text-to-speech using the operating system's voices via the Web Speech API.
import { settings } from "./settings.svelte";

let cached: SpeechSynthesisVoice[] = [];

function synth(): SpeechSynthesis | null {
  return typeof window !== "undefined" && "speechSynthesis" in window ? window.speechSynthesis : null;
}

export function chineseVoices(): SpeechSynthesisVoice[] {
  const s = synth();
  if (!s) return [];
  cached = s.getVoices();
  return cached.filter((v) => /^(zh|cmn)([-_](CN|Hans|SG))?/i.test(v.lang) && !/HK|TW|yue/i.test(v.lang));
}

/** Resolves once the voice list is populated (it loads asynchronously in some engines). */
export function voicesReady(): Promise<SpeechSynthesisVoice[]> {
  const s = synth();
  if (!s) return Promise.resolve([]);
  if (s.getVoices().length) return Promise.resolve(chineseVoices());
  return new Promise((resolve) => {
    const done = () => resolve(chineseVoices());
    s.addEventListener("voiceschanged", done, { once: true });
    setTimeout(done, 1500);
  });
}

export function speak(text: string) {
  const s = synth();
  if (!s || !text) return;
  s.cancel();
  const u = new SpeechSynthesisUtterance(text);
  u.lang = "zh-CN";
  u.rate = settings.speechRate;
  const voices = chineseVoices();
  const voice = voices.find((v) => v.name === settings.voice) ?? voices[0];
  if (voice) u.voice = voice;
  s.speak(u);
}

/** Add a sentence to the queue without interrupting what is being said. */
export function speakQueued(text: string, rate = settings.speechRate) {
  const s = synth();
  if (!s || !text.trim()) return;
  const u = new SpeechSynthesisUtterance(text);
  u.lang = "zh-CN";
  u.rate = rate;
  const voices = chineseVoices();
  const voice = voices.find((v) => v.name === settings.voice) ?? voices[0];
  if (voice) u.voice = voice;
  s.speak(u);
}

export function stopSpeaking() {
  synth()?.cancel();
}

/** Whether anything is being said or is waiting to be said. */
export const isSpeaking = () => {
  const s = synth();
  return !!s && (s.speaking || s.pending);
};

export const speechAvailable = () => synth() !== null;
