// Making the sound of every story at once (Settings › Audio): first what it would take is
// counted, and only then, when the learner says so, is it made.
//
// The state lives here, not in the Settings page, so that a making that is under way is
// still shown when the page is left and come back to.
import { listen } from "@tauri-apps/api/event";
import { api, errorText, type StoryAudioMade, type StoryAudioStatus } from "./api";
import { sentences } from "./reading";

export const bulk = $state({
  /** idle → counting → ready (counted) → making → ready */
  phase: "idle" as "idle" | "counting" | "ready" | "making",
  /** counting: stories read so far; making: sentences made so far */
  done: 0,
  total: 0,
  stories: 0,
  /** What is made and what is still to make, as counted. */
  status: null as StoryAudioStatus | null,
  error: "",
});

/** Every story's sentences, each once, as last counted. */
let parts: string[] = [];
/** Bumped to make a counting that is under way give up. */
let run = 0;

/** MiniMax's list price in US dollars for a million characters (its pricing page: the
 *  turbo models 60, the others 100). An estimate: the account's own plan may differ. */
export const pricePerMillion = (model: string) => (model.includes("turbo") ? 60 : 100);
export const costOf = (characters: number, model: string) => (characters * pricePerMillion(model)) / 1_000_000;

/** The choices (model, voice, speed, feeling) the count was made for. */
let countedFor = "";
/** Forget the count if it was made for other choices than these. */
export function keepCountFor(choices: string) {
  if (bulk.phase !== "idle" && choices !== countedFor) resetAll();
}

/** Forget the count. */
export function resetAll() {
  if (bulk.phase === "making") return;
  run++;
  parts = [];
  Object.assign(bulk, { phase: "idle", done: 0, total: 0, stories: 0, status: null, error: "" });
}

/** Read every story and count what its sound would take. Nothing is made, or sent anywhere. */
export async function countAll(choices: string) {
  if (bulk.phase === "counting" || bulk.phase === "making") return;
  countedFor = choices;
  const mine = ++run;
  Object.assign(bulk, { phase: "counting", done: 0, total: 0, stories: 0, status: null, error: "" });
  try {
    const stories = await api.listStories(null);
    if (mine !== run) return;
    bulk.total = stories.length;
    const all = new Set<string>();
    for (const s of stories) {
      // (the same sentences the Reading page reads, so the same files are found and made)
      const segs = await api.storySegments(s.id);
      if (mine !== run) return;
      for (const p of sentences(segs)) all.add(p.text);
      bulk.done++;
    }
    parts = [...all];
    const status = await api.storyAudioStatus(parts);
    if (mine !== run) return;
    Object.assign(bulk, { phase: "ready", stories: stories.length, status });
  } catch (e) {
    if (mine !== run) return;
    Object.assign(bulk, { phase: "idle", error: errorText(e) });
  }
}

/** Stop counting, or stop making after the sentence it is on. */
export function stopAll() {
  if (bulk.phase === "counting") resetAll();
  else if (bulk.phase === "making") api.storyAudioCancel().catch(() => {});
}

/** Make what was counted as missing. What was made before a stop or a failure is kept. */
export async function makeAll(): Promise<StoryAudioMade | null> {
  if (bulk.phase !== "ready" || !bulk.status || !parts.length) return null;
  Object.assign(bulk, { phase: "making", done: bulk.status.have, total: bulk.status.total, error: "" });
  const stop = await listen<{ done: number; total: number }>("story-audio", (e) => {
    if (bulk.phase === "making") Object.assign(bulk, e.payload);
  });
  try {
    const made = await api.storyAudioMake(parts);
    Object.assign(bulk, { phase: "ready", status: made.status, error: made.error ?? "" });
    return made;
  } catch (e) {
    Object.assign(bulk, { phase: "ready", error: errorText(e) });
    return null;
  } finally {
    stop();
  }
}
