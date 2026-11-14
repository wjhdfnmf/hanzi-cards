// Keeping this device the same as the learner's others: what the side bar's button and the
// console (components/Console.svelte) share.
import { api, errorText, type SyncChanges, type SyncReport, type SyncStatus } from "./api";

export const sync = $state({
  /** What is set up on this device (role "" = nothing). */
  status: null as SyncStatus | null,
  /** Is the other side there? Client: the server answered a ping. Server: it is listening. */
  light: "unknown" as "unknown" | "ok" | "bad",
  /** What the light means right now (shown on hover). */
  note: "",
  busy: false,
  /** Goes up when a sync brought something in: the pages reload their data. */
  epoch: 0,
  consoleOpen: false,
});

export async function refreshStatus(): Promise<SyncStatus | null> {
  try {
    const s = await api.syncStatus();
    sync.status = s;
    if (s.role === "server") {
      sync.light = s.online ? "ok" : "bad";
      sync.note = s.online ? `online · ${s.localAddress ?? "this computer"}:${s.port}` : "offline";
    } else if (!s.role) {
      sync.light = "unknown";
      sync.note = "";
    }
    return s;
  } catch {
    return null;
  }
}

/** Client: ask the server whether it is there. Server: is it listening? */
export async function ping() {
  const p = await api.syncPing();
  sync.light = p.ok ? "ok" : "bad";
  sync.note = p.ok ? (sync.status?.role === "client" ? `server answered in ${p.ms} ms` : sync.note) : p.error;
  return p;
}

const plural = (n: number, one: string, many = `${one}s`) => `${n.toLocaleString()} ${n === 1 ? one : many}`;

/** "3 cards, 14 answers" ("nothing" when empty). */
export function describe(c: SyncChanges): string {
  const parts = [
    c.decks && plural(c.decks, "deck"),
    c.cards && plural(c.cards, "card"),
    c.reviews && plural(c.reviews, "answer"),
    c.stories && plural(c.stories, "story", "stories"),
    c.removed && `${c.removed.toLocaleString()} removed`,
  ].filter(Boolean);
  return parts.length ? parts.join(", ") : "nothing";
}

const any = (c: SyncChanges) => c.decks + c.cards + c.reviews + c.stories + c.removed > 0;

export async function syncNow(): Promise<SyncReport> {
  sync.busy = true;
  try {
    const r = await api.syncNow();
    sync.light = "ok";
    if (any(r.received)) sync.epoch++;
    await refreshStatus();
    return r;
  } catch (e) {
    // (a server that can't be reached shows on the light too)
    if (sync.status?.role === "client") {
      sync.light = "bad";
      sync.note = errorText(e);
    }
    throw e;
  } finally {
    sync.busy = false;
  }
}

/** One line for after a sync from the side bar. */
export function summary(r: SyncReport): string {
  const [got, gave] = [any(r.received), any(r.sent)];
  if (!got && !gave) return "Already up to date.";
  return [got && `In: ${describe(r.received)}.`, gave && `Out: ${describe(r.sent)}.`].filter(Boolean).join(" ");
}
