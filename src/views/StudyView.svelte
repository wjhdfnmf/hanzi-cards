<script lang="ts">
  import { onMount } from "svelte";
  import {
    api,
    errorText,
    type Deck,
    type Direction,
    type IntervalPreview,
    type StudyItem,
    type StudyMode,
  } from "../lib/api";
  import { t } from "../lib/i18n.svelte";
  import { intervalLabel, durationLabel, pct } from "../lib/format";
  import { nav } from "../lib/nav.svelte";
  import { settings } from "../lib/settings.svelte";
  import { speak } from "../lib/speech";
  import Hanzi from "../components/Hanzi.svelte";
  import Pinyin from "../components/Pinyin.svelte";
  import Icon from "../components/Icon.svelte";

  type Phase = "setup" | "session" | "done";
  const MODES: StudyMode[] = ["due", "new", "all", "difficult"];
  const DIRS: Direction[] = ["recognition", "recall", "listening"];
  const LIMITS = [10, 20, 50, 100, 0];
  const PREF_KEY = "study-selection";

  let phase = $state<Phase>("setup");
  let decks = $state<Deck[]>([]);
  let tags = $state<string[]>([]);
  let error = $state("");

  // Selection (remembered per device).
  let sel = $state({
    deckIds: [] as number[],
    tags: [] as string[],
    directions: ["recognition"] as Direction[],
    mode: "due" as StudyMode,
    limit: 20,
    shuffle: true,
  });
  let matching = $state<number | null>(null);
  let dueCount = $state(0);
  let newRemaining = $state(0);

  // Session state.
  let queue = $state<StudyItem[]>([]);
  let total = $state(0);
  let done = $state(0);
  let revealed = $state(false);
  let previews = $state<IntervalPreview[]>([]);
  let shownAt = 0;
  let busy = false;
  let result = $state({ reviewed: 0, correct: 0, ms: 0 });
  const current = $derived(queue[0] ?? null);

  onMount(async () => {
    try {
      const saved = JSON.parse(localStorage.getItem(PREF_KEY) ?? "null");
      if (saved) sel = { ...sel, ...saved };
    } catch {
      /* storage unavailable */
    }
    if (nav.studyDeckIds) {
      sel.deckIds = nav.studyDeckIds;
      nav.studyDeckIds = null;
    }
    [decks, tags] = await Promise.all([api.listDecks(), api.listTags()]);
    // Forget decks/tags that no longer exist.
    sel.deckIds = sel.deckIds.filter((id) => decks.some((d) => d.id === id));
    sel.tags = sel.tags.filter((tg) => tags.includes(tg));
  });

  // Recount matching cards whenever the selection changes.
  let countTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    const filter = $state.snapshot(sel);
    try {
      localStorage.setItem(PREF_KEY, JSON.stringify(filter));
    } catch {
      /* ignore */
    }
    clearTimeout(countTimer);
    countTimer = setTimeout(async () => {
      if (!filter.directions.length) {
        matching = 0;
        return;
      }
      try {
        const info = await api.buildSession({ ...filter });
        matching = info.items.length;
        dueCount = info.dueCount;
        newRemaining = info.newRemainingToday;
      } catch (e) {
        error = errorText(e);
      }
    }, 150);
  });

  function toggle<T>(list: T[], v: T): T[] {
    return list.includes(v) ? list.filter((x) => x !== v) : [...list, v];
  }

  function shuffle<T>(a: T[]): T[] {
    const arr = [...a];
    for (let i = arr.length - 1; i > 0; i--) {
      const j = Math.floor(Math.random() * (i + 1));
      [arr[i], arr[j]] = [arr[j], arr[i]];
    }
    return arr;
  }

  async function start() {
    error = "";
    try {
      const info = await api.buildSession($state.snapshot(sel));
      if (!info.items.length) {
        error = t("study.nothing");
        return;
      }
      queue = sel.shuffle ? shuffle(info.items) : info.items;
      total = queue.length;
      done = 0;
      result = { reviewed: 0, correct: 0, ms: 0 };
      phase = "session";
      await show();
    } catch (e) {
      error = errorText(e);
    }
  }

  async function show() {
    revealed = false;
    previews = [];
    const item = queue[0];
    if (!item) {
      phase = "done";
      return;
    }
    shownAt = Date.now();
    if (item.direction === "listening") speak(item.card.hanzi);
    previews = await api.previewIntervals(item.card.id, item.direction);
  }

  function reveal() {
    if (!current || revealed) return;
    revealed = true;
    if (settings.autoplayAudio && current.direction !== "listening") speak(current.card.hanzi);
  }

  async function rate(rating: number) {
    if (!current || !revealed || busy) return;
    busy = true;
    const item = current;
    const ms = Date.now() - shownAt;
    try {
      const res = await api.answerCard(item.card.id, item.direction, rating, ms);
      result.reviewed++;
      result.ms += Math.min(ms, 10 * 60 * 1000);
      if (rating > 1) result.correct++;
      const rest = queue.slice(1);
      // Short learning steps come back later in this session.
      if (res.days === null && res.intervalSecs < 30 * 60) {
        const pos = Math.min(rest.length, 3 + Math.floor(Math.random() * 3));
        rest.splice(pos, 0, item);
      } else {
        done++;
      }
      queue = rest;
      await show();
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }

  function endSession() {
    phase = result.reviewed ? "done" : "setup";
  }

  function onkeydown(e: KeyboardEvent) {
    if (phase !== "session" || e.ctrlKey || e.metaKey || e.altKey) return;
    if ((e.target as HTMLElement)?.closest("input, textarea, select")) return;
    if (e.key === " " || e.key === "Enter") {
      e.preventDefault();
      if (!revealed) reveal();
      else rate(3);
    } else if (revealed && ["1", "2", "3", "4"].includes(e.key)) {
      rate(Number(e.key));
    } else if (e.key.toLowerCase() === "r" && current) {
      speak(current.card.hanzi);
    }
  }

  const RATINGS = [
    { n: 1, key: "study.again", cls: "again" },
    { n: 2, key: "study.hard", cls: "hard" },
    { n: 3, key: "study.good", cls: "good" },
    { n: 4, key: "study.easy", cls: "easy" },
  ] as const;
</script>

<svelte:window {onkeydown} />

{#if phase === "setup"}
  <div class="page">
    <div class="page-head">
      <div>
        <h1>{t("study.title")}</h1>
        <p class="muted">{t("study.subtitle")}</p>
      </div>
    </div>

    <div class="setup">
      <div class="options">
        <section>
          <h3>{t("study.mode")}</h3>
          <div class="modes">
            {#each MODES as m (m)}
              <button class="mode" class:on={sel.mode === m} onclick={() => (sel.mode = m)}>
                <strong>{t(`study.mode.${m}`)}</strong>
                <span class="small muted">{t(`study.mode.${m}.hint`)}</span>
              </button>
            {/each}
          </div>
        </section>

        <section>
          <h3>{t("study.decks")}</h3>
          <div class="chips">
            <button class="chip" class:on={sel.deckIds.length === 0} onclick={() => (sel.deckIds = [])}>
              {t("study.allDecks")}
            </button>
            {#each decks as d (d.id)}
              <button
                class="chip"
                class:on={sel.deckIds.includes(d.id)}
                onclick={() => (sel.deckIds = toggle(sel.deckIds, d.id))}
              >
                {d.name}
                <span class="faint">{d.cardCount}</span>
              </button>
            {/each}
          </div>
        </section>

        {#if tags.length}
          <section>
            <h3>{t("study.tags")}</h3>
            <div class="chips">
              <button class="chip" class:on={sel.tags.length === 0} onclick={() => (sel.tags = [])}>
                {t("study.anyTag")}
              </button>
              {#each tags as tg (tg)}
                <button class="chip" class:on={sel.tags.includes(tg)} onclick={() => (sel.tags = toggle(sel.tags, tg))}>
                  {tg}
                </button>
              {/each}
            </div>
          </section>
        {/if}

        <section>
          <h3>{t("study.directions")}</h3>
          <div class="chips">
            {#each DIRS as d (d)}
              <button
                class="chip"
                class:on={sel.directions.includes(d)}
                onclick={() => (sel.directions = toggle(sel.directions, d))}
              >
                {t(`dir.${d}`)}
              </button>
            {/each}
          </div>
        </section>

        <section class="inline">
          <label class="field">
            <span>{t("study.limit")}</span>
            <select bind:value={sel.limit}>
              {#each LIMITS as l (l)}<option value={l}>{l === 0 ? t("study.limit.none") : l}</option>{/each}
            </select>
          </label>
          <label class="check">
            <input type="checkbox" bind:checked={sel.shuffle} />
            {t("study.shuffle")}
          </label>
        </section>
      </div>

      <aside class="summary card-surface">
        <div class="nums">
          <div>
            <div class="num">{dueCount}</div>
            <div class="small muted">{t("study.dueToday")}</div>
          </div>
          <div>
            <div class="num">{newRemaining}</div>
            <div class="small muted">{t("study.newAvailable")}</div>
          </div>
        </div>
        <p class="muted small">{t("study.matching", { n: matching ?? "…" })}</p>
        <button class="btn primary lg" disabled={!matching} onclick={start}>
          <Icon name="play" size={16} />
          {t("study.start")}
        </button>
        {#if error}<div class="notice err small">{error}</div>{/if}
      </aside>
    </div>
  </div>
{:else if phase === "session" && current}
  {@const c = current.card}
  {@const dir = current.direction}
  <div class="session">
    <div class="topbar">
      <button class="btn ghost" onclick={endSession}>{t("study.end")}</button>
      <div class="bar" aria-hidden="true"><div style="width:{total ? (done / total) * 100 : 0}%"></div></div>
      <span class="small muted">{t("study.progress", { done, total })}</span>
    </div>

    <div class="stage">
      <div class="flash card-surface">
        <span class="dirtag small faint">{t(`dir.${dir}`)}</span>

        <div class="front">
          {#if dir === "recognition"}
            <Hanzi hanzi={c.hanzi} pinyin={c.pinyin} colored={revealed} size={c.hanzi.length > 4 ? 56 : 84} />
          {:else if dir === "recall"}
            <div class="meaning-q">{c.meaning || "—"}</div>
          {:else}
            <button class="listen" onclick={() => speak(c.hanzi)} aria-label={t("study.play")}>
              <Icon name="speaker" size={40} />
            </button>
          {/if}
          {#if !revealed}
            <p class="faint prompt">
              {dir === "recognition" ? t("study.whatMeaning") : dir === "recall" ? t("study.howWrite") : t("study.whatHear")}
            </p>
          {/if}
        </div>

        {#if revealed}
          <div class="back">
            {#if dir !== "recognition"}
              <Hanzi hanzi={c.hanzi} pinyin={c.pinyin} size={c.hanzi.length > 4 ? 48 : 72} />
            {/if}
            <div class="py-line">
              <span class="py-big"><Pinyin text={c.pinyin} /></span>
              <button class="btn ghost icon" onclick={() => speak(c.hanzi)} aria-label={t("study.play")}>
                <Icon name="speaker" />
              </button>
            </div>
            {#if dir !== "recall"}<div class="meaning">{c.meaning}</div>{/if}
            {#if c.notes}<div class="notes small muted">{c.notes}</div>{/if}
          </div>
        {/if}
      </div>

      <div class="actions">
        {#if !revealed}
          <button class="btn primary lg reveal" onclick={reveal}>{t("study.showAnswer")}</button>
        {:else}
          <div class="ratings">
            {#each RATINGS as r (r.n)}
              <button class="rate {r.cls}" onclick={() => rate(r.n)}>
                <span class="ivl small">{previews[r.n - 1] ? intervalLabel(previews[r.n - 1].secs, previews[r.n - 1].days) : ""}</span>
                <strong>{t(r.key)}</strong>
                <span class="key faint">{r.n}</span>
              </button>
            {/each}
          </div>
        {/if}
        <p class="faint small keys">{t("study.keys")}</p>
        {#if error}<div class="notice err small">{error}</div>{/if}
      </div>
    </div>
  </div>
{:else}
  <div class="page">
    <div class="donebox card-surface">
      <div class="seal han">好</div>
      <h1>{t("study.done")}</h1>
      <div class="nums three">
        <div>
          <div class="num">{result.reviewed}</div>
          <div class="small muted">{t("study.reviewed")}</div>
        </div>
        <div>
          <div class="num">{pct(result.reviewed ? result.correct / result.reviewed : null)}</div>
          <div class="small muted">{t("study.correct")}</div>
        </div>
        <div>
          <div class="num">{durationLabel(result.ms)}</div>
          <div class="small muted">{t("study.time")}</div>
        </div>
      </div>
      <div class="row">
        <button class="btn" onclick={() => (phase = "setup")}>{t("study.back")}</button>
        <button class="btn primary" onclick={start}>{t("study.again.btn")}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .setup {
    display: grid;
    grid-template-columns: 1fr 300px;
    gap: 28px;
    align-items: start;
  }
  .options {
    display: flex;
    flex-direction: column;
    gap: 22px;
  }
  section h3 {
    margin-bottom: 9px;
  }
  section.inline {
    display: flex;
    align-items: flex-end;
    gap: 22px;
  }
  section.inline .field {
    width: 160px;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 7px;
  }
  .modes {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(190px, 1fr));
    gap: 10px;
  }
  .mode {
    text-align: left;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 12px 14px;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    background: var(--surface);
  }
  .mode:hover {
    border-color: var(--text-3);
  }
  .mode.on {
    border-color: var(--accent);
    background: var(--accent-soft);
    box-shadow: inset 0 0 0 1px var(--accent);
  }
  .summary {
    position: sticky;
    top: 24px;
    padding: 22px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .nums {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }
  .nums.three {
    grid-template-columns: repeat(3, 1fr);
    width: 100%;
    margin: 18px 0 24px;
  }
  .num {
    font-size: 30px;
    font-weight: 650;
    font-variant-numeric: tabular-nums;
  }

  /* session */
  .session {
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  .topbar {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 14px 24px;
  }
  .bar {
    flex: 1;
    height: 6px;
    border-radius: 6px;
    background: var(--surface-3);
    overflow: hidden;
  }
  .bar div {
    height: 100%;
    background: var(--accent);
    border-radius: 6px;
    transition: width 0.25s ease;
  }
  .stage {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 22px;
    padding: 12px 24px 32px;
  }
  .flash {
    position: relative;
    width: min(680px, 100%);
    min-height: 360px;
    padding: 40px 32px 32px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    text-align: center;
    gap: 18px;
  }
  .dirtag {
    position: absolute;
    top: 14px;
    left: 18px;
  }
  .front {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 14px;
  }
  .prompt {
    margin-top: 6px;
  }
  .meaning-q {
    font-size: 28px;
    font-weight: 550;
    max-width: 560px;
  }
  .listen {
    width: 96px;
    height: 96px;
    border-radius: 50%;
    border: 1px solid var(--border);
    background: var(--surface-2);
    display: grid;
    place-items: center;
    color: var(--accent);
  }
  .listen:hover {
    background: var(--accent-soft);
  }
  .back {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding-top: 16px;
    border-top: 1px solid var(--border);
    width: 100%;
  }
  .py-line {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .py-big {
    font-size: 22px;
  }
  .meaning {
    font-size: 18px;
    max-width: 560px;
  }
  .notes {
    max-width: 560px;
    white-space: pre-wrap;
  }
  .actions {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    min-height: 100px;
  }
  .reveal {
    min-width: 260px;
  }
  .ratings {
    display: grid;
    grid-template-columns: repeat(4, 120px);
    gap: 10px;
  }
  .rate {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 8px 6px 9px;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    background: var(--surface);
    border-bottom-width: 3px;
  }
  .rate:hover {
    background: var(--surface-2);
  }
  .rate.again {
    border-bottom-color: var(--tone-1);
  }
  .rate.hard {
    border-bottom-color: var(--warn);
  }
  .rate.good {
    border-bottom-color: var(--ok);
  }
  .rate.easy {
    border-bottom-color: var(--series-1);
  }
  .ivl {
    color: var(--text-2);
    min-height: 18px;
  }
  .key {
    position: absolute;
    top: 4px;
    right: 8px;
    font-size: 11px;
  }
  .keys {
    margin-top: 4px;
  }

  /* done */
  .donebox {
    max-width: 520px;
    margin: 40px auto;
    padding: 36px;
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
  }
  .seal {
    width: 64px;
    height: 64px;
    border-radius: 14px;
    background: var(--accent);
    color: #fff;
    font-size: 36px;
    display: grid;
    place-items: center;
    margin-bottom: 14px;
  }
  .row {
    display: flex;
    gap: 10px;
  }

  @media (max-width: 900px) {
    .setup {
      grid-template-columns: 1fr;
    }
    .summary {
      position: static;
    }
    .ratings {
      grid-template-columns: repeat(2, 140px);
    }
  }
</style>
