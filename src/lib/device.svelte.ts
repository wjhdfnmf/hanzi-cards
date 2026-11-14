// What kind of screen the app is on.
//
// `phone`: a narrow screen (an iPhone, or a desktop window made very narrow). It changes the
// layout only: one pane at a time, the side bar hiding itself.
// `ios`: running on an iPhone or iPad. It changes what is offered: the computer-only features
// (GPU libraries, downloaded speech models, choosing data folders) are left out there.

const phoneQuery = typeof window !== "undefined" ? window.matchMedia("(max-width: 640px)") : null;

export const ios =
  typeof navigator !== "undefined" &&
  (/iPhone|iPad|iPod/.test(navigator.userAgent) || (navigator.platform === "MacIntel" && navigator.maxTouchPoints > 1));

export const device = $state({
  phone: phoneQuery?.matches ?? false,
  /** The side bar, on a phone: shown, or slid away to give the page the whole width. */
  railOpen: true,
  /** Where a page with a list and its items is (Settings, Decks, Reading): at the list, the
   *  side bar always shows; inside an item it stays away, whatever the scrolling. Other pages
   *  (null) show it on a pull past the top. */
  level: null as "list" | "detail" | null,
});

/** Whether the side bar shows (on a phone). */
export function railShown(): boolean {
  if (!device.phone) return true;
  if (device.level === "list") return true;
  if (device.level === "detail") return false;
  return device.railOpen;
}

/** For a page with a list and its items: which of the two is showing. (Call while the
 *  component is being set up.) */
export function trackLevel(level: () => "list" | "detail") {
  $effect(() => {
    device.level = device.phone ? level() : null;
    return () => {
      device.level = null;
    };
  });
}

phoneQuery?.addEventListener("change", (e) => {
  device.phone = e.matches;
  device.railOpen = true;
});

/** Pages call this when they want the whole screen (a study session, a story being read). */
export function hideRail() {
  if (device.phone) device.railOpen = false;
}

export function showRail() {
  device.railOpen = true;
}
