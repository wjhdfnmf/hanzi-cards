// Text-to-speech using the operating system's voices.
//
// Normally through the webview's Web Speech API, which always plays on the system's default
// output. When Settings › Audio › Output names a device (Windows), the app makes the speech
// itself as a WAV (tts.rs) and plays it on that device instead.
import { api } from "./api";
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

// ---------- the webview's speech (default output) ----------

function webSpeak(text: string, rate: number, interrupt: boolean) {
  const s = synth();
  if (!s || !text.trim()) return;
  if (interrupt) s.cancel();
  const u = new SpeechSynthesisUtterance(text);
  u.lang = "zh-CN";
  u.rate = rate;
  const voices = chineseVoices();
  const voice = voices.find((v) => v.name === settings.voice) ?? voices[0];
  if (voice) u.voice = voice;
  s.speak(u);
}

// ---------- speech on a chosen output device ----------

/** Whether the app can make speech itself (Windows). Known shortly after start. */
let canRoute = false;
export const routingReady: Promise<boolean> = api
  .ttsAvailable()
  .then((ok) => (canRoute = ok))
  .catch(() => false);

/** Whether a device other than the default is chosen (and can be used). */
const routed = () => canRoute && settings.audioOutput !== "";

/** The output devices the system offers (names need microphone permission). */
export async function outputDevices(): Promise<MediaDeviceInfo[]> {
  const all = (await navigator.mediaDevices?.enumerateDevices?.().catch(() => [])) ?? [];
  return all.filter((d) => d.kind === "audiooutput" && d.deviceId !== "default" && d.deviceId !== "communications");
}

/** The chosen device's current id, found by id or else by name; null if it's gone. */
async function chosenDevice(): Promise<string | null> {
  const outs = await outputDevices();
  const byId = outs.find((d) => d.deviceId === settings.audioOutput);
  if (byId) return byId.deviceId;
  const byName = settings.audioOutputLabel ? outs.find((d) => d.label === settings.audioOutputLabel) : undefined;
  return byName?.deviceId ?? null;
}

/** Waiting to be said; each one's WAV is being made from the moment it's queued, so the next
 * sentence is ready when the current one ends (making one takes about 0.1 s). */
const queue: { text: string; rate: number; wav: Promise<ArrayBuffer> }[] = [];
let current: HTMLAudioElement | null = null;
let playing = false;
/** Bumped by stop: a running queue that sees a newer generation gives up. */
let generation = 0;

async function play() {
  if (playing) return;
  playing = true;
  const mine = generation;
  try {
    while (queue.length && mine === generation) {
      const { text, rate, wav: making } = queue.shift()!;
      const device = await chosenDevice();
      if (device === null) {
        // unplugged or renamed: the default output rather than silence
        webSpeak(text, rate, false);
        continue;
      }
      try {
        const wav = await making;
        if (mine !== generation) break;
        const url = URL.createObjectURL(new Blob([wav], { type: "audio/wav" }));
        const audio = new Audio(url);
        current = audio;
        await (audio as HTMLAudioElement & { setSinkId(id: string): Promise<void> }).setSinkId(device);
        await new Promise<void>((done) => {
          audio.onended = () => done();
          audio.onerror = () => done();
          audio.onpause = () => done(); // stopped
          audio.play().catch(() => done());
        });
        URL.revokeObjectURL(url);
      } catch (e) {
        console.warn("speech on the chosen output failed; using the default", e);
        webSpeak(text, rate, false);
      }
    }
  } finally {
    if (mine === generation) {
      playing = false;
      current = null;
    }
  }
}

function stopRouted() {
  generation++;
  queue.length = 0;
  current?.pause();
  current = null;
  playing = false;
}

function routedSpeak(text: string, rate: number, interrupt: boolean) {
  if (!text.trim()) return;
  if (interrupt) {
    stopRouted();
    synth()?.cancel();
  }
  const wav = api.ttsWav(text, settings.voice, rate);
  wav.catch(() => {}); // a failure is handled where it's played
  queue.push({ text, rate, wav });
  void play();
}

// ---------- playing a recording (e.g. the learner's own answer) ----------

let clip: HTMLAudioElement | null = null;

/** Play a recording on the chosen output (or the default), stopping any speech first.
 * Resolves when it has finished. */
export async function playClip(blob: Blob): Promise<void> {
  stopSpeaking();
  stopClip();
  const url = URL.createObjectURL(blob);
  const audio = new Audio(url);
  clip = audio;
  try {
    if (settings.audioOutput) {
      const device = await chosenDevice();
      if (device) await (audio as HTMLAudioElement & { setSinkId(id: string): Promise<void> }).setSinkId(device).catch(() => {});
    }
    await new Promise<void>((done) => {
      audio.onended = () => done();
      audio.onerror = () => done();
      audio.onpause = () => done();
      audio.play().catch(() => done());
    });
  } finally {
    URL.revokeObjectURL(url);
    if (clip === audio) clip = null;
  }
}

export function stopClip() {
  clip?.pause();
  clip = null;
}

// ---------- what the app calls ----------

export function speak(text: string) {
  if (!text) return;
  if (routed()) routedSpeak(text, settings.speechRate, true);
  else webSpeak(text, settings.speechRate, true);
}

/** Add a sentence to the queue without interrupting what is being said. */
export function speakQueued(text: string, rate = settings.speechRate) {
  if (routed()) routedSpeak(text, rate, false);
  else webSpeak(text, rate, false);
}

export function stopSpeaking() {
  stopRouted();
  synth()?.cancel();
}

/** Whether anything is being said or is waiting to be said. */
export const isSpeaking = () => {
  const s = synth();
  return playing || queue.length > 0 || (!!s && (s.speaking || s.pending));
};

export const speechAvailable = () => synth() !== null || canRoute;
