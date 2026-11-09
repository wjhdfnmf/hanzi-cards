<script lang="ts">
  import { tick, untrack } from "svelte";
  import { ask } from "@tauri-apps/plugin-dialog";
  import {
    api,
    errorText,
    type Candidate,
    type Card,
    type Deck,
    type DictEntry,
    type Segment,
  } from "../lib/api";
  import { t } from "../lib/i18n.svelte";
  import { hasHanzi, numberedToMarks, toneLastSyllable } from "../lib/pinyin";
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
  let segments = $state<Segment[]>([]);
  // Other cards with the same characters: { deck id, deck path }.
  let duplicates = $state<{ deckId: number; deck: string }[]>([]);
  const dupInThisDeck = $derived(duplicates.some((d) => d.deckId === form.deck));
  const dupOtherDecks = $derived([...new Set(duplicates.filter((d) => d.deckId !== form.deck).map((d) => d.deck))]);
  let allTags = $state<string[]>([]);
  let dictReady = $state(false);
  let saving = $state(false);
  let error = $state("");
  let hanziInput: HTMLInputElement | undefined = $state();
  // svelte-ignore state_referenced_locally
  const isNew = card === null; // the editor is re-created for each card

  api.listTags().then((tags) => (allTags = tags));
  api.dictStatus().then((s) => (dictReady = s.entries > 0));

  // ---------- typing characters by pinyin ----------
  // Letters typed in the characters field (before the caret) are treated as pinyin;
  // matching dictionary words are offered in a list, like an input method.

  type Run = { start: number; end: number; text: string };
  let candidates = $state<Candidate[]>([]);
  let candIndex = $state(0);
  let activeRun: Run | null = null;
  let composing = false; // an OS input method is composing; leave it alone
  let candSeq = 0;
  let candTimer: ReturnType<typeof setTimeout> | undefined;

  const PINYIN_RUN = /[a-zA-ZüÜ][a-zA-ZüÜ:'1-5]*$/;
  const PINYIN_ANY = /[a-zA-ZüÜ][a-zA-ZüÜ:'0-9]*/g;

  function pinyinRun(): Run | null {
    const el = hanziInput;
    if (!el) return null;
    const caret = el.selectionStart ?? form.hanzi.length;
    const m = form.hanzi.slice(0, caret).match(PINYIN_RUN);
    return m ? { start: caret - m[0].length, end: caret, text: m[0] } : null;
  }

  function closeCandidates() {
    candSeq++;
    clearTimeout(candTimer);
    candidates = [];
    activeRun = null;
  }

  function updateCandidates() {
    const run = composing || !dictReady ? null : pinyinRun();
    if (!run) {
      closeCandidates();
      return;
    }
    const seq = ++candSeq;
    clearTimeout(candTimer);
    candTimer = setTimeout(async () => {
      const list = await api.dictCandidates(run.text, 9).catch(() => []);
      if (seq !== candSeq) return;
      activeRun = run;
      candidates = list;
      candIndex = 0;
      placeList();
    }, 60);
  }

  // The list is positioned against the window so the dialog's scrolling body can't clip it.
  let listPos = $state("");
  function placeList() {
    const r = hanziInput?.getBoundingClientRect();
    if (!r) return;
    const top = r.bottom + 4;
    listPos = `top:${top}px;left:${r.left}px;min-width:${r.width}px;max-height:${Math.max(160, window.innerHeight - top - 12)}px`;
  }

  async function pick(c: Candidate) {
    const run = activeRun;
    if (!run) return;
    const rest = run.text.slice(c.consumed); // pinyin not used by this word stays for the next pick
    form.hanzi = form.hanzi.slice(0, run.start) + c.entry.simplified + rest + form.hanzi.slice(run.end);
    prefer = [...prefer.filter((id) => !c.parts.includes(id)), ...c.parts];
    const caret = run.start + c.entry.simplified.length + rest.length;
    closeCandidates();
    await tick();
    hanziInput?.focus();
    hanziInput?.setSelectionRange(caret, caret);
    if (rest) updateCandidates();
  }

  function onHanziKeydown(e: KeyboardEvent) {
    if (!candidates.length || composing || e.isComposing) return;
    const n = candidates.length;
    if (e.key === "ArrowDown") candIndex = (candIndex + 1) % n;
    else if (e.key === "ArrowUp") candIndex = (candIndex - 1 + n) % n;
    else if ((e.key === "Enter" && !e.ctrlKey && !e.metaKey) || e.key === " ") pick(candidates[candIndex]);
    else if (e.key === "Escape") {
      e.stopPropagation(); // close the list, not the dialog
      closeCandidates();
    } else return;
    e.preventDefault();
  }

  function onHanziKeyup(e: KeyboardEvent) {
    if (["ArrowLeft", "ArrowRight", "Home", "End"].includes(e.key)) updateCandidates();
  }

  // ---------- pinyin field: tone digits ----------
  // Typing 1-4 after a syllable puts that tone mark on it; 5 = neutral tone (no mark).

  function onPinyinInput(e: Event) {
    const ev = e as InputEvent;
    const el = ev.currentTarget as HTMLInputElement;
    if (ev.inputType !== "insertText" || !ev.data || !/^[1-5]$/.test(ev.data)) return;
    const caret = el.selectionStart ?? el.value.length;
    const toned = toneLastSyllable(el.value.slice(0, caret - 1), Number(ev.data));
    if (toned === null) return;
    el.value = toned + el.value.slice(caret);
    el.setSelectionRange(toned.length, toned.length);
    form.pinyin = el.value;
  }

  // ---------- automatic pinyin & meaning ----------
  // Filled from the dictionary as characters change. A field the user has edited keeps
  // their text; a field still holding the last automatic value is updated.

  let prefer = $state<number[]>([]); // dictionary entries the user chose (readings to keep)
  let lastAuto = { pinyin: "", meaning: "" };
  let checkExisting = !isNew; // editing: only treat values as automatic if they match

  const hanziText = $derived(form.hanzi.replace(PINYIN_ANY, "").trim());

  function meaningOf(e: DictEntry, max: number) {
    return e.definitions
      .filter((d) => !d.startsWith("CL:"))
      .slice(0, max)
      .join("; ");
  }

  /** A word's main sense, short: "you (informal, as opposed to …); you" -> "you". */
  function mainSense(e: DictEntry) {
    const first = meaningOf(e, 1).split(";")[0].trim();
    const bare = first.replace(/\s*\([^)]*\)/g, "").trim();
    return bare || first;
  }

  function autoValues(segs: Segment[]) {
    const words = segs.filter((s) => s.entry);
    if (segs.length === 1 && words.length === 1) {
      return { pinyin: words[0].entry!.pinyin, meaning: meaningOf(words[0].entry!, 4) };
    }
    return {
      pinyin: words.map((s) => s.entry!.pinyin).join(" "),
      meaning: words.map((s) => mainSense(s.entry!)).join(" + "),
    };
  }

  function applyAuto(v: { pinyin: string; meaning: string }) {
    if (checkExisting) {
      checkExisting = false;
      if (form.pinyin === v.pinyin) lastAuto.pinyin = v.pinyin;
      if (form.meaning === v.meaning) lastAuto.meaning = v.meaning;
      return;
    }
    if (!form.pinyin.trim() || form.pinyin === lastAuto.pinyin) form.pinyin = v.pinyin;
    if (!form.meaning.trim() || form.meaning === lastAuto.meaning) form.meaning = v.meaning;
    lastAuto = { ...v };
  }

  let lookupTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    const h = hanziText;
    const pref = [...prefer];
    clearTimeout(lookupTimer);
    if (!h || !hasHanzi(h)) {
      suggestions = [];
      segments = [];
      duplicates = [];
      if (!checkExisting) untrack(() => applyAuto({ pinyin: "", meaning: "" }));
      return;
    }
    lookupTimer = setTimeout(async () => {
      const [entries, segs, dups] = await Promise.all([
        api.dictLookup(h),
        api.dictSegment(h, pref),
        api.cardsByHanzi(h),
      ]);
      if (hanziText !== h) return;
      suggestions = entries.slice(0, 5);
      segments = segs;
      duplicates = dups.filter((d) => d.card.id !== card?.id).map((d) => ({ deckId: d.card.deckId, deck: d.deckName }));
      applyAuto(autoValues(segs));
    }, 150);
  });

  /** Use a dictionary entry for the whole word; later character changes keep updating. */
  function use(e: DictEntry) {
    form.pinyin = e.pinyin;
    form.meaning = meaningOf(e, 4);
    lastAuto = { pinyin: form.pinyin, meaning: form.meaning };
    checkExisting = false;
    prefer = [...prefer.filter((id) => id !== e.id), e.id];
  }

  const wordByWord = $derived(segments.filter((s) => s.entry).length > 1 ? segments : []);

  // ---------- saving ----------

  async function save(addAnother = false) {
    error = "";
    if (!form.hanzi.trim()) {
      hanziInput?.focus();
      return;
    }
    // Same word twice in one deck is almost always a mistake: ask first.
    const movedHere = isNew || card?.deckId !== form.deck || card?.hanzi !== form.hanzi.trim();
    if (dupInThisDeck && movedHere) {
      const ok = await ask(t("editor.dupConfirm", { hanzi: form.hanzi.trim() }), { kind: "warning" });
      if (!ok) return;
    }
    saving = true;
    try {
      const input = {
        deckId: form.deck,
        hanzi: form.hanzi.trim(),
        pinyin: numberedToMarks(form.pinyin).trim(),
        meaning: form.meaning.trim(),
        notes: form.notes,
        tags: form.tags.split(",").map((s) => s.trim()).filter(Boolean),
      };
      const saved = card ? await api.updateCard(card.id, input) : await api.createCard(input);
      onsaved(saved);
      if (addAnother) {
        showToast(t("editor.saved"));
        form = { ...form, hanzi: "", pinyin: "", meaning: "", notes: "" };
        lastAuto = { pinyin: "", meaning: "" };
        prefer = [];
        suggestions = [];
        segments = [];
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

  // Keep the candidate list under the field when the dialog scrolls or the window resizes.
  $effect(() => {
    const follow = () => candidates.length && placeList();
    document.addEventListener("scroll", follow, true);
    window.addEventListener("resize", follow);
    return () => {
      document.removeEventListener("scroll", follow, true);
      window.removeEventListener("resize", follow);
    };
  });
</script>

<Modal title={isNew ? t("editor.newCard") : t("editor.editCard")} {onclose} width={640}>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="form" {onkeydown}>
    <label class="field">
      <span>{t("editor.deck")}</span>
      <select bind:value={form.deck}>
        {#each decks as d (d.id)}<option value={d.id}>{"  ".repeat(d.depth)}{d.name}</option>{/each}
      </select>
    </label>

    <div class="row">
      <div class="field grow">
        <label for="editor-hanzi">{t("editor.hanzi")}</label>
        <div class="combo">
          <input
            id="editor-hanzi"
            class="han big"
            type="text"
            autocomplete="off"
            spellcheck="false"
            lang="zh-CN"
            role="combobox"
            aria-expanded={candidates.length > 0}
            aria-controls="editor-candidates"
            aria-activedescendant={candidates.length ? `editor-cand-${candIndex}` : undefined}
            bind:this={hanziInput}
            bind:value={form.hanzi}
            oninput={updateCandidates}
            onclick={updateCandidates}
            onkeydown={onHanziKeydown}
            onkeyup={onHanziKeyup}
            onblur={closeCandidates}
            oncompositionstart={() => {
              composing = true;
              closeCandidates();
            }}
            oncompositionend={() => {
              composing = false;
              updateCandidates();
            }}
          />
          {#if candidates.length}
            <ul
              class="cands"
              id="editor-candidates"
              role="listbox"
              aria-label={t("editor.candidates")}
              style={listPos}
            >
              {#each candidates as c, i (`${c.entry.id}:${c.entry.simplified}`)}
                <li
                  id="editor-cand-{i}"
                  role="option"
                  aria-selected={i === candIndex}
                  class:on={i === candIndex}
                  onmousedown={(e) => {
                    e.preventDefault();
                    pick(c);
                  }}
                  onmouseenter={() => (candIndex = i)}
                >
                  <span class="cand-han"><Hanzi hanzi={c.entry.simplified} pinyin={c.entry.pinyin} size={22} /></span>
                  <span class="cand-py"><Pinyin text={c.entry.pinyin} /></span>
                  <span class="cand-def">{meaningOf(c.entry, 2)}</span>
                </li>
              {/each}
              <li class="cand-help" role="presentation">{t("editor.candidateKeys")}</li>
            </ul>
          {/if}
        </div>
        <small class="faint">{dictReady ? t("editor.hanziHint") : t("editor.hanziNoDict")}</small>
      </div>
      <label class="field grow">
        <span>{t("editor.pinyin")}</span>
        <input
          type="text"
          autocomplete="off"
          spellcheck="false"
          bind:value={form.pinyin}
          oninput={onPinyinInput}
          onblur={() => (form.pinyin = numberedToMarks(form.pinyin))}
          placeholder="ni3 hao3"
        />
        <small class="faint">{t("editor.pinyinHint")}</small>
      </label>
    </div>

    {#if hanziText && form.pinyin.trim()}
      <div class="preview">
        <Hanzi hanzi={hanziText} pinyin={numberedToMarks(form.pinyin)} size={30} />
        <Pinyin text={numberedToMarks(form.pinyin)} />
      </div>
    {/if}

    {#if dupInThisDeck}
      <div class="notice err small" role="alert">{t("editor.dupSameDeck", { hanzi: form.hanzi.trim() })}</div>
    {/if}
    {#if dupOtherDecks.length}
      <div class="notice small">{t("editor.dupOtherDecks", { decks: dupOtherDecks.join(", ") })}</div>
    {/if}

    {#if wordByWord.length}
      <div class="suggest">
        <div class="small faint">{t("editor.wordByWord")}</div>
        <div class="words">
          {#each wordByWord as s, i (i)}
            {#if s.entry}
              <div class="word">
                <Hanzi hanzi={s.text} pinyin={s.entry.pinyin} size={20} />
                <Pinyin text={s.entry.pinyin} />
                <span class="defs">{meaningOf(s.entry, 1)}</span>
              </div>
            {/if}
          {/each}
        </div>
      </div>
    {/if}

    {#if suggestions.length}
      <div class="suggest">
        <div class="small faint">{t("editor.fromDict")}</div>
        {#each suggestions as s (s.id)}
          <div class="sugg">
            <Pinyin text={s.pinyin} />
            <span class="defs">{meaningOf(s, 4)}</span>
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
    min-width: 0;
  }
  .big {
    font-size: 22px;
    padding: 6px 10px;
  }
  .combo input {
    width: 100%;
  }
  .cands {
    position: fixed;
    z-index: 60;
    overflow-y: auto;
    width: max-content;
    max-width: min(460px, 80vw);
    margin: 0;
    padding: 4px;
    list-style: none;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: var(--shadow-lg);
  }
  .cands li[role="option"] {
    display: grid;
    grid-template-columns: auto auto 1fr;
    align-items: baseline;
    gap: 10px;
    padding: 5px 10px;
    border-radius: 6px;
    cursor: pointer;
  }
  .cands li.on {
    background: var(--accent-soft);
  }
  .cand-han {
    white-space: nowrap;
  }
  .cand-py {
    font-size: 13px;
    white-space: nowrap;
  }
  .cand-def {
    color: var(--text-2);
    font-size: 13px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 260px;
  }
  .cand-help {
    padding: 4px 10px 2px;
    font-size: 11.5px;
    color: var(--text-3);
    border-top: 1px solid var(--border);
    margin-top: 2px;
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
  .words {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 18px;
  }
  .word {
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
  }
  .defs {
    flex: 1;
    color: var(--text-2);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .word .defs {
    max-width: 200px;
  }
  .btn.small {
    height: 26px;
    padding: 0 10px;
    font-size: 12.5px;
  }
</style>
