<script lang="ts">
  // The Reading font menu (Reading page and Settings): fonts this computer has, grouped by style.
  import { FONT_GROUPS, READING_FONTS, installedFonts } from "../lib/reading";
  import { settings, saveSetting } from "../lib/settings.svelte";
  import { fontFiles } from "../lib/fonts.svelte";
  import { t } from "../lib/i18n.svelte";

  type Key = Parameters<typeof t>[0];
  const installed = installedFonts();
  // the chosen font stays listed even if it isn't on this computer (e.g. settings from elsewhere)
  const downloaded = $derived(new Set(fontFiles.list.filter((f) => f.downloaded).map((f) => f.key)));
  const shown = $derived(
    READING_FONTS.filter((f) => installed.has(f.key) || downloaded.has(f.key) || f.key === settings.readingFont),
  );
</script>

<select value={settings.readingFont} onchange={(e) => saveSetting("readingFont", e.currentTarget.value)}>
  {#each FONT_GROUPS as g (g)}
    {@const fonts = shown.filter((f) => f.group === g)}
    {#if fonts.length}
      <optgroup label={t(`reading.fontGroup.${g}` as Key)}>
        {#each fonts as f (f.key)}
          <option value={f.key}>{t(`reading.font.${f.key}` as Key)}</option>
        {/each}
      </optgroup>
    {/if}
  {/each}
</select>
