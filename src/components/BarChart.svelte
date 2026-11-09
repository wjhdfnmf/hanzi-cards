<script lang="ts">
  // Single-series bar chart with a hover tooltip. Bars are thin, rounded at the data end,
  // anchored to the baseline; grid and axes stay recessive.
  let {
    values,
    labels,
    tooltip,
    color = "var(--series-1)",
    height = 160,
    ariaLabel,
  }: {
    values: number[];
    labels: string[];
    tooltip: (i: number) => string;
    color?: string;
    height?: number;
    ariaLabel: string;
  } = $props();

  let width = $state(600);
  let hover = $state<number | null>(null);

  const padL = 34;
  const padB = 22;
  const padT = 8;
  const max = $derived(Math.max(1, ...values));
  const niceMax = $derived(niceCeil(max));
  const ticks = $derived([0, niceMax / 2, niceMax]);
  const plotW = $derived(Math.max(10, width - padL - 4));
  const plotH = $derived(height - padB - padT);
  const step = $derived(plotW / Math.max(1, values.length));
  const barW = $derived(Math.max(2, Math.min(28, step - 2)));
  // Show at most ~8 x labels.
  const labelEvery = $derived(Math.max(1, Math.ceil(values.length / 8)));

  function niceCeil(v: number) {
    if (v <= 4) return 4;
    const p = Math.pow(10, Math.floor(Math.log10(v)));
    for (const m of [1, 2, 2.5, 5, 10]) if (m * p >= v) return m * p;
    return 10 * p;
  }
  const y = (v: number) => padT + plotH - (v / niceMax) * plotH;

  function barPath(x: number, w: number, top: number, bottom: number) {
    const h = bottom - top;
    if (h <= 0) return "";
    const r = Math.min(4, w / 2, h);
    return `M${x},${bottom} V${top + r} Q${x},${top} ${x + r},${top} H${x + w - r} Q${x + w},${top} ${x + w},${top + r} V${bottom} Z`;
  }
</script>

<div class="chart" bind:clientWidth={width}>
  <svg {width} {height} role="img" aria-label={ariaLabel}>
    {#each ticks as tk (tk)}
      <line x1={padL} x2={width - 4} y1={y(tk)} y2={y(tk)} class="grid" />
      <text x={padL - 6} y={y(tk) + 4} class="axis" text-anchor="end">{Math.round(tk)}</text>
    {/each}
    {#each values as v, i (i)}
      {@const x = padL + i * step + (step - barW) / 2}
      <path d={barPath(x, barW, y(v), y(0))} fill={color} opacity={hover === null || hover === i ? 1 : 0.55} />
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
      {#if i % labelEvery === 0 || i === values.length - 1}
        <text x={padL + i * step + step / 2} y={height - 6} class="axis" text-anchor="middle">{labels[i]}</text>
      {/if}
    {/each}
  </svg>
  {#if hover !== null}
    {@const left = Math.min(width - 150, Math.max(0, padL + hover * step + step / 2 - 75))}
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
    width: 150px;
    pointer-events: none;
    background: var(--text);
    color: var(--bg);
    font-size: 12px;
    padding: 5px 9px;
    border-radius: 6px;
    text-align: center;
    box-shadow: var(--shadow-lg);
  }
</style>
