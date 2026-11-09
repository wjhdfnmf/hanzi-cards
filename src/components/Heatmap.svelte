<script lang="ts">
  // Calendar heatmap (weeks as columns), sequential single-hue scale.
  import type { DayStat } from "../lib/api";
  import { t } from "../lib/i18n.svelte";

  let { days }: { days: DayStat[] } = $props();
  let hover = $state<DayStat | null>(null);

  const cell = 13;
  const gap = 3;

  const max = $derived(Math.max(1, ...days.map((d) => d.reviews)));
  // Pad the front so the first column starts on Monday.
  const cells = $derived.by(() => {
    if (!days.length) return [] as (DayStat | null)[];
    const first = new Date(days[0].date + "T00:00:00");
    const pad = (first.getDay() + 6) % 7;
    return [...Array(pad).fill(null), ...days] as (DayStat | null)[];
  });
  const weeks = $derived(Math.ceil(cells.length / 7));

  function level(n: number) {
    if (n === 0) return 0;
    const f = n / max;
    return f <= 0.25 ? 1 : f <= 0.5 ? 2 : f <= 0.75 ? 3 : 4;
  }
</script>

<div class="wrap">
  <svg
    width={weeks * (cell + gap)}
    height={7 * (cell + gap)}
    role="img"
    aria-label={t("stats.activity")}
  >
    {#each cells as d, i (i)}
      {#if d}
        <rect
          x={Math.floor(i / 7) * (cell + gap)}
          y={(i % 7) * (cell + gap)}
          width={cell}
          height={cell}
          rx="3"
          fill="var(--seq-{level(d.reviews)})"
          role="presentation"
          onmouseenter={() => (hover = d)}
          onmouseleave={() => (hover = null)}
        />
      {/if}
    {/each}
  </svg>
  <div class="foot small faint">
    {#if hover}
      {hover.date} · {hover.reviews} {t("stats.reviews")}
    {:else}
      <span class="legend">
        {#each [0, 1, 2, 3, 4] as l (l)}<i style="background:var(--seq-{l})"></i>{/each}
      </span>
    {/if}
  </div>
</div>

<style>
  .wrap {
    overflow-x: auto;
  }
  svg {
    display: block;
  }
  .foot {
    margin-top: 6px;
    height: 18px;
  }
  .legend {
    display: inline-flex;
    gap: 3px;
  }
  .legend i {
    width: 11px;
    height: 11px;
    border-radius: 3px;
    display: inline-block;
  }
</style>
