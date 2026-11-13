<script lang="ts">
  // Bar chart with a hover tooltip: one series, or a few stacked on one axis. Bars are thin,
  // rounded at the data end, anchored to the baseline; grid and axes stay recessive.
  let {
    values = [],
    stacks,
    colors = [],
    labels,
    tooltip,
    color = "var(--series-1)",
    height = 160,
    format = (v: number) => String(Math.round(v)),
    ariaLabel,
  }: {
    /** One series. */
    values?: number[];
    /** Or several, stacked: one array of values per series, first at the bottom. */
    stacks?: number[][];
    colors?: string[];
    labels: string[];
    tooltip: (i: number) => string;
    color?: string;
    height?: number;
    /** How an axis value is written. */
    format?: (v: number) => string;
    ariaLabel: string;
  } = $props();

  let width = $state(600);
  let hover = $state<number | null>(null);

  const series = $derived(stacks && stacks.length ? stacks : [values]);
  const fills = $derived(stacks && stacks.length ? colors : [color]);
  const totals = $derived(labels.map((_, i) => series.reduce((s, row) => s + (row[i] ?? 0), 0)));

  const padL = 40;
  const padB = 22;
  const padT = 8;
  const gap = 2; // surface between stacked parts
  const max = $derived(Math.max(1, ...totals));
  const niceMax = $derived(niceCeil(max));
  const ticks = $derived([0, niceMax / 2, niceMax]);
  const plotW = $derived(Math.max(10, width - padL - 4));
  const plotH = $derived(height - padB - padT);
  const step = $derived(plotW / Math.max(1, totals.length));
  const barW = $derived(Math.max(2, Math.min(28, step - 2)));
  // Every label when there are few bars, about 8 otherwise; the last bar is always named, and
  // a label that would run into it is left out.
  const labelEvery = $derived(totals.length <= 12 ? 1 : Math.ceil(totals.length / 8));
  const labelled = (i: number) => {
    const last = totals.length - 1;
    return i === last || (i % labelEvery === 0 && last - i >= labelEvery / 2);
  };

  function niceCeil(v: number) {
    if (v <= 4) return 4;
    const p = Math.pow(10, Math.floor(Math.log10(v)));
    for (const m of [1, 2, 2.5, 5, 10]) if (m * p >= v) return m * p;
    return 10 * p;
  }
  const y = (v: number) => padT + plotH - (v / niceMax) * plotH;

  function barPath(x: number, w: number, top: number, bottom: number, round: boolean) {
    const h = bottom - top;
    if (h <= 0) return "";
    const r = round ? Math.min(4, w / 2, h) : 0;
    return `M${x},${bottom} V${top + r} Q${x},${top} ${x + r},${top} H${x + w - r} Q${x + w},${top} ${x + w},${top + r} V${bottom} Z`;
  }
  /** The parts of one bar, bottom up: [top, bottom, colour, is the topmost]. */
  function parts(i: number): [number, number, string, boolean][] {
    const out: [number, number, string, boolean][] = [];
    let base = 0;
    const last = series.reduce((k, row, s) => ((row[i] ?? 0) > 0 ? s : k), -1);
    series.forEach((row, s) => {
      const v = row[i] ?? 0;
      if (v <= 0) return;
      const bottom = y(base) - (base > 0 ? gap : 0);
      out.push([y(base + v), bottom, fills[s] ?? color, s === last]);
      base += v;
    });
    return out;
  }
</script>

<div class="chart" bind:clientWidth={width}>
  <svg {width} {height} role="img" aria-label={ariaLabel}>
    {#each ticks as tk (tk)}
      <line x1={padL} x2={width - 4} y1={y(tk)} y2={y(tk)} class="grid" />
      <text x={padL - 6} y={y(tk) + 4} class="axis" text-anchor="end">{format(tk)}</text>
    {/each}
    {#each totals as _, i (i)}
      {@const x = padL + i * step + (step - barW) / 2}
      <g opacity={hover === null || hover === i ? 1 : 0.55}>
        {#each parts(i) as [top, bottom, fill, round], s (s)}
          <path d={barPath(x, barW, top, bottom, round)} {fill} />
        {/each}
      </g>
      <!-- generous hit target -->
      <rect
        x={padL + i * step}
        y={padT}
        width={step}
        height={plotH}
        fill="transparent"
        role="presentation"
        onmouseenter={() => (hover = i)}
        onmouseleave={() => (hover = null)}
      />
      {#if labelled(i)}
        <text x={padL + i * step + step / 2} y={height - 6} class="axis" text-anchor="middle">{labels[i]}</text>
      {/if}
    {/each}
  </svg>
  {#if hover !== null}
    {@const left = Math.min(width - 190, Math.max(0, padL + hover * step + step / 2 - 95))}
    <div class="tip" style="left:{left}px">{tooltip(hover)}</div>
  {/if}
</div>

<style>
  .chart {
    position: relative;
    width: 100%;
  }
  svg {
    display: block;
    overflow: visible;
  }
  .grid {
    stroke: var(--grid);
    stroke-width: 1;
  }
  .axis {
    fill: var(--text-3);
    font-size: 11px;
  }
  .tip {
    position: absolute;
    top: -6px;
    width: 190px;
    pointer-events: none;
    background: var(--text);
    color: var(--bg);
    font-size: 12px;
    padding: 5px 9px;
    border-radius: 6px;
    text-align: center;
    box-shadow: var(--shadow-lg);
    white-space: pre-line;
  }
</style>
