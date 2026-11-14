<script lang="ts">
  import { onMount } from "svelte";
  import { trackStudyTime } from "../lib/studyTime";
  import { open, ask, save } from "@tauri-apps/plugin-dialog";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { api, errorText, type Deck, type DictEntry, type Segment, type Story, type StoryFolder, type StoryInfo } from "../lib/api";
  import CardEditor from "../components/CardEditor.svelte";
  import { readAloud, stopSpeaking } from "../lib/speech";
  import { voice } from "../lib/voice.svelte";
  import { t } from "../lib/i18n.svelte";
  import { settings, saveSetting } from "../lib/settings.svelte";
  import { showToast } from "../lib/toast.svelte";
  import Icon from "../components/Icon.svelte";
  import Modal from "../components/Modal.svelte";
  import Pinyin from "../components/Pinyin.svelte";
  import { hanziTones } from "../lib/pinyin";
  import { SIZE_MIN, SIZE_MAX, readingFont, clampSize } from "../lib/reading";
  import FontSelect from "../components/FontSelect.svelte";
  import StoryCover, { coverOf, LOOKS, type Look } from "../components/StoryCover.svelte";

  let folders = $state<StoryFolder[]>([]);
  let selected = $state<number | null>(null); // null = all stories
  let stories = $state<StoryInfo[]>([]);
  let loading = $state(true);

  const folder = $derived(folders.find((f) => f.id === selected) ?? null);
  const indent = (f: StoryFolder) => `${"  ".repeat(f.depth)}${f.name}`;

  // ---------- the open story ----------
  let story = $state<Story | null>(null);
  // raw: a long story is thousands of segments, only ever replaced whole
  let segments = $state.raw<Segment[] | null>(null);

  async function openStory(id: number) {
    stopReading();
    try {
      story = await api.getStory(id);
      segments = null;
      hover = null;
      remember();
      const mine = id;
      const segs = await api.storySegments(id);
      if (story?.id === mine) segments = segs;
    } catch (e) {
      showToast(errorText(e), "err");
    }
  }

  function closeStory() {
    stopReading();
    if (full) setFull(false);
    story = null;
    segments = null;
    hover = null;
    remember();
  }

  // Where you were (collection and story), so the page opens there next time.
  const LAST_KEY = "reading.last";
  function remember() {
    try {
      localStorage.setItem(LAST_KEY, JSON.stringify({ folder: selected, story: story?.id ?? null }));
    } catch {
      /* not essential */
    }
  }

  // ---------- text settings ----------
  const fontFamily = $derived(readingFont(settings.readingFont));
  const toneText = $derived(settings.toneColors && settings.readingToneColors);
  /** Settings › Reading: illustrated covers and a banner, or the plain list and title. */
  const look = $derived<Look | null>((LOOKS as readonly string[]).includes(settings.readingStyle) ? (settings.readingStyle as Look) : null);
  const books = $derived(look !== null);
  function setSize(n: number) {
    const v = clampSize(n);
    if (v !== settings.readingSize) saveSetting("readingSize", v);
  }

  // ---------- word lookup on hover ----------
  // One tooltip for the whole text (not one per word): placed next to the word under the pointer.
  let hover = $state<{ i: number; x: number; top: number; bottom: number } | null>(null);
  const hoverSeg = $derived(hover && segments ? segments[hover.i] : null);

  // The tooltip can hold a button (make a card of the word), so the pointer must be able to
  // reach it: leaving the word doesn't close it at once, and being over it keeps it open.
  let overTip = false;
  let tipTimer: ReturnType<typeof setTimeout> | undefined;
  function over(e: MouseEvent) {
    const el = (e.target as HTMLElement).closest<HTMLElement>("[data-i]");
    let next: typeof hover = null;
    if (el) {
      const i = Number(el.dataset.i);
      if (hover?.i === i) {
        clearTimeout(tipTimer);
        return;
      }
      const r = el.getBoundingClientRect();
      next = { i, x: r.left + r.width / 2, top: r.top, bottom: r.bottom };
    }
    clearTimeout(tipTimer);
    if (!hover) hover = next;
    else tipTimer = setTimeout(() => !overTip && (hover = next), 160);
  }
  function leaveText() {
    clearTimeout(tipTimer);
    tipTimer = setTimeout(() => !overTip && (hover = null), 200);
  }

  // ---------- a card from a word in the text ----------
  let decks = $state<Deck[]>([]);
  let adding = $state<{ hanzi: string; pinyin: string; meaning: string } | null>(null);
  async function addCard(e: DictEntry) {
    overTip = false;
    hover = null;
    try {
      decks = await api.listDecks();
      if (!decks.length) return showToast(t("reading.noDecks"), "err");
      adding = { hanzi: e.simplified, pinyin: e.pinyin, meaning: e.definitions.slice(0, 3).join("; ") };
    } catch (err) {
      showToast(errorText(err), "err");
    }
  }
  /** The text again, so the new card's word shows as "in your cards". */
  async function cardAdded() {
    const id = story?.id;
    if (id === undefined) return;
    const segs = await api.storySegments(id).catch(() => null);
    if (segs && story?.id === id) segments = segs;
  }

  // ---------- reading the story aloud ----------
  // The text is read a sentence at a time. The sentence being read is marked, and the word
  // too where the voice says which word it is on (the Windows voices do).
  let playing = $state<{ from: number; to: number; word: number | null } | null>(null);
  /** The story as sentences: the segments each one covers, and its text. */
  function sentences(segs: Segment[]): { from: number; to: number; text: string }[] {
    const out: { from: number; to: number; text: string }[] = [];
    let from = 0;
    segs.forEach((s, i) => {
      if (/[。！？!?；;…\n]/.test(s.text) || i === segs.length - 1) {
        const text = segs.slice(from, i + 1).map((x) => x.text).join("");
        if (/\p{L}/u.test(text)) out.push({ from, to: i, text });
        from = i + 1;
      }
    });
    return out;
  }
  /** The segment where the learner's selection in the story starts, if they have selected text. */
  function selectionStart(): number | null {
    const sel = window.getSelection();
    if (!sel || sel.isCollapsed || !sel.rangeCount) return null;
    const r = sel.getRangeAt(0);
    const art = document.querySelector("article.text");
    if (!art || !art.contains(r.startContainer)) return null;
    let n: Node | null = r.startContainer;
    if (n === art) n = art.childNodes[r.startOffset] ?? null;
    // a selection that starts at the very end of a word starts with what follows it
    else if (n.nodeType === Node.TEXT_NODE && r.startOffset >= (n.textContent ?? "").length) {
      let up: Node = n;
      while (up.parentNode && up.parentNode !== art && !up.nextSibling) up = up.parentNode;
      n = up.nextSibling ?? n;
    }
    if (!n) return null;
    const index = (el: Element | null | undefined) => {
      const hit = el?.closest<HTMLElement>("[data-i], [data-s]");
      return hit && art.contains(hit) ? Number(hit.dataset.i ?? hit.dataset.s) : null;
    };
    const inWord = index(n instanceof Element ? n : n.parentElement);
    if (inWord !== null) return inWord;
    // between words (punctuation, spaces): the next word
    for (let next = n.nextSibling; next; next = next.nextSibling) {
      if (next instanceof HTMLElement && (next.dataset.i || next.dataset.s)) return Number(next.dataset.i ?? next.dataset.s);
    }
    return null;
  }
  // Pressing the button can clear the selection before the click arrives, so it is read as
  // the press begins.
  let pressedAt: number | null = null;
  function play() {
    if (!segments) return;
    const segs = segments;
    let parts = sentences(segs);
    if (!parts.length) return;
    // with text selected, reading begins where the selection does (mid-sentence if need be)
    const from = pressedAt ?? selectionStart();
    pressedAt = null;
    if (from !== null) {
      const p = parts.findIndex((x) => x.to >= from);
      if (p >= 0) {
        parts = parts.slice(p);
        if (from > parts[0].from) {
          const text = segs.slice(from, parts[0].to + 1).map((x) => x.text).join("");
          if (/\p{L}/u.test(text)) parts[0] = { from, to: parts[0].to, text };
          else parts = parts.slice(1);
        }
        if (!parts.length) return;
        window.getSelection()?.removeAllRanges(); // (it would cover the marking of what is being read)
      }
    }
    readAloud(
      parts.map((p) => p.text),
      {
        start: (i) => {
          playing = { from: parts[i].from, to: parts[i].to, word: null };
          document.querySelector(`article [data-i="${parts[i].from}"], article [data-s="${parts[i].from}"]`)?.scrollIntoView({ block: "nearest", behavior: "smooth" });
        },
        word: (i, at) => {
          // which segment holds the character the voice has reached
          let seen = 0;
          for (let k = parts[i].from; k <= parts[i].to; k++) {
            seen += segs[k].text.length;
            if (at < seen) {
              playing = { from: parts[i].from, to: parts[i].to, word: k };
              return;
            }
          }
        },
        done: () => (playing = null),
      },
    );
  }
  function stopReading() {
    if (playing) stopSpeaking();
    playing = null;
  }

  // ---------- bookmarks ----------
  let onlyBookmarked = $state(false);
  async function toggleBookmark() {
    if (!story) return;
    const id = story.id;
    const on = !story.bookmarked;
    try {
      await api.bookmarkStory(id, on);
      story = { ...story, bookmarked: on };
      stories = stories.map((s) => (s.id === id ? { ...s, bookmarked: on } : s));
    } catch (e) {
      showToast(errorText(e), "err");
    }
  }

  // ---------- full screen: the story alone, over the whole screen ----------
  let full = $state(false);
  function setFull(on: boolean) {
    full = on;
    hover = null;
    getCurrentWindow().setFullscreen(on).catch(() => {}); // (in a browser preview there is no window to change)
  }

  /** Keep the tooltip in the window: below the word, or above it when there's no room. */
  function place(node: HTMLElement, h: { x: number; top: number; bottom: number }) {
    const set = (h: { x: number; top: number; bottom: number }) => {
      const pad = 8;
      const w = node.offsetWidth;
      const ht = node.offsetHeight;
      const left = Math.min(window.innerWidth - pad - w, Math.max(pad, h.x - w / 2));
      const below = h.bottom + 6;
      const top = below + ht > window.innerHeight - pad ? Math.max(pad, h.top - 6 - ht) : below;
      node.style.left = `${left}px`;
      node.style.top = `${top}px`;
    };
    set(h);
    return { update: set };
  }

  // ---------- resizing the collection list ----------
  const SIDE_KEY = "reading.sideWidth";
  const SIDE_DEFAULT = 240;
  let sideWidth = $state(
    (() => {
      try {
        return Number(JSON.parse(localStorage.getItem(SIDE_KEY) ?? "null")?.w) || SIDE_DEFAULT;
      } catch {
        return SIDE_DEFAULT;
      }
    })(),
  );
  function saveSide() {
    try {
      localStorage.setItem(SIDE_KEY, JSON.stringify({ w: sideWidth }));
    } catch {
      /* not essential */
    }
  }
  function dragSide(e: PointerEvent) {
    e.preventDefault();
    const handle = e.currentTarget as HTMLElement;
    const x0 = e.clientX;
    const start = sideWidth;
    const max = Math.max(260, window.innerWidth * 0.6);
    handle.setPointerCapture(e.pointerId);
    document.body.classList.add("col-resizing");
    const move = (ev: PointerEvent) => (sideWidth = Math.round(Math.min(max, Math.max(160, start + ev.clientX - x0))));
    const up = () => {
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", up);
      handle.removeEventListener("pointercancel", up);
      document.body.classList.remove("col-resizing");
      saveSide();
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", up);
    handle.addEventListener("pointercancel", up);
  }

  // ---------- collection tree ----------
  const COLLAPSED_KEY = "reading.collapsed";
  let collapsed = $state<number[]>(
    (() => {
      try {
        const saved = JSON.parse(localStorage.getItem(COLLAPSED_KEY) ?? "[]");
        return Array.isArray(saved) ? saved.filter((x) => typeof x === "number") : [];
      } catch {
        return [];
      }
    })(),
  );
  function toggleCollapsed(id: number) {
    collapsed = collapsed.includes(id) ? collapsed.filter((x) => x !== id) : [...collapsed, id];
    try {
      localStorage.setItem(COLLAPSED_KEY, JSON.stringify(collapsed));
    } catch {
      /* not essential */
    }
  }
  const visibleFolders = $derived.by(() => {
    const parent = new Map(folders.map((f) => [f.id, f.parentId]));
    return folders.filter((f) => {
      for (let p = f.parentId; p !== null && p !== undefined; p = parent.get(p) ?? null) {
        if (collapsed.includes(p)) return false;
      }
      return true;
    });
  });
  function subtree(id: number): Set<number> {
    const out = new Set([id]);
    for (const f of folders) if (f.parentId !== null && out.has(f.parentId)) out.add(f.id);
    return out;
  }
  let allCount = $state(0); // every story, for "All stories"

  // time with a story open counts as study time on the Statistics page (not time spent
  // looking through the library)
  onMount(() => trackStudyTime("reading", 180_000, () => story !== null));

  // ---------- loading ----------
  onMount(async () => {
    let last: { folder: number | null; story: number | null } = { folder: null, story: null };
    try {
      last = { ...last, ...JSON.parse(localStorage.getItem(LAST_KEY) ?? "null") };
    } catch {
      /* start at the top */
    }
    await loadFolders();
    if (last.folder !== null && folders.some((f) => f.id === last.folder)) selected = last.folder;
    await loadStories();
    if (last.story !== null) {
      try {
        await api.getStory(last.story); // still there?
        await openStory(last.story);
      } catch {
        remember();
      }
    }
  });

  async function loadFolders() {
    try {
      folders = await api.listStoryFolders();
    } catch (e) {
      showToast(errorText(e), "err");
    }
  }

  let request = 0;
  async function loadStories() {
    const mine = ++request;
    loading = true;
    try {
      const rows = await api.listStories(selected);
      if (mine !== request) return;
      stories = rows;
      allCount = selected === null ? rows.length : await api.storyTotal();
    } catch (e) {
      showToast(errorText(e), "err");
    } finally {
      if (mine === request) loading = false;
    }
  }
  async function refresh() {
    await loadFolders();
    await loadStories();
  }

  function choose(id: number | null) {
    selected = id;
    story = null;
    segments = null;
    hover = null;
    remember();
    loadStories();
  }

  // ---------- collections: add, edit, delete ----------
  let folderForm = $state<{ id: number | null; name: string; description: string; parentId: number | null } | null>(
    null,
  );

  async function saveFolder() {
    if (!folderForm) return;
    try {
      if (folderForm.id === null) {
        const id = await api.createStoryFolder(folderForm.name, folderForm.description, folderForm.parentId);
        if (folderForm.parentId !== null) collapsed = collapsed.filter((x) => x !== folderForm!.parentId);
        folderForm = null;
        await loadFolders();
        choose(id);
      } else {
        await api.updateStoryFolder(folderForm.id, folderForm.name, folderForm.description, folderForm.parentId);
        folderForm = null;
        await refresh();
      }
    } catch (e) {
      showToast(errorText(e), "err");
    }
  }

  async function removeFolder() {
    if (!folder) return;
    const subs = subtree(folder.id).size - 1;
    const message = subs
      ? t("reading.deleteFolderConfirmNested", { name: folder.path, d: subs, n: folder.totalStories })
      : t("reading.deleteFolderConfirm", { name: folder.path, n: folder.storyCount });
    if (!(await ask(message, { title: t("reading.deleteFolder"), kind: "warning" }))) return;
    try {
      await api.deleteStoryFolder(folder.id);
      await loadFolders();
      choose(null);
    } catch (e) {
      showToast(errorText(e), "err");
    }
  }

  // ---------- stories: add, edit, delete, import ----------
  let storyForm = $state<{ id: number | null; title: string; body: string; folderId: number | null } | null>(null);

  async function saveStory() {
    if (!storyForm) return;
    try {
      const f = storyForm;
      if (f.id === null) {
        const id = await api.createStory(f.folderId, f.title, f.body);
        storyForm = null;
        await refresh();
        await openStory(id);
      } else {
        await api.updateStory(f.id, f.folderId, f.title, f.body);
        storyForm = null;
        await refresh();
        await openStory(f.id);
      }
    } catch (e) {
      showToast(errorText(e), "err");
    }
  }

  async function removeStory() {
    if (!story) return;
    if (!(await ask(t("reading.deleteStoryConfirm", { name: story.title }), { title: t("reading.deleteStory"), kind: "warning" })))
      return;
    try {
      await api.deleteStory(story.id);
      closeStory();
      await refresh();
    } catch (e) {
      showToast(errorText(e), "err");
    }
  }

  let importMenu = $state(false);

  // ---------- export: files to keep or to share (the same files the import reads) ----------

  /** The open story as a text file. */
  async function exportStory() {
    if (!story) return;
    const name = story.title.replace(/[\\/:*?"<>|]/g, "_").trim() || "story";
    const path = await save({ defaultPath: `${name}.txt`, filters: [{ name: t("reading.importFiles"), extensions: ["txt"] }] });
    if (!path) return;
    try {
      await api.exportStory(story.id, path);
      showToast(t("reading.exportedStory", { path }));
    } catch (e) {
      showToast(errorText(e), "err");
    }
  }

  /** The collection shown (or every story) as a new folder inside one the learner picks. */
  async function exportCollection() {
    const into = await open({ directory: true, multiple: false, title: t("reading.exportInto") });
    if (!into) return;
    try {
      const r = await api.exportStoryFolder(selected, into as string);
      showToast(t("reading.exported", { s: r.stories, path: r.path }));
    } catch (e) {
      showToast(errorText(e), "err");
    }
  }

  /** A folder: it becomes a collection here, its subfolders collections inside it. */
  async function importFolder() {
    importMenu = false;
    const path = await open({ directory: true, multiple: false });
    if (!path) return;
    try {
      const r = await api.importStoryFolder(selected, path as string);
      showToast(t("reading.importedFolder", { s: r.stories, c: r.collections, k: r.skipped }));
      await refresh();
    } catch (e) {
      showToast(errorText(e), "err");
    }
  }

  // Key vocabulary lines: "word – pinyin – meaning"
  const vocab = $derived(
    (story?.vocab ?? "")
      .split("\n")
      .map((l) => l.split(/\s+[–—-]\s+/))
      .filter((p) => p[0]?.trim())
      .map(([word, py = "", ...meaning]) => ({ word: word.trim(), pinyin: py.trim(), meaning: meaning.join(" – ").trim() })),
  );
  let showVocab = $state(true);

  async function importFiles() {
    importMenu = false;
    const picked = await open({ multiple: true, filters: [{ name: t("reading.importFiles"), extensions: ["txt"] }] });
    if (!picked) return;
    const paths = Array.isArray(picked) ? picked : [picked];
    if (!paths.length) return;
    try {
      const n = await api.importStories(selected, paths as string[]);
      showToast(t("reading.imported", { n }));
      await refresh();
    } catch (e) {
      showToast(errorText(e), "err");
    }
  }

  const folderName = (id: number | null) => (id === null ? "" : (folders.find((f) => f.id === id)?.path ?? ""));
</script>

{#snippet storyTools(corner: boolean)}
  {#if story}
  <div class="tools" class:corner>
    <button
      class="btn ghost icon mark"
      class:on={story.bookmarked}
      title={t(story.bookmarked ? "reading.unbookmark" : "reading.bookmark")}
      aria-label={t(story.bookmarked ? "reading.unbookmark" : "reading.bookmark")}
      aria-pressed={story.bookmarked}
      onclick={toggleBookmark}><span aria-hidden="true">{story.bookmarked ? "★" : "☆"}</span></button
    >
    <button
      class="btn"
      class:primary={!playing}
      disabled={!segments || !voice.available}
      onpointerdown={() => (pressedAt = playing ? null : selectionStart())}
      title={voice.available ? t("reading.playHint") : t("speech.noVoice")}
      onclick={() => (playing ? stopReading() : play())}
    >
      <Icon name={playing ? "close" : "speaker"} size={15} />{playing ? t("reading.stop") : t("reading.play")}
    </button>
    <button class="btn ghost" onclick={() => setFull(!full)} title={t("reading.fullScreenHint")}>
      {full ? t("reading.exitFull") : t("reading.fullScreen")}
    </button>
    <button
      class="btn ghost icon"
      title={t("reading.editStory")}
      aria-label={t("reading.editStory")}
      onclick={() => story && (storyForm = { id: story.id, title: story.title, body: story.body, folderId: story.folderId })}
    >
      <Icon name="edit" />
    </button>
    <button class="btn ghost icon" title={t("reading.exportStory")} aria-label={t("reading.exportStory")} onclick={exportStory}>
      <Icon name="download" />
    </button>
    <button class="btn ghost icon" title={t("reading.deleteStory")} aria-label={t("reading.deleteStory")} onclick={removeStory}>
      <Icon name="trash" />
    </button>
  </div>
  {/if}
{/snippet}

<div class="layout" class:full style="--side:{sideWidth}px">
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="side-handle"
    title={t("decks.resizeHint")}
    onpointerdown={dragSide}
    ondblclick={() => {
      sideWidth = SIDE_DEFAULT;
      saveSide();
    }}
  ></div>
  <aside class="folderlist">
    <div class="fl-head">
      <h2>{t("reading.title")}</h2>
      <button
        class="btn ghost icon"
        title={t("reading.newFolder")}
        aria-label={t("reading.newFolder")}
        onclick={() => (folderForm = { id: null, name: "", description: "", parentId: null })}
      >
        <Icon name="plus" />
      </button>
    </div>
    <button class="folder" class:on={selected === null} onclick={() => choose(null)}>
      <span class="fname">{t("reading.allStories")}</span>
      <span class="faint small">{allCount}</span>
    </button>
    {#each visibleFolders as f (f.id)}
      <div class="folderrow" style="padding-left:{f.depth * 14}px">
        {#if f.childCount}
          <button
            class="twisty"
            class:open={!collapsed.includes(f.id)}
            aria-label={collapsed.includes(f.id) ? t("reading.expand") : t("reading.collapse")}
            aria-expanded={!collapsed.includes(f.id)}
            onclick={() => toggleCollapsed(f.id)}>▸</button
          >
        {:else}
          <span class="twisty-space"></span>
        {/if}
        <button class="folder" class:on={selected === f.id} onclick={() => choose(f.id)}>
          <span class="fname">{f.name}</span>
          <span class="faint small">{f.totalStories}</span>
        </button>
      </div>
    {/each}
  </aside>

  <section class="main">
    <!-- text settings, always at the top of the page -->
    <div class="textbar" role="toolbar" aria-label={t("reading.textSettings")}>
      <label class="tb-field">
        <span class="small muted">{t("reading.font")}</span>
        <FontSelect />
      </label>
      <div class="tb-field">
        <span class="small muted">{t("reading.size")}</span>
        <button
          class="btn ghost icon sz"
          title={t("reading.smaller")}
          aria-label={t("reading.smaller")}
          disabled={settings.readingSize <= SIZE_MIN}
          onclick={() => setSize(settings.readingSize - 2)}><span class="a small-a">A</span></button
        >
        <input
          type="range"
          min={SIZE_MIN}
          max={SIZE_MAX}
          step="1"
          value={settings.readingSize}
          oninput={(e) => (settings.readingSize = Number(e.currentTarget.value))}
          onchange={(e) => setSize(Number(e.currentTarget.value))}
          aria-label={t("reading.size")}
        />
        <button
          class="btn ghost icon sz"
          title={t("reading.larger")}
          aria-label={t("reading.larger")}
          disabled={settings.readingSize >= SIZE_MAX}
          onclick={() => setSize(settings.readingSize + 2)}><span class="a big-a">A</span></button
        >
        <span class="small muted num">{settings.readingSize}px</span>
      </div>
      <span class="sample han" style="font-family:{fontFamily}">汉字阅读</span>
      {#if full && story}{@render storyTools(true)}{/if}
    </div>

    {#if story}
      <div class="head">
        <div class="title">
          <button class="btn ghost back" onclick={closeStory}>
            <span class="flip"><Icon name="arrow" size={16} /></span>{folder?.name ?? t("reading.allStories")}
          </button>
        </div>
        {#if !full}{@render storyTools(false)}{/if}
      </div>
      {#if look}
        {@const about = stories.find((x) => x.id === story?.id)?.preview ?? story.body.slice(0, 160)}
        {@const cv = coverOf(story.title, about, look)}
        <header class="banner look-{look}" style="--rs:{settings.readingSize}px; --cv-hi:{cv.hi}; --cv-ink:{cv.ink}">
          <StoryCover title={story.title} text={about} {look} band />
          <h1 class="han story-title">{story.title}</h1>
          {#if story.folderId !== null}<p class="small banner-sub">{folderName(story.folderId)}</p>{/if}
        </header>
        {:else}
        <h1 class="han plain-title">{story.title}</h1>
        {#if story.folderId !== null && story.folderId !== selected}
          <p class="small faint plain-sub">{folderName(story.folderId)}</p>
        {/if}
      {/if}

      {#if vocab.length}
        <section class="vocab card-surface">
          <button class="vocab-head" aria-expanded={showVocab} onclick={() => (showVocab = !showVocab)}>
            <span class="twisty" class:open={showVocab} aria-hidden="true">▸</span>
            <span>{t("reading.keyVocab")}</span>
            <small class="faint">{vocab.length}</small>
          </button>
          {#if showVocab}
            <ul class="vocab-list">
              {#each vocab as v, i (i)}
                <li>
                  <span class="vw han" lang="zh-CN">{v.word}</span>
                  <span class="vp"><Pinyin text={v.pinyin} /></span>
                  <span class="vm muted">{v.meaning}</span>
                </li>
              {/each}
            </ul>
          {/if}
        </section>
      {/if}
      <!-- svelte-ignore a11y_mouse_events_have_key_events -->
      <article
        class="text card-surface"
        lang="zh-CN"
        style="font-family:{fontFamily}; font-size:{settings.readingSize}px; line-height:{settings.readingLineHeight}"
        onmouseover={over}
        onmouseleave={leaveText}
      >
        {#if segments}
          {#each segments as s, i (i)}{#if s.entry}<span
                class="w"
                class:on={hover?.i === i}
                class:said={playing !== null && i >= playing.from && i <= playing.to}
                class:now={playing?.word === i}
                data-i={i}
                >{#if toneText}{#each hanziTones(s.text, s.entry.pinyin) as c, k (k)}<span
                      class={c.tone ? `tone-${c.tone}` : ""}>{c.ch}</span
                    >{/each}{:else}{s.text}{/if}</span
              >{:else if playing !== null && i >= playing.from && i <= playing.to}<span class="said" data-s={i}>{s.text}</span
              >{:else}{s.text}{/if}{/each}
        {:else}
          {story.body}
        {/if}
      </article>
    {:else}
      <div class="head">
        <div class="title">
          {#if folder && folder.depth > 0}
            <p class="crumbs small faint">{folder.path.split(" › ").slice(0, -1).join(" › ")}</p>
          {/if}
          <h1>{folder?.name ?? t("reading.allStories")}</h1>
          {#if folder?.description}<p class="muted">{folder.description}</p>{/if}
        </div>
        <div class="tools">
          {#if folder}
            <button
              class="btn ghost icon"
              title={t("reading.editFolder")}
              aria-label={t("reading.editFolder")}
              onclick={() =>
                (folderForm = { id: folder.id, name: folder.name, description: folder.description, parentId: folder.parentId })}
            >
              <Icon name="edit" />
            </button>
            <button class="btn ghost icon" title={t("reading.deleteFolder")} aria-label={t("reading.deleteFolder")} onclick={removeFolder}>
              <Icon name="trash" />
            </button>
            <button
              class="btn ghost icon"
              title={t("reading.newSubfolder")}
              aria-label={t("reading.newSubfolder")}
              onclick={() => (folderForm = { id: null, name: "", description: "", parentId: folder.id })}
            >
              <Icon name="folder" />
            </button>
          {/if}
          <div class="menuwrap">
            <button class="btn" aria-haspopup="menu" aria-expanded={importMenu} onclick={() => (importMenu = !importMenu)}>
              <Icon name="upload" size={16} />{t("reading.import")}
            </button>
            {#if importMenu}
              <div class="menu card-surface" role="menu">
                <button role="menuitem" onclick={importFiles} title={t("reading.importHint")}>{t("reading.importFilesItem")}</button>
                <button role="menuitem" onclick={importFolder} title={t("reading.importFolderHint")}>{t("reading.importFolder")}</button>
              </div>
            {/if}
          </div>
          <button class="btn" disabled={!stories.length && !folder?.totalStories} title={t("reading.exportHint")} onclick={exportCollection}>
            <Icon name="download" size={16} />{t("reading.export")}
          </button>
          <button class="btn primary" onclick={() => (storyForm = { id: null, title: "", body: "", folderId: selected })}>
            <Icon name="plus" size={16} />{t("reading.addStory")}
          </button>
        </div>
      </div>

      {#if !loading && stories.length === 0}
        <div class="empty card-surface">
          <Icon name="book" size={28} />
          <p class="muted">{t("reading.empty")}</p>
        </div>
      {:else}
        {#if onlyBookmarked || stories.some((s) => s.bookmarked)}
          <label class="only small muted">
            <input type="checkbox" bind:checked={onlyBookmarked} />
            {t("reading.onlyBookmarked", { n: stories.filter((s) => s.bookmarked).length })}
          </label>
        {/if}
        {#if look}
          <div class="shelf">
            {#each onlyBookmarked ? stories.filter((s) => s.bookmarked) : stories as s (s.id)}
              {@const cv = coverOf(s.title, s.preview, look)}
              <button class="book" onclick={() => openStory(s.id)}>
                <span class="cover look-{look}" style="--cv-hi:{cv.hi}; --cv-ink:{cv.ink}">
                  <StoryCover title={s.title} text={s.preview} {look} />
                  {#if s.bookmarked}<span class="ribbon" title={t("reading.bookmarked")}>★</span>{/if}
                  <span class="plate han" class:t2={s.title.length > 4} class:t3={s.title.length > 8} class:t4={s.title.length > 16}
                    >{s.title}</span
                  >
                </span>
                <span class="st-meta small faint">
                  {t("reading.chars", { n: s.chars })}{#if s.folderId !== null && s.folderId !== selected}
                    · {folderName(s.folderId)}{/if}
                </span>
              </button>
            {/each}
          </div>
        {:else}
          <div class="list">
            {#each onlyBookmarked ? stories.filter((s) => s.bookmarked) : stories as s (s.id)}
              <button class="story card-surface" onclick={() => openStory(s.id)}>
                <span class="st-title han">{#if s.bookmarked}<span class="star" title={t("reading.bookmarked")}>★</span> {/if}{s.title}</span>
                <span class="st-preview han muted">{s.preview}</span>
                <span class="small faint">
                  {t("reading.chars", { n: s.chars })}{#if s.folderId !== null && s.folderId !== selected}
                    · {folderName(s.folderId)}{/if}
                </span>
              </button>
            {/each}
          </div>
        {/if}
      {/if}
    {/if}
  </section>
</div>

{#if hover && hoverSeg?.entry}
  {@const e = hoverSeg.entry}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="tip"
    use:place={hover}
    onmouseenter={() => {
      overTip = true;
      clearTimeout(tipTimer);
    }}
    onmouseleave={() => {
      overTip = false;
      hover = null;
    }}
  >
    <div class="tip-head">
      <span class="tip-hz han" lang="zh-CN">{e.simplified}{#if e.traditional !== e.simplified}<span class="trad"
            >（{e.traditional}）</span
          >{/if}</span>
      {#if e.inCards}<span class="badge review">{t("reading.inCards")}</span>{/if}
    </div>
    <div class="tip-py"><Pinyin text={e.pinyin} /></div>
    <div class="tip-defs">{e.definitions.slice(0, 5).join("; ")}</div>
    {#if !e.inCards}
      <button class="btn small tip-add" onclick={() => addCard(e)}><Icon name="plus" size={14} />{t("reading.addCard")}</button>
    {/if}
  </div>
{/if}

{#if adding}
  <CardEditor card={null} {decks} deckId={null} prefill={adding} onclose={() => (adding = null)} onsaved={cardAdded} />
{/if}

<svelte:window
  onkeydown={(e) => {
    if (e.key === "Escape" && full && !adding && !storyForm) setFull(false);
  }}
  onscroll={() => (hover = null)}
  onresize={() => (hover = null)}
  onclick={(e) => {
    if (importMenu && !(e.target as HTMLElement).closest(".menuwrap")) importMenu = false;
  }}
/>

{#if folderForm}
  {@const blocked = folderForm.id !== null ? subtree(folderForm.id) : new Set<number>()}
  <Modal
    title={folderForm.id === null ? t("reading.newFolder") : t("reading.editFolder")}
    onclose={() => (folderForm = null)}
    width={440}
  >
    <form
      class="form"
      onsubmit={(e) => {
        e.preventDefault();
        saveFolder();
      }}
    >
      <label class="field">
        <span>{t("reading.folderName")}</span>
        <!-- svelte-ignore a11y_autofocus -->
        <input type="text" bind:value={folderForm.name} autofocus />
      </label>
      <label class="field">
        <span>{t("decks.parent")}</span>
        <select bind:value={folderForm.parentId}>
          <option value={null}>{t("decks.topLevel")}</option>
          {#each folders.filter((f) => !blocked.has(f.id)) as f (f.id)}<option value={f.id}>{indent(f)}</option>{/each}
        </select>
      </label>
      <label class="field">
        <span>{t("decks.description")}</span>
        <input type="text" bind:value={folderForm.description} />
      </label>
      <div class="formbtns">
        <button type="button" class="btn ghost" onclick={() => (folderForm = null)}>{t("common.cancel")}</button>
        <button type="submit" class="btn primary" disabled={!folderForm.name.trim()}>{t("common.save")}</button>
      </div>
    </form>
  </Modal>
{/if}

{#if storyForm}
  <Modal
    title={storyForm.id === null ? t("reading.addStory") : t("reading.editStory")}
    onclose={() => (storyForm = null)}
    width={680}
  >
    <form
      class="form"
      onsubmit={(e) => {
        e.preventDefault();
        saveStory();
      }}
    >
      <label class="field">
        <span>{t("reading.storyTitle")}</span>
        <!-- svelte-ignore a11y_autofocus -->
        <input type="text" bind:value={storyForm.title} autofocus />
      </label>
      <label class="field">
        <span>{t("reading.folder")}</span>
        <select bind:value={storyForm.folderId}>
          <option value={null}>{t("reading.noFolder")}</option>
          {#each folders as f (f.id)}<option value={f.id}>{indent(f)}</option>{/each}
        </select>
      </label>
      <label class="field">
        <span>{t("reading.storyText")}</span>
        <textarea class="han" rows="14" bind:value={storyForm.body} placeholder={t("reading.storyTextHint")}></textarea>
      </label>
      <div class="formbtns">
        <button type="button" class="btn ghost" onclick={() => (storyForm = null)}>{t("common.cancel")}</button>
        <button type="submit" class="btn primary" disabled={!storyForm.title.trim() || !storyForm.body.trim()}
          >{t("common.save")}</button
        >
      </div>
    </form>
  </Modal>
{/if}

<style>
  .layout {
    display: grid;
    grid-template-columns: var(--side) minmax(0, 1fr);
    height: 100%;
    position: relative;
  }
  .side-handle {
    position: absolute;
    top: 0;
    bottom: 0;
    left: calc(var(--side) - 4px);
    width: 8px;
    cursor: col-resize;
    z-index: 5;
    touch-action: none;
  }
  .side-handle:hover,
  .side-handle:active {
    background: linear-gradient(to right, transparent 3px, var(--accent) 3px, var(--accent) 5px, transparent 5px);
  }
  .folderlist {
    border-right: 1px solid var(--border);
    padding: 22px 12px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .fl-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 6px 10px 10px;
  }
  .folder {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    border: none;
    background: transparent;
    padding: 8px 10px;
    border-radius: 8px;
    text-align: left;
  }
  .folder:hover {
    background: var(--surface-2);
  }
  .folder.on {
    background: var(--surface-3);
    font-weight: 600;
  }
  .fname {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .folderrow {
    display: flex;
    align-items: center;
    gap: 2px;
  }
  .folderrow .folder {
    flex: 1;
    min-width: 0;
  }
  .twisty,
  .twisty-space {
    flex: 0 0 18px;
    width: 18px;
  }
  .twisty {
    border: none;
    background: transparent;
    padding: 0;
    color: var(--text-3);
    font-size: 12px;
    line-height: 1;
    transition: transform 0.12s;
  }
  .twisty.open {
    transform: rotate(90deg);
  }

  .main {
    --pad: 28px;
    overflow-y: auto;
    min-width: 0;
    padding: 0 var(--pad) 28px;
  }
  .textbar {
    position: sticky;
    top: 0;
    z-index: 4;
    display: flex;
    align-items: center;
    gap: 22px;
    flex-wrap: wrap;
    margin: 0 calc(-1 * var(--pad)) 18px;
    padding: 10px 28px;
    background: var(--bg);
    border-bottom: 1px solid var(--border);
  }
  .tb-field {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .tb-field :global(select) {
    width: auto;
  }
  .tb-field input[type="range"] {
    width: 140px;
    accent-color: var(--accent);
  }
  .sz {
    width: 30px;
    height: 30px;
    padding: 0;
  }
  .a {
    font-weight: 650;
    line-height: 1;
  }
  .small-a {
    font-size: 12px;
  }
  .big-a {
    font-size: 18px;
  }
  .num {
    font-variant-numeric: tabular-nums;
    min-width: 36px;
  }
  .sample {
    margin-left: auto;
    font-size: 20px;
    color: var(--text-2);
  }

  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 16px;
    margin-bottom: 14px;
    flex-wrap: wrap;
  }
  .tools {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }
  .crumbs {
    margin: 0 0 2px;
  }
  .back {
    margin: 0 0 6px -10px;
    color: var(--text-2);
  }
  .flip {
    display: inline-flex;
    transform: scaleX(-1);
  }
  .menuwrap {
    position: relative;
  }
  .menu {
    position: absolute;
    top: calc(100% + 4px);
    right: 0;
    z-index: 20;
    display: flex;
    flex-direction: column;
    padding: 4px;
    min-width: 180px;
    box-shadow: var(--shadow-lg);
  }
  .menu button {
    border: none;
    background: transparent;
    text-align: left;
    padding: 8px 10px;
    border-radius: 6px;
    white-space: nowrap;
  }
  .menu button:hover {
    background: var(--surface-2);
  }
  .vocab {
    max-width: 52em;
    margin-bottom: 12px;
    padding: 10px 16px;
  }
  .vocab-head {
    display: flex;
    align-items: baseline;
    gap: 8px;
    border: none;
    background: none;
    padding: 0;
    font: inherit;
    font-weight: 600;
    color: inherit;
    cursor: pointer;
  }
  .vocab-head .twisty {
    display: inline-block;
    width: 12px;
    color: var(--text-3);
    font-size: 12px;
    font-weight: normal;
    transition: transform 0.12s;
  }
  .vocab-head .twisty.open {
    transform: rotate(90deg);
  }
  .vocab-list {
    list-style: none;
    margin: 8px 0 2px;
    padding: 0;
    display: grid;
    grid-template-columns: max-content max-content minmax(0, 1fr);
    gap: 4px 14px;
  }
  .vocab-list li {
    display: contents;
  }
  .vw {
    font-size: 18px;
  }
  .vp {
    font-weight: 600;
    white-space: nowrap;
  }
  .text {
    padding: 28px 32px;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    max-width: 52em;
  }
  .w {
    border-radius: 4px;
    cursor: help;
  }
  .w.on {
    background: var(--accent-soft);
    box-shadow: 0 0 0 2px var(--accent-soft);
  }

  .empty {
    padding: 48px;
    text-align: center;
    color: var(--text-3);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
  }
  /* the plain library: a list of titles with their opening lines */
  .list {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: 12px;
  }
  .story {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
    padding: 14px 16px;
    text-align: left;
    border: 1px solid var(--border);
    cursor: pointer;
    min-width: 0;
  }
  .story:hover {
    border-color: var(--text-3);
  }
  .st-title {
    font-size: 17px;
    font-weight: 600;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .st-preview {
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    font-size: 14px;
    line-height: 1.5;
  }
  .plain-title {
    margin-bottom: 12px;
    text-decoration: none !important; /* (themes underline page titles; a story's title is only bold) */
  }
  .plain-title:has(+ .plain-sub) {
    margin-bottom: 2px;
  }
  .plain-sub {
    margin: 0 0 12px;
  }
  /* the library: a shelf of book covers */
  .shelf {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 26px 22px;
    padding: 6px 2px 20px;
  }
  .book {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 0;
    border: none;
    background: none;
    text-align: left;
    cursor: pointer;
    min-width: 0;
    max-width: 200px;
  }
  .cover {
    position: relative;
    display: block;
    aspect-ratio: 3 / 4;
    border-radius: 4px 10px 10px 4px;
    overflow: hidden;
    box-shadow:
      0 1px 2px rgb(0 0 0 / 0.25),
      0 8px 18px -8px rgb(0 0 0 / 0.45);
    transition:
      transform 0.16s ease,
      box-shadow 0.16s ease;
    transform-origin: 50% 100%;
  }
  /* the spine and the sheen of a bound book */
  .cover::after {
    content: "";
    position: absolute;
    inset: 0;
    background:
      linear-gradient(to right, rgb(0 0 0 / 0.28) 0, rgb(0 0 0 / 0.05) 5px, rgb(255 255 255 / 0.22) 7px, transparent 12px),
      linear-gradient(115deg, rgb(255 255 255 / 0.14), transparent 40%);
    pointer-events: none;
  }
  .book:hover .cover,
  .book:focus-visible .cover {
    transform: translateY(-5px) rotate(-1.2deg);
    box-shadow:
      0 2px 3px rgb(0 0 0 / 0.25),
      0 18px 26px -10px rgb(0 0 0 / 0.5);
  }
  .book:focus-visible {
    outline: none;
  }
  .book:focus-visible .cover {
    outline: 3px solid var(--accent);
    outline-offset: 2px;
  }
  .plate {
    position: absolute;
    inset: 9% 9% auto 13%;
    max-height: 46%;
    overflow: hidden;
    display: -webkit-box;
    -webkit-line-clamp: 4;
    line-clamp: 4;
    -webkit-box-orient: vertical;
    text-align: center;
    text-wrap: balance;
    color: #fff;
    font-weight: 700;
    font-size: 30px;
    line-height: 1.25;
    letter-spacing: 0.04em;
    overflow-wrap: anywhere;
    text-shadow: 0 1px 2px rgb(0 0 0 / 0.45);
  }
  .plate.t2 {
    font-size: 23px;
  }
  .plate.t3 {
    font-size: 18px;
    letter-spacing: 0.01em;
  }
  .plate.t4 {
    font-size: 15px;
  }
  .ribbon {
    position: absolute;
    top: 0;
    right: 12px;
    z-index: 1;
    width: 22px;
    padding: 5px 0 11px;
    text-align: center;
    font-size: 12px;
    line-height: 1;
    color: #7a4b00;
    background: #ffd166;
    clip-path: polygon(0 0, 100% 0, 100% 100%, 50% 78%, 0 100%);
  }
  .st-meta {
    padding-left: 2px;
  }
  /* --- cloth & gilt: the title stamped in gold, in a book face --- */
  .look-cloth .plate,
  .look-cloth .story-title {
    color: var(--cv-hi);
    font-family: "Songti SC", "Noto Serif CJK SC", "Source Han Serif SC", "Noto Serif SC", SimSun, Georgia, serif;
    text-shadow: 0 1px 0 rgb(0 0 0 / 0.45);
    letter-spacing: 0.08em;
  }
  .look-cloth .plate {
    inset: 15% 15% auto 17%;
  }
  .banner.look-cloth::after {
    inset: 8px;
    background: none;
    border: 1px solid color-mix(in srgb, var(--cv-hi) 65%, transparent);
    border-radius: 6px;
  }
  /* --- thread-bound: sewn on the right, the title on a paper slip, written downwards --- */
  .cover.look-thread {
    border-radius: 10px 4px 4px 10px;
  }
  .cover.look-thread::after {
    background: linear-gradient(115deg, rgb(255 255 255 / 0.1), transparent 40%);
  }
  .look-thread .plate {
    inset: 8% auto auto 11%;
    max-height: 72%;
    max-width: 52%;
    display: block;
    writing-mode: vertical-rl;
    text-align: start;
    padding: 9px 5px;
    background: #f7f1e1;
    color: #221a14;
    border: 1px solid rgb(0 0 0 / 0.6);
    box-shadow: 0 0 0 2px #f7f1e1, 0 0 0 3px rgb(0 0 0 / 0.35), 0 1px 3px 3px rgb(0 0 0 / 0.18);
    text-shadow: none;
    font-family: "Kaiti SC", STKaiti, KaiTi, "Songti SC", "Noto Serif CJK SC", SimSun, serif;
    font-size: 22px;
    font-weight: 600;
    letter-spacing: 0.12em;
    line-height: 1.3;
  }
  .look-thread .plate.t2 {
    font-size: 19px;
  }
  .look-thread .plate.t3 {
    font-size: 15px;
  }
  .look-thread .plate.t4 {
    font-size: 13px;
  }
  .look-thread .ribbon {
    right: auto;
    left: 62%;
  }
  .banner.look-thread .story-title,
  .banner.look-thread .banner-sub {
    color: var(--cv-ink);
    text-shadow: none;
    font-family: "Kaiti SC", STKaiti, KaiTi, "Songti SC", "Noto Serif CJK SC", SimSun, serif;
  }
  /* --- poster: flat colour, heavy type --- */
  .look-poster .plate {
    inset: 9% 9% auto 14%;
    text-align: left;
    text-wrap: wrap;
    color: var(--cv-ink);
    font-weight: 800;
    letter-spacing: 0;
    line-height: 1.12;
    text-shadow: none;
  }
  .banner.look-poster .story-title,
  .banner.look-poster .banner-sub {
    color: var(--cv-ink);
    text-shadow: none;
    font-weight: 800;
  }
  /* (only the illustrated covers need the title's side darkened) */
  .banner.look-thread::after,
  .banner.look-poster::after {
    display: none;
  }
  /* the head of an open story: its cover, as a strip */
  .banner {
    position: relative;
    max-width: calc(52 * var(--rs)); /* as wide as the text below it */
    min-height: 118px;
    margin-bottom: 14px;
    padding: 22px 30% 22px 28px;
    border-radius: var(--radius);
    overflow: hidden;
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 4px;
  }
  .banner::after {
    content: "";
    position: absolute;
    inset: 0;
    background: linear-gradient(to right, rgb(0 0 0 / 0.22), transparent 70%);
    pointer-events: none;
  }
  .story-title,
  .banner-sub {
    position: relative;
    z-index: 1;
  }
  .story-title {
    color: #fff;
    font-size: 30px;
    font-weight: 700;
    line-height: 1.25;
    text-decoration: none !important; /* (themes underline page titles; a story's title is only bold) */
    text-shadow: 0 1px 2px rgb(0 0 0 / 0.4);
    overflow-wrap: anywhere;
  }
  .banner-sub {
    margin: 0;
    color: rgb(255 255 255 / 0.82);
  }

  .tip {
    position: fixed;
    z-index: 50;
    width: max-content;
    max-width: 320px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 10px 12px;
    background: var(--surface);
    color: var(--text);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: var(--shadow-lg);
    font-size: 14px;
    line-height: 1.4;
  }
  /* what is being read aloud: the sentence, and the word within it */
  .said {
    background: var(--accent-soft);
    box-decoration-break: clone;
    -webkit-box-decoration-break: clone;
  }
  .w.now {
    background: var(--accent);
    color: var(--accent-text);
    border-radius: 3px;
  }
  .w.now :global(span) {
    color: inherit;
  }
  .tip-add {
    align-self: flex-start;
    margin-top: 4px;
  }
  .mark {
    font-size: 19px;
    line-height: 1;
  }
  .mark.on,
  .star {
    color: var(--warn);
  }
  .only {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 10px;
    cursor: pointer;
  }
  .only input {
    width: auto;
  }
  /* full screen: only the story, over everything else in the window */
  .layout.full .folderlist,
  .layout.full .side-handle {
    display: none;
  }
  .layout.full .main {
    position: fixed;
    inset: 0;
    z-index: 40;
    background: var(--bg);
    --pad: max(28px, calc((100vw - 1000px) / 2));
  }
  /* full screen: the story's buttons sit in the bar's corner, under the sample text */
  .tools.corner {
    flex-basis: 100%;
    justify-content: flex-end;
    margin-top: -6px;
  }
  .tip-head {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .tip-hz {
    font-size: 22px;
  }
  .trad {
    font-size: 15px;
    color: var(--text-3);
  }
  .tip-py {
    font-weight: 600;
    font-size: 15px;
  }
  .tip-defs {
    color: var(--text-2);
  }

  .form {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .form textarea {
    font-size: 16px;
    line-height: 1.7;
    resize: vertical;
  }
  .formbtns {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 6px;
  }
</style>
