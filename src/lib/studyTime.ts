// Time spent reading a story or talking with the AI tutor, for the Statistics page. (Time
// answering flashcards is recorded with each answer.) A second counts while there is something
// being studied (a story is open, a conversation is under way: not just the page being
// shown), the window is in front and the learner has done something recently. The total is
// sent to the app every half minute.
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api } from "./api";

// Whether the app's window can be looking at the learner: not minimized, not hidden, and the
// window in front. The system is asked (once a second, while something is being studied)
// because the page's own idea of this can lag: a minimized window's page may still think it
// is visible. A window that another window covers completely is not in front, since the
// covering window is; the system has no direct "is it covered" answer, so being in front
// is what stands for it. If the system can't be asked, the page's own checks decide alone.
let inFront = true;
function lookAtWindow() {
  const w = getCurrentWindow();
  Promise.all([w.isMinimized(), w.isVisible(), w.isFocused()])
    .then(([minimized, visible, focused]) => (inFront = !minimized && visible && focused))
    .catch(() => (inFront = true));
}

const FLUSH_MS = 30_000;
const EVENTS = ["pointerdown", "pointermove", "keydown", "wheel", "scroll"] as const;

/**
 * Start counting. `idleMs`: how long without a click, key or scroll before the learner is
 * taken to have left (longer where listening and speaking are the activity). `studying`:
 * whether there is anything being studied right now; asked every second.
 * Returns the function that stops counting and sends what is left.
 */
export function trackStudyTime(kind: "reading" | "tutor", idleMs: number, studying: () => boolean): () => void {
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
    const here = studying();
    if (here) lookAtWindow(); // (the answer is used from the next second on)
    if (passed < 5000 && here && inFront && document.visibilityState === "visible" && document.hasFocus() && now - active < idleMs) {
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
