<script lang="ts">
  // Pick decks to study, one level at a time: subdecks stay out of the way until you
  // open their parent. Selecting a deck includes everything inside it.
  import type { Deck } from "../lib/api";
  import { t } from "../lib/i18n.svelte";

  let { decks, selected = $bindable() }: { decks: Deck[]; selected: number[] } = $props();

  /** The deck whose subdecks are shown (null = top level). */
  let level = $state<number | null>(null);

  const byId = $derived(new Map(decks.map((d) => [d.id, d])));
  const children = $derived(decks.filter((d) => (d.parentId ?? null) === level));
  const trail = $derived.by(() => {
    const out: Deck[] = [];
    for (let id = level; id !== null; id = byId.get(id)?.parentId ?? null) {
      const d = byId.get(id);
      if (!d) break;
      out.unshift(d);
    }
    return out;
  });

  // If the open deck disappears (deleted elsewhere), go back to the top.
  $effect(() => {
    if (level !== null && !byId.has(level)) level = null;
  });

  function ancestors(d: Deck): number[] {
    const out: number[] = [];
    for (let p = d.parentId ?? null; p !== null; p = byId.get(p)?.parentId ?? null) out.push(p);
    return out;
  }

  /** Included because a deck it is inside is selected. */
  const included = (d: Deck) => ancestors(d).some((a) => selected.includes(a));
  /** How many selected decks are somewhere inside `d`. */
  const selectedInside = (d: Deck) =>
    selected.filter((id) => {
      const s = byId.get(id);
      return s ? ancestors(s).includes(d.id) : false;
    }).length;

  function toggle(d: Deck) {
    if (selected.includes(d.id)) {
      selected = selected.filter((id) => id !== d.id);
    } else {
      // Selecting a parent covers its subdecks: drop them from the list.
      selected = [...selected.filter((id) => !(byId.get(id) && ancestors(byId.get(id)!).includes(d.id))), d.id];
    }
  }
</script>

<div class="picker">
  <div class="summary">
    {#if selected.length === 0}
      <span class="muted small">{t("picker.studyingAll")}</span>
    {:else}
      {#each selected as id (id)}
        {@const d = byId.get(id)}
        {#if d}
          <span class="pick">
            {d.path}
            <button class="x" aria-label={t("picker.remove", { name: d.path })} onclick={() => toggle(d)}>×</button>
          </span>
        {/if}
      {/each}
      <button class="btn ghost small" onclick={() => (selected = [])}>{t("picker.clear")}</button>
    {/if}
  </div>

  <nav class="crumbs small" aria-label={t("picker.location")}>
    <button class:here={level === null} onclick={() => (level = null)}>{t("study.allDecks")}</button>
    {#each trail as d (d.id)}
      <span class="sep" aria-hidden="true">›</span>
      <button class:here={level === d.id} onclick={() => (level = d.id)}>{d.name}</button>
    {/each}
  </nav>

  <ul class="list">
    {#each children as d (d.id)}
      {@const on = selected.includes(d.id)}
      {@const inc = !on && included(d)}
      {@const inside = selectedInside(d)}
      <li class:on class:inc>
        <label class="row">
          <input type="checkbox" checked={on || inc} disabled={inc} onchange={() => toggle(d)} />
          <span class="name">{d.name}</span>
          {#if inc}<span class="tag small">{t("picker.included")}</span>{/if}
          {#if inside && !on}<span class="tag small">{t("picker.selectedInside", { n: inside })}</span>{/if}
          <span class="counts small">
            {#if d.totalDue}<span class="due">{d.totalDue}</span>{/if}
            <span class="faint">{d.totalCards}</span>
          </span>
        </label>
        {#if d.childCount}
          <button
            class="dig"
            title={t("picker.open", { name: d.name })}
            aria-label={t("picker.open", { name: d.name })}
            onclick={() => (level = d.id)}
          >
            <span class="small faint">{t("picker.subdecks", { n: d.childCount })}</span>
            <span class="chev" aria-hidden="true">›</span>
          </button>
        {/if}
      </li>
    {/each}
  </ul>
</div>

<style>
  .picker {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .summary {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    min-height: 28px;
  }
  .pick {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 4px 2px 10px;
    border-radius: 999px;
    background: var(--accent-soft);
    color: var(--accent);
    font-size: 13px;
  }
  .x {
    border: none;
    background: transparent;
    color: inherit;
    font-size: 15px;
    line-height: 1;
    padding: 0 4px;
    border-radius: 999px;
  }
  .x:hover {
    background: rgb(0 0 0 / 0.08);
  }
  .btn.small {
    height: 26px;
    padding: 0 10px;
    font-size: 12.5px;
  }
  .crumbs {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 2px;
  }
  .crumbs button {
    border: none;
    background: transparent;
    padding: 2px 6px;
    border-radius: 6px;
    color: var(--accent);
  }
  .crumbs button.here {
    color: var(--text);
    font-weight: 600;
  }
  .crumbs button:not(.here):hover {
    background: var(--surface-2);
  }
  .sep {
    color: var(--text-3);
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    max-height: 280px;
    overflow-y: auto;
  }
  li {
    display: flex;
    align-items: center;
    border-bottom: 1px solid var(--border);
  }
  li:last-child {
    border-bottom: none;
  }
  li.on {
    background: var(--accent-soft);
  }
  .row {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 12px;
    cursor: pointer;
  }
  .row input {
    width: 16px;
    height: 16px;
    margin: 0;
    accent-color: var(--accent);
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  li.inc .name {
    color: var(--text-2);
  }
  .tag {
    color: var(--accent);
    white-space: nowrap;
  }
  .counts {
    margin-left: auto;
    display: flex;
    gap: 8px;
    font-variant-numeric: tabular-nums;
  }
  .due {
    color: var(--ok);
    font-weight: 600;
  }
  .dig {
    display: flex;
    align-items: center;
    gap: 6px;
    align-self: stretch;
    border: none;
    border-left: 1px solid var(--border);
    background: transparent;
    padding: 0 12px;
  }
  .dig:hover {
    background: var(--surface-2);
  }
  .chev {
    font-size: 18px;
    color: var(--text-2);
  }
</style>
