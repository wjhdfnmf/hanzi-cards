<script lang="ts">
  import { onMount } from "svelte";
  import { open, save, ask } from "@tauri-apps/plugin-dialog";
  import {
    api,
    errorText,
    type Card,
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
  import Modal from "../components/Modal.svelte";
  import CardEditor from "../components/CardEditor.svelte";

  let decks = $state<Deck[]>([]);
  let selectedDeck = $state<number | null>(null); // null = all cards
  let cards = $state<CardRow[]>([]);
  let search = $state("");
  let checked = $state<number[]>([]);
  let loading = $state(true);

  let editing = $state<{ card: Card | null } | null>(null);
  let deckForm = $state<{ id: number | null; name: string; description: string; parentId: number | null } | null>(
    null,
  );

  const deck = $derived(decks.find((d) => d.id === selectedDeck) ?? null);
  const allChecked = $derived(cards.length > 0 && checked.length === cards.length);
  const deckPath = $derived(new Map(decks.map((d) => [d.id, d.path])));
  // Show which deck each card is in when the list mixes decks.
  const showDeckColumn = $derived(selectedDeck === null || (deck?.childCount ?? 0) > 0);

  // ---------- deck tree ----------

  const COLLAPSED_KEY = "hanzi-cards.collapsedDecks";
  let collapsed = $state<number[]>(loadCollapsed());

  function loadCollapsed(): number[] {
    try {
      return JSON.parse(localStorage.getItem(COLLAPSED_KEY) ?? "[]");
    } catch {
      return [];
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

  onMount(async () => {
    await loadDecks();
    if (decks.length) selectedDeck = decks[0].id;
  });

  async function loadDecks() {
    decks = await api.listDecks();
  }

  async function loadCards() {
    loading = true;
    try {
      cards = await api.listCards(selectedDeck, search);
      checked = checked.filter((id) => cards.some((c) => c.id === id));
    } finally {
      loading = false;
    }
  }

  let searchTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    void selectedDeck;
    void search;
    clearTimeout(searchTimer);
    searchTimer = setTimeout(loadCards, 120);
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
    await api.deleteDeck(deck.id);
    selectedDeck = null;
    await refresh();
    if (decks.length) selectedDeck = decks[0].id;
  }

  async function removeChecked() {
    const ok = await ask(t("decks.deleteCards", { n: checked.length }), { kind: "warning" });
    if (!ok) return;
    await api.deleteCards(checked);
    checked = [];
    await refresh();
  }

  async function moveChecked(e: Event) {
    const target = Number((e.currentTarget as HTMLSelectElement).value);
    (e.currentTarget as HTMLSelectElement).value = "";
    if (!target) return;
    await api.moveCards(checked, target);
    checked = [];
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
    checked = checked.includes(id) ? checked.filter((x) => x !== id) : [...checked, id];
  }
</script>

<div class="layout">
  <aside class="decklist">
    <div class="dl-head">
      <h2>{t("decks.title")}</h2>
      <button
        class="btn ghost icon"
        title={t("decks.newDeck")}
        aria-label={t("decks.newDeck")}
        onclick={() => (deckForm = { id: null, name: "", description: "", parentId: null })}
      >
        <Icon name="plus" />
      </button>
    </div>
    <button class="deck" class:on={selectedDeck === null} onclick={() => (selectedDeck = null)}>
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
        <button class="deck" class:on={selectedDeck === d.id} onclick={() => (selectedDeck = d.id)}>
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
        <button class="btn" onclick={importFile} title={t("decks.importHint")}><Icon name="upload" size={16} />{t("decks.import")}</button>
        <div class="menuwrap">
          <button class="btn" aria-haspopup="menu" aria-expanded={exportMenu} onclick={() => (exportMenu = !exportMenu)}>
            <Icon name="download" size={16} />{t("decks.export")}
          </button>
          {#if exportMenu}
            <div class="menu card-surface" role="menu">
              <button role="menuitem" onclick={() => exportFile("csv")}>{t("decks.exportCsv")}</button>
              <button role="menuitem" onclick={() => exportFile("pleco")}>{t("decks.exportPleco")}</button>
            </div>
          {/if}
        </div>
        {#if deck}
          <button class="btn" onclick={() => studyDeck(deck.id)}><Icon name="play" size={14} />{t("decks.studyDeck")}</button>
        {/if}
        <button class="btn primary" disabled={!decks.length} onclick={() => (editing = { card: null })}>
          <Icon name="plus" size={16} />{t("decks.addCard")}
        </button>
      </div>
    </div>

    <div class="filterbar">
      <div class="searchbox">
        <Icon name="search" size={16} />
        <input type="search" placeholder={t("decks.searchCards")} bind:value={search} />
      </div>
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
        <table>
          <thead>
            <tr>
              <th class="cb"><input type="checkbox" checked={allChecked} onchange={toggleAll} aria-label="Select all" /></th>
              <th>{t("col.hanzi")}</th>
              <th>{t("col.pinyin")}</th>
              <th class="wide">{t("col.meaning")}</th>
              {#if showDeckColumn}<th>{t("decks.colDeck")}</th>{/if}
              <th>{t("col.tags")}</th>
              <th>{t("col.status")}</th>
              <th>{t("col.next")}</th>
            </tr>
          </thead>
          <tbody>
            {#each cards as c (c.id)}
              <tr onclick={() => (editing = { card: c })} class:checked={checked.includes(c.id)}>
                <td class="cb" onclick={(e) => e.stopPropagation()}>
                  <input type="checkbox" checked={checked.includes(c.id)} onchange={() => toggleOne(c.id)} aria-label="Select" />
                </td>
                <td class="hz"><Hanzi hanzi={c.hanzi} pinyin={c.pinyin} size={20} /></td>
                <td><Pinyin text={c.pinyin} /></td>
                <td class="wide meaning">{c.meaning}</td>
                {#if showDeckColumn}<td class="small muted deckcol">{deckPath.get(c.deckId) ?? ""}</td>{/if}
                <td>{#each c.tags as tg (tg)}<span class="chip">{tg}</span> {/each}</td>
                <td><span class="badge {c.status}">{t(`status.${c.status}`)}</span></td>
                <td class="small muted">{dueLabel(c.nextDue)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  </section>
</div>

{#if editing}
  <CardEditor
    card={editing.card}
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
  .layout {
    display: grid;
    grid-template-columns: 240px 1fr;
    height: 100%;
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
  .empty {
    padding: 40px;
    text-align: center;
  }
  .table {
    overflow: hidden;
  }
  table {
    width: 100%;
    border-collapse: collapse;
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
  .hz {
    white-space: nowrap;
  }
  .meaning {
    max-width: 320px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
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
  .deckcol {
    white-space: nowrap;
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
