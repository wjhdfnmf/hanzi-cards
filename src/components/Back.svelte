<script lang="ts">
  // On a phone, the way back from a page to the list it was chosen from (Settings, Decks,
  // Reading show one of the two at a time there).
  import Icon from "./Icon.svelte";
  let { label, onclick }: { label: string; onclick: () => void } = $props();
</script>

<button class="back" aria-label={label} {onclick}>
  <Icon name="chevron-left" size={22} />
  <span>{label}</span>
</button>

<style>
  .back {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    margin: 0 0 6px -8px;
    padding: 6px 8px 6px 2px;
    border: none;
    background: transparent;
    color: var(--accent);
    font: inherit;
    font-weight: 550;
    border-radius: 8px;
  }
  .back:active {
    background: var(--accent-soft);
  }
</style>
