<script lang="ts">
  import { onMount } from "svelte";
  import { open, save, ask } from "@tauri-apps/plugin-dialog";
  import { api, errorText, type Card, type CardRow, type Deck } from "../lib/api";
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
  let deckForm = $state<{ id: number | null; name: string; description: string } | null>(null);

  const deck = $derived(decks.find((d) => d.id === selectedDeck) ?? null);
  const allChecked = $derived(cards.length > 0 && checked.length === cards.length);

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
        selectedDeck = await api.createDeck(deckForm.name, deckForm.description);
      } else {
        await api.updateDeck(deckForm.id, deckForm.name, deckForm.description);
      }
      deckForm = null;
      await refresh();
    } catch (e) {
      showToast(errorText(e), "err");
    }
  }

  async function removeDeck() {
    if (!deck) return;
    const ok = await ask(t("decks.deleteConfirm", { name: deck.name, n: deck.cardCount }), {
      title: t("decks.deleteDeck"),
      kind: "warning",
    });
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

  async function importFile() {
    if (selectedDeck === null) {
      showToast(t("decks.chooseDeck"), "err");
      return;
    }
    const path = await open({
      multiple: false,
      filters: [{ name: "CSV / TSV", extensions: ["csv", "tsv", "txt"] }],
    });
    if (!path) return;
    try {
      const res = await api.importCards(selectedDeck, path as string);
      showToast(t("decks.imported", { n: res.imported, s: res.skipped }));
      await refresh();
    } catch (e) {
      showToast(errorText(e), "err");
    }
  }

  async function exportFile() {
    const name = (deck?.name ?? "all-cards").replace(/[\\/:*?"<>|]/g, "_");
    const path = await save({ defaultPath: `${name}.csv`, filters: [{ name: "CSV", extensions: ["csv"] }] });
    if (!path) return;
    try {
      const n = await api.exportCards(selectedDeck, path);
      showToast(t("decks.exported", { n }));
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
        onclick={() => (deckForm = { id: null, name: "", description: "" })}
      >
        <Icon name="plus" />
      </button>
    </div>
    <button class="deck" class:on={selectedDeck === null} onclick={() => (selectedDeck = null)}>
      <span class="dname">{t("decks.allCards")}</span>
      <span class="faint small">{decks.reduce((s, d) => s + d.cardCount, 0)}</span>
    </button>
    {#each decks as d (d.id)}
      <button class="deck" class:on={selectedDeck === d.id} onclick={() => (selectedDeck = d.id)}>
        <span class="dname">{d.name}</span>
        <span class="counts small">
          {#if d.dueCount}<span class="due" title={t("common.due")}>{d.dueCount}</span>{/if}
          {#if d.newCount}<span class="new" title={t("common.new")}>{d.newCount}</span>{/if}
          <span class="faint">{d.cardCount}</span>
        </span>
      </button>
    {/each}
  </aside>

  <section class="main">
    <div class="head">
      <div class="title">
        <h1>{deck?.name ?? t("decks.allCards")}</h1>
        {#if deck?.description}<p class="muted">{deck.description}</p>{/if}
      </div>
      <div class="tools">
        {#if deck}
          <button class="btn ghost icon" title={t("decks.renameDeck")} aria-label={t("decks.renameDeck")}
            onclick={() => (deckForm = { id: deck.id, name: deck.name, description: deck.description })}>
            <Icon name="edit" />
          </button>
          <button class="btn ghost icon" title={t("decks.deleteDeck")} aria-label={t("decks.deleteDeck")} onclick={removeDeck}>
            <Icon name="trash" />
          </button>
          <button class="btn" onclick={importFile} title={t("decks.importHint")}><Icon name="upload" size={16} />{t("decks.import")}</button>
        {/if}
        <button class="btn" onclick={exportFile}><Icon name="download" size={16} />{t("decks.export")}</button>
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
          {#each decks as d (d.id)}<option value={d.id}>{d.name}</option>{/each}
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

{#if deckForm}
  <Modal title={deckForm.id === null ? t("decks.newDeck") : t("decks.renameDeck")} onclose={() => (deckForm = null)} width={440}>
    <form class="deckform" onsubmit={(e) => { e.preventDefault(); saveDeck(); }}>
      <label class="field">
        <span>{t("decks.deckName")}</span>
        <!-- svelte-ignore a11y_autofocus -->
        <input type="text" bind:value={deckForm.name} autofocus />
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
