<script lang="ts">
  // Pinyin with each syllable coloured by tone.
  import { syllables, toneOf } from "../lib/pinyin";
  import { settings } from "../lib/settings.svelte";

  let { text, colored = true }: { text: string; colored?: boolean } = $props();

  // Keep the user's own spacing: colour syllables inside each space-separated word.
  const words = $derived(
    text
      .trim()
      .split(/\s+/)
      .filter(Boolean)
      .map((w) => syllables(w).map((s) => ({ s, tone: toneOf(s) }))),
  );
</script>

<span class="py"
  >{#each words as word, wi (wi)}{#if wi > 0}{" "}{/if}{#each word as syl, si (si)}<span
        class={colored && settings.toneColors ? `tone-${syl.tone}` : ""}>{syl.s}</span
      >{/each}{/each}</span
>
