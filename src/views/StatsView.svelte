<script lang="ts">
  // Statistics. Two views of the same figures: Simple answers "how am I doing?" with a few
  // headline numbers, a sentence or two on what they mean and the charts worth a daily look;
  // Advanced adds the breakdowns (by deck, HSK level, direction, hour, answer, memory).
  // Both follow the filters in the row at the top.
  import { onMount } from "svelte";
  import { api, errorText, type Deck, type Direction, type Stats, type StatSum } from "../lib/api";
  import { t, i18n } from "../lib/i18n.svelte";
  import { durationLabel, fmtNum, pct } from "../lib/format";
  import { showToast } from "../lib/toast.svelte";
  import BarChart from "../components/BarChart.svelte";
  import Heatmap from "../components/Heatmap.svelte";
  import Hanzi from "../components/Hanzi.svelte";
  import Pinyin from "../components/Pinyin.svelte";
  import Icon from "../components/Icon.svelte";

  type Period = 7 | 30 | 90 | 365 | null; // null = all time
  const PERIODS = [
    { days: 7, key: "stats.range.7" },
    { days: 30, key: "stats.range.30" },
    { days: 90, key: "stats.range.90" },
    { days: 365, key: "stats.range.365" },
    { days: null, key: "stats.range.all" },
  ] as const;
  const DIRECTIONS: Direction[] = ["recognition", "recall", "listening", "speaking", "read_aloud", "say_meaning"];

  // ---------- what is shown: remembered between visits ----------
  const SAVED = "stats.view";
  const saved = (() => {
    try {
      return JSON.parse(localStorage.getItem(SAVED) ?? "{}") as { mode?: string; period?: Period; metric?: string };
    } catch {
      return {};
    }
  })();
  let mode = $state<"simple" | "advanced">(saved.mode === "advanced" ? "advanced" : "simple");
  let period = $state<Period>(PERIODS.some((p) => p.days === saved.period) ? (saved.period as Period) : 30);
  let metric = $state<"time" | "reviews" | "new">(
    saved.metric === "reviews" || saved.metric === "new" ? saved.metric : "time",
  );
  let deckId = $state<number | null>(null);
  let direction = $state<Direction | null>(null);
  let asTable = $state(false);
  $effect(() => {
    try {
      localStorage.setItem(SAVED, JSON.stringify({ mode, period, metric }));
    } catch {
      /* not essential */
    }
  });

  let decks = $state<Deck[]>([]);
  let stats = $state.raw<Stats | null>(null);
  let loading = $state(true);
  let request = 0;
  onMount(async () => {
    decks = await api.listDecks().catch(() => []);
  });
  $effect(() => {
    const filter = { deckId, days: period, direction: mode === "advanced" ? direction : null };
    const mine = ++request;
    loading = true;
    api
      .getStats(filter)
      .then((s) => {
        if (mine === request) stats = s;
      })
      .catch((e) => mine === request && showToast(errorText(e), "err"))
      .finally(() => mine === request && (loading = false));
  });

  // ---------- small helpers ----------
  const locale = $derived(i18n.lang === "zh" ? "zh-CN" : undefined);
  const shortDate = (iso: string) => new Date(iso + "T00:00:00").toLocaleDateString(locale, { month: "short", day: "numeric" });
  const monthLabel = (ym: string) => new Date(ym + "-01T00:00:00").toLocaleDateString(locale, { month: "short", year: "2-digit" });
  const totalMs = (s: { timeMs: number; readingMs: number; tutorMs: number }) => s.timeMs + s.readingMs + s.tutorMs;
  const remembered = (s: StatSum | null | undefined) => (s && s.retentionOf > 0 ? s.retained / s.retentionOf : null);
  const share = (a: number, b: number) => (b > 0 ? a / b : null);
  const periodName = $derived(t(PERIODS.find((p) => p.days === period)!.key));
  /** "▲ 12%" against the stretch before; nothing when there is nothing to compare with. */
  function change(now: number, before: number | undefined): string {
    if (before === undefined || before <= 0 || now === before) return "";
    const d = (now - before) / before;
    return `${d > 0 ? "▲" : "▼"} ${Math.abs(d) >= 9.995 ? "999+" : Math.round(Math.abs(d) * 100)}%`;
  }
  function changePoints(now: number | null, before: number | null): string {
    if (now === null || before === null) return "";
    const d = Math.round((now - before) * 100);
    return d === 0 ? "" : `${d > 0 ? "▲" : "▼"} ${Math.abs(d)} ${t("stats.points")}`;
  }
  const axisTime = (minutes: number) =>
    minutes >= 120 ? `${Math.round((minutes / 60) * 10) / 10}${t("stats.unit.h")}` : `${Math.round(minutes)}${t("stats.unit.m")}`;
  const seconds = (ms: number, n: number) => (n > 0 ? `${(ms / n / 1000).toFixed(1)}${t("stats.unit.s")}` : "—");

  // ---------- the chart over time: days, weeks or months, whichever keeps the bars readable ----------
  interface Point {
    label: string;
    reviews: number;
    correct: number;
    newCards: number;
    cards: number;
    reading: number;
    tutor: number;
  }
  const points = $derived.by((): Point[] => {
    if (!stats) return [];
    let days = stats.days;
    if (period !== null) days = days.slice(-period);
    else {
      const first = days.findIndex((d) => d.reviews || d.readingMs || d.tutorMs);
      days = days.slice(first < 0 ? -30 : Math.max(0, Math.min(first, days.length - 30)));
    }
    const size = days.length <= 92 ? "day" : days.length <= 400 ? "week" : "month";
    const out: Point[] = [];
    let key = "";
    days.forEach((d, i) => {
      const k = size === "day" ? d.date : size === "week" ? String(Math.floor((days.length - 1 - i) / 7)) : d.date.slice(0, 7);
      if (k !== key) {
        key = k;
        out.push({ label: size === "month" ? monthLabel(k) : shortDate(d.date), reviews: 0, correct: 0, newCards: 0, cards: 0, reading: 0, tutor: 0 });
      }
      const p = out[out.length - 1];
      p.reviews += d.reviews;
      p.correct += d.correct;
      p.newCards += d.newCards;
      p.cards += d.timeMs;
      p.reading += d.readingMs;
      p.tutor += d.tutorMs;
    });
    return out;
  });
  const otherTime = $derived(!!stats && stats.otherTimeIncluded && stats.period.readingMs + stats.period.tutorMs > 0);
  const pointTip = (i: number) => {
    const p = points[i];
    const parts = [durationLabel(p.cards + p.reading + p.tutor), `${fmtNum(p.reviews)} ${t("stats.reviews")}`];
    if (p.reviews) parts.push(`${pct(p.correct / p.reviews)} ${t("stats.correctLower")}`);
    if (p.newCards) parts.push(t("stats.newN", { n: p.newCards }));
    return `${p.label}\n${parts.join(" · ")}`;
  };

  const forecastLabels = $derived(
    stats
      ? stats.forecast.map((_, i) => {
          if (i === 0) return t("stats.today.short");
          const d = new Date();
          d.setDate(d.getDate() + i);
          return d.toLocaleDateString(locale, { month: "short", day: "numeric" });
        })
      : [],
  );

  // ---------- progress: an ordered scale, so one hue from light to dark ----------
  const states = $derived(
    stats
      ? ([
          { key: "stats.state.new", n: stats.unstudiedCards, color: "var(--seq-0)" },
          { key: "stats.state.learning", n: stats.learning, color: "var(--seq-2)" },
          { key: "stats.state.young", n: stats.young, color: "var(--seq-3)" },
          { key: "stats.state.mature", n: stats.mature, color: "var(--seq-4)" },
        ] as const)
      : [],
  );
  const stateTotal = $derived(states.reduce((s, x) => s + x.n, 0));

  // ---------- what the numbers mean, in a sentence each ----------
  const takeaways = $derived.by((): string[] => {
    if (!stats || stats.allTime.reviews + stats.allTime.readingMs + stats.allTime.tutorMs === 0) return [];
    const s = stats;
    const out: string[] = [];
    const time = totalMs(s.period);
    if (s.period.daysStudied > 0) {
      out.push(
        t(period === null ? "stats.tip.timeAll" : "stats.tip.time", {
          total: durationLabel(time),
          days: s.period.daysStudied,
          period: periodName,
          avg: durationLabel(time / s.period.daysStudied),
        }),
      );
    } else out.push(t("stats.tip.noStudy", { period: periodName }));
    const r = remembered(s.period);
    if (r !== null && s.period.retentionOf >= 20) {
      const gap = r - s.targetRetention;
      const key = gap < -0.03 ? "stats.tip.below" : gap > 0.04 ? "stats.tip.above" : "stats.tip.onTarget";
      out.push(t(key, { r: pct(r), target: pct(s.targetRetention) }));
    }
    if (s.dueNow > 0) {
      const each = s.period.reviews > 0 ? s.period.timeMs / s.period.reviews : 8000;
      out.push(t("stats.tip.due", { n: fmtNum(s.dueNow), t: durationLabel(Math.max(60_000, s.dueNow * each)) }));
    } else if (s.studiedItems > 0) out.push(t("stats.tip.caughtUp", { n: fmtNum(s.dueTomorrow) }));
    return out;
  });

  // ---------- advanced ----------
  // An ordered scale from forgot to easy: two opposite hues, strong at the ends (the Study
  // page's red, orange and green can't be told apart with red-green colour blindness).
  const BUTTONS = [
    { key: "study.again", color: "var(--cat-2)" },
    { key: "study.hard", color: "color-mix(in srgb, var(--cat-2) 45%, var(--surface))" },
    { key: "study.good", color: "color-mix(in srgb, var(--cat-1) 45%, var(--surface))" },
    { key: "study.easy", color: "var(--cat-1)" },
  ] as const;
  const KINDS = ["stats.kind.learning", "stats.kind.young", "stats.kind.mature"] as const;
  const RANGES = {
    today: "stats.ret.today",
    yesterday: "stats.ret.yesterday",
    week: "stats.ret.week",
    month: "stats.ret.month",
    year: "stats.ret.year",
    all: "stats.ret.all",
  } as const;
  const cell = ([ok, n]: [number, number]) => (n > 0 ? `${pct(ok / n)} · ${fmtNum(n)}` : "—");
  const weekdayNames = $derived(
    Array.from({ length: 7 }, (_, i) => new Date(2024, 0, 1 + i).toLocaleDateString(locale, { weekday: "short" })), // 1 Jan 2024 was a Monday
  );
  const STABILITY = ["<1", "1–3", "3–7", "7–14", "14–30", "30–90", "90–180", "180–365", "365+"];
  const bestHour = $derived.by(() => {
    if (!stats) return null;
    const busy = stats.hours.map((h, i) => ({ ...h, i })).filter((h) => h.reviews >= 20);
    if (busy.length < 2) return null;
    return busy.reduce((a, b) => (b.correct / b.reviews > a.correct / a.reviews ? b : a));
  });
  const hourLabel = (h: number) => `${String(h).padStart(2, "0")}:00`;
</script>

<div class="page">
  <div class="page-head">
    <h1>{t("stats.title")}</h1>
    <div class="seg boxed" role="group" aria-label={t("stats.view")}>
      <button class:on={mode === "simple"} aria-pressed={mode === "simple"} onclick={() => (mode = "simple")}>{t("stats.simple")}</button>
      <button class:on={mode === "advanced"} aria-pressed={mode === "advanced"} onclick={() => (mode = "advanced")}>{t("stats.advanced")}</button>
    </div>
  </div>

  <div class="filters">
    <label>
      <span class="small muted">{t("stats.filter.deck")}</span>
      <select value={deckId ?? ""} onchange={(e) => (deckId = e.currentTarget.value ? Number(e.currentTarget.value) : null)}>
        <option value="">{t("stats.filter.allDecks")}</option>
        {#each decks as d (d.id)}<option value={d.id}>{"  ".repeat(d.depth)}{d.name}</option>{/each}
      </select>
    </label>
    {#if mode === "advanced"}
      <label>
        <span class="small muted">{t("stats.filter.direction")}</span>
        <select value={direction ?? ""} onchange={(e) => (direction = (e.currentTarget.value || null) as Direction | null)}>
          <option value="">{t("stats.filter.allDirections")}</option>
          {#each DIRECTIONS as d (d)}<option value={d}>{t(`dir.${d}`)}</option>{/each}
        </select>
      </label>
    {/if}
    <div class="seg boxed" role="group" aria-label={t("stats.filter.period")}>
      {#each PERIODS as p (p.key)}
        <button class:on={period === p.days} aria-pressed={period === p.days} onclick={() => (period = p.days)}>{t(p.key)}</button>
      {/each}
    </div>
  </div>

  {#if !stats}
    <p class="muted">{t("common.loading")}</p>
  {:else}
    {@const s = stats}
    {@const r = remembered(s.period)}
    <div class="body" class:loading>
      {#if s.allTime.reviews === 0 && totalMs(s.allTime) === 0}
        <div class="notice">{deckId !== null || (mode === "advanced" && direction) ? t("stats.emptyFilter") : t("stats.empty")}</div>
      {/if}

      {#if takeaways.length}
        <ul class="takeaways card-surface">
          {#each takeaways as line (line)}<li>{line}</li>{/each}
        </ul>
      {/if}

      <div class="tiles">
        <div class="tile card-surface">
          <div class="label small muted">{t("stats.tile.time")}</div>
          <div class="value">{durationLabel(totalMs(s.period))}</div>
          <div class="sub small faint">
            {change(totalMs(s.period), s.previous ? totalMs(s.previous) : undefined) || t("stats.tile.daysStudied", { n: s.period.daysStudied })}
          </div>
        </div>
        <div class="tile card-surface">
          <div class="label small muted">{t("stats.tile.reviews")}</div>
          <div class="value">{fmtNum(s.period.reviews)}</div>
          <div class="sub small faint">
            {change(s.period.reviews, s.previous?.reviews) || t("stats.tile.today", { n: fmtNum(s.today.reviews) })}
          </div>
        </div>
        <div class="tile card-surface">
          <div class="label small muted">{t("stats.tile.remembered")}</div>
          <div class="value">{pct(r)}</div>
          <div class="sub small faint">
            {t("stats.tile.target", { p: pct(s.targetRetention) })}
            {changePoints(r, remembered(s.previous))}
          </div>
        </div>
        <div class="tile card-surface">
          <div class="label small muted">{t("stats.tile.newWords")}</div>
          <div class="value">{fmtNum(s.period.newWords)}</div>
          <div class="sub small faint">{change(s.period.newWords, s.previous?.newWords) || t("stats.unstudied", { n: fmtNum(s.unstudiedCards) })}</div>
        </div>
        <div class="tile card-surface">
          <div class="label small muted">{t("stats.streak")}</div>
          <div class="value streak"><Icon name="flame" size={22} />{s.streak}</div>
          <div class="sub small faint">{t("stats.best", { n: s.bestStreak })}</div>
        </div>
        <div class="tile card-surface">
          <div class="label small muted">{t("stats.tile.due")}</div>
          <div class="value">{fmtNum(s.dueNow)}</div>
          <div class="sub small faint">{t("stats.tile.tomorrow", { n: fmtNum(s.dueTomorrow) })}</div>
        </div>
      </div>
      <p class="alltime small faint">
        {t("stats.allTimeLine", { reviews: fmtNum(s.allTime.reviews), time: durationLabel(totalMs(s.allTime)), days: fmtNum(s.allTime.daysStudied) })}
        {#if s.previous}· {t("stats.compared", { period: periodName })}{/if}
      </p>

      <section class="panel card-surface">
        <div class="panel-head">
          <h2>{t(metric === "time" ? "stats.chart.time" : metric === "reviews" ? "stats.chart.reviews" : "stats.chart.new")}</h2>
          <div class="seg">
            <button class:on={metric === "time"} onclick={() => (metric = "time")}>{t("stats.metric.time")}</button>
            <button class:on={metric === "reviews"} onclick={() => (metric = "reviews")}>{t("stats.metric.reviews")}</button>
            <button class:on={metric === "new"} onclick={() => (metric = "new")}>{t("stats.metric.new")}</button>
            <button class="link small" onclick={() => (asTable = !asTable)}>{asTable ? t("stats.chart") : t("stats.table")}</button>
          </div>
        </div>
        {#if asTable}
          <div class="tablewrap">
            <table>
              <thead>
                <tr>
                  <th>{t("stats.date")}</th><th>{t("stats.col.time")}</th><th>{t("stats.col.reviews")}</th><th>{t("stats.correctPct")}</th><th>{t("stats.metric.new")}</th>
                  {#if otherTime}<th>{t("stats.time.reading")}</th><th>{t("stats.time.tutor")}</th>{/if}
                </tr>
              </thead>
              <tbody>
                {#each [...points].reverse().filter((p) => p.reviews || p.reading || p.tutor) as p (p.label)}
                  <tr>
                    <td>{p.label}</td>
                    <td>{durationLabel(p.cards + p.reading + p.tutor)}</td>
                    <td>{fmtNum(p.reviews)}</td>
                    <td>{pct(share(p.correct, p.reviews))}</td>
                    <td>{fmtNum(p.newCards)}</td>
                    {#if otherTime}<td>{durationLabel(p.reading)}</td><td>{durationLabel(p.tutor)}</td>{/if}
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        {:else if metric === "time" && otherTime}
          <BarChart
            stacks={[points.map((p) => p.cards / 60000), points.map((p) => p.reading / 60000), points.map((p) => p.tutor / 60000)]}
            colors={["var(--cat-1)", "var(--cat-2)", "var(--cat-3)"]}
            labels={points.map((p) => p.label)}
            tooltip={pointTip}
            format={axisTime}
            ariaLabel={t("stats.chart.time")}
          />
          <div class="legend">
            <span><i style="background:var(--cat-1)"></i>{t("stats.time.cards")} <strong>{durationLabel(s.period.timeMs)}</strong></span>
            <span><i style="background:var(--cat-2)"></i>{t("stats.time.reading")} <strong>{durationLabel(s.period.readingMs)}</strong></span>
            <span><i style="background:var(--cat-3)"></i>{t("stats.time.tutor")} <strong>{durationLabel(s.period.tutorMs)}</strong></span>
          </div>
        {:else}
          <BarChart
            values={points.map((p) => (metric === "time" ? (p.cards + p.reading + p.tutor) / 60000 : metric === "reviews" ? p.reviews : p.newCards))}
            labels={points.map((p) => p.label)}
            tooltip={pointTip}
            format={metric === "time" ? axisTime : undefined}
            ariaLabel={t(metric === "time" ? "stats.chart.time" : metric === "reviews" ? "stats.chart.reviews" : "stats.chart.new")}
          />
        {/if}
        {#if metric === "time" && !s.otherTimeIncluded}<p class="small faint note">{t("stats.time.cardsOnly")}</p>{/if}
      </section>

      <div class="two">
        <section class="panel card-surface">
          <h2>{t("stats.cardStates")}</h2>
          {#if stateTotal > 0}
            <div class="stack tall" role="img" aria-label={t("stats.cardStates")}>
              {#each states as st (st.key)}
                {#if st.n > 0}<div style="flex:{st.n};background:{st.color}" title="{t(st.key)}: {fmtNum(st.n)}"></div>{/if}
              {/each}
            </div>
          {/if}
          <div class="legend">
            {#each states as st (st.key)}
              <span><i style="background:{st.color}"></i>{t(st.key)} <strong>{fmtNum(st.n)}</strong></span>
            {/each}
          </div>
          <p class="small faint note">{t("stats.wordsNote", { words: fmtNum(s.totalWords), cards: fmtNum(s.totalCards) })}</p>
        </section>
        <section class="panel card-surface">
          <h2>{t("stats.activity")}</h2>
          <Heatmap
            days={s.days.slice(-182).map((d) => ({ date: d.date, value: (d.timeMs + d.readingMs + d.tutorMs) / 60000 }))}
            tooltip={(d) => `${shortDate(d.date)} · ${durationLabel(d.value * 60000)}`}
            ariaLabel={t("stats.activity")}
            less={t("stats.less")}
            more={t("stats.more")}
          />
        </section>
      </div>

      <section class="panel card-surface">
        <div class="panel-head">
          <h2>{t("stats.forecast")}</h2>
          {#if mode === "advanced"}<span class="small faint">{t("stats.dailyLoad", { n: fmtNum(Math.round(s.dailyLoad)) })}</span>{/if}
        </div>
        <BarChart
          values={s.forecast}
          labels={forecastLabels}
          tooltip={(i) => `${forecastLabels[i]} · ${s.forecast[i]} ${t("common.due")}`}
          ariaLabel={t("stats.forecast")}
          height={140}
        />
      </section>

      {#if mode === "advanced"}
        <div class="two">
          <section class="panel card-surface">
            <h2>{t("stats.hsk")}</h2>
            {#if s.hsk.length === 0}
              <p class="muted">{t("stats.hskNone")}</p>
            {:else}
              <div class="meters">
                {#each s.hsk as h (h.level)}
                  <div class="mname">{h.level}</div>
                  <div class="meter" title={t("stats.hskTip", { studied: fmtNum(h.studied), mature: fmtNum(h.mature), words: fmtNum(h.words) })}>
                    <div style="width:{(h.studied / h.words) * 100}%;background:var(--seq-2)"></div>
                    <div style="width:{(h.mature / h.words) * 100}%;background:var(--seq-4)"></div>
                  </div>
                  <div class="mfig small">{fmtNum(h.studied)} / {fmtNum(h.words)}</div>
                {/each}
              </div>
              <div class="legend">
                <span><i style="background:var(--seq-2)"></i>{t("stats.started")}</span>
                <span><i style="background:var(--seq-4)"></i>{t("stats.state.mature")}</span>
              </div>
            {/if}
          </section>
          <section class="panel card-surface">
            <h2>{t("stats.memory")}</h2>
            <div class="figs">
              <div><div class="fig">{pct(s.avgRetrievability)}</div><div class="small muted">{t("stats.mem.recall")}</div></div>
              <div><div class="fig">{fmtNum(Math.round(s.knownEstimate))}</div><div class="small muted">{t("stats.mem.known", { n: fmtNum(s.studiedItems) })}</div></div>
              <div><div class="fig">{fmtNum(s.leeches)}</div><div class="small muted">{t("stats.mem.leeches")}</div></div>
            </div>
            <table>
              <thead>
                <tr><th></th><th>{t("stats.kind.young")}</th><th>{t("stats.kind.mature")}</th></tr>
              </thead>
              <tbody>
                {#each s.retentionTable as row (row.range)}
                  <tr><td>{t(RANGES[row.range])}</td><td>{cell(row.young)}</td><td>{cell(row.mature)}</td></tr>
                {/each}
              </tbody>
            </table>
            <p class="small faint note">{t("stats.ret.note")}</p>
          </section>
        </div>

        <section class="panel card-surface">
          <h2>{t("stats.answers")}</h2>
          <div class="answers">
            {#each KINDS as kind, k (kind)}
              {@const row = s.buttons[k]}
              {@const n = row.reduce((a, b) => a + b, 0)}
              <div class="mname">{t(kind)}</div>
              <div class="stack tall" role="img" aria-label={t(kind)}>
                {#each BUTTONS as b, i (b.key)}
                  {#if row[i] > 0}<div style="flex:{row[i]};background:{b.color}" title="{t(b.key)}: {fmtNum(row[i])} ({pct(row[i] / n)})"></div>{/if}
                {/each}
              </div>
              <div class="mfig small">
                {#if n}{pct((n - row[0]) / n)} {t("stats.correctLower")} · {fmtNum(n)} · {seconds(s.kindTimeMs[k], n)}{:else}—{/if}
              </div>
            {/each}
          </div>
          <div class="legend">
            {#each BUTTONS as b, i (b.key)}
              <span><i style="background:{b.color}"></i>{t(b.key)} <strong>{fmtNum(s.buttons.reduce((a, row) => a + row[i], 0))}</strong></span>
            {/each}
          </div>
        </section>

        <div class="two">
          <section class="panel card-surface">
            <div class="panel-head">
              <h2>{t("stats.byHour")}</h2>
              {#if bestHour}<span class="small faint">{t("stats.bestHour", { h: hourLabel(bestHour.i), p: pct(bestHour.correct / bestHour.reviews) })}</span>{/if}
            </div>
            <BarChart
              values={s.hours.map((h) => h.reviews)}
              labels={s.hours.map((_, i) => String(i))}
              tooltip={(i) => `${hourLabel(i)} · ${fmtNum(s.hours[i].reviews)} ${t("stats.reviews")}` + (s.hours[i].reviews ? ` · ${pct(s.hours[i].correct / s.hours[i].reviews)} ${t("stats.correctLower")}` : "")}
              ariaLabel={t("stats.byHour")}
              height={140}
            />
          </section>
          <section class="panel card-surface">
            <h2>{t("stats.byWeekday")}</h2>
            <BarChart
              values={s.weekdays.map((h) => h.reviews)}
              labels={weekdayNames}
              tooltip={(i) => `${weekdayNames[i]} · ${fmtNum(s.weekdays[i].reviews)} ${t("stats.reviews")}` + (s.weekdays[i].reviews ? ` · ${pct(s.weekdays[i].correct / s.weekdays[i].reviews)} ${t("stats.correctLower")}` : "")}
              ariaLabel={t("stats.byWeekday")}
              height={140}
            />
          </section>
        </div>

        <div class="two">
          <section class="panel card-surface">
            <h2>{t("stats.byDirection")}</h2>
            {#if s.byDirection.length === 0}
              <p class="muted">{t("stats.noneInPeriod")}</p>
            {:else}
              <table>
                <thead>
                  <tr><th>{t("stats.filter.direction")}</th><th>{t("stats.col.reviews")}</th><th>{t("stats.correctPct")}</th><th>{t("stats.col.each")}</th><th>{t("stats.col.time")}</th></tr>
                </thead>
                <tbody>
                  {#each s.byDirection as d (d.direction)}
                    <tr>
                      <td>{t(`dir.${d.direction}`)}</td>
                      <td>{fmtNum(d.reviews)}</td>
                      <td>{pct(share(d.correct, d.reviews))}</td>
                      <td>{seconds(d.timeMs, d.reviews)}</td>
                      <td>{durationLabel(d.timeMs)}</td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            {/if}
          </section>
          <section class="panel card-surface">
            <h2>{t("stats.byDeck")}</h2>
            <div class="tablewrap">
              <table>
                <thead>
                  <tr><th>{t("stats.filter.deck")}</th><th>{t("stats.col.words")}</th><th>{t("stats.started")}</th><th>{t("stats.col.reviews")}</th><th>{t("stats.correctPct")}</th><th>{t("stats.col.time")}</th></tr>
                </thead>
                <tbody>
                  {#each s.byDeck as d (d.deckId)}
                    <tr>
                      <td>
                        {#if d.deckId !== deckId}<button class="link" onclick={() => (deckId = d.deckId)} title={t("stats.openDeck")}>{d.name}</button>{:else}{d.name}{/if}
                      </td>
                      <td>{fmtNum(d.words)}</td>
                      <td>{pct(share(d.studied, d.words))}</td>
                      <td>{fmtNum(d.reviews)}</td>
                      <td>{pct(share(d.correct, d.reviews))}</td>
                      <td>{d.timeMs ? durationLabel(d.timeMs) : "—"}</td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>
          </section>
        </div>

        <div class="two">
          <section class="panel card-surface">
            <h2>{t("stats.stability")}</h2>
            <BarChart
              values={s.stability}
              labels={STABILITY}
              tooltip={(i) => `${STABILITY[i]} ${t("stats.unit.days")} · ${fmtNum(s.stability[i])}`}
              ariaLabel={t("stats.stability")}
              height={140}
            />
            <p class="small faint note">{t("stats.stabilityNote")}</p>
          </section>
          <section class="panel card-surface">
            <h2>{t("stats.difficulty")}</h2>
            <BarChart
              values={s.difficulty}
              labels={s.difficulty.map((_, i) => String(i + 1))}
              tooltip={(i) => `${i + 1} / 10 · ${fmtNum(s.difficulty[i])}`}
              ariaLabel={t("stats.difficulty")}
              height={140}
            />
            <p class="small faint note">{t("stats.difficultyNote")}</p>
          </section>
        </div>
      {/if}

      <section class="panel card-surface">
        <h2>{t("stats.hardest")}</h2>
        {#if s.hardest.length === 0}
          <p class="muted">{t("stats.noHard")}</p>
        {:else}
          <ul class="hard">
            {#each s.hardest as h (h.cardId + h.direction)}
              <li>
                <Hanzi hanzi={h.hanzi} pinyin={h.pinyin} size={20} />
                <Pinyin text={h.pinyin} />
                <span class="muted grow">{h.meaning}</span>
                <span class="chip">{t(`dir.${h.direction}.short`)}</span>
                <span class="small faint">{t("stats.lapses", { n: h.lapses })}</span>
              </li>
            {/each}
          </ul>
        {/if}
      </section>
    </div>
  {/if}
</div>

<style>
  .filters {
    display: flex;
    flex-wrap: wrap;
    align-items: end;
    gap: 14px;
    margin-bottom: 14px;
  }
  .filters label {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .filters select {
    width: auto;
    min-width: 170px;
    max-width: 280px;
  }
  .filters .seg {
    margin-left: auto;
  }
  .body.loading {
    opacity: 0.6;
    transition: opacity 0.15s 0.15s;
  }
  .takeaways {
    list-style: none;
    margin: 0 0 14px;
    padding: 14px 18px;
    border-radius: var(--radius);
    display: flex;
    flex-direction: column;
    gap: 6px;
    border-left: 3px solid var(--accent);
  }
  .tiles {
    display: grid;
    grid-template-columns: repeat(6, 1fr);
    gap: 12px;
    margin-bottom: 6px;
  }
  .tile {
    padding: 14px 16px;
    border-radius: var(--radius);
    min-width: 0;
  }
  .value {
    font-size: 26px;
    font-weight: 650;
    font-variant-numeric: tabular-nums;
    line-height: 1.3;
    white-space: nowrap;
  }
  .sub {
    font-variant-numeric: tabular-nums;
  }
  .streak {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .streak :global(svg) {
    color: var(--series-2);
  }
  .alltime {
    margin: 0 2px 14px;
  }
  .panel {
    padding: 18px 20px;
    border-radius: var(--radius);
    margin-bottom: 14px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-width: 0;
  }
  .panel-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }
  .seg {
    display: flex;
    gap: 2px;
    align-items: center;
  }
  /* on the page itself (not in a panel): its own ground, so page art stays behind it */
  .seg.boxed {
    padding: 2px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
  }
  .seg button {
    border: none;
    background: transparent;
    padding: 4px 10px;
    border-radius: 6px;
    color: var(--text-2);
    white-space: nowrap;
  }
  .seg button.on {
    background: var(--surface-3);
    color: var(--text);
    font-weight: 600;
  }
  .link {
    border: none;
    background: transparent;
    padding: 0;
    text-decoration: underline;
    color: inherit;
    text-align: left;
  }
  .seg .link {
    margin-left: 8px;
    padding: 4px 0;
    color: var(--text-3);
  }
  .stack {
    display: flex;
    gap: 2px;
    height: 14px;
    border-radius: 4px;
    overflow: hidden;
  }
  .stack.tall {
    height: 22px;
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 18px;
    color: var(--text-2);
  }
  .legend i {
    display: inline-block;
    width: 10px;
    height: 10px;
    border-radius: 3px;
    margin-right: 6px;
  }
  .legend strong {
    color: var(--text);
    margin-left: 2px;
    font-variant-numeric: tabular-nums;
  }
  .note {
    margin: 0;
  }
  .two {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 14px;
  }
  .meters,
  .answers {
    display: grid;
    grid-template-columns: max-content 1fr max-content;
    align-items: center;
    gap: 8px 12px;
  }
  .mname {
    font-weight: 600;
    white-space: nowrap;
  }
  .mfig {
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .meter {
    position: relative;
    height: 12px;
    border-radius: 6px;
    background: var(--seq-0);
    overflow: hidden;
  }
  .meter div {
    position: absolute;
    inset: 0 auto 0 0;
    border-radius: 6px;
  }
  .figs {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 12px;
  }
  .fig {
    font-size: 22px;
    font-weight: 650;
    font-variant-numeric: tabular-nums;
  }
  .tablewrap {
    max-height: 300px;
    overflow: auto;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-variant-numeric: tabular-nums;
  }
  th,
  td {
    text-align: left;
    padding: 6px 10px;
    border-bottom: 1px solid var(--border);
  }
  td:not(:first-child) {
    white-space: nowrap;
  }
  tbody tr:last-child td {
    border-bottom: none;
  }
  th {
    font-size: 12px;
    color: var(--text-3);
    position: sticky;
    top: 0;
    background: var(--surface);
  }
  .hard {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .hard li {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 7px 0;
    border-bottom: 1px solid var(--border);
  }
  .hard li:last-child {
    border-bottom: none;
  }
  .grow {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  @media (max-width: 1180px) {
    .tiles {
      grid-template-columns: repeat(3, 1fr);
    }
  }
  @media (max-width: 960px) {
    .two {
      grid-template-columns: 1fr;
    }
    .filters .seg {
      margin-left: 0;
    }
  }
</style>
