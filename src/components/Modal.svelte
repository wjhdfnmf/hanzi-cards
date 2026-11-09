<script lang="ts">
  import type { Snippet } from "svelte";
  import Icon from "./Icon.svelte";

  let {
    title,
    onclose,
    width = 560,
    children,
    footer,
  }: { title: string; onclose: () => void; width?: number; children: Snippet; footer?: Snippet } = $props();

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.stopPropagation();
      onclose();
    }
  }
</script>

<svelte:window {onkeydown} />

<div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && onclose()}>
  <div class="modal card-surface" role="dialog" aria-modal="true" aria-label={title} style="max-width:{width}px">
    <header>
      <h2>{title}</h2>
      <button class="btn ghost icon" onclick={onclose} aria-label="Close"><Icon name="close" /></button>
    </header>
    <div class="body">{@render children()}</div>
    {#if footer}<footer>{@render footer()}</footer>{/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgb(0 0 0 / 0.35);
    display: grid;
    place-items: center;
    padding: 24px;
    z-index: 50;
    animation: fade 0.12s ease-out;
  }
  .modal {
    width: 100%;
    max-height: calc(100vh - 48px);
    display: flex;
    flex-direction: column;
    box-shadow: var(--shadow-lg);
    animation: pop 0.14s ease-out;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 18px 8px 22px;
  }
  .body {
    padding: 8px 22px 18px;
    overflow: auto;
  }
  footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 14px 22px;
    border-top: 1px solid var(--border);
  }
  @keyframes fade {
    from {
      opacity: 0;
    }
  }
  @keyframes pop {
    from {
      transform: translateY(6px) scale(0.98);
      opacity: 0;
    }
  }
</style>
