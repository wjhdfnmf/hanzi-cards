<script lang="ts">
  import type { Snippet } from "svelte";
  import Icon from "./Icon.svelte";
  import { device } from "../lib/device.svelte";

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

<!-- (on a phone it's a page of its own, over everything, with the way back on the left) -->
<div class="backdrop" class:page={device.phone} role="presentation" onclick={(e) => e.target === e.currentTarget && onclose()}>
  <div class="modal card-surface" role="dialog" aria-modal="true" aria-label={title} style="max-width:{device.phone ? "none" : `${width}px`}">
    <header>
      {#if device.phone}
        <button class="btn ghost icon back" onclick={onclose} aria-label="Close"><Icon name="chevron-left" size={22} /></button>
      {/if}
      <h2>{title}</h2>
      {#if !device.phone}
        <button class="btn ghost icon" onclick={onclose} aria-label="Close"><Icon name="close" /></button>
      {/if}
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
  .page {
    padding: 0;
    place-items: stretch;
    /* (exactly the screen's width, however wide the content would like to be) */
    grid-template-columns: minmax(0, 1fr);
    background: var(--bg);
  }
  .page .modal {
    min-width: 0;
    max-height: none;
    height: 100dvh;
    border: none;
    border-radius: 0;
    box-shadow: none;
    padding: env(safe-area-inset-top) env(safe-area-inset-right) 0 env(safe-area-inset-left);
  }
  .page header {
    justify-content: flex-start;
    gap: 4px;
    padding: 12px 16px 8px 8px;
  }
  .page .back {
    color: var(--accent);
  }
  .page .body {
    flex: 1;
    padding: 8px 16px 18px;
    overflow-x: hidden;
  }
  .page footer {
    padding: 12px 16px calc(env(safe-area-inset-bottom) + 12px);
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
