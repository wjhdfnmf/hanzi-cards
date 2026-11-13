<script lang="ts">
  // AI conversation tutor: a spoken role-play with Claude built around the learner's words.
  import { onMount, onDestroy, tick } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import {
    api,
    errorText,
    type Deck,
    type Segment,
    type TutorEvent,
    type TutorOptions,
    type TutorGrade,
    type TutorSuggestion,
    type TutorSummary,
  } from "../lib/api";
  import { t } from "../lib/i18n.svelte";
  import { goto, nav } from "../lib/nav.svelte";
  import { showToast } from "../lib/toast.svelte";
  import { recordUtterance } from "../lib/mic";
  import { isSpeaking, speakQueued, stopSpeaking } from "../lib/speech";
  import Icon from "../components/Icon.svelte";
  import DeckPicker from "../components/DeckPicker.svelte";
  import Pinyin from "../components/Pinyin.svelte";
  import PinyinInput from "../components/PinyinInput.svelte";
  import { settings } from "../lib/settings.svelte";

  type Phase = "setup" | "chat" | "summary";
  type Msg = {
    role: "user" | "tutor";
    text: string;
    note?: string;
    segments?: Segment[] | null;
    showPinyin?: boolean;
    english?: string;
    translating?: boolean;
  };

  const SCENARIOS = ["meet", "cafe", "family", "weekend", "fruit", "doctor", "free"] as const;

  // ---------- per-conversation behaviour (remembered for next time) ----------
  const DEFAULT_OPTIONS: TutorOptions = {
    level: "hsk1",
    length: "normal",
    corrections: "gentle",
    english: "asked",
    newWords: 1,
    lead: "tutor",
    onlyMyWords: false,
    model: "",
    notes: "",
  };
  function loadJson<T>(key: string, fallback: T): T {
    try {
      const raw = localStorage.getItem(key);
      return raw ? { ...fallback, ...JSON.parse(raw) } : fallback;
    } catch {
      return fallback;
    }
  }
  let options = $state<TutorOptions>(loadJson("tutor.options", DEFAULT_OPTIONS));
  /** Voice speed for this conversation (0 = the one in Settings). */
  let voiceRate = $state<number>(loadJson("tutor.voice", { rate: 0 }).rate);
  $effect(() => {
    try {
      localStorage.setItem("tutor.options", JSON.stringify(options));
      localStorage.setItem("tutor.voice", JSON.stringify({ rate: voiceRate }));
    } catch {}
  });
  const rate = () => voiceRate || settings.speechRate;
  let typePinyin = $state<boolean>((() => {
    try {
      return localStorage.getItem("tutor.typePinyin") !== "false";
    } catch {
      return true;
    }
  })());

  /** What to read aloud: drop English glosses like 冰（ice） or (ice). */
  const speakable = (s: string) => s.replace(/[（(][^（()）]*[A-Za-z][^（()）]*[)）]/g, "");

  let phase = $state<Phase>("setup");
  let keySet = $state<boolean | null>(null);
  let canSpeak = $state(false);
  let decks = $state<Deck[]>([]);
  let deckIds = $state<number[]>([]);
  let scenario = $state<string>("meet");
  let custom = $state("");
  let starting = $state(false);
  let spentMonth = $state<number | null>(null);
  let error = $state("");

  let sessionId = $state<number | null>(null);
  let messages = $state<Msg[]>([]);
  let grades = $state<TutorGrade[]>([]);
  let suggestions = $state<TutorSuggestion[]>([]);
  let cost = $state(0);
  /** "idle" | "recording" | "transcribing" | "thinking" */
  let busy = $state<"idle" | "recording" | "transcribing" | "thinking">("idle");
  let level = $state(0);
  let draft = $state("");
  let recording: AbortController | null = null;
  let finishing: AbortController | null = null;
  let scroller = $state<HTMLDivElement | null>(null);

  let summary = $state<TutorSummary | null>(null);
  let reviewPick = $state<Record<string, boolean>>({});
  let cardPick = $state<Record<string, boolean>>({});
  let addDeck = $state<number | null>(null);
  let recorded = $state(false);
  let added = $state(false);

  // Text that has streamed in but isn't a whole sentence yet (for speaking aloud).
  let pending = "";

  // This page stays alive (hidden) when you switch pages, so a conversation lasts until you
  // end it or close the app. The setup data is refreshed each time the page is shown.
  let shown = $derived(nav.view === "tutor");
  $effect(() => {
    if (shown) {
      if (phase === "setup") loadSetup();
    } else {
      // leaving the page: stop the microphone and the voice, keep the conversation
      recording?.abort();
      stopSpeaking();
    }
  });

  async function loadSetup() {
    try {
      const [key, speech, ds] = await Promise.all([api.aiKeyStatus(), api.speechStatus(), api.listDecks()]);
      keySet = key.set;
      if (key.set) spentMonth = (await api.aiSpending().catch(() => null))?.thisMonth ?? null;
      canSpeak = speech.whisperModels.length > 0;
      if (canSpeak) api.speechWarmup().catch(() => {});
      decks = ds;
      if (addDeck === null || !ds.some((d) => d.id === addDeck)) addDeck = ds[0]?.id ?? null;
    } catch (e) {
      error = errorText(e);
      keySet = false;
    }
  }

  onMount(() => {
    let unlisten: (() => void) | undefined;
    listen<{ sessionId: number; event: TutorEvent }>("tutor-event", (e) => {
      if (e.payload.sessionId !== sessionId) return;
      onEvent(e.payload.event);
    }).then((f) => (unlisten = f));
    return () => unlisten?.();
  });

  // ---------- Say again: the selection only, or stop if something is being said ----------

  /** Whether the tutor's voice is playing (polled: speech synthesis has no reliable event for "all done"). */
  let speaking = $state(false);
  onMount(() => {
    const timer = setInterval(() => (speaking = isSpeaking()), 150);
    return () => clearInterval(timer);
  });

  /** The part of the current selection that lies inside `el`, as text ("" if none). */
  function selectedIn(el: Element | null): string {
    const sel = window.getSelection();
    if (!el || !sel || sel.isCollapsed || !sel.rangeCount) return "";
    const r = sel.getRangeAt(0);
    if (!r.intersectsNode(el)) return "";
    const part = document.createRange();
    part.selectNodeContents(el);
    if (r.compareBoundaryPoints(Range.START_TO_START, part) > 0) part.setStart(r.startContainer, r.startOffset);
    if (r.compareBoundaryPoints(Range.END_TO_END, part) < 0) part.setEnd(r.endContainer, r.endOffset);
    return part.toString().trim();
  }

  function sayAgain(i: number, m: Msg) {
    if (speaking || isSpeaking()) {
      stopSpeaking();
      speaking = false;
      return;
    }
    const chosen = selectedIn(scroller?.querySelector(`[data-msg="${i}"]`) ?? null);
    speakQueued(speakable(chosen || m.text), rate());
    speaking = true;
  }

  onDestroy(() => {
    recording?.abort();
    stopSpeaking();
    if (sessionId !== null) {
      const id = sessionId;
      api.tutorCancel(id).finally(() => api.tutorEnd(id).catch(() => {}));
    }
  });

  const fmtCost = (c: number) => `$${c.toFixed(c < 0.1 ? 3 : 2)}`;
  const hasChinese = (s: string) => /[㐀-鿿]/.test(s);

  function onEvent(ev: TutorEvent) {
    if (ev.kind === "text") {
      const last = messages[messages.length - 1];
      if (last?.role === "tutor") last.text += ev.text;
      else messages.push({ role: "tutor", text: ev.text });
      pending += ev.text;
      speakFinishedSentences(false);
      scrollDown();
    } else if (ev.kind === "grade") {
      grades.push(ev.grade);
    } else if (ev.kind === "suggestion") {
      if (!suggestions.some((s) => s.hanzi === ev.suggestion.hanzi)) suggestions.push(ev.suggestion);
    }
  }

  /** Speak each complete sentence as soon as it arrives (all of it at the end of a turn). */
  function speakFinishedSentences(flush: boolean) {
    const re = /[^。！？!?；\n]*[。！？!?；\n]+/g;
    let m: RegExpExecArray | null;
    let used = 0;
    while ((m = re.exec(pending))) {
      used = re.lastIndex;
      if (hasChinese(m[0]) && shown) speakQueued(speakable(m[0]), rate());
    }
    pending = pending.slice(used);
    if (flush) {
      if (hasChinese(pending) && shown) speakQueued(speakable(pending), rate());
      pending = "";
    }
  }

  async function scrollDown() {
    await tick();
    scroller?.scrollTo({ top: scroller.scrollHeight, behavior: "smooth" });
  }

  async function start() {
    error = "";
    starting = true;
    try {
      const sc = scenario === "custom" ? custom.trim() || "free" : scenario;
      const s = await api.tutorStart(deckIds, sc, { ...options, notes: options.notes.trim() });
      sessionId = s.sessionId;
      messages = [];
      grades = [];
      suggestions = [];
      cost = 0;
      phase = "chat";
      await turn(null);
    } catch (e) {
      error = errorText(e);
    } finally {
      starting = false;
    }
  }

  async function turn(text: string | null) {
    if (sessionId === null) return;
    stopSpeaking();
    pending = "";
    if (text !== null) messages.push({ role: "user", text });
    scrollDown();
    busy = "thinking";
    try {
      const r = await api.tutorSend(sessionId, text);
      cost = r.cost || cost;
      speakFinishedSentences(true);
      const last = messages[messages.length - 1];
      if (r.stopReason === "refusal") {
        if (last?.role === "tutor") messages.pop();
        messages.push({ role: "tutor", text: "", note: t("tutor.refused") });
      } else if (r.stopReason === "cancelled" && last?.role === "tutor") {
        last.note = t("tutor.cancelled");
      }
    } catch (e) {
      messages.push({ role: "tutor", text: "", note: errorText(e) });
    } finally {
      busy = "idle";
      scrollDown();
    }
  }

  async function talk() {
    if (busy === "recording") {
      finishing?.abort(); // done talking: keep it
      return;
    }
    if (busy !== "idle") return;
    stopSpeaking();
    busy = "recording";
    recording = new AbortController();
    finishing = new AbortController();
    try {
      // Learners pause mid-sentence, so allow longer gaps than in the drill.
      const pcm = await recordUtterance({ signal: recording.signal, stopSignal: finishing.signal, onLevel: (l) => (level = l), maxMs: 20000, silenceMs: 1500 });
      level = 0;
      if (!pcm) {
        busy = "idle";
        return;
      }
      busy = "transcribing";
      const heard = (await api.tutorTranscribe(sessionId!, pcm)).trim();
      busy = "idle";
      if (heard) await turn(heard);
    } catch (e) {
      busy = "idle";
      showToast(errorText(e), "err");
    } finally {
      recording = null;
      finishing = null;
      level = 0;
    }
  }

  function sendTyped() {
    const text = draft.trim();
    if (!text || busy !== "idle") return;
    draft = "";
    turn(text);
  }

  function stop() {
    recording?.abort();
    stopSpeaking();
    if (busy === "thinking" && sessionId !== null) api.tutorCancel(sessionId);
  }

  async function togglePinyin(m: Msg) {
    m.showPinyin = !m.showPinyin;
    if (m.showPinyin && !m.segments) m.segments = await api.dictSegment(m.text).catch(() => null);
  }

  async function translate(m: Msg) {
    if (m.english !== undefined) {
      m.english = undefined;
      return;
    }
    m.translating = true;
    try {
      m.english = await api.tutorTranslate(m.text, sessionId);
    } catch (e) {
      showToast(errorText(e), "err");
    } finally {
      m.translating = false;
    }
  }

  async function end() {
    if (sessionId === null) return;
    stop();
    try {
      summary = await api.tutorEnd(sessionId);
    } catch (e) {
      showToast(errorText(e), "err");
      return;
    }
    sessionId = null;
    reviewPick = Object.fromEntries(summary.grades.filter((g) => g.cardId !== null).map((g) => [g.hanzi, true]));
    cardPick = Object.fromEntries(summary.suggestions.map((s) => [s.hanzi, !s.exists]));
    recorded = false;
    added = false;
    phase = "summary";
  }

  async function recordReviews() {
    if (!summary) return;
    let n = 0;
    try {
      for (const g of summary.grades) {
        if (g.cardId === null || !reviewPick[g.hanzi]) continue;
        await api.answerCard(g.cardId, "speaking", g.correct ? 3 : 1, 0);
        n++;
      }
      recorded = true;
      showToast(t("tutor.recorded", { n }));
    } catch (e) {
      showToast(errorText(e), "err");
    }
  }

  async function addCards() {
    if (!summary || addDeck === null) return;
    let n = 0;
    try {
      for (const s of summary.suggestions) {
        if (!cardPick[s.hanzi]) continue;
        await api.createCard({ deckId: addDeck, hanzi: s.hanzi, pinyin: s.pinyin, meaning: s.meaning, notes: "", tags: ["ai-tutor"] });
        n++;
      }
      added = true;
      showToast(t("tutor.added", { n }));
    } catch (e) {
      showToast(errorText(e), "err");
    }
  }

  const reviewCount = $derived(summary ? summary.grades.filter((g) => g.cardId !== null && reviewPick[g.hanzi]).length : 0);
  const addCount = $derived(summary ? summary.suggestions.filter((s) => cardPick[s.hanzi]).length : 0);

  // One line per word in the live panel: ✓ only if every use was right.
  const practised = $derived.by(() => {
    const out: { hanzi: string; correct: boolean; times: number }[] = [];
    for (const g of grades) {
      const e = out.find((x) => x.hanzi === g.hanzi);
      if (e) {
        e.correct &&= g.correct;
        e.times++;
      } else out.push({ hanzi: g.hanzi, correct: g.correct, times: 1 });
    }
    return out;
  });

  function onKey(e: KeyboardEvent) {
    if (phase !== "chat" || nav.view !== "tutor") return;
    const typing = (e.target as HTMLElement)?.tagName === "INPUT" || (e.target as HTMLElement)?.tagName === "TEXTAREA";
    if (e.key === "Escape") {
      e.preventDefault();
      stop();
    } else if (e.key === " " && !typing && canSpeak) {
      e.preventDefault();
      talk();
    }
  }
</script>

<svelte:window onkeydown={onKey} />

{#if phase === "setup"}
  <div class="page narrow">
    <div class="page-head"><h1>{t("tutor.title")}</h1></div>
    <p class="muted intro">{t("tutor.intro")}</p>

    {#if keySet === false}
      <div class="notice">
        <p>{t("tutor.noKey")}</p>
        <button class="btn primary" onclick={() => goto("settings", "ai")}>{t("tutor.addKey")}</button>
      </div>
      {#if error}<div class="notice err small">{error}</div>{/if}
    {:else if keySet}
      <div class="setup-grid">
      <div class="setup-main">
      <section class="card-surface block">
        <h3>{t("tutor.decks")}</h3>
        <DeckPicker {decks} bind:selected={deckIds} />
      </section>

      <section class="card-surface block">
        <h3>{t("tutor.scenario")}</h3>
        <div class="chips">
          {#each SCENARIOS as s (s)}
            <button class="chip" class:on={scenario === s} onclick={() => (scenario = s)}>{t(`tutor.scenario.${s}`)}</button>
          {/each}
          <button class="chip" class:on={scenario === "custom"} onclick={() => (scenario = "custom")}>
            {t("tutor.scenario.custom")}
          </button>
        </div>
        {#if scenario === "custom"}
          <input class="custom" bind:value={custom} placeholder={t("tutor.customPlaceholder")} maxlength="300" />
        {/if}
      </section>
        {#if !canSpeak}<div class="notice small">{t("tutor.noWhisper")}</div>{/if}
        {#if error}<div class="notice err small">{error}</div>{/if}
        <div class="actions">
          <button class="btn primary lg" disabled={starting || (scenario === "custom" && !custom.trim())} onclick={start}>
            <Icon name="chat" size={16} />
            {starting ? t("tutor.starting") : t("tutor.start")}
          </button>
        </div>
      </div>

      <aside class="card-surface block behaviour">
        <h3>{t("tutor.behaviour")}</h3>
        <label class="opt">
          <span>{t("tutor.opt.level")}</span>
          <select bind:value={options.level}>
            <option value="hsk1">{t("tutor.opt.level.hsk1")}</option>
            <option value="hsk2">{t("tutor.opt.level.hsk2")}</option>
            <option value="hsk3">{t("tutor.opt.level.hsk3")}</option>
          </select>
        </label>
        <label class="opt">
          <span>{t("tutor.opt.length")}</span>
          <select bind:value={options.length}>
            <option value="tiny">{t("tutor.opt.length.tiny")}</option>
            <option value="short">{t("tutor.opt.length.short")}</option>
            <option value="normal">{t("tutor.opt.length.normal")}</option>
            <option value="long">{t("tutor.opt.length.long")}</option>
          </select>
        </label>
        <label class="opt">
          <span>{t("tutor.opt.corrections")}</span>
          <select bind:value={options.corrections}>
            <option value="off">{t("tutor.opt.corrections.off")}</option>
            <option value="gentle">{t("tutor.opt.corrections.gentle")}</option>
            <option value="every">{t("tutor.opt.corrections.every")}</option>
          </select>
        </label>
        <label class="opt">
          <span>{t("tutor.opt.english")}</span>
          <select bind:value={options.english}>
            <option value="never">{t("tutor.opt.english.never")}</option>
            <option value="asked">{t("tutor.opt.english.asked")}</option>
            <option value="hints">{t("tutor.opt.english.hints")}</option>
          </select>
        </label>
        <label class="opt">
          <span>{t("tutor.opt.newWords")}</span>
          <select bind:value={options.newWords}>
            <option value={0}>{t("tutor.opt.newWords.0")}</option>
            <option value={1}>{t("tutor.opt.newWords.1")}</option>
            <option value={2}>{t("tutor.opt.newWords.2")}</option>
          </select>
        </label>
        <label class="opt">
          <span>{t("tutor.opt.lead")}</span>
          <select bind:value={options.lead}>
            <option value="tutor">{t("tutor.opt.lead.tutor")}</option>
            <option value="learner">{t("tutor.opt.lead.learner")}</option>
          </select>
        </label>
        <label class="opt check">
          <input type="checkbox" bind:checked={options.onlyMyWords} />
          <span>{t("tutor.opt.onlyMyWords")}</span>
        </label>
        <label class="opt">
          <span>{t("tutor.opt.voice")}</span>
          <select bind:value={voiceRate}>
            <option value={0}>{t("tutor.opt.voice.settings")}</option>
            <option value={0.6}>{t("tutor.opt.voice.slow")}</option>
            <option value={0.8}>{t("tutor.opt.voice.relaxed")}</option>
            <option value={1}>{t("tutor.opt.voice.normal")}</option>
          </select>
        </label>
        <label class="opt">
          <span>{t("tutor.opt.model")}</span>
          <select bind:value={options.model}>
            <option value="">{t("tutor.opt.model.settings")}</option>
            <option value="claude-opus-5-5">Claude Opus 5.5</option>
            <option value="claude-haiku-5-5">Claude Haiku 5.5</option>
          </select>
        </label>
        <label class="opt col">
          <span>{t("tutor.opt.notes")}</span>
          <textarea rows="3" maxlength="600" bind:value={options.notes} placeholder={t("tutor.opt.notesPlaceholder")}></textarea>
        </label>
        {#if spentMonth !== null}
          <p class="faint small spent">{t("tutor.spentMonth", { cost: fmtCost(spentMonth) })}</p>
        {/if}
        <button class="link small reset" onclick={() => ((options = { ...DEFAULT_OPTIONS }), (voiceRate = 0))}>{t("tutor.opt.reset")}</button>
      </aside>
      </div>

    {:else}
      <p class="faint">{t("common.loading")}</p>
    {/if}
  </div>
{:else if phase === "chat"}
  <div class="chat-shell">
    <div class="convo">
      <div class="topbar">
        <span class="small muted">{t("tutor.cost", { cost: fmtCost(cost) })}</span>
        <button class="btn" onclick={end} disabled={busy === "thinking"}>{t("tutor.end")}</button>
      </div>

      <div class="messages" bind:this={scroller}>
        {#each messages as m, i (i)}
          <div class="msg {m.role}">
            {#if m.text}
              <div class="bubble">
                {#if m.role === "tutor" && m.showPinyin && m.segments}
                  <div class="py-line">
                    {#each m.segments as seg, si (si)}
                      {#if seg.entry}<Pinyin text={seg.entry.pinyin} />{" "}{/if}
                    {/each}
                  </div>
                {/if}
                <div class="text han" data-msg={i} lang={hasChinese(m.text) ? "zh-CN" : undefined}>{m.text}</div>
                {#if m.english}<div class="english small">{m.english}</div>{/if}
              </div>
              {#if m.role === "tutor" && !(busy === "thinking" && i === messages.length - 1)}
                <div class="tools">
                  <button
                    class="link small"
                    title={t("tutor.replayHint")}
                    onmousedown={(e) => e.preventDefault()}
                    onclick={() => sayAgain(i, m)}
                  >
                    <Icon name={speaking ? "close" : "speaker"} size={13} />
                    {speaking ? t("tutor.stopAudio") : t("tutor.replay")}
                  </button>
                  <button class="link small" class:on={m.showPinyin} onclick={() => togglePinyin(m)}>{t("tutor.pinyin")}</button>
                  <button class="link small" class:on={m.english !== undefined} disabled={m.translating} onclick={() => translate(m)}>
                    {m.translating ? "…" : t("tutor.translate")}
                  </button>
                </div>
              {/if}
            {/if}
            {#if m.note}<div class="note small">{m.note}</div>{/if}
          </div>
        {/each}
        {#if busy === "thinking" && messages[messages.length - 1]?.role !== "tutor"}
          <div class="msg tutor"><div class="bubble faint">{t("tutor.thinking")}</div></div>
        {/if}
      </div>

      <div class="composer">
        {#if canSpeak}
          <button
            class="mic"
            class:rec={busy === "recording"}
            style="--level:{level}"
            disabled={busy === "thinking" || busy === "transcribing"}
            onclick={talk}
            title={t("tutor.hold")}
          >
            <Icon name="mic" size={22} />
          </button>
        {/if}
        {#if busy === "recording"}
          <span class="status">{t("tutor.listening")}</span>
        {:else if busy === "transcribing"}
          <span class="status">{t("tutor.transcribing")}</span>
        {:else}
          <PinyinInput
            bind:value={draft}
            bind:pinyin={typePinyin}
            placeholder={t("tutor.typePlaceholder")}
            disabled={busy !== "idle"}
            onsubmit={sendTyped}
          />
          {#if busy === "thinking"}
            <button class="btn" onclick={stop}>{t("tutor.stop")}</button>
          {:else}
            <button class="btn primary" disabled={!draft.trim()} onclick={sendTyped}>{t("tutor.send")}</button>
          {/if}
        {/if}
      </div>
    </div>

    <aside class="side card-surface">
      <h3>{t("tutor.practised")}</h3>
      {#if practised.length}
        <ul class="words">
          {#each practised as w (w.hanzi)}
            <li><span class="mark" class:bad={!w.correct}>{w.correct ? "✓" : "✗"}</span><span class="han">{w.hanzi}</span>{#if w.times > 1}<span class="faint small">×{w.times}</span>{/if}</li>
          {/each}
        </ul>
      {:else}
        <p class="faint small">{t("tutor.noneYet")}</p>
      {/if}
      <h3>{t("tutor.suggested")}</h3>
      {#if suggestions.length}
        <ul class="words">
          {#each suggestions as s (s.hanzi)}
            <li class="sugg">
              <span class="han">{s.hanzi}</span>
              <span class="small"><Pinyin text={s.pinyin} /></span>
              <span class="small muted">{s.meaning}</span>
            </li>
          {/each}
        </ul>
      {:else}
        <p class="faint small">{t("tutor.noneYet")}</p>
      {/if}
    </aside>
  </div>
{:else if summary}
  <div class="page narrow">
    <div class="page-head">
      <h1>{t("tutor.summary")}</h1>
      <span class="muted">{t("tutor.summaryTurns", { n: summary.turns, cost: fmtCost(summary.cost) })}</span>
    </div>

    {#if !summary.grades.length && !summary.suggestions.length}
      <p class="muted">{t("tutor.nothingToReview")}</p>
    {/if}

    {#if summary.grades.length}
      <section class="card-surface block">
        <h3>{t("tutor.practised")}</h3>
        <ul class="pick">
          {#each summary.grades as g (g.hanzi)}
            <li>
              <label>
                <input type="checkbox" disabled={g.cardId === null || recorded} bind:checked={reviewPick[g.hanzi]} />
                <span class="mark" class:bad={!g.correct}>{g.correct ? "✓" : "✗"}</span>
                <span class="han">{g.hanzi}</span>
              </label>
            </li>
          {/each}
        </ul>
        <p class="faint small">{t("tutor.recordHint")}</p>
        <button class="btn primary" disabled={!reviewCount || recorded} onclick={recordReviews}>{t("tutor.recordReviews")}</button>
      </section>
    {/if}

    {#if summary.suggestions.length}
      <section class="card-surface block">
        <h3>{t("tutor.suggested")}</h3>
        <ul class="pick">
          {#each summary.suggestions as s (s.hanzi)}
            <li>
              <label>
                <input type="checkbox" disabled={added} bind:checked={cardPick[s.hanzi]} />
                <span class="han">{s.hanzi}</span>
                <span class="small"><Pinyin text={s.pinyin} /></span>
                <span class="small muted">{s.meaning}</span>
                {#if s.exists}<span class="small faint">({t("tutor.exists")})</span>{/if}
              </label>
            </li>
          {/each}
        </ul>
        <div class="add-row">
          <label class="small">
            {t("tutor.addTo")}
            <select bind:value={addDeck} disabled={added}>
              {#each decks as d (d.id)}<option value={d.id}>{d.path}</option>{/each}
            </select>
          </label>
          <button class="btn primary" disabled={!addCount || added || addDeck === null} onclick={addCards}>
            {t("tutor.addCards", { n: addCount })}
          </button>
        </div>
      </section>
    {/if}

    <div class="actions">
      <button class="btn lg" onclick={() => ((phase = "setup"), (summary = null))}>{t("tutor.again")}</button>
    </div>
  </div>
{/if}

<style>
  .narrow {
    max-width: 760px;
  }
  .intro {
    margin: -6px 0 18px;
    line-height: 1.5;
  }
  .block {
    border-radius: var(--radius);
    padding: 14px 18px;
    margin-bottom: 14px;
  }
  .block h3 {
    margin: 0 0 10px;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .custom {
    margin-top: 10px;
    width: 100%;
  }
  .narrow:has(.setup-grid) {
    max-width: 1080px;
  }
  .setup-grid {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 320px;
    gap: 14px;
    align-items: start;
  }
  .behaviour {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .opt {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    font-size: 13.5px;
  }
  .opt select {
    width: 170px;
  }
  .opt.col {
    flex-direction: column;
    align-items: stretch;
    gap: 4px;
  }
  .opt.check {
    justify-content: flex-start;
  }
  .opt.check input {
    accent-color: var(--accent);
  }
  .reset {
    align-self: flex-end;
  }
  .spent {
    margin: 4px 0 0;
  }
  @media (max-width: 900px) {
    .setup-grid {
      grid-template-columns: 1fr;
    }
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    margin-top: 8px;
  }
  .notice p {
    margin: 0 0 10px;
  }

  .chat-shell {
    display: grid;
    grid-template-columns: 1fr 260px;
    gap: 16px;
    height: 100vh;
    padding: 16px;
    box-sizing: border-box;
  }
  .convo {
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .topbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding-bottom: 10px;
  }
  .messages {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 4px 4px 12px;
  }
  .msg {
    display: flex;
    flex-direction: column;
    max-width: 78%;
  }
  .msg.user {
    align-self: flex-end;
    align-items: flex-end;
  }
  .msg.tutor {
    align-self: flex-start;
  }
  .bubble {
    padding: 10px 14px;
    border-radius: 14px;
    background: var(--surface);
    border: 1px solid var(--border);
  }
  .msg.user .bubble {
    background: var(--accent-soft);
    border-color: transparent;
  }
  .text {
    font-size: 20px;
    line-height: 1.5;
    white-space: pre-wrap;
  }
  .py-line {
    font-size: 13px;
    color: var(--text-2);
    margin-bottom: 2px;
  }
  .english {
    margin-top: 6px;
    color: var(--text-2);
  }
  .tools {
    display: flex;
    gap: 10px;
    margin: 4px 6px 0;
  }
  .link {
    border: none;
    background: none;
    padding: 0;
    color: var(--text-2);
    display: inline-flex;
    align-items: center;
    gap: 4px;
    cursor: pointer;
  }
  .link:hover,
  .link.on {
    color: var(--accent);
  }
  .note {
    margin: 4px 6px 0;
    color: var(--text-2);
    font-style: italic;
  }
  .composer {
    display: flex;
    align-items: center;
    gap: 10px;
    padding-top: 10px;
    border-top: 1px solid var(--border);
  }
  .type {
    flex: 1;
  }
  .status {
    flex: 1;
    color: var(--text-2);
  }
  .mic {
    width: 48px;
    height: 48px;
    border-radius: 50%;
    border: none;
    background: var(--accent);
    color: var(--accent-text);
    display: grid;
    place-items: center;
    flex-shrink: 0;
    cursor: pointer;
    transition: transform 0.08s;
  }
  .mic:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .mic.rec {
    transform: scale(calc(1 + var(--level, 0) * 0.25));
    box-shadow: 0 0 0 6px var(--accent-soft);
  }

  .side {
    border-radius: var(--radius);
    padding: 14px 16px;
    overflow-y: auto;
  }
  .side h3 {
    margin: 0 0 8px;
    font-size: 14px;
  }
  .side h3:not(:first-child) {
    margin-top: 18px;
  }
  .words {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .words li {
    display: flex;
    align-items: baseline;
    gap: 8px;
  }
  .words .han {
    font-size: 18px;
  }
  .sugg {
    flex-wrap: wrap;
  }
  .mark {
    color: var(--ok);
    font-weight: 700;
  }
  .mark.bad {
    color: var(--accent);
  }
  .pick {
    list-style: none;
    margin: 0 0 10px;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .pick label {
    display: flex;
    align-items: baseline;
    gap: 8px;
  }
  .pick .han {
    font-size: 18px;
  }
  .add-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  .add-row label {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  @media (max-width: 900px) {
    .chat-shell {
      grid-template-columns: 1fr;
    }
    .side {
      display: none;
    }
  }
</style>
