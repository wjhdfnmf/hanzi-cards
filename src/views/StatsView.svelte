<script lang="ts">
  import { onMount } from "svelte";
  import { api, type Stats } from "../lib/api";
  import { t, i18n } from "../lib/i18n.svelte";
  import { durationLabel, fmtNum, pct } from "../lib/format";
  import BarChart from "../components/BarChart.svelte";
  import Heatmap from "../components/Heatmap.svelte";
  import Hanzi from "../components/Hanzi.svelte";
  import Pinyin from "../components/Pinyin.svelte";
  import Icon from "../components/Icon.svelte";

  let stats = $state<Stats | null>(null);
  let range = $state<30 | 90 | 365>(30);
  let asTable = $state(false);

  onMount(async () => {
    stats = await api.getStats(365);
  });

  const days = $derived(stats ? stats.days.slice(-range) : []);
  const shortDate = (iso: string) => {
    const d = new Date(iso + "T00:00:00");
    return d.toLocaleDateString(i18n.lang === "zh" ? "zh-CN" : undefined, { month: "short", day: "numeric" });
  };
  const forecastLabels = $derived(
    stats
      ? stats.forecast.map((_, i) => {
          if (i === 0) return t("stats.today.short");
          const d = new Date();
          d.setDate(d.getDate() + i);
          return d.toLocaleDateString(i18n.lang === "zh" ? "zh-CN" : undefined, { month: "short", day: "numeric" });
        })
      : [],
  );

  const states = $derived(
    stats
      ? [
          { key: "stats.state.new", n: stats.unstudiedCards, color: "var(--series-1)" },
          { key: "stats.state.learning", n: stats.learning, color: "var(--series-2)" },
          { key: "stats.state.young", n: stats.young, color: "var(--series-3)" },
          { key: "stats.state.mature", n: stats.mature, color: "var(--series-4)" },
        ] as const
      : [],
  );
  const stateTotal = $derived(states.reduce((s, x) => s + x.n, 0));
</script>

<div class="page">
  <div class="page-head">
    <h1>{t("stats.title")}</h1>
  </div>

  {#if !stats}
    <p class="muted">{t("common.loading")}</p>
  {:else}
    <div class="tiles">
      <div class="tile card-surface">
        <div class="label small muted">{t("stats.today")}</div>
        <div class="value">{fmtNum(stats.todayReviews)}</div>
        <div class="sub small faint">{t("stats.timeStudied", { t: durationLabel(stats.todayTimeMs) })}</div>
      </div>
      <div class="tile card-surface">
        <div class="label small muted">{t("stats.retention")}</div>
        <div class="value">{pct(stats.retention)}</div>
        <div class="sub small faint">{fmtNum(stats.totalReviews)} {t("stats.reviews")}</div>
      </div>
      <div class="tile card-surface">
        <div class="label small muted">{t("stats.streak")}</div>
        <div class="value streak"><Icon name="flame" size={22} />{stats.streak}</div>
        <div class="sub small faint">{t("stats.best", { n: stats.bestStreak })}</div>
      </div>
      <div class="tile card-surface">
        <div class="label small muted">{t("stats.totalCards")}</div>
        <div class="value">{fmtNum(stats.totalCards)}</div>
        <div class="sub small faint">{t("stats.unstudied", { n: stats.unstudiedCards })}</div>
      </div>
    </div>

    {#if stats.totalReviews === 0}
      <div class="notice">{t("stats.empty")}</div>
    {/if}

    <section class="panel card-surface">
      <h2>{t("stats.cardStates")}</h2>
      {#if stateTotal > 0}
        <div class="stack" role="img" aria-label={t("stats.cardStates")}>
          {#each states as s (s.key)}
            {#if s.n > 0}
              <div style="flex:{s.n};background:{s.color}" title="{t(s.key)}: {s.n}"></div>
            {/if}
          {/each}
        </div>
      {/if}
      <div class="legend">
        {#each states as s (s.key)}
          <span><i style="background:{s.color}"></i>{t(s.key)} <strong>{fmtNum(s.n)}</strong></span>
        {/each}
      </div>
    </section>

    <section class="panel card-surface">
      <div class="panel-head">
        <h2>{t("stats.reviewsPerDay")}</h2>
        <div class="seg">
          {#each [30, 90, 365] as const as r (r)}
            <button class:on={range === r} onclick={() => (range = r)}>{t(`stats.range.${r}`)}</button>
          {/each}
          <button class="link small" onclick={() => (asTable = !asTable)}>
            {asTable ? t("stats.chart") : t("stats.table")}
          </button>
        </div>
      </div>
      {#if asTable}
        <div class="tablewrap">
          <table>
            <thead>
              <tr><th>{t("stats.date")}</th><th>{t("stats.reviews")}</th><th>{t("stats.correctPct")}</th><th>{t("study.time")}</th></tr>
            </thead>
            <tbody>
              {#each [...days].reverse().filter((d) => d.reviews > 0) as d (d.date)}
                <tr>
                  <td>{d.date}</td>
                  <td>{d.reviews}</td>
                  <td>{pct(d.correct / d.reviews)}</td>
                  <td>{durationLabel(d.timeMs)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {:else}
        <BarChart
          values={days.map((d) => d.reviews)}
          labels={days.map((d) => shortDate(d.date))}
          tooltip={(i) =>
            `${shortDate(days[i].date)} · ${days[i].reviews} ${t("stats.reviews")}` +
            (days[i].reviews ? ` · ${pct(days[i].correct / days[i].reviews)}` : "")}
          ariaLabel={t("stats.reviewsPerDay")}
        />
      {/if}
    </section>

    <div class="two">
      <section class="panel card-surface">
        <h2>{t("stats.forecast")}</h2>
        <BarChart
          values={stats.forecast}
          labels={forecastLabels}
          tooltip={(i) => `${forecastLabels[i]} · ${stats!.forecast[i]} ${t("common.due")}`}
          ariaLabel={t("stats.forecast")}
          height={140}
        />
      </section>
      <section class="panel card-surface">
        <h2>{t("stats.activity")}</h2>
        <Heatmap days={stats.days.slice(-182)} />
      </section>
    </div>

    <section class="panel card-surface">
      <h2>{t("stats.hardest")}</h2>
      {#if stats.hardest.length === 0}
        <p class="muted">{t("stats.noHard")}</p>
      {:else}
        <ul class="hard">
          {#each stats.hardest as h (h.cardId + h.direction)}
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
  {/if}
</div>

<style>
  .tiles {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 12px;
    margin-bottom: 14px;
  }
  .tile {
    padding: 16px 18px;
    border-radius: var(--radius);
  }
  .value {
    font-size: 30px;
    font-weight: 650;
    font-variant-numeric: tabular-nums;
    line-height: 1.25;
  }
  .streak {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .streak :global(svg) {
    color: var(--series-2);
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
  }
  .seg {
    display: flex;
    gap: 2px;
    align-items: center;
  }
  .seg button {
    border: none;
    background: transparent;
    padding: 4px 10px;
    border-radius: 6px;
    color: var(--text-2);
  }
  .seg button.on {
    background: var(--surface-3);
    color: var(--text);
    font-weight: 600;
  }
  .seg .link {
    margin-left: 8px;
    text-decoration: underline;
    color: var(--text-3);
  }
  .stack {
    display: flex;
    gap: 2px;
    height: 14px;
    border-radius: 4px;
    overflow: hidden;
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: 18px;
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
  }
  .two {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 14px;
  }
  .tablewrap {
    max-height: 280px;
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
  @media (max-width: 960px) {
    .tiles {
      grid-template-columns: repeat(2, 1fr);
    }
    .two {
      grid-template-columns: 1fr;
    }
  }
</style>
