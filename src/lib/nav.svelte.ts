export type View = "study" | "tutor" | "decks" | "reading" | "dictionary" | "stats" | "settings";

export const nav = $state({
  view: "study" as View,
  /** Set by "Study this deck" to preselect decks on the Study page. */
  studyDeckIds: null as number[] | null,
  /** Settings section to open (e.g. "speech" from the Study page); read once by Settings. */
  settingsSection: null as string | null,
  /** Start a session with the usual choices as soon as the Study page can (Siri asked). */
  autoStart: false,
});

export function goto(view: View, settingsSection: string | null = null) {
  nav.settingsSection = settingsSection;
  nav.view = view;
}

export function studyDeck(deckId: number) {
  nav.studyDeckIds = [deckId];
  nav.view = "study";
}
