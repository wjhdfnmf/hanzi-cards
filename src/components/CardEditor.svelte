<script lang="ts">
  import { api, errorText, type Card, type Deck, type DictEntry } from "../lib/api";
  import { t } from "../lib/i18n.svelte";
  import { hasHanzi, numberedToMarks } from "../lib/pinyin";
  import { showToast } from "../lib/toast.svelte";
  import Modal from "./Modal.svelte";
  import Hanzi from "./Hanzi.svelte";
  import Pinyin from "./Pinyin.svelte";

  let {
    card = null,
    decks,
    deckId,
    prefill = null,
    onclose,
    onsaved,
  }: {
    card?: Card | null;
    decks: Deck[];
    deckId: number | null;
    prefill?: { hanzi: string; pinyin: string; meaning: string } | null;
    onclose: () => void;
    onsaved: (card: Card) => void;
  } = $props();

  // Initial values come from props once; the form then owns its state.
  const init = () => ({
    deck: card?.deckId ?? deckId ?? decks[0]?.id ?? 0,
    hanzi: card?.hanzi ?? prefill?.hanzi ?? "",
    pinyin: card?.pinyin ?? prefill?.pinyin ?? "",
    meaning: card?.meaning ?? prefill?.meaning ?? "",
    notes: card?.notes ?? "",
    tags: card?.tags.join(", ") ?? "",
  });
  const start = init();
  let form = $state(start);
  let suggestions = $state<DictEntry[]>([]);
  let duplicates = $state<string[]>([]);
  let allTags = $state<string[]>([]);
  let saving = $state(false);
  let error = $state("");
  let hanziInput: HTMLInputElement | undefined = $state();
  // svelte-ignore state_referenced_locally
  const isNew = card === null; // the editor is re-created for each card

  api.listTags().then((tags) => (allTags = tags));

  let lookupTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    const h = form.hanzi.trim();
    clearTimeout(lookupTimer);
    if (!h || !hasHanzi(h)) {
      suggestions = [];
      duplicates = [];
      return;
    }
    lookupTimer = setTimeout(async () => {
      const [entries, dups] = await Promise.all([api.dictLookup(h), api.cardsByHanzi(h)]);
      if (form.hanzi.trim() !== h) return;
      suggestions = entries.slice(0, 5);
      duplicates = dups.filter((d) => d.card.id !== card?.id).map((d) => d.deckName);
      if (isNew && entries.length && !form.pinyin.trim() && !form.meaning.trim()) use(entries[0]);
    }, 250);
  });

  function use(e: DictEntry) {
    form.pinyin = e.pinyin;
    form.meaning = e.definitions.slice(0, 4).join("; ");
  }

  async function save(addAnother = false) {
    error = "";
    if (!form.hanzi.trim()) {
      hanziInput?.focus();
      return;
    }
    saving = true;
    try {
      const input = {
        deckId: form.deck,
        hanzi: form.hanzi,
        pinyin: numberedToMarks(form.pinyin),
        meaning: form.meaning,
        notes: form.notes,
        tags: form.tags.split(",").map((s) => s.trim()).filter(Boolean),
      };
      const saved = card ? await api.updateCard(card.id, input) : await api.createCard(input);
      onsaved(saved);
      if (addAnother) {
        showToast(t("editor.saved"));
        form = { ...form, hanzi: "", pinyin: "", meaning: "", notes: "" };
        suggestions = [];
        hanziInput?.focus();
      } else {
        onclose();
      }
    } catch (e) {
      error = errorText(e);
    } finally {
      saving = false;
    }
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      save(isNew);
    }
  }

  $effect(() => {
    hanziInput?.focus();
  });
</script>

<Modal title={isNew ? t("editor.newCard") : t("editor.editCard")} {onclose} width={620}>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="form" {onkeydown}>
    <label class="field">
      <span>{t("editor.deck")}</span>
      <select bind:value={form.deck}>
        {#each decks as d (d.id)}<option value={d.id}>{d.name}</option>{/each}
      </select>
    </label>

    <div class="row">
      <label class="field grow">
        <span>{t("editor.hanzi")}</span>
        <input class="han big" type="text" bind:this={hanziInput} bind:value={form.hanzi} lang="zh-CN" />
      </label>
      <label class="field grow">
        <span>{t("editor.pinyin")}</span>
        <input
          type="text"
          bind:value={form.pinyin}
          onblur={() => (form.pinyin = numberedToMarks(form.pinyin))}
          placeholder="ni3 hao3"
        />
        <small class="faint">{t("editor.pinyinHint")}</small>
      </label>
    </div>

    {#if form.hanzi.trim() && form.pinyin.trim()}
      <div class="preview">
        <Hanzi hanzi={form.hanzi} pinyin={numberedToMarks(form.pinyin)} size={30} />
        <Pinyin text={numberedToMarks(form.pinyin)} />
      </div>
    {/if}

    {#if duplicates.length}
      <div class="notice small">{t("editor.duplicate", { decks: duplicates.join(", ") })}</div>
    {/if}

    {#if suggestions.length}
      <div class="suggest">
        <div class="small faint">{t("editor.fromDict")}</div>
        {#each suggestions as s (s.id)}
          <div class="sugg">
            <Pinyin text={s.pinyin} />
            <span class="defs">{s.definitions.slice(0, 4).join("; ")}</span>
            <button class="btn ghost small" type="button" onclick={() => use(s)}>{t("editor.use")}</button>
          </div>
        {/each}
      </div>
    {/if}

    <label class="field">
      <span>{t("editor.meaning")}</span>
      <input type="text" bind:value={form.meaning} />
    </label>

    <label class="field">
      <span>{t("editor.notes")}</span>
      <textarea bind:value={form.notes} rows="2"></textarea>
    </label>

    <label class="field">
      <span>{t("editor.tags")}</span>
      <input type="text" bind:value={form.tags} list="tag-list" placeholder={t("editor.tagsHint")} />
      <datalist id="tag-list">
        {#each allTags as tag (tag)}<option value={tag}></option>{/each}
      </datalist>
    </label>

    {#if error}<div class="notice err">{error}</div>{/if}
  </div>

  {#snippet footer()}
    <button class="btn ghost" onclick={onclose}>{t("common.cancel")}</button>
    {#if isNew}
      <button class="btn" disabled={saving} onclick={() => save(true)}>{t("editor.saveAdd")}</button>
    {/if}
    <button class="btn primary" disabled={saving} onclick={() => save(false)}>{t("common.save")}</button>
  {/snippet}
</Modal>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .row {
    display: flex;
    gap: 14px;
  }
  .grow {
    flex: 1;
  }
  .big {
    font-size: 22px;
    padding: 6px 10px;
  }
  .preview {
    display: flex;
    align-items: baseline;
    gap: 14px;
    padding: 4px 2px;
    font-size: 16px;
  }
  .suggest {
    display: flex;
    flex-direction: column;
    gap: 4px;
    background: var(--surface-2);
    border-radius: var(--radius);
    padding: 10px 12px;
  }
  .sugg {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .defs {
    flex: 1;
    color: var(--text-2);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .btn.small {
    height: 26px;
    padding: 0 10px;
    font-size: 12.5px;
  }
</style>
