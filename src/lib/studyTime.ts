// Time spent on the Reading and AI Tutor pages, for the Statistics page. (Time answering
// flashcards is recorded with each answer.) A second counts while the window is in front and
// the learner has done something recently; the total is sent to the app every half minute.
import { api } from "./api";

const FLUSH_MS = 30_000;
const EVENTS = ["pointerdown", "pointermove", "keydown", "wheel", "scroll"] as const;

/**
 * Start counting. `idleMs`: how long without a click, key or scroll before the learner is
 * taken to have left (longer where listening and speaking are the activity).
 * Returns the function that stops counting and sends what is left.
 */
export function trackStudyTime(kind: "reading" | "tutor", idleMs: number): () => void {
  let last = Date.now();
  let active = last;
  let pending = 0;
  const touch = () => (active = Date.now());
  for (const e of EVENTS) window.addEventListener(e, touch, { passive: true, capture: true });

  const flush = () => {
    const seconds = Math.floor(pending / 1000);
    if (seconds <= 0) return;
    pending -= seconds * 1000;
    api.logStudyTime(kind, seconds).catch(() => {}); // statistics only: never in the way
  };
  const tick = () => {
    const now = Date.now();
    const passed = now - last;
    last = now;
    // (a long gap between ticks means the computer slept: not study time)
    if (passed < 5000 && document.visibilityState === "visible" && document.hasFocus() && now - active < idleMs) {
      pending += passed;
    }
    if (pending >= FLUSH_MS) flush();
  };
  const timer = setInterval(tick, 1000);
  return () => {
    clearInterval(timer);
    tick();
    flush();
    for (const e of EVENTS) window.removeEventListener(e, touch, { capture: true });
  };
}
