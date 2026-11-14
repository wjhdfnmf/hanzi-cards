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
});

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
