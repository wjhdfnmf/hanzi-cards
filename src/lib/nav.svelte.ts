export type View = "study" | "decks" | "dictionary" | "stats" | "settings";

export const nav = $state({
  view: "study" as View,
  /** Set by "Study this deck" to preselect decks on the Study page. */
  studyDeckIds: null as number[] | null,
});

export function goto(view: View) {
  nav.view = view;
}

export function studyDeck(deckId: number) {
  nav.studyDeckIds = [deckId];
  nav.view = "study";
}
