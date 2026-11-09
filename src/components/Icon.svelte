<script lang="ts">
  // Minimal stroke icon set (24px grid).
  let { name, size = 18 }: { name: string; size?: number } = $props();

  const paths: Record<string, string> = {
    study: "M4 5h11a3 3 0 0 1 3 3v11H7a3 3 0 0 1-3-3V5Z M18 8h2v13H8 M8 9h6 M8 13h4",
    decks: "M12 3 3 8l9 5 9-5-9-5Z M3 13l9 5 9-5 M3 17.5l9 5 9-5",
    dictionary: "M5 4h10a4 4 0 0 1 4 4v12H9a4 4 0 0 1-4-4V4Z M5 16a4 4 0 0 1 4-4h10 M9 8h6",
    stats: "M4 20V10 M10 20V4 M16 20v-7 M22 20H2",
    settings:
      "M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6Z M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1Z",
    plus: "M12 5v14 M5 12h14",
    search: "M11 18a7 7 0 1 0 0-14 7 7 0 0 0 0 14Z M20 20l-4-4",
    chat: "M4 5h16v11H10l-6 4V5Z M8 9h8 M8 12h5",
    mic: "M12 3a3 3 0 0 0-3 3v5a3 3 0 0 0 6 0V6a3 3 0 0 0-3-3Z M5 11a7 7 0 0 0 14 0 M12 18v3",
    speaker: "M11 5 6 9H3v6h3l5 4V5Z M15.5 8.5a5 5 0 0 1 0 7 M18.5 5.5a9 9 0 0 1 0 13",
    edit: "M4 20h4L19 9l-4-4L4 16v4Z M13.5 6.5l4 4",
    trash: "M4 7h16 M10 11v6 M14 11v6 M6 7l1 13h10l1-13 M9 7V4h6v3",
    upload: "M12 16V4 M7 9l5-5 5 5 M4 20h16",
    download: "M12 4v12 M7 11l5 5 5-5 M4 20h16",
    close: "M6 6l12 12 M18 6 6 18",
    check: "M5 12l5 5 9-10",
    play: "M7 4v16l13-8L7 4Z",
    flame: "M12 22a7 7 0 0 0 7-7c0-4-3-6-4-10-2 2-3 4-3 6-1-1-2-2-2-4-2 2-5 5-5 8a7 7 0 0 0 7 7Z",
    globe: "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18Z M3 12h18 M12 3c2.5 2.5 3.5 5.5 3.5 9s-1 6.5-3.5 9c-2.5-2.5-3.5-5.5-3.5-9s1-6.5 3.5-9Z",
    book: "M4 5a2 2 0 0 1 2-2h14v16H6a2 2 0 0 0-2 2V5Z M4 19a2 2 0 0 1 2-2h14",
    arrow: "M5 12h14 M13 6l6 6-6 6",
    folder: "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7Z",
  };
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.8"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  <path d={paths[name] ?? ""} />
</svg>
