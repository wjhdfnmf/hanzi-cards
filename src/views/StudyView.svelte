<script lang="ts">
  import { listen as onTauriEvent } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import {
    api,
    errorText,
    type Deck,
    type Direction,
    type IntervalPreview,
    type SpeechStatus,
    type SpokenAnswer,
    type StudyItem,
    type StudyMode,
  } from "../lib/api";
  import { t } from "../lib/i18n.svelte";
  import { intervalLabel, durationLabel, pct } from "../lib/format";
  import { goto, nav } from "../lib/nav.svelte";
  import { settings, saveSetting } from "../lib/settings.svelte";
  import { playClip, speak, stopClip, stopSpeaking } from "../lib/speech";
  import { voice } from "../lib/voice.svelte";
  import { showToast } from "../lib/toast.svelte";
  import Hanzi from "../components/Hanzi.svelte";
  import Pinyin from "../components/Pinyin.svelte";
  import Icon from "../components/Icon.svelte";
  import DeckPicker from "../components/DeckPicker.svelte";
  import { recordUtterance, wavBlob } from "../lib/mic";

  type Phase = "setup" | "session" | "done";
  const MODES: StudyMode[] = ["due", "new", "all", "difficult"];
  const DIRS: Direction[] = ["recognition", "recall", "listening", "speaking", "read_aloud", "say_meaning"];
  /** Directions answered by speaking (checked by speech recognition). */
  const isSpoken = (d: Direction | undefined) => d === "speaking" || d === "read_aloud" || d === "say_meaning";
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
  // The count at the top: cards answered at least once, and how many of those are still to
  // come round again (learning steps) before the session ends.
  let done = $state(0);
  let again = $state(0);
  let answered = new Set<string>();
  const itemKey = (x: StudyItem) => `${x.card.id}:${x.direction}`;
  // The session as it was before the latest answer, for the Back button (one step back).
  type Step = { item: StudyItem; queue: StudyItem[]; done: number; again: number; answered: Set<string>; result: typeof result };
  let last = $state.raw<Step | null>(null);
  let revealed = $state(false);
  let previews = $state<IntervalPreview[]>([]);
  let shownAt = 0;
  let busy = false;
  /** The mode the running session was started with. */
  let sessionMode: StudyMode = "due";
  let result = $state({ reviewed: 0, correct: 0, ms: 0 });
  const current = $derived(queue[0] ?? null);

  // Speaking: the learner says the answer; Windows speech recognition checks it.
  let speech = $state<SpeechStatus>({
    available: false,
    reason: null,
    engine: null,
    windows: false,
    whisperModels: [],
    whisperModel: null,
  });
  let listening = $state(false);
  /** Whisper is working out what was said. */
  let thinking = $state(false);
  /** Microphone level while recording (Whisper), 0..1. */
  let level = $state(0);
  let recording: AbortController | null = null;
  /** The learner's last spoken answer to this card ("Play back my answers"): in memory only,
   * dropped when the next card shows. */
  let myAnswer = $state<Blob | null>(null);
  let playingMine = $state(false);

  function forgetMyAnswer() {
    stopClip();
    myAnswer = null;
    playingMine = false;
  }

  async function playMine() {
    if (playingMine) return forgetPlaying();
    if (!myAnswer) return;
    playingMine = true;
    try {
      await playClip(myAnswer);
    } finally {
      playingMine = false;
    }
  }

  function forgetPlaying() {
    stopClip();
    playingMine = false;
  }
  let spoken = $state<SpokenAnswer | null>(null);
  let attempts = 0;
  /** Rating suggested from the spoken answer (highlighted; Enter accepts it). */
  let suggested = $state<number | null>(null);

  // ---------- first run: offer the recommended speech model ----------
  let suggestModel = $state<{ name: string; mb: number } | null>(null);
  let modelProgress = $state<number | null>(null);
  async function checkSpeechModel() {
    try {
      if (localStorage.getItem("speech-model-offer") === "dismissed") return;
    } catch {}
    if (speech.whisperModels.length) {
      suggestModel = null;
      return;
    }
    try {
      const [info, models] = await Promise.all([api.whisperDeviceInfo(), api.whisperModels()]);
      const m = models.find((x) => x.name === info.recommended);
      if (m) suggestModel = { name: m.name, mb: m.sizeMb };
    } catch {}
  }
  function dismissModelOffer() {
    suggestModel = null;
    try {
      localStorage.setItem("speech-model-offer", "dismissed");
    } catch {}
  }
  async function installSuggested() {
    if (!suggestModel) return;
    const name = suggestModel.name;
    modelProgress = 0;
    const stop = await onTauriEvent<{ name: string; received: number; total: number }>("whisper-download", (e) => {
      if (e.payload.name === name) modelProgress = e.payload.total ? Math.floor((e.payload.received / e.payload.total) * 100) : 0;
    });
    try {
      await api.whisperDownload(name);
      await saveSetting("whisperModel", name);
      showToast(t("speech.modelReady"));
      suggestModel = null;
      speech = await api.speechStatus();
    } catch (e) {
      showToast(errorText(e), "err");
    } finally {
      stop();
      modelProgress = null;
    }
  }

  /** Why a direction can't be studied on this computer ("" = it can). */
  // ---------- the recommended setup (decks are left as chosen) ----------
  // Due & new every day; reading (recognition), writing (recall: production is the stronger
  // practice) and listening (tones and homophones); short shuffled sessions; any tag.
  const RECOMMENDED_DIRS: Direction[] = ["recognition", "recall", "listening"];
  const recommended = () => ({
    mode: "due" as StudyMode,
    directions: RECOMMENDED_DIRS.filter((d) => dirOff(d) === ""),
    limit: 20,
    shuffle: true,
    tags: [] as string[],
  });
  const usingRecommended = $derived.by(() => {
    const r = recommended();
    return (
      sel.mode === r.mode &&
      sel.limit === r.limit &&
      sel.shuffle === r.shuffle &&
      sel.tags.length === 0 &&
      sel.directions.length === r.directions.length &&
      r.directions.every((d) => sel.directions.includes(d))
    );
  });
  function useRecommended() {
    sel = { ...sel, ...recommended() }; // sel.deckIds stays
  }

  function dirOff(d: Direction): string {
    if (d === "speaking" || d === "read_aloud") return speech.available ? "" : (speech.reason ?? t("speech.unavailable"));
    if (d === "say_meaning") return speech.whisperModels.length ? "" : t("speech.needWhisper");
    return "";
  }

  /** The learner overrules the recognizer: they did say it right. */
  function markRight() {
    if (!spoken) return;
    spoken = { ...spoken, correct: true, note: "overridden" };
    suggested = 3;
    if (!revealed) reveal();
  }

  onMount(() => {
    try {
      const saved = JSON.parse(localStorage.getItem(PREF_KEY) ?? "null");
      if (saved) sel = { ...sel, ...saved };
    } catch {
      /* storage unavailable */
    }
  });

  // This page stays alive (hidden) when you switch pages, so a session can be continued.
  // Answers are saved as you rate them; "End study" (or closing the app) ends the session.
  const shown = $derived(nav.view === "study");
  $effect(() => {
    if (!shown) {
      // leaving: stop listening and the voice; the session waits here
      stopListening();
      stopSpeaking();
      return;
    }
    if (phase === "setup") loadSetup();
    else if (phase === "session") shownAt = Date.now(); // time away doesn't count as answer time
  });

  // "Study this deck" from Decks & Cards
  $effect(() => {
    const ids = nav.studyDeckIds;
    if (!ids) return;
    nav.studyDeckIds = null;
    sel.deckIds = ids;
    if (phase === "session") showToast(t("study.sessionRunning"));
  });

  async function loadSetup() {
    [decks, tags, speech] = await Promise.all([
      api.listDecks(),
      api.listTags(),
      api.speechStatus().catch(() => speech),
    ]);
    sel.directions = sel.directions.filter((d) => !dirOff(d));
    checkSpeechModel();
    if (!sel.directions.length) sel.directions = ["recognition"];
    // Forget decks/tags that no longer exist.
    sel.deckIds = sel.deckIds.filter((id) => decks.some((d) => d.id === id));
    sel.tags = sel.tags.filter((tg) => tags.includes(tg));
  }

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
        const info = await api.sessionCounts({ ...filter });
        // a slower answer for an earlier selection doesn't replace a newer one
        if (JSON.stringify($state.snapshot(sel)) !== JSON.stringify(filter)) return;
        matching = info.matching;
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

  /** Keep the same word (in another direction) from coming up twice in a row. */
  /** Keep the same word's directions apart: each next card is the first one whose word wasn't
   * among the last few shown (when the session has enough other words). */
  const SAME_WORD_GAP = 6;
  function spreadSameWords(items: StudyItem[]): StudyItem[] {
    const rest = [...items];
    const out: StudyItem[] = [];
    while (rest.length) {
      const recent = new Set(out.slice(-SAME_WORD_GAP).map((x) => x.card.hanzi));
      const i = rest.findIndex((x) => !recent.has(x.card.hanzi));
      out.push(rest.splice(Math.max(0, i), 1)[0]);
    }
    return out;
  }

  async function start() {
    error = "";
    try {
      const info = await api.buildSession($state.snapshot(sel));
      if (!info.items.length) {
        error = t("study.nothing");
        return;
      }
      sessionMode = sel.mode;
      queue = spreadSameWords(sel.shuffle ? shuffle(info.items) : info.items);
      total = queue.length;
      done = 0;
      again = 0;
      answered = new Set();
      last = null;
      result = { reviewed: 0, correct: 0, ms: 0 };
      phase = "session";
      if (sel.directions.some((d) => (isSpoken(d) && speech.engine === "whisper") || d === "say_meaning"))
        api.speechWarmup().catch(() => {});
      await show();
    } catch (e) {
      error = errorText(e);
    }
  }

  async function show() {
    revealed = false;
    previews = [];
    spoken = null;
    forgetMyAnswer(); // the last card's recording goes
    suggested = null;
    attempts = 0;
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

  /** A missed card comes back at least this many cards later (Settings › Studying › Advanced). */
  const MIN_GAP = $derived(Math.max(1, Math.round(settings.minGapCards || 5)));
  const twoButtons = $derived(settings.answerButtons === "2");
  /** The suggestion on the buttons shown: with Wrong / Right, any pass is Right (Good). */
  const shownSuggested = $derived(suggested === null ? null : twoButtons ? (suggested >= 2 ? 3 : 1) : suggested);

  async function rate(rating: number) {
    if (!current || !revealed || busy) return;
    if (twoButtons) rating = rating >= 2 ? 3 : 1;
    busy = true;
    const item = current;
    const ms = Date.now() - shownAt;
    const before: Step = { item, queue, done, again, answered: new Set(answered), result: { ...result } };
    try {
      const res = await api.answerCard(item.card.id, item.direction, rating, ms);
      last = before;
      result.reviewed++;
      result.ms += Math.min(ms, 10 * 60 * 1000);
      if (rating > 1) result.correct++;
      if (res.leech) showToast(t("study.leech", { word: item.card.hanzi }), "err");
      if (res.unleeched) showToast(t("study.unleeched", { word: item.card.hanzi }));
      let rest = queue.slice(1);
      // This word's other directions wait for the next session (they stay due).
      if (settings.oneDirectionPerSession) rest = rest.filter((x) => x.card.hanzi !== item.card.hanzi);
      // Short learning steps come back later in this session (in cram, only a missed card),
      // about as many cards later as fit in the step at this session's pace: a longer gap makes
      // the next recall harder and the memory stronger, and never fewer than a few cards.
      if (res.days === null && res.intervalSecs < 30 * 60 && (sessionMode !== "all" || rating === 1)) {
        const pace = Math.max(4, result.ms / result.reviewed / 1000); // seconds per card
        const pos = Math.min(rest.length, Math.max(MIN_GAP, Math.round(res.intervalSecs / pace)));
        rest.splice(pos, 0, item);
      }
      answered.add(itemKey(item));
      again = rest.filter((x) => answered.has(itemKey(x))).length;
      done = total - (rest.length - again); // every card but those not yet shown
      queue = rest;
      await show();
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }

  async function listen() {
    if (!current || revealed || listening || !isSpoken(current.direction)) return;
    const item = current;
    listening = true;
    error = "";
    try {
      // Other words in the session are offered as distractors.
      const others = queue.slice(1, 40).map((i) => i.card.hanzi);
      let res: SpokenAnswer;
      // English answers always go to Whisper; Chinese ones to the chosen engine.
      if (item.direction === "say_meaning" || speech.engine === "whisper") {
        recording = new AbortController();
        const pcm = await recordUtterance({ signal: recording.signal, onLevel: (l) => (level = l) });
        const aborted = recording.signal.aborted;
        recording = null;
        level = 0;
        if (aborted || current !== item) return;
        if (!pcm) {
          res = { status: "silence", heard: "", heardPinyin: "", confidence: "", correct: false, suggested: 1, note: "" };
        } else {
          if (settings.replayAnswers) myAnswer = wavBlob(pcm);
          listening = false;
          thinking = true;
          try {
            res =
              item.direction === "say_meaning"
                ? await api.speechCheckEnglish(item.card.id, pcm)
                : await api.speechCheckAudio(item.card.id, others, pcm);
          } finally {
            thinking = false;
          }
        }
      } else {
        // Windows' recognizer uses the microphone itself; to play the answer back, record it
        // alongside (the system shares the microphone) and stop when the recognizer is done
        const side = settings.replayAnswers ? { cancel: new AbortController(), stop: new AbortController() } : null;
        const copy = side
          ? recordUtterance({ signal: side.cancel.signal, stopSignal: side.stop.signal, startTimeoutMs: 10000, silenceMs: 3000, maxMs: 10000 }).catch(() => null)
          : null;
        let heardRes: SpokenAnswer | null = null;
        try {
          heardRes = await api.speechListen(item.card.id, others);
        } finally {
          // keep the copy only when there's an answer to go with it
          if (side) (heardRes && heardRes.status !== "cancelled" ? side.stop : side.cancel).abort();
        }
        res = heardRes;
        const pcm = await copy;
        if (pcm && current === item && res.status !== "cancelled") myAnswer = wavBlob(pcm);
      }
      if (current !== item || res.status === "cancelled") return;
      attempts++;
      spoken = res;
      if (res.correct) {
        suggested = attempts === 1 ? res.suggested : Math.min(res.suggested, 2);
        reveal();
      } else if (res.status === "heard" && attempts >= 2) {
        suggested = 1;
        reveal();
      }
    } catch (e) {
      error = errorText(e);
    } finally {
      listening = false;
    }
  }

  function stopListening() {
    if (recording) recording.abort();
    else if (listening) api.speechCancel();
  }

  /** Back to the card just answered, its answer taken back, to answer it again. */
  async function back() {
    if (!last || busy) return;
    busy = true;
    error = "";
    try {
      await api.undoAnswer(last.item.card.id, last.item.direction);
      stopListening();
      queue = last.queue;
      done = last.done;
      again = last.again;
      answered = last.answered;
      result = last.result;
      last = null;
      await show();
      revealed = true; // the answer is what they came back to change
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }

  function endSession() {
    stopListening();
    forgetMyAnswer();
    phase = result.reviewed ? "done" : "setup";
  }

  function onkeydown(e: KeyboardEvent) {
    if (!shown) return; // another page is showing
    if (phase !== "session" || e.ctrlKey || e.metaKey || e.altKey) return;
    if ((e.target as HTMLElement)?.closest("input, textarea, select")) return;
    if (isSpoken(current?.direction) && !revealed) {
      if (e.key === " ") {
        e.preventDefault();
        listen();
        return;
      }
      if (e.key === "Escape" && listening) {
        e.preventDefault();
        stopListening();
        return;
      }
    }
    if (e.key === " " || e.key === "Enter") {
      e.preventDefault();
      if (!revealed) {
        stopListening();
        reveal();
      } else if (e.key === "Enter") rate(shownSuggested ?? 3);
      else revealed = false; // Space again hides the answer, like the button
    } else if (revealed && ["1", "2", "3", "4"].includes(e.key)) {
      // Wrong / Right: 1 is Wrong, 2 (or 3, 4) is Right
      rate(twoButtons ? (e.key === "1" ? 1 : 3) : Number(e.key));
    } else if (revealed && (e.key === "9" || e.key === "0")) {
      // a second pair for the right hand, not shown on the buttons: 9 wrong, 0 right
      rate(e.key === "9" ? 1 : 3);
    } else if (e.key.toLowerCase() === "r" && current) {
      speak(current.card.hanzi);
    }
  }

  const RATINGS_4 = [
    { n: 1, key: "study.again", cls: "again", k: "1" },
    { n: 2, key: "study.hard", cls: "hard", k: "2" },
    { n: 3, key: "study.good", cls: "good", k: "3" },
    { n: 4, key: "study.easy", cls: "easy", k: "4" },
  ] as const;
  const RATINGS_2 = [
    { n: 1, key: "study.wrong", cls: "again", k: "1" },
    { n: 3, key: "study.right", cls: "good", k: "2" },
  ] as const;
  const RATINGS = $derived(twoButtons ? RATINGS_2 : RATINGS_4);
</script>

<svelte:window {onkeydown} />

{#if phase === "setup"}
  <div class="page">
    <div class="page-head">
      <div>
        <h1>{t("study.title")}</h1>
        <p class="muted">{t("study.subtitle")}</p>
      </div>
      <button
        class="btn recommend"
        class:on={usingRecommended}
        disabled={usingRecommended}
        title={t("study.recommendedHint")}
        onclick={useRecommended}
      >
        <Icon name={usingRecommended ? "check" : "flame"} size={15} />
        {usingRecommended ? t("study.usingRecommended") : t("study.recommended")}
      </button>
    </div>

    {#if suggestModel}
      <div class="notice model-offer">
        <span>{t("speech.offerModel", { name: suggestModel.name, mb: suggestModel.mb })}</span>
        <span class="offer-actions">
          {#if modelProgress !== null}
            <progress max="100" value={modelProgress}></progress>
            <span class="small">{t("settings.downloading", { p: modelProgress })}</span>
          {:else}
            <button class="btn primary small" onclick={installSuggested}>{t("speech.offerInstall")}</button>
            <button class="btn ghost small" onclick={() => goto("settings", "speech")}>{t("speech.offerChoose")}</button>
            <button class="btn ghost small" onclick={dismissModelOffer}>{t("speech.offerLater")}</button>
          {/if}
        </span>
      </div>
    {/if}

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
          <DeckPicker {decks} bind:selected={sel.deckIds} />
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
              {@const off = dirOff(d) !== ""}
              <button
                class="chip"
                class:on={sel.directions.includes(d)}
                disabled={off}
                title={off ? dirOff(d) : undefined}
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
      <button class="btn" onclick={endSession} title={t("study.endHint")}>
        <Icon name="check" size={15} />
        {t("study.end")}
      </button>
      <button class="btn" onclick={back} disabled={!last} title={t("study.backHint")}>
        <span class="flip"><Icon name="arrow" size={15} /></span>
        {t("study.lastCard")}
      </button>
      <div class="bar" aria-hidden="true"><div style="width:{total ? ((done - again / 2) / total) * 100 : 0}%"></div></div>
      <span class="count small">{again ? t("study.progressAgain", { done, total, n: again }) : t("study.progress", { done, total })}</span>
    </div>

    <div class="stage">
      <div class="flash card-surface">
        <span class="dirtag small faint">{t(`dir.${dir}`)}</span>

        <div class="front">
          {#if dir === "recognition"}
            <Hanzi hanzi={c.hanzi} pinyin={c.pinyin} colored={revealed} define={revealed} size={c.hanzi.length > 4 ? 56 : 84} />
          {:else if dir === "recall"}
            <div class="meaning-q">{c.meaning || "—"}</div>
          {:else if isSpoken(dir)}
            {#if dir === "speaking"}
              <div class="meaning-q">{c.meaning || "—"}</div>
            {:else}
              <Hanzi hanzi={c.hanzi} pinyin={c.pinyin} colored={revealed} define={revealed} size={c.hanzi.length > 4 ? 56 : 84} />
            {/if}
            {#if !revealed}
              <button
                class="mic"
                class:live={listening}
                disabled={thinking}
                style="--level:{level}"
                onclick={() => (listening ? stopListening() : listen())}
                aria-label={listening ? t("speech.stop") : t("speech.speak")}
              >
                <Icon name="mic" size={34} />
              </button>
            {/if}
            {#if spoken && spoken.status !== "cancelled"}
              <div class="heard small" class:right={spoken.correct} class:wrong={!spoken.correct}>
                {#if spoken.status === "silence"}
                  {t("speech.silence")}
                {:else if spoken.status === "unclear"}
                  {t("speech.unclear")}
                {:else}
                  {t("speech.heard")}
                  {#if dir === "say_meaning"}<span>“{spoken.heard}”</span>{:else}<span class="han">{spoken.heard}</span>{/if}
                  {#if spoken.heardPinyin}<span class="faint">{spoken.heardPinyin}</span>{/if}
                  ·
                  {spoken.note === "overridden"
                    ? t("speech.overridden")
                    : spoken.correct
                      ? t("speech.right")
                      : spoken.note === "tone"
                        ? t("speech.tone")
                        : t("speech.wrong")}
                {/if}
              </div>
              {#if myAnswer || (!spoken.correct && (spoken.status === "heard" || spoken.status === "unclear"))}
                <div class="answer-actions">
                  {#if myAnswer}
                    <button class="btn ghost small" onclick={playMine}>
                      <Icon name="speaker" size={14} />
                      {playingMine ? t("speech.stopMine") : t("speech.playMine")}
                    </button>
                  {/if}
                  {#if !spoken.correct && (spoken.status === "heard" || spoken.status === "unclear")}
                    <button class="btn ghost small override" onclick={markRight} title={t("speech.overrideHint")}>
                      <Icon name="check" size={14} />
                      {t("speech.override")}
                    </button>
                  {/if}
                </div>
              {/if}
            {/if}
          {:else}
            <button class="listen" onclick={() => speak(c.hanzi)} aria-label={t("study.play")} disabled={!voice.available} title={voice.available ? undefined : t("speech.noVoice")}>
              <Icon name="speaker" size={40} />
            </button>
          {/if}
          {#if !revealed}
            <p class="faint prompt">
              {dir === "recognition"
                ? t("study.whatMeaning")
                : dir === "recall"
                  ? t("study.howWrite")
                  : isSpoken(dir)
                    ? thinking
                      ? t("speech.thinking")
                      : listening
                      ? t("speech.listening")
                      : spoken && !spoken.correct
                        ? t("speech.retry")
                        : dir === "read_aloud"
                          ? t("speech.promptRead")
                          : dir === "say_meaning"
                            ? t("speech.promptMeaning")
                            : t("speech.prompt")
                    : t("study.whatHear")}
            </p>
          {/if}
        </div>

        {#if revealed}
          <div class="back">
            {#if dir !== "recognition" && dir !== "read_aloud" && dir !== "say_meaning"}
              <Hanzi hanzi={c.hanzi} pinyin={c.pinyin} define size={c.hanzi.length > 4 ? 48 : 72} />
            {/if}
            <div class="py-line">
              <span class="py-big"><Pinyin text={c.pinyin} /></span>
              <button class="btn ghost icon" onclick={() => speak(c.hanzi)} aria-label={t("study.play")} disabled={!voice.available} title={voice.available ? undefined : t("speech.noVoice")}>
                <Icon name="speaker" />
              </button>
            </div>
            {#if dir !== "recall" && dir !== "speaking"}<div class="meaning">{c.meaning}</div>{/if}
            {#if c.notes}<div class="notes small muted">{c.notes}</div>{/if}
          </div>
        {/if}
      </div>

      <div class="actions">
        <!-- stays where it is and becomes Hide: the answer buttons come below it, so a second
             click in the same place never answers the card -->
        <button class="btn lg reveal" class:primary={!revealed} onclick={() => (revealed ? (revealed = false) : reveal())}>
          {revealed ? t("study.hideAnswer") : t("study.showAnswer")}
        </button>
        {#if revealed}
          <div class="ratings" class:two={twoButtons}>
            {#each RATINGS as r (r.n)}
              <button class="rate {r.cls}" class:suggested={shownSuggested === r.n} onclick={() => rate(r.n)}>
                {#if !twoButtons}
                  <span class="ivl small">{previews[r.n - 1] ? intervalLabel(previews[r.n - 1].secs, previews[r.n - 1].days) : ""}</span>
                {/if}
                {#if twoButtons}<span class="mark"><Icon name={r.n === 1 ? "close" : "check"} size={34} /></span>{/if}
                <strong>{t(r.key)}</strong>
                <span class="key faint">{r.k}</span>
              </button>
            {/each}
          </div>
        {/if}
        <p class="faint small keys">{isSpoken(dir) && !revealed ? t("speech.keys") : t(twoButtons ? "study.keys2" : "study.keys")}</p>
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
  .mic {
    width: 76px;
    height: 76px;
    border-radius: 50%;
    border: 2px solid var(--accent);
    background: var(--surface);
    color: var(--accent);
    display: grid;
    place-items: center;
    margin-top: 6px;
  }
  .mic:hover {
    background: var(--accent-soft);
  }
  .mic.live {
    background: var(--accent);
    color: var(--accent-text);
    animation: pulse 1.2s ease-in-out infinite;
    transform: scale(calc(1 + var(--level, 0) * 0.25));
    transition: transform 0.08s;
  }
  .mic:disabled {
    opacity: 0.6;
  }
  @keyframes pulse {
    50% {
      box-shadow: 0 0 0 10px var(--accent-soft);
    }
  }
  .heard {
    padding: 4px 12px;
    border-radius: 999px;
    background: var(--surface-2);
  }
  .heard .han {
    font-size: 18px;
    margin: 0 4px;
  }
  .heard.right {
    color: var(--ok);
  }
  .heard.wrong {
    color: var(--accent);
  }
  .model-offer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
    margin-bottom: 14px;
  }
  .offer-actions {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .offer-actions .btn.small {
    height: 28px;
    padding: 0 10px;
    font-size: 12.5px;
  }
  .answer-actions {
    display: flex;
    justify-content: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .answer-actions .btn {
    margin-top: 6px;
    height: 28px;
    padding: 0 10px;
    font-size: 12.5px;
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .override {
    margin-top: 6px;
    height: 28px;
    padding: 0 10px;
    font-size: 12.5px;
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .rate.suggested {
    outline: 2px solid var(--focus);
    outline-offset: 2px;
  }
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
  .recommend {
    flex-shrink: 0;
    align-self: center;
  }
  .recommend:not(.on) {
    border-color: var(--accent);
    color: var(--accent);
  }
  .recommend.on {
    opacity: 1;
    color: var(--ok);
    border-color: transparent;
    background: var(--ok-soft);
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
  /* the count has a label of its own, so whatever a theme draws in that corner stays behind it */
  .count {
    padding: 2px 11px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
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
  .flip {
    display: inline-flex;
    transform: scaleX(-1);
  }
  .ratings {
    display: grid;
    grid-template-columns: repeat(4, 120px);
    gap: 10px;
  }
  /* Wrong / Right: two big square tiles */
  .ratings.two {
    grid-template-columns: repeat(2, 136px);
    gap: 16px;
  }
  .ratings.two .rate {
    aspect-ratio: 1;
    justify-content: center;
    gap: 8px;
    border-radius: var(--radius-lg);
    border-bottom-width: 4px;
    font-size: 17px;
  }
  .ratings.two .rate.again {
    background: color-mix(in srgb, var(--tone-1) 7%, var(--surface));
  }
  .ratings.two .rate.good {
    background: color-mix(in srgb, var(--ok) 7%, var(--surface));
  }
  .ratings.two .rate.again:hover {
    background: color-mix(in srgb, var(--tone-1) 14%, var(--surface));
  }
  .ratings.two .rate.good:hover {
    background: color-mix(in srgb, var(--ok) 14%, var(--surface));
  }
  .ratings.two .rate.suggested {
    outline-offset: 3px;
  }
  .mark {
    display: grid;
    place-items: center;
  }
  .rate.again .mark {
    color: var(--tone-1);
  }
  .rate.good .mark {
    color: var(--ok);
  }
  .ratings.two .key {
    top: 8px;
    right: 10px;
    font-size: 12px;
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
    color: var(--accent-text);
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
