<script module lang="ts">
  // What was typed last: kept while the app runs, so switching pages doesn't lose it.
  let lastQuery = "";
</script>

<script lang="ts">
  import { onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { api, errorText, type Deck, type DictEntry, type DictStatus } from "../lib/api";
  import { t } from "../lib/i18n.svelte";
  import { fmtNum } from "../lib/format";
  import { speak } from "../lib/speech";
  import { voice } from "../lib/voice.svelte";
  import { showToast } from "../lib/toast.svelte";
  import Hanzi from "../components/Hanzi.svelte";
  import Pinyin from "../components/Pinyin.svelte";
  import Icon from "../components/Icon.svelte";
  import CardEditor from "../components/CardEditor.svelte";

  let status = $state<DictStatus | null>(null);
  let query = $state(lastQuery);
  let results = $state<DictEntry[]>([]);
  let searching = $state(false);
  let installing = $state(false);
  let installError = $state("");
  let decks = $state<Deck[]>([]);
  let adding = $state<DictEntry | null>(null);
  let input: HTMLInputElement | undefined = $state();

  onMount(async () => {
    [status, decks] = await Promise.all([api.dictStatus(), api.listDecks()]);
    input?.focus();
    input?.setSelectionRange(query.length, query.length);
  });

  let timer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    lastQuery = query;
    const q = query.trim();
    clearTimeout(timer);
    if (!q || !status?.entries) {
      results = [];
      return;
    }
    timer = setTimeout(async () => {
      searching = true;
      try {
        const r = await api.dictSearch(q, 80);
        if (query.trim() === q) results = r;
      } finally {
        searching = false;
      }
    }, 160);
  });

  async function install(fromFile: boolean) {
    installError = "";
    let path: string | null = null;
    if (fromFile) {
      const picked = await open({
        multiple: false,
        filters: [{ name: "CC-CEDICT", extensions: ["u8", "txt", "gz"] }],
      });
      if (!picked) return;
      path = picked as string;
    }
    installing = true;
    try {
      const n = path ? await api.dictImportFile(path) : await api.dictDownload();
      status = await api.dictStatus();
      showToast(t("dict.installed", { n: fmtNum(n) }));
      input?.focus();
    } catch (e) {
      installError = errorText(e);
    } finally {
      installing = false;
    }
  }
</script>

<div class="page">
  <div class="page-head">
    <div>
      <h1>{t("dict.title")}</h1>
      {#if status?.entries}<p class="muted small">{t("dict.entries", { n: fmtNum(status.entries) })}</p>{/if}
    </div>
  </div>

  {#if status && !status.entries}
    <div class="install card-surface">
      <div class="glyph han">词</div>
      <h2>{t("dict.notInstalled")}</h2>
      <p class="muted">{t("dict.installHint")}</p>
      {#if installing}
        <div class="notice">{t("dict.downloading")}</div>
      {:else}
        <div class="row">
          <button class="btn primary lg" onclick={() => install(false)}>
            <Icon name="download" size={16} />{t("dict.download")}
          </button>
          <button class="btn lg" onclick={() => install(true)}>
            <Icon name="folder" size={16} />{t("dict.importFile")}
          </button>
        </div>
      {/if}
      {#if installError}<div class="notice err">{installError}</div>{/if}
      <p class="faint small">{t("dict.attribution")}</p>
    </div>
  {:else if status}
    <div class="searchbox">
      <Icon name="search" size={18} />
      <input
        bind:this={input}
        type="search"
        placeholder={t("dict.placeholder")}
        bind:value={query}
        lang="zh-CN"
        autocapitalize="off"
        autocorrect="off"
        spellcheck="false"
      />
    </div>

    {#if !query.trim()}
      <p class="faint hint">{t("dict.hint")}</p>
    {:else if !searching && results.length === 0}
      <p class="muted hint">{t("dict.noResults")}</p>
    {/if}

    <ul class="results">
      {#each results as e (e.id)}
        <li class="entry card-surface">
          <div class="word">
            <Hanzi hanzi={e.simplified} pinyin={e.pinyin} size={28} />
            {#if e.traditional !== e.simplified}
              <span class="trad han faint" title={t("dict.traditional")}>{e.traditional}</span>
            {/if}
          </div>
          <div class="body">
            <div class="py"><Pinyin text={e.pinyin} /></div>
            <ol class="defs">
              {#each e.definitions as d, i (i)}<li>{d}</li>{/each}
            </ol>
          </div>
          <div class="acts">
            <button class="btn ghost icon" onclick={() => speak(e.simplified)} aria-label={t("study.play")} disabled={!voice.available} title={voice.available ? undefined : t("speech.noVoice")}>
              <Icon name="speaker" />
            </button>
            {#if e.inCards}
              <span class="badge review"><Icon name="check" size={12} /> {t("dict.inCards")}</span>
            {/if}
            <button class="btn" onclick={() => (adding = e)} aria-label={t("dict.add")}><Icon name="plus" size={16} /><span class="lbl">{t("dict.add")}</span></button>
          </div>
        </li>
      {/each}
    </ul>

    <p class="faint small attrib">{t("dict.attribution")}</p>
  {/if}
</div>

{#if adding}
  <CardEditor
    {decks}
    deckId={decks[0]?.id ?? null}
    prefill={{
      hanzi: adding.simplified,
      pinyin: adding.pinyin,
      meaning: adding.definitions.slice(0, 4).join("; "),
    }}
    onclose={() => (adding = null)}
    onsaved={(card) => {
      results = results.map((r) => (r.simplified === card.hanzi ? { ...r, inCards: true } : r));
      showToast(t("editor.saved"));
    }}
  />
{/if}

<style>
  .install {
    max-width: 560px;
    margin: 30px auto;
    padding: 36px;
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 14px;
  }
  .glyph {
    width: 64px;
    height: 64px;
    border-radius: 14px;
    background: var(--accent-soft);
    color: var(--accent);
    font-size: 34px;
    display: grid;
    place-items: center;
  }
  .row {
    display: flex;
    gap: 10px;
    flex-wrap: wrap;
    justify-content: center;
  }
  .searchbox {
    position: relative;
    color: var(--text-3);
    margin-bottom: 16px;
  }
  .searchbox :global(svg) {
    position: absolute;
    left: 14px;
    top: 14px;
  }
  .searchbox input {
    font-size: 17px;
    padding: 11px 14px 11px 42px;
    border-radius: 12px;
  }
  .hint {
    padding: 8px 4px;
  }
  .results {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .entry {
    display: grid;
    grid-template-columns: 170px 1fr auto;
    gap: 18px;
    padding: 14px 18px;
    border-radius: var(--radius);
    align-items: start;
  }
  .word {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .trad {
    font-size: 15px;
  }
  .py {
    font-size: 15px;
    margin-bottom: 4px;
  }
  .defs {
    margin: 0;
    padding-left: 20px;
    color: var(--text-2);
  }
  .defs li::marker {
    color: var(--text-3);
    font-size: 12px;
  }
  .acts {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .attrib {
    margin-top: 22px;
    text-align: center;
  }
  /* a phone: the word and its buttons on top, the pinyin and meanings across the width below */
  @media (max-width: 640px) {
    .entry {
      grid-template-columns: minmax(0, 1fr) auto;
      gap: 6px 12px;
      padding: 12px 14px;
    }
    .body {
      grid-column: 1 / -1;
    }
    .acts {
      grid-column: 2;
      grid-row: 1;
    }
    .acts .lbl,
    .hint {
      display: none;
    }
  }
</style>
