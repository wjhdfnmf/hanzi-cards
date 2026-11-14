<script lang="ts">
  // Calendar heatmap (weeks as columns, Monday at the top), sequential single-hue scale.
  let {
    days,
    tooltip,
    ariaLabel,
    less = "",
    more = "",
  }: {
    /** One entry per day, oldest first. */
    days: { date: string; value: number }[];
    tooltip: (d: { date: string; value: number }) => string;
    ariaLabel: string;
    /** Words for the two ends of the scale. */
    less?: string;
    more?: string;
  } = $props();
  let hover = $state<{ date: string; value: number } | null>(null);
  const cell = 13;
  const gap = 3;

  const max = $derived(Math.max(1, ...days.map((d) => d.value)));
  // Pad the front so the first column starts on Monday.
  const cells = $derived.by(() => {
    if (!days.length) return [] as ({ date: string; value: number } | null)[];
    const first = new Date(days[0].date + "T00:00:00");
    const pad = (first.getDay() + 6) % 7;
    return [...Array(pad).fill(null), ...days] as ({ date: string; value: number } | null)[];
  });
  const weeks = $derived(Math.ceil(cells.length / 7));

  function level(n: number) {
    if (n <= 0) return 0;
    const f = n / max;
    return f <= 0.25 ? 1 : f <= 0.5 ? 2 : f <= 0.75 ? 3 : 4;
  }
</script>

<div>
<div class="wrap">
  <svg width={weeks * (cell + gap)} height={7 * (cell + gap)} role="img" aria-label={ariaLabel}>
    {#each cells as d, i (i)}
      {#if d}
        <rect
          x={Math.floor(i / 7) * (cell + gap)}
          y={(i % 7) * (cell + gap)}
          width={cell}
          height={cell}
          rx="3"
          fill="var(--seq-{level(d.value)})"
          role="presentation"
          onmouseenter={() => (hover = d)}
          onmouseleave={() => (hover = null)}
        />
      {/if}
    {/each}
  </svg>
</div>
<div class="foot small faint">
    {#if hover}
      {tooltip(hover)}
    {:else}
      <span class="legend">
        {less}
        {#each [0, 1, 2, 3, 4] as l (l)}<i style="background:var(--seq-{l})"></i>{/each}
        {more}
      </span>
    {/if}
</div>
</div>

<style>
  .wrap {
    overflow-x: auto;
    overflow-y: hidden; /* (a sideways scroll bar must not bring an up-and-down one with it) */
    /* Right to left, so that when the weeks don't fit it is the newest that show, at any
       window size, and the older ones are a scroll to the left. (Setting the scroll position
       from a script only worked if the weeks already didn't fit when the page opened.) */
    direction: rtl;
  }
  svg {
    display: block;
    direction: ltr;
    margin-right: auto; /* at the left when everything fits */
  }
  .foot {
    margin-top: 6px;
    height: 18px;
  }
  .legend {
    display: inline-flex;
    align-items: center;
    gap: 3px;
  }
  .legend i {
    width: 11px;
    height: 11px;
    border-radius: 3px;
    display: inline-block;
  }
</style>
