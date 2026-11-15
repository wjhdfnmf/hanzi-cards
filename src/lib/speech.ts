// Text-to-speech using the operating system's voices.
//
// Normally through the webview's Web Speech API, which always plays on the system's default
// output. When Settings › Audio › Output names a device (Windows), the app makes the speech
// itself as a WAV (tts.rs) and plays it on that device instead.
import { api } from "./api";
import { settings } from "./settings.svelte";
import { ios } from "./device.svelte";

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

/** How loud to speak (Settings › Audio), for the system's voices and for sound files alike. */
function volume(): number {
  if (ios) return 1; // (an iPhone's own buttons set how loud)
  const v = Number(settings.speechVolume);
  return Number.isFinite(v) ? Math.min(1, Math.max(0, v)) : 1;
}

function webSpeak(text: string, rate: number, interrupt: boolean) {
  const s = synth();
  if (!s || !text.trim()) return;
  if (interrupt) s.cancel();
  const u = new SpeechSynthesisUtterance(text);
  u.lang = "zh-CN";
  u.rate = rate;
  u.volume = volume();
  // (an iPhone speaks with its own Chinese voice, the one chosen in its Settings)
  if (!ios) {
    const voices = chineseVoices();
    const voice = voices.find((v) => v.name === settings.voice) ?? voices[0];
    if (voice) u.voice = voice;
  }
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

// ---------- the built-in voice (Kokoro), made by the app as a sound ----------

let haveSystem = true;
let haveBuiltIn = false;
/** Told by voice.svelte.ts once it knows which voices this computer has. */
export function setVoices(system: boolean, builtIn: boolean) {
  haveSystem = system;
  haveBuiltIn = builtIn;
}
/** The built-in voice speaks when it is the one chosen, or the only one there is. */
const builtIn = () => haveBuiltIn && (settings.voice === BUILT_IN || !haveSystem);
/** The setting's value for the built-in voice. */
export const BUILT_IN = "kokoro";
/** Speech is made as a sound file (and played by the page) for the built-in voice and for a chosen output. */
const asSound = () => builtIn() || routed();

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
const queue: { text: string; rate: number; wav: Promise<ArrayBuffer>; engine?: "kokoro" }[] = [];
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
      const { text, rate, wav: making, engine } = queue.shift()!;
      const device = routed() ? await chosenDevice() : null;
      if (routed() && device === null && engine !== "kokoro") {
        // unplugged or renamed: the default output rather than silence
        webSpeak(text, rate, false);
        continue;
      }
      try {
        const wav = await making;
        if (mine !== generation) break;
        const url = URL.createObjectURL(new Blob([wav], { type: "audio/wav" }));
        const audio = new Audio(url);
        audio.volume = volume();
        current = audio;
        if (device) await (audio as HTMLAudioElement & { setSinkId(id: string): Promise<void> }).setSinkId(device);
        await new Promise<void>((done) => {
          audio.onended = () => done();
          audio.onerror = () => done();
          audio.onpause = () => done(); // stopped
          audio.play().catch(() => done());
        });
        URL.revokeObjectURL(url);
      } catch (e) {
        console.warn("speech as a sound failed; using the system voice", e);
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

function routedSpeak(text: string, rate: number, interrupt: boolean, engine = builtIn() ? ("kokoro" as const) : undefined) {
  if (!text.trim()) return;
  if (interrupt) {
    stopRouted();
    synth()?.cancel();
  }
  const wav = api.ttsWav(text, settings.voice, rate, engine);
  wav.catch(() => {}); // a failure is handled where it's played
  queue.push({ text, rate, wav, engine });
  void play();
}

/** Say something in the built-in voice whatever voice is chosen (Settings' sample button). */
export function sampleBuiltIn(text: string) {
  routedSpeak(text, settings.speechRate, true, "kokoro");
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
  audio.volume = volume();
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
  if (asSound()) routedSpeak(text, settings.speechRate, true);
  else webSpeak(text, settings.speechRate, true);
}

/** Add a sentence to the queue without interrupting what is being said. */
export function speakQueued(text: string, rate = settings.speechRate) {
  if (asSound()) routedSpeak(text, rate, false);
  else webSpeak(text, rate, false);
}

/** A reading that is paused: the sound stays where it is, and nothing further is said. */
let paused = false;
/** Ends the wait for the sound that is playing (a paused sound says nothing when stopped). */
let release: (() => void) | null = null;
export function pauseSpeaking() {
  paused = true;
  current?.pause();
  synth()?.pause();
}
export function resumeSpeaking() {
  paused = false;
  current?.play().catch(() => release?.());
  synth()?.resume();
}

export function stopSpeaking() {
  paused = false;
  reading++;
  stopRouted();
  synth()?.cancel();
  release?.();
  release = null;
}

// ---------- reading a text aloud, part by part ----------

/** Bumped whenever speech is stopped or a new reading starts: a reading that sees a newer
 * number gives up. */
let reading = 0;

/**
 * Read `parts` (sentences) one after another and say where the reading is, so a page can
 * show it: `start(i)` when part i begins, `word(i, at, length)` for each word as it is said
 * (characters into the part; only where the voice reports it), `done()` at the end or when
 * stopped. Stop it with `stopSpeaking()`.
 */
export function readAloud(
  parts: string[],
  on: { start: (i: number) => void; word?: (i: number, at: number, length: number) => void; done: () => void },
  rate = settings.speechRate,
  /** A part's sound if it was made beforehand (an MP3; null when it wasn't): played as it
   *  is, at the speed in Settings, instead of being said by the voice. */
  made?: (text: string) => Promise<ArrayBuffer | null>,
  /** When each word of a made part is said (ms into its sound; `at` counts from the part's
   *  first character that isn't space), so the word being said can be marked. */
  timing?: (text: string) => Promise<{ at: number; len: number; begin: number; end: number }[]>,
) {
  stopSpeaking();
  const mine = ++reading;
  const alive = () => mine === reading;

  const sayOnWeb = (i: number) =>
    new Promise<void>((next) => {
      const s = synth();
      if (!s) return next();
      const u = new SpeechSynthesisUtterance(parts[i]);
      u.lang = "zh-CN";
      u.rate = rate;
      u.volume = volume();
      const voices = chineseVoices();
      const voice = voices.find((v) => v.name === settings.voice) ?? voices[0];
      if (voice) u.voice = voice;
      u.onboundary = (e) => {
        if (alive() && e.name === "word") on.word?.(i, e.charIndex, e.charLength || 1);
      };
      u.onend = () => next();
      u.onerror = () => next();
      s.speak(u);
    });

  const sayAsSound = async (i: number, device: string | null) => {
    const wav = await api.ttsWav(parts[i], settings.voice, rate, builtIn() ? "kokoro" : undefined);
    if (!alive()) return;
    const url = URL.createObjectURL(new Blob([wav], { type: "audio/wav" }));
    const audio = new Audio(url);
    audio.volume = volume();
    current = audio; // so that stopping pauses it
    try {
      if (device) await (audio as HTMLAudioElement & { setSinkId(id: string): Promise<void> }).setSinkId(device);
      await new Promise<void>((done) => {
        release = done;
        audio.onended = () => done();
        audio.onerror = () => done();
        audio.onpause = () => {
          if (!paused) done(); // (stopped; a pause waits here)
        };
        audio.play().catch(() => done());
      });
    } finally {
      URL.revokeObjectURL(url);
      if (current === audio) current = null;
    }
  };

  const playMade = async (mp3: ArrayBuffer, device: string | null, i: number) => {
    const words = (timing && on.word ? await timing(parts[i]).catch(() => []) : []).slice().sort((a, b) => a.begin - b.begin);
    if (!alive()) return;
    const lead = parts[i].length - parts[i].trimStart().length;
    const url = URL.createObjectURL(new Blob([mp3], { type: "audio/mpeg" }));
    const audio = new Audio(url);
    audio.volume = volume();
    // (how fast is the player's business: the sound was made once, at its natural speed)
    audio.playbackRate = settings.speechRate;
    current = audio;
    madeNow = audio;
    // the word being said, looked up as the sound plays (several times a second is enough)
    let said = -1;
    const mark = () => {
      const ms = audio.currentTime * 1000;
      let now = -1;
      for (let k = 0; k < words.length && words[k].begin <= ms; k++) now = k;
      if (now >= 0 && now !== said && alive()) {
        said = now;
        on.word?.(i, lead + words[now].at, words[now].len);
      }
    };
    const watch = words.length ? setInterval(mark, 40) : undefined;
    try {
      if (device) await (audio as HTMLAudioElement & { setSinkId(id: string): Promise<void> }).setSinkId(device);
      await new Promise<void>((done, failed) => {
        release = done;
        audio.onended = () => done();
        audio.onerror = () => failed(new Error("a made sentence couldn't be played"));
        audio.onpause = () => {
          if (!paused) done(); // (stopped; a pause waits here)
        };
        audio.play().catch(failed);
      });
    } finally {
      clearInterval(watch);
      URL.revokeObjectURL(url);
      if (current === audio) current = null;
      if (madeNow === audio) madeNow = null;
    }
  };

  (async () => {
    for (let i = 0; i < parts.length && alive(); i++) {
      if (!parts[i].trim()) continue;
      // (paused between two parts: the next one waits)
      while (paused && alive()) await new Promise((r) => setTimeout(r, 100));
      if (!alive()) break;
      on.start(i);
      const device = routed() ? await chosenDevice() : null;
      if (!alive()) break;
      try {
        const mp3 = made ? await made(parts[i]).catch(() => null) : null;
        if (!alive()) break;
        if (mp3) await playMade(mp3, device, i);
        else if (builtIn() || device) await sayAsSound(i, device);
        else await sayOnWeb(i);
      } catch (e) {
        console.warn("reading aloud: a part failed", e);
        if (alive()) await sayOnWeb(i);
      }
    }
    if (alive()) reading++;
    on.done();
  })();
}

/** A made sentence that is playing now (see `readAloud`). */
let madeNow: HTMLAudioElement | null = null;
/** The speed was changed while a made sentence plays: it changes at once. */
export function setMadeRate(rate: number) {
  if (madeNow) madeNow.playbackRate = rate;
}

/** Whether anything is being said or is waiting to be said. */
export const isSpeaking = () => {
  const s = synth();
  return playing || queue.length > 0 || (!!s && (s.speaking || s.pending));
};

export const speechAvailable = () => synth() !== null || canRoute || haveBuiltIn;
