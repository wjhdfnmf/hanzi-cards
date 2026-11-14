<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { open, save, ask } from "@tauri-apps/plugin-dialog";
  import {
    api,
    errorText,
    type Card,
    type CardCopy,
    type CardRow,
    type Deck,
    type DuplicateWord,
    type PlecoPreview,
  } from "../lib/api";
  import { t } from "../lib/i18n.svelte";
  import { dueLabel } from "../lib/format";
  import { studyDeck } from "../lib/nav.svelte";
  import { showToast } from "../lib/toast.svelte";
  import Hanzi from "../components/Hanzi.svelte";
  import Pinyin from "../components/Pinyin.svelte";
  import Icon from "../components/Icon.svelte";
  import Back from "../components/Back.svelte";
  import { device } from "../lib/device.svelte";
  import Modal from "../components/Modal.svelte";
  import CardEditor from "../components/CardEditor.svelte";
  import { stripTones, syllables, toneOf } from "../lib/pinyin";

  let decks = $state<Deck[]>([]);
  let selectedDeck = $state<number | null>(null); // null = all cards
  /** On a phone: the deck list, or the chosen deck's cards (one at a time). */
  let deckOpen = $state(false);
  function openDeck(id: number | null) {
    selectedDeck = id;
    deckOpen = true;
  }
  // raw: only ever replaced whole, so thousands of rows needn't each become a reactive proxy
  let cards = $state.raw<CardRow[]>([]);
  let search = $state("");
  let checked = $state<number[]>([]);
  const checkedSet = $derived(new Set(checked));
  let loading = $state(true);

  // ---------- drawing a big deck a page at a time ----------
  // Every row is a handful of components (tone-coloured hanzi and pinyin); drawing thousands at
  // once is what made big decks slow to open. Draw the first page, then more as the list scrolls.
  const PAGE = 150;
  let limit = $state(PAGE);
  /** Shows the next page when the end of the list scrolls near; re-arms after each page. */
  function more(node: HTMLElement, _shown: number) {
    const io = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) limit += PAGE * 2;
      },
      { root: node.closest(".main"), rootMargin: "800px 0px" },
    );
    io.observe(node);
    return {
      // still in view after a page was added (a tall window): observing again reports it again
      update() {
        io.unobserve(node);
        io.observe(node);
      },
      destroy: () => io.disconnect(),
    };
  }

  // ---------- resizing: the deck list and the table's columns ----------
  const SIDE_KEY = "decks.sideWidth";
  const COLS_KEY = "decks.colWidths";
  const SIDE_DEFAULT = 240;
  type Col = SortKey; // every column but the checkboxes
  // sized so every column fits a 1280px-wide window; drag for more
  const COL_DEFAULT: Record<Col, number> = {
    hanzi: 110,
    pinyin: 110,
    meaning: 100, // least width; it takes whatever room the others leave
    deck: 120,
    tags: 100,
    status: 85,
    next: 120, // "Next review" and its sort arrow
  };
  const COL_MIN = 50;
  /** Columns whose title needs more than COL_MIN (a saved narrower width is raised). */
  const COL_TITLE_MIN: Partial<Record<Col, number>> = { next: 120, status: 80 };
  const colMin = (k: Col) => Math.max(COL_MIN, COL_TITLE_MIN[k] ?? 0);
  function loadJson<T>(key: string, fallback: T): T {
    try {
      return { ...fallback, ...JSON.parse(localStorage.getItem(key) ?? "null") };
    } catch {
      return fallback;
    }
  }
  let sideWidth = $state(loadJson(SIDE_KEY, { w: SIDE_DEFAULT }).w);
  let colW = $state<Record<Col, number>>(
    (() => {
      const w = loadJson(COLS_KEY, COL_DEFAULT);
      for (const k of Object.keys(w) as Col[]) w[k] = Math.max(colMin(k), w[k]);
      return w;
    })(),
  );
  const tableMin = $derived(
    36 + (Object.keys(colW) as Col[]).reduce((s, k) => (k === "deck" && !showDeckColumn ? s : s + colW[k]), 0),
  );
  function persist() {
    try {
      localStorage.setItem(SIDE_KEY, JSON.stringify({ w: sideWidth }));
      localStorage.setItem(COLS_KEY, JSON.stringify(colW));
    } catch {
      /* not essential */
    }
  }

  /** Drag a handle sideways; `set` gets the start width plus how far the pointer has moved. */
  function drag(e: PointerEvent, start: number, set: (w: number) => void) {
    e.preventDefault();
    e.stopPropagation();
    const handle = e.currentTarget as HTMLElement;
    const x0 = e.clientX;
    handle.setPointerCapture(e.pointerId);
    document.body.classList.add("col-resizing");
    const move = (ev: PointerEvent) => set(start + ev.clientX - x0);
    const up = () => {
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", up);
      handle.removeEventListener("pointercancel", up);
      document.body.classList.remove("col-resizing");
      persist();
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", up);
    handle.addEventListener("pointercancel", up);
  }
  function dragSide(e: PointerEvent) {
    const max = Math.max(260, window.innerWidth * 0.6);
    drag(e, sideWidth, (w) => (sideWidth = Math.round(Math.min(max, Math.max(160, w)))));
  }
  function dragCol(e: PointerEvent, key: Col) {
    // start from the width on screen: the meaning column is often wider than its least width
    const th = (e.currentTarget as HTMLElement).closest("th")!;
    drag(e, th.getBoundingClientRect().width, (w) => (colW[key] = Math.round(Math.max(colMin(key), w))));
  }
  function resetSide() {
    sideWidth = SIDE_DEFAULT;
    persist();
  }
  function resetCol(key: Col) {
    colW[key] = COL_DEFAULT[key];
    persist();
  }

  // ---------- sorting by column (click a header: ascending, descending, off) ----------
  type SortKey = "hanzi" | "pinyin" | "meaning" | "deck" | "tags" | "status" | "next";
  type Sort = { key: SortKey; dir: 1 | -1 } | null;
  let sort = $state<Sort>(
    (() => {
      try {
        const saved = JSON.parse(localStorage.getItem("decks.sort") ?? "null");
        return saved && saved.key in COL_DEFAULT && (saved.dir === 1 || saved.dir === -1) ? (saved as Sort) : null;
      } catch {
        return null;
      }
    })(),
  );
  function sortBy(key: SortKey) {
    sort = sort?.key !== key ? { key, dir: 1 } : sort.dir === 1 ? { key, dir: -1 } : null;
    try {
      localStorage.setItem("decks.sort", JSON.stringify(sort));
    } catch {}
  }
  const zh = new Intl.Collator("zh-CN"); // characters in pinyin order
  const text = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });
  const STATUS_ORDER = { new: 0, learning: 1, review: 2 } as const;
  /** Pinyin without tone marks, then by tone: ma1 < ma2 < ma3 < ma4. Kept: a sort asks thousands of times. */
  const pinyinKeys = new Map<string, { bare: string; tones: string }>();
  const pinyinKey = (p: string) => {
    let k = pinyinKeys.get(p);
    if (!k) {
      const syl = syllables(p.toLowerCase());
      k = { bare: stripTones(p.toLowerCase()).replace(/\s+/g, " ").trim(), tones: syl.map(toneOf).join("") };
      pinyinKeys.set(p, k);
    }
    return k;
  };
  const sortedCards = $derived.by(() => {
    if (!sort) return cards;
    const { key, dir } = sort;
    // empty values always go last, whichever the direction
    const blank = (c: CardRow) =>
      key === "pinyin" ? !c.pinyin : key === "meaning" ? !c.meaning : key === "tags" ? !c.tags.length : key === "next" ? c.nextDue === null : false;
    const cmp = (a: CardRow, b: CardRow): number => {
      switch (key) {
        case "hanzi":
          return zh.compare(a.hanzi, b.hanzi);
        case "pinyin": {
          const x = pinyinKey(a.pinyin);
          const y = pinyinKey(b.pinyin);
          return text.compare(x.bare, y.bare) || x.tones.localeCompare(y.tones);
        }
        case "meaning":
          return text.compare(a.meaning, b.meaning);
        case "deck":
          return text.compare(deckPath.get(a.deckId) ?? "", deckPath.get(b.deckId) ?? "");
        case "tags":
          return text.compare(a.tags.join(", "), b.tags.join(", "));
        case "status":
          return STATUS_ORDER[a.status] - STATUS_ORDER[b.status];
        case "next":
          return (a.nextDue ?? 0) - (b.nextDue ?? 0);
      }
    };
    return [...cards].sort((a, b) => {
      const ba = blank(a);
      const bb = blank(b);
      if (ba !== bb) return ba ? 1 : -1;
      return dir * cmp(a, b) || zh.compare(a.hanzi, b.hanzi);
    });
  });
  const shownCards = $derived(sortedCards.length > limit ? sortedCards.slice(0, limit) : sortedCards);

  let editing = $state<{ card: Card | null; copies: CardCopy[] } | null>(null);

  // ---------- copies ----------
  // A word that several decks hold with the same pinyin and meaning is listed once; its row
  // stands for the copies too. Ticking "Show copies" lists every card.
  const COPIES_KEY = "decks.showCopies";
  let showCopies = $state(
    (() => {
      try {
        return localStorage.getItem(COPIES_KEY) === "true";
      } catch {
        return false;
      }
    })(),
  );
  function saveShowCopies() {
    try {
      localStorage.setItem(COPIES_KEY, String(showCopies));
    } catch {
      /* not essential */
    }
  }
  const folded = $derived(cards.reduce((n, c) => n + c.copies.length, 0));
  /** Every deck a row's word is in, one to a line. */
  const decksOf = (c: CardRow) => [c.deckId, ...c.copies.map((x) => x.deckId)].map((id) => deckPath.get(id) ?? "").join("\n");
  let deckForm = $state<{ id: number | null; name: string; description: string; parentId: number | null } | null>(
    null,
  );

  const deck = $derived(decks.find((d) => d.id === selectedDeck) ?? null);
  const allChecked = $derived(cards.length > 0 && checkedSet.size === cards.length);
  const deckPath = $derived(new Map(decks.map((d) => [d.id, d.path])));
  // Show which deck each card is in when the list mixes decks.
  const showDeckColumn = $derived(selectedDeck === null || (deck?.childCount ?? 0) > 0);

  // ---------- deck tree ----------

  const COLLAPSED_KEY = "hanzi-cards.collapsedDecks";
  let collapsed = $state<number[]>(loadCollapsed());

  function loadCollapsed(): number[] {
    try {
      const saved = JSON.parse(localStorage.getItem(COLLAPSED_KEY) ?? "[]");
      return Array.isArray(saved) ? saved.filter((x) => typeof x === "number") : [];
    } catch {
      return [];
    }
  }

  // every deck that has subdecks closed at once, or (when they all are) opened again
  const parents = $derived(decks.filter((d) => d.childCount > 0).map((d) => d.id));
  const allCollapsed = $derived(parents.length > 0 && parents.every((id) => collapsed.includes(id)));
  function toggleAllDecks() {
    collapsed = allCollapsed ? [] : [...parents];
    try {
      localStorage.setItem(COLLAPSED_KEY, JSON.stringify(collapsed));
    } catch {
      /* not essential */
    }
  }
  function toggleCollapsed(id: number) {
    collapsed = collapsed.includes(id) ? collapsed.filter((x) => x !== id) : [...collapsed, id];
    try {
      localStorage.setItem(COLLAPSED_KEY, JSON.stringify(collapsed));
    } catch {
      /* not essential */
    }
  }

  /** Decks whose ancestors are all expanded (the list is already in tree order). */
  const visibleDecks = $derived.by(() => {
    const parent = new Map(decks.map((d) => [d.id, d.parentId]));
    return decks.filter((d) => {
      for (let p = d.parentId; p !== null && p !== undefined; p = parent.get(p) ?? null) {
        if (collapsed.includes(p)) return false;
      }
      return true;
    });
  });

  /** A deck and all decks inside it. */
  function subtree(id: number): Set<number> {
    const out = new Set([id]);
    for (const d of decks) if (d.parentId !== null && out.has(d.parentId)) out.add(d.id);
    return out; // decks are in tree order, so parents come first
  }

  const indent = (d: Deck) => `${"  ".repeat(d.depth)}${d.name}`;

  let ready = $state(false); // the first deck is chosen: don't load every card before that
  onMount(async () => {
    await loadDecks();
    if (decks.length) selectedDeck = decks[0].id;
    ready = true;
  });

  async function loadDecks() {
    try {
      decks = await api.listDecks();
    } catch (e) {
      showToast(errorText(e), "err");
    }
  }

  let request = 0;
  /** `fresh`: a different deck or search, so start again from the first page. */
  async function loadCards(fresh = false) {
    const mine = ++request;
    loading = true;
    try {
      const rows = await api.listCards(selectedDeck, search, !showCopies);
      if (mine !== request) return; // a newer deck or search was asked for meanwhile
      cards = rows;
      if (fresh) limit = PAGE;
      const ids = new Set(rows.map((c) => c.id));
      if (checked.some((id) => !ids.has(id))) checked = checked.filter((id) => ids.has(id));
    } catch (e) {
      if (mine === request) showToast(errorText(e), "err");
    } finally {
      if (mine === request) loading = false;
    }
  }

  let searchTimer: ReturnType<typeof setTimeout> | undefined;
  let lastDeck: number | null | undefined;
  $effect(() => {
    const d = selectedDeck;
    void search;
    void showCopies;
    if (!ready) return;
    clearTimeout(searchTimer);
    if (d !== lastDeck) {
      lastDeck = d; // a click on a deck: load straight away; typing waits for a pause
      untrack(() => loadCards(true));
    } else {
      searchTimer = setTimeout(() => loadCards(true), 120);
    }
  });

  async function refresh() {
    await Promise.all([loadDecks(), loadCards()]);
  }

  async function saveDeck() {
    if (!deckForm) return;
    try {
      if (deckForm.id === null) {
        selectedDeck = await api.createDeck(deckForm.name, deckForm.description, deckForm.parentId);
        if (deckForm.parentId !== null) collapsed = collapsed.filter((x) => x !== deckForm!.parentId);
      } else {
        await api.updateDeck(deckForm.id, deckForm.name, deckForm.description, deckForm.parentId);
      }
      deckForm = null;
      await refresh();
    } catch (e) {
      showToast(errorText(e), "err");
    }
  }

  async function removeDeck() {
    if (!deck) return;
    const subdecks = subtree(deck.id).size - 1;
    const message = subdecks
      ? t("decks.deleteConfirmNested", { name: deck.path, d: subdecks, n: deck.totalCards })
      : t("decks.deleteConfirm", { name: deck.path, n: deck.cardCount });
    const ok = await ask(message, { title: t("decks.deleteDeck"), kind: "warning" });
    if (!ok) return;
    try {
      await api.deleteDeck(deck.id);
    } catch (e) {
      showToast(errorText(e), "err");
    }
    await loadDecks();
    selectedDeck = decks.length ? decks[0].id : null; // changing deck loads its cards
  }

  async function removeChecked() {
    const withCopies = cards.filter((c) => checkedSet.has(c.id) && c.copies.length).length;
    const message = withCopies
      ? t("decks.deleteCardsCopies", { n: checked.length, c: withCopies })
      : t("decks.deleteCards", { n: checked.length });
    const ok = await ask(message, { kind: "warning" });
    if (!ok) return;
    try {
      await api.deleteCards(checked);
      checked = [];
    } catch (e) {
      showToast(errorText(e), "err");
    }
    await refresh();
  }

  async function moveChecked(e: Event) {
    const target = Number((e.currentTarget as HTMLSelectElement).value);
    (e.currentTarget as HTMLSelectElement).value = "";
    if (!target) return;
    try {
      await api.moveCards(checked, target);
      checked = [];
    } catch (e) {
      showToast(errorText(e), "err");
    }
    await refresh();
  }

  // ---------- import ----------

  function wordList(words: DuplicateWord[], max = 8): string {
    const shown = words.slice(0, max).map((w) => `${w.hanzi} (${w.decks.join(", ")})`);
    if (words.length > max) shown.push(t("dupes.more", { n: words.length - max }));
    return shown.join(", ");
  }

  async function importFile() {
    const path = await open({
      multiple: false,
      filters: [
        { name: t("decks.importFiles"), extensions: ["xml", "csv", "tsv", "txt"] },
        { name: "Pleco (XML)", extensions: ["xml"] },
        { name: "CSV / TSV", extensions: ["csv", "tsv", "txt"] },
      ],
    });
    if (!path) return;
    if ((path as string).toLowerCase().endsWith(".xml")) {
      await startPlecoImport(path as string);
      return;
    }
    if (selectedDeck === null) {
      showToast(t("decks.chooseDeckCsv"), "err");
      return;
    }
    try {
      const preview = await api.importPreview(selectedDeck, path as string);
      let includeOther = false;
      if (preview.otherDecks.length) {
        includeOther = await ask(t("dupes.prompt", { n: preview.otherDecks.length, list: wordList(preview.otherDecks) }), {
          title: t("dupes.title"),
          kind: "info",
          okLabel: t("dupes.importToo"),
          cancelLabel: t("dupes.skip"),
        });
      }
      const res = await api.importCards(selectedDeck, path as string, includeOther);
      showToast(t("decks.importedDetail", { n: res.imported, s: res.skipped, o: res.skippedOther }));
      await refresh();
    } catch (e) {
      showToast(errorText(e), "err");
    }
  }

  // ---------- Pleco ----------

  let pleco = $state<{
    path: string;
    preview: PlecoPreview;
    defaultDeck: number; // 0 = a new deck called "Pleco"
    useDictionary: boolean;
    includeOther: boolean;
    busy: boolean;
  } | null>(null);

  async function startPlecoImport(path: string) {
    try {
      const defaultDeck = selectedDeck ?? 0;
      const preview = await api.plecoPreview(path, defaultDeck || null);
      pleco = { path, preview, defaultDeck, useDictionary: preview.dictionary, includeOther: false, busy: false };
    } catch (e) {
      showToast(errorText(e), "err");
    }
  }

  /** Duplicates depend on where uncategorised cards go. */
  async function changePlecoDefault() {
    if (!pleco) return;
    try {
      pleco.preview = await api.plecoPreview(pleco.path, pleco.defaultDeck || null);
    } catch (e) {
      showToast(errorText(e), "err");
    }
  }

  async function runPlecoImport() {
    if (!pleco) return;
    pleco.busy = true;
    try {
      const r = await api.importPleco(pleco.path, pleco.defaultDeck || null, pleco.useDictionary, pleco.includeOther);
      pleco = null;
      showToast(t("pleco.imported", { n: r.imported, d: r.decks, s: r.skipped + r.skippedOther, p: r.pinyinCorrected }));
      await refresh();
    } catch (e) {
      if (pleco) pleco.busy = false;
      showToast(errorText(e), "err");
    }
  }

  // ---------- export ----------

  let exportMenu = $state(false);

  async function exportFile(format: "csv" | "pleco") {
    exportMenu = false;
    const name = (deck?.name ?? "all-cards").replace(/[\\/:*?"<>|]/g, "_");
    const pleco = format === "pleco";
    const path = await save({
      defaultPath: `${name}.${pleco ? "xml" : "csv"}`,
      filters: [pleco ? { name: "Pleco (XML)", extensions: ["xml"] } : { name: "CSV", extensions: ["csv"] }],
    });
    if (!path) return;
    try {
      const n = pleco ? await api.exportPleco(selectedDeck, path) : await api.exportCards(selectedDeck, path);
      showToast(t(pleco ? "pleco.exported" : "decks.exported", { n }));
    } catch (e) {
      showToast(errorText(e), "err");
    }
  }

  function toggleAll() {
    checked = allChecked ? [] : cards.map((c) => c.id);
  }
  function toggleOne(id: number) {
    checked = checkedSet.has(id) ? checked.filter((x) => x !== id) : [...checked, id];
  }
</script>

<div class="layout" class:phone={device.phone} class:open={deckOpen} style="--side:{sideWidth}px">
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="side-handle" title={t("decks.resizeHint")} onpointerdown={dragSide} ondblclick={resetSide}></div>
  <aside class="decklist">
    <div class="dl-head">
      <h2>{t("decks.title")}</h2>
      {#if parents.length}
        <button
          class="btn ghost icon all"
          title={t(allCollapsed ? "decks.expandAll" : "decks.collapseAll")}
          aria-label={t(allCollapsed ? "decks.expandAll" : "decks.collapseAll")}
          onclick={toggleAllDecks}
        >
          <Icon name={allCollapsed ? "expand-all" : "collapse-all"} />
        </button>
      {/if}
      <button
        class="btn ghost icon"
        title={t("decks.newDeck")}
        aria-label={t("decks.newDeck")}
        onclick={() => (deckForm = { id: null, name: "", description: "", parentId: null })}
      >
        <Icon name="plus" />
      </button>
    </div>
    <button class="deck" class:on={selectedDeck === null} onclick={() => openDeck(null)}>
      <span class="dname">{t("decks.allCards")}</span>
      <span class="faint small">{decks.reduce((s, d) => s + d.cardCount, 0)}</span>
    </button>
    {#each visibleDecks as d (d.id)}
      <div class="deckrow" style="padding-left:{d.depth * 14}px">
        {#if d.childCount}
          <button
            class="twisty"
            class:open={!collapsed.includes(d.id)}
            aria-label={collapsed.includes(d.id) ? t("decks.expand") : t("decks.collapse")}
            aria-expanded={!collapsed.includes(d.id)}
            onclick={() => toggleCollapsed(d.id)}>▸</button
          >
        {:else}
          <span class="twisty-space"></span>
        {/if}
        <button class="deck" class:on={selectedDeck === d.id} onclick={() => openDeck(d.id)}>
          <span class="dname">{d.name}</span>
          <span class="counts small">
            {#if d.totalDue}<span class="due" title={t("common.due")}>{d.totalDue}</span>{/if}
            {#if d.totalNew}<span class="new" title={t("common.new")}>{d.totalNew}</span>{/if}
            <span class="faint">{d.totalCards}</span>
          </span>
        </button>
      </div>
    {/each}
  </aside>

  <section class="main">
    <div class="head">
      <div class="title">
        {#if device.phone}<Back label={t("decks.title")} onclick={() => (deckOpen = false)} />{/if}
        {#if deck && deck.depth > 0}
          <p class="crumbs small faint">{deck.path.split(" › ").slice(0, -1).join(" › ")}</p>
        {/if}
        <h1>{deck?.name ?? t("decks.allCards")}</h1>
        {#if deck?.description}<p class="muted">{deck.description}</p>{/if}
      </div>
      <div class="tools">
        {#if deck}
          <button
            class="btn ghost icon"
            title={t("decks.renameDeck")}
            aria-label={t("decks.renameDeck")}
            onclick={() =>
              (deckForm = { id: deck.id, name: deck.name, description: deck.description, parentId: deck.parentId })}
          >
            <Icon name="edit" />
          </button>
          <button class="btn ghost icon" title={t("decks.deleteDeck")} aria-label={t("decks.deleteDeck")} onclick={removeDeck}>
            <Icon name="trash" />
          </button>
          <button
            class="btn ghost icon"
            title={t("decks.newSubdeck")}
            aria-label={t("decks.newSubdeck")}
            onclick={() => (deckForm = { id: null, name: "", description: "", parentId: deck.id })}
          >
            <Icon name="folder" />
          </button>
        {/if}
        <button class="btn" onclick={importFile} title={t("decks.importHint")} aria-label={t("decks.import")}
          ><Icon name="upload" size={16} /><span class="lbl">{t("decks.import")}</span></button
        >
        <div class="menuwrap">
          <button class="btn" aria-haspopup="menu" aria-expanded={exportMenu} aria-label={t("decks.export")} onclick={() => (exportMenu = !exportMenu)}>
            <Icon name="download" size={16} /><span class="lbl">{t("decks.export")}</span>
          </button>
          {#if exportMenu}
            <div class="menu card-surface" role="menu">
              <button role="menuitem" onclick={() => exportFile("csv")}>{t("decks.exportCsv")}</button>
              <button role="menuitem" onclick={() => exportFile("pleco")}>{t("decks.exportPleco")}</button>
            </div>
          {/if}
        </div>
        {#if deck}
          <button class="btn" onclick={() => studyDeck(deck.id)} aria-label={t("decks.studyDeck")}
            ><Icon name="play" size={14} /><span class="lbl">{t("decks.studyDeck")}</span></button
          >
        {/if}
        <button class="btn primary" disabled={!decks.length} onclick={() => (editing = { card: null, copies: [] })}>
          <Icon name="plus" size={16} />{t("decks.addCard")}
        </button>
      </div>
    </div>

    <div class="filterbar">
      <div class="searchbox">
        <Icon name="search" size={16} />
        <input type="search" placeholder={t("decks.searchCards")} bind:value={search} autocapitalize="off" autocorrect="off" spellcheck="false" />
      </div>
      {#if folded || showCopies}
        <label class="copies small muted" title={t("decks.showCopiesHint")}>
          <input type="checkbox" bind:checked={showCopies} onchange={saveShowCopies} />
          {showCopies ? t("decks.showCopies") : t("decks.showCopiesN", { n: folded })}
        </label>
      {/if}
      {#if checked.length}
        <span class="small muted">{t("decks.selected", { n: checked.length })}</span>
        <select class="move" onchange={moveChecked}>
          <option value="">{t("decks.moveTo")}</option>
          {#each decks as d (d.id)}<option value={d.id}>{indent(d)}</option>{/each}
        </select>
        <button class="btn danger" onclick={removeChecked}><Icon name="trash" size={16} />{t("common.delete")}</button>
      {/if}
    </div>

    {#if !loading && cards.length === 0}
      <div class="empty card-surface">
        <p class="muted">{search ? t("decks.noResults") : t("decks.empty")}</p>
      </div>
    {:else}
      <div class="table card-surface">
        <table style="min-width:{tableMin}px">
          <colgroup>
            <col style="width:36px" />
            <col style="width:{colW.hanzi}px" />
            <col style="width:{colW.pinyin}px" />
            <col />
            {#if showDeckColumn}<col style="width:{colW.deck}px" />{/if}
            <col style="width:{colW.tags}px" />
            <col style="width:{colW.status}px" />
            <col style="width:{colW.next}px" />
          </colgroup>
          <thead>
            <tr>
              <th class="cb"><input type="checkbox" checked={allChecked} onchange={toggleAll} aria-label="Select all" /></th>
              {#snippet head(key: SortKey, label: string, cls = "")}
                <th class={cls} aria-sort={sort?.key === key ? (sort.dir === 1 ? "ascending" : "descending") : "none"}>
                  <button class="sorthead" class:on={sort?.key === key} onclick={() => sortBy(key)} title={t("decks.sortHint")}>
                    <span class="label">{label}</span><span class="arrow" aria-hidden="true"
                      >{sort?.key === key ? (sort.dir === 1 ? "▲" : "▼") : "↕"}</span
                    >
                  </button>
                  <!-- svelte-ignore a11y_no_static_element_interactions -->
                  <span
                    class="col-handle"
                    title={t("decks.resizeHint")}
                    onpointerdown={(e) => dragCol(e, key)}
                    ondblclick={() => resetCol(key)}
                  ></span>
                </th>
              {/snippet}
              {@render head("hanzi", t("col.hanzi"))}
              {@render head("pinyin", t("col.pinyin"))}
              {@render head("meaning", t("col.meaning"), "wide")}
              {#if showDeckColumn}{@render head("deck", t("decks.colDeck"))}{/if}
              {@render head("tags", t("col.tags"))}
              {@render head("status", t("col.status"))}
              {@render head("next", t("col.next"))}
            </tr>
          </thead>
          <tbody>
            {#each shownCards as c (c.id)}
              <tr onclick={() => (editing = { card: c, copies: c.copies })} class:checked={checkedSet.has(c.id)}>
                <td class="cb" onclick={(e) => e.stopPropagation()}>
                  <input type="checkbox" checked={checkedSet.has(c.id)} onchange={() => toggleOne(c.id)} aria-label="Select" />
                </td>
                <td class="hz clip" title={c.hanzi}
                  ><Hanzi hanzi={c.hanzi} pinyin={c.pinyin} size={20} />{#if c.copies.length && !showDeckColumn}<span
                      class="chip ncopies">×{c.copies.length + 1}</span
                    >{/if}</td
                >
                <td class="clip" title={c.pinyin}><Pinyin text={c.pinyin} /></td>
                <td class="wide clip" title={c.meaning}>{c.meaning}</td>
                {#if showDeckColumn}<td class="small muted clip" title={decksOf(c)}
                    >{#if c.copies.length}<span class="chip ncopies">+{c.copies.length}</span>{/if}{deckPath.get(c.deckId) ?? ""}</td
                  >{/if}
                <td class="clip" title={c.tags.join(", ")}>{#each c.tags as tg (tg)}<span class="chip">{tg}</span> {/each}</td>
                <td class="clip"><span class="badge {c.status}">{t(`status.${c.status}`)}</span></td>
                <td class="small muted clip">{dueLabel(c.nextDue)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
        {#if shownCards.length < sortedCards.length}
          <div class="more small faint" use:more={shownCards.length}>
            {t("decks.showing", { n: shownCards.length, total: sortedCards.length })}
          </div>
        {/if}
      </div>
    {/if}
  </section>
</div>

{#if editing}
  <CardEditor
    card={editing.card}
    copies={editing.copies}
    {decks}
    deckId={selectedDeck}
    onclose={() => (editing = null)}
    onsaved={() => refresh()}
  />
{/if}

<svelte:window
  onclick={(e) => {
    if (exportMenu && !(e.target as HTMLElement).closest(".menuwrap")) exportMenu = false;
  }}
/>

{#if pleco}
  {@const p = pleco.preview}
  <Modal title={t("pleco.title")} onclose={() => (pleco = null)} width={600}>
    <div class="pleco">
      <p>{t("pleco.summary", { n: p.cards, c: p.categories.length })}</p>
      {#if p.categories.length}
        <table class="cats">
          <tbody>
            {#each p.categories as c (c.name)}
              <tr>
                <td>{c.name}</td>
                <td class="arrow">→</td>
                <td>
                  <strong>{c.deck}</strong>
                  <span class="faint small">{c.exists ? t("pleco.existingDeck") : t("pleco.newDeck")}</span>
                </td>
                <td class="num small muted">{t("pleco.cards", { n: c.cards })}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
      {#if p.uncategorized}
        <label class="field">
          <span>{t("pleco.uncategorized", { n: p.uncategorized })}</span>
          <select bind:value={pleco.defaultDeck} onchange={changePlecoDefault}>
            <option value={0}>{t("pleco.plecoDeck")}</option>
            {#each decks as d (d.id)}<option value={d.id}>{indent(d)}</option>{/each}
          </select>
        </label>
      {/if}
      {#if p.duplicates.sameDeck}
        <p class="small muted">{t("pleco.dupesSame", { n: p.duplicates.sameDeck })}</p>
      {/if}
      {#if p.duplicates.otherDecks.length}
        <div class="notice small dupes">
          <div>{t("pleco.dupesOther", { n: p.duplicates.otherDecks.length })}</div>
          <div class="dupwords">{wordList(p.duplicates.otherDecks, 12)}</div>
          <label class="check">
            <input type="checkbox" bind:checked={pleco.includeOther} />
            <span>{t("pleco.includeOther")}</span>
          </label>
        </div>
      {/if}
      <label class="check">
        <input type="checkbox" bind:checked={pleco.useDictionary} disabled={!p.dictionary} />
        <span>{t("pleco.useDict")}</span>
      </label>
      <p class="small faint hint">{p.dictionary ? t("pleco.useDictHint") : t("pleco.noDict")}</p>
      <p class="small muted">{t("pleco.skipNote")}</p>
    </div>
    {#snippet footer()}
      <button class="btn ghost" onclick={() => (pleco = null)}>{t("common.cancel")}</button>
      <button class="btn primary" disabled={pleco?.busy || !p.cards} onclick={runPlecoImport}>
        {t("pleco.import", { n: p.cards })}
      </button>
    {/snippet}
  </Modal>
{/if}

{#if deckForm}
  {@const blocked = deckForm.id !== null ? subtree(deckForm.id) : new Set<number>()}
  <Modal title={deckForm.id === null ? t("decks.newDeck") : t("decks.renameDeck")} onclose={() => (deckForm = null)} width={440}>
    <form class="deckform" onsubmit={(e) => { e.preventDefault(); saveDeck(); }}>
      <label class="field">
        <span>{t("decks.deckName")}</span>
        <!-- svelte-ignore a11y_autofocus -->
        <input type="text" bind:value={deckForm.name} autofocus />
      </label>
      <label class="field">
        <span>{t("decks.parent")}</span>
        <select bind:value={deckForm.parentId}>
          <option value={null}>{t("decks.topLevel")}</option>
          {#each decks.filter((d) => !blocked.has(d.id)) as d (d.id)}<option value={d.id}>{indent(d)}</option>{/each}
        </select>
      </label>
      <label class="field">
        <span>{t("decks.description")}</span>
        <input type="text" bind:value={deckForm.description} />
      </label>
      <div class="formbtns">
        <button type="button" class="btn ghost" onclick={() => (deckForm = null)}>{t("common.cancel")}</button>
        <button type="submit" class="btn primary" disabled={!deckForm.name.trim()}>{t("common.save")}</button>
      </div>
    </form>
  </Modal>
{/if}

<style>
  .dl-head .all {
    margin-left: auto;
  }
  .layout {
    display: grid;
    grid-template-columns: var(--side) minmax(0, 1fr);
    height: 100%;
    position: relative;
  }
  /* a phone: the deck list, or one deck */
  .layout.phone {
    grid-template-columns: minmax(0, 1fr);
  }
  .phone .side-handle,
  .phone.open .decklist,
  .phone:not(.open) .main {
    display: none;
  }
  .phone .decklist {
    border-right: none;
    padding: 14px 12px;
  }
  .phone .deck.on {
    background: transparent;
  }
  /* the tools as icons alone, except adding a card */
  .phone .tools .lbl {
    display: none;
  }
  /* each card as one compact row: the characters, then the pinyin over the meaning (tags,
     status and due date stay on the card itself) */
  .phone table {
    display: block;
    min-width: 0 !important;
  }
  .phone table thead,
  .phone table colgroup {
    display: none;
  }
  .phone table tbody {
    display: block;
  }
  .phone table tbody tr {
    display: grid;
    grid-template-columns: minmax(56px, max-content) minmax(0, 1fr);
    column-gap: 14px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
  }
  .phone table tbody td {
    border: none;
    padding: 0;
  }
  .phone table td.cb,
  .phone table tbody td:nth-child(n + 5) {
    display: none;
  }
  .phone table td.hz {
    grid-row: span 2;
    align-self: center;
  }
  .phone table td.wide {
    color: var(--text-2);
    font-size: 13px;
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
  :global(body.col-resizing) {
    cursor: col-resize;
    user-select: none;
  }
  .decklist {
    border-right: 1px solid var(--border);
    padding: 22px 12px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .dl-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 6px 10px 10px;
  }
  .deck {
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
  .deck:hover {
    background: var(--surface-2);
  }
  .deck.on {
    background: var(--surface-3);
    font-weight: 600;
  }
  .dname {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .counts {
    display: flex;
    gap: 7px;
    font-variant-numeric: tabular-nums;
  }
  .due {
    color: var(--ok);
    font-weight: 600;
  }
  .new {
    color: var(--series-1);
    font-weight: 600;
  }
  .main {
    padding: 24px 28px;
    overflow-y: auto;
    min-width: 0;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 16px;
    margin-bottom: 16px;
    flex-wrap: wrap;
  }
  .tools {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }
  .filterbar {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 14px;
  }
  .searchbox {
    position: relative;
    flex: 1;
    max-width: 420px;
    color: var(--text-3);
  }
  .searchbox :global(svg) {
    position: absolute;
    left: 10px;
    top: 9px;
  }
  .searchbox input {
    padding-left: 34px;
  }
  .move {
    width: auto;
  }
  .copies {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    white-space: nowrap;
    cursor: pointer;
  }
  .copies input {
    width: auto;
  }
  .ncopies {
    margin: 0 6px 0 0;
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }
  .hz .ncopies {
    margin: 0 0 0 8px;
    vertical-align: middle;
  }
  .empty {
    padding: 40px;
    text-align: center;
  }
  .table {
    overflow-x: auto;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    /* widths come from the <col>s, so a column can be dragged and long rows don't reflow the table */
    table-layout: fixed;
  }
  .clip {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  th {
    position: relative;
  }
  .sorthead {
    max-width: 100%;
  }
  .sorthead .label {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .col-handle {
    position: absolute;
    top: 0;
    bottom: 0;
    right: -4px;
    width: 8px;
    cursor: col-resize;
    z-index: 1;
    touch-action: none;
  }
  .col-handle:hover,
  .col-handle:active {
    background: linear-gradient(to right, transparent 3px, var(--accent) 3px, var(--accent) 5px, transparent 5px);
  }
  th:last-child .col-handle {
    right: 0;
  }
  .more {
    padding: 12px;
    text-align: center;
  }
  .sorthead {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    border: none;
    background: none;
    padding: 0;
    font: inherit;
    color: inherit;
    cursor: pointer;
    white-space: nowrap;
  }
  .sorthead:hover,
  .sorthead.on {
    color: var(--text);
  }
  .sorthead .arrow {
    font-size: 9px;
    opacity: 0.35;
  }
  .sorthead.on .arrow {
    opacity: 1;
    color: var(--accent);
  }
  th {
    text-align: left;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-3);
    padding: 10px 12px;
    border-bottom: 1px solid var(--border);
    white-space: nowrap;
  }
  td {
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
    vertical-align: middle;
  }
  tbody tr {
    cursor: pointer;
  }
  tbody tr:hover {
    background: var(--surface-2);
  }
  tbody tr.checked {
    background: var(--accent-soft);
  }
  tbody tr:last-child td {
    border-bottom: none;
  }
  .cb {
    width: 36px;
  }
  .cb input {
    accent-color: var(--accent);
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
    min-width: 200px;
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
  .pleco {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .pleco p {
    margin: 0;
  }
  .cats {
    width: 100%;
    border-collapse: collapse;
    background: var(--surface-2);
    border-radius: var(--radius);
  }
  .cats td {
    padding: 6px 10px;
    border-bottom: 1px solid var(--border);
  }
  .cats tr:last-child td {
    border-bottom: none;
  }
  .cats .arrow {
    color: var(--text-3);
    width: 16px;
  }
  .cats .num {
    text-align: right;
    white-space: nowrap;
  }
  .cats strong {
    margin-right: 6px;
  }
  .hint {
    margin-top: -6px !important;
    padding-left: 24px;
  }
  .deckrow {
    display: flex;
    align-items: center;
    gap: 2px;
  }
  .deckrow .deck {
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
  .crumbs {
    margin: 0 0 2px;
  }
  .dupes {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .dupwords {
    color: var(--text-2);
  }
  .deckform {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .formbtns {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 6px;
  }
</style>
