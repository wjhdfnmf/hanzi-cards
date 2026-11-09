<script lang="ts">
  // Chinese characters, optionally coloured by tone (when pinyin lines up with the characters).
  import { hanziTones } from "../lib/pinyin";
  import { settings } from "../lib/settings.svelte";

  let {
    hanzi,
    pinyin = "",
    colored = true,
    size = 0,
  }: { hanzi: string; pinyin?: string; colored?: boolean; size?: number } = $props();

  const parts = $derived(hanziTones(hanzi, pinyin));
  const useColor = $derived(colored && settings.toneColors && pinyin !== "");
</script>

<span class="han hz" lang="zh-CN" style={size ? `font-size:${size}px` : ""}
  >{#each parts as p, i (i)}<span class={useColor && p.tone ? `tone-${p.tone}` : ""}>{p.ch}</span>{/each}</span
>

<style>
  .hz {
    line-height: 1.2;
    letter-spacing: 0.02em;
  }
</style>
