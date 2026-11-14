<script lang="ts">
  import { onMount } from "svelte";
  import { t, i18n } from "./lib/i18n.svelte";
  import { nav, goto, type View } from "./lib/nav.svelte";
  import { settings, loadSettings, saveSetting } from "./lib/settings.svelte";
  import { refreshFonts, ensureFont, fontFiles } from "./lib/fonts.svelte";
  import { toast } from "./lib/toast.svelte";
  import { api, errorText, type StartupProblem } from "./lib/api";
  import Icon from "./components/Icon.svelte";
  import Recovery from "./components/Recovery.svelte";
  import StudyView from "./views/StudyView.svelte";
  import DecksView from "./views/DecksView.svelte";
  import ReadingView from "./views/ReadingView.svelte";
  import DictionaryView from "./views/DictionaryView.svelte";
  import StatsView from "./views/StatsView.svelte";
  import SettingsView from "./views/SettingsView.svelte";
  import TutorView from "./views/TutorView.svelte";
  import Console from "./components/Console.svelte";
  import { sync, refreshStatus, ping, syncNow, summary } from "./lib/sync.svelte";
  import { showToast } from "./lib/toast.svelte";
  import { device, showRail } from "./lib/device.svelte";

  let startupError = $state("");
  /** A database that couldn't be opened (damaged): the recovery page shows instead of the app. */
  let problem = $state<StartupProblem | null>(null);
  let tutorOpened = $state(false);
  $effect(() => {
    if (nav.view === "tutor") tutorOpened = true;
  });

  onMount(async () => {
    try {
      const found = await api.startupProblem();
      if (found) {
        // language and theme, when the settings can still be read (they're with the cards)
        await loadSettings().catch(() => {});
        problem = found;
        return;
      }
      await loadSettings();
      await refreshFonts();
      watchSync();
    } catch (e) {
      startupError = errorText(e);
    }
  });

  // ---------- keeping devices the same: the button above "Collapse", once it is set up ----------
  let watching: ReturnType<typeof setInterval> | undefined;
  async function watchSync() {
    const s = await refreshStatus();
    if (s?.role === "client") ping().catch(() => {});
    clearInterval(watching);
    let n = 0;
    watching = setInterval(async () => {
      if (!sync.status?.role || sync.busy) return;
      const s = await refreshStatus(); // (server: has a device left changes?)
      if (s?.role === "client" && ++n % 3 === 0) ping().catch(() => {});
    }, 20_000);
  }
  async function syncClicked() {
    try {
      showToast(summary(await syncNow()));
    } catch (e) {
      showToast(errorText(e), "err");
    }
  }
  async function lightClicked() {
    try {
      if (sync.status?.role === "server") {
        await api.syncOnline(!sync.status.online);
        await refreshStatus();
      } else {
        const p = await ping();
        showToast(p.ok ? sync.note : p.error, p.ok ? "ok" : "err");
      }
    } catch (e) {
      showToast(errorText(e), "err");
    }
  }

  // the Reading font, when it's one the app downloaded
  $effect(() => {
    void fontFiles.list;
    if (settings.loaded && !problem) ensureFont(settings.readingFont).catch(() => {});
  });

  // ---------- the side bar: full, or icons only ----------
  // Icons only when chosen (the toggle, remembered) or when the window is too narrow for both.
  const COLLAPSED_KEY = "app.sidebarCollapsed";
  let collapsed = $state(
    (() => {
      try {
        return localStorage.getItem(COLLAPSED_KEY) === "true";
      } catch {
        return false;
      }
    })(),
  );
  const narrowQuery = window.matchMedia("(max-width: 900px)");
  let narrow = $state(narrowQuery.matches);
  onMount(() => {
    const on = (e: MediaQueryListEvent) => (narrow = e.matches);
    narrowQuery.addEventListener("change", on);
    return () => narrowQuery.removeEventListener("change", on);
  });
  const compact = $derived(collapsed || narrow);
  function toggleSidebar() {
    collapsed = !collapsed;
    try {
      localStorage.setItem(COLLAPSED_KEY, String(collapsed));
    } catch {
      /* not essential */
    }
  }

  // ---------- on a phone: the side bar comes only when it's asked for ----------
  // Pulling a page down past its top shows it; scrolling down or tapping anything on the page
  // hides it again. (Pages that want the whole screen call hideRail.)
  const lastTop = new WeakMap<EventTarget, number>();
  // For a moment after the bar comes or goes, scrolling doesn't move it: the page reflows to
  // the new width, and a flick may still be gliding.
  let settleUntil = 0;
  $effect(() => {
    void device.railOpen;
    settleUntil = performance.now() + 500;
  });
  function scrolled(e: Event) {
    if (!device.phone || !(e.target instanceof HTMLElement)) return;
    const top = e.target.scrollTop;
    const before = lastTop.get(e.target) ?? 0;
    lastTop.set(e.target, top);
    if (performance.now() < settleUntil) return;
    if (top < -40) showRail(); // (iOS lets a page be pulled past its top)
    else if (top > before + 10 && top > 40) device.railOpen = false;
  }
  /** The scrolling pane a touch is in. */
  function paneOf(el: EventTarget | null): HTMLElement | null {
    for (let n = el instanceof HTMLElement ? el : null; n; n = n.parentElement) {
      const y = getComputedStyle(n).overflowY;
      if ((y === "auto" || y === "scroll") && n.scrollHeight > n.clientHeight) return n;
    }
    return null;
  }
  // A page that doesn't scroll (or is already at its top) can't be pulled past it the usual
  // way, so a downward drag that starts at the top counts too.
  let pullFrom: { x: number; y: number } | null = null;
  function touchStart(e: TouchEvent) {
    const p = e.touches[0];
    const pane = paneOf(e.target);
    pullFrom = device.phone && !device.railOpen && (!pane || pane.scrollTop <= 0) ? { x: p.clientX, y: p.clientY } : null;
  }
  function touchMove(e: TouchEvent) {
    if (!pullFrom) return;
    const p = e.touches[0];
    if (p.clientY - pullFrom.y > 70 && Math.abs(p.clientX - pullFrom.x) < 50) {
      showRail();
      pullFrom = null;
    }
  }
  function tapped() {
    if (device.phone && device.railOpen) device.railOpen = false;
  }

  const items: { view: View; icon: string; key: Parameters<typeof t>[0] }[] = [
    { view: "study", icon: "study", key: "nav.study" },
    { view: "tutor", icon: "chat", key: "nav.tutor" },
    { view: "decks", icon: "decks", key: "nav.decks" },
    { view: "reading", icon: "book", key: "nav.reading" },
    { view: "dictionary", icon: "dictionary", key: "nav.dictionary" },
    { view: "stats", icon: "stats", key: "nav.stats" },
    { view: "settings", icon: "settings", key: "nav.settings" },
  ];
</script>

<svelte:window ontouchstart={touchStart} ontouchmove={touchMove} />

{#if problem}
  <main class="alone"><Recovery kind={problem.kind} reason={problem.reason} message={problem.message} /></main>
{:else}
<div
  class="shell"
  class:compact
  class:phone={device.phone}
  class:railhidden={device.phone && !device.railOpen}
>
  <nav class="sidebar" aria-hidden={device.phone && !device.railOpen ? "true" : undefined}>
    <div class="brand" title={compact ? t("app.name") : undefined}>
      <div class="logo han">文</div>
      {#if !compact}
        <div>
          <div class="name">{t("app.name")}</div>
          <div class="faint small">{i18n.lang === "zh" ? "Hanzi Cards" : "汉字卡"}</div>
        </div>
      {/if}
    </div>

    <div class="links">
      {#each items as it (it.view)}
        <button
          class="navlink"
          class:on={nav.view === it.view}
          title={compact ? t(it.key) : undefined}
          aria-label={compact ? t(it.key) : undefined}
          onclick={() => goto(it.view)}
        >
          <Icon name={it.icon} size={compact ? 22 : 18} />
          {#if !compact}<span>{t(it.key)}</span>{/if}
        </button>
      {/each}
    </div>

    <div class="foot">
      {#if sync.status?.role}
        {@const server = sync.status.role === "server"}
        <div class="syncbox">
          <button
            class="navlink light"
            title={(server ? t(sync.status.online ? "sync.goOffline" : "sync.goOnline") : t("sync.check")) + (sync.note ? `\n${sync.note}` : "")}
            aria-label={server ? t(sync.status.online ? "sync.online" : "sync.offline") : t("sync.server")}
            onclick={lightClicked}
          >
            <span class="lamp" class:ok={sync.light === "ok"} class:bad={sync.light === "bad"}></span>
            {#if !compact}<span>{server ? t(sync.status.online ? "sync.online" : "sync.offline") : t("sync.server")}</span>{/if}
          </button>
          <button
            class="navlink go"
            class:busy={sync.busy}
            disabled={sync.busy}
            title={sync.status.pending ? t("sync.pending") : t("sync.hint")}
            aria-label={t("sync.sync")}
            onclick={syncClicked}
          >
            <span class="turn"><Icon name="sync" size={compact ? 20 : 17} /></span>
            {#if !compact}<span>{sync.busy ? t("sync.syncing") : t("sync.sync")}</span>{/if}
            {#if sync.status.pending}<span class="waiting" aria-hidden="true"></span>{/if}
          </button>
        </div>
      {/if}
      {#if !narrow}
        <button
          class="navlink toggle"
          title={collapsed ? t("nav.expand") : t("nav.collapse")}
          aria-label={collapsed ? t("nav.expand") : t("nav.collapse")}
          aria-expanded={!collapsed}
          onclick={toggleSidebar}
        >
          <Icon name={collapsed ? "chevrons-right" : "chevrons-left"} size={compact ? 20 : 18} />
          {#if !compact}<span>{t("nav.collapse")}</span>{/if}
        </button>
      {/if}
      <!-- one switch: a click anywhere changes the language -->
      <button
        class="lang"
        title={t("nav.switchLanguage")}
        aria-label={t("nav.switchLanguage")}
        onclick={() => saveSetting("language", i18n.lang === "zh" ? "en" : "zh")}
      >
        <span class:on={i18n.lang === "en"}>EN</span>
        <span class:on={i18n.lang === "zh"}>{compact ? "中" : "中文"}</span>
      </button>
    </div>
  </nav>

  <main onscrollcapture={scrolled} onclickcapture={tapped}>
    {#if startupError}
      <div class="page"><div class="notice err">{startupError}</div></div>
    {:else if settings.loaded}
      {#key `${nav.view}:${sync.epoch}`}
        {#if nav.view === "study"}<!-- kept alive below -->
        {:else if nav.view === "tutor"}<!-- kept alive below -->
        {:else if nav.view === "decks"}<DecksView />
        {:else if nav.view === "reading"}<ReadingView />
        {:else if nav.view === "dictionary"}<DictionaryView />
        {:else if nav.view === "stats"}<StatsView />
        {:else}<SettingsView />{/if}
      {/key}
      <!-- Study and the tutor stay alive when you switch pages, so a study session or a
           conversation can be continued until it's ended or the app closes. -->
      <!-- (a sync that brought something in starts the Study page afresh, like the others) -->
      {#key sync.epoch}
        <div class="keep" class:hidden={nav.view !== "study"}><StudyView /></div>
      {/key}
      {#if tutorOpened}
        <div class="keep" class:hidden={nav.view !== "tutor"}><TutorView /></div>
      {/if}
    {/if}
  </main>
</div>
{/if}

{#if sync.consoleOpen}
  <Console
    onclose={() => {
      sync.consoleOpen = false;
      watchSync();
    }}
  />
{/if}

{#if toast.message}
  {#key toast.id}
    <div class="toast {toast.kind}" class:phone={device.phone} role="status">{toast.message}</div>
  {/key}
{/if}

<style>
  .shell {
    display: grid;
    grid-template-columns: 220px 1fr;
    height: 100vh;
    transition: grid-template-columns 0.18s ease;
  }
  .shell.compact {
    grid-template-columns: 76px 1fr;
  }
  .sidebar {
    background: var(--sidebar);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    padding: 18px 12px;
    gap: 22px;
    min-width: 0;
    overflow: hidden;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 6px;
  }
  .compact .brand {
    justify-content: center;
    padding: 0;
  }
  .logo {
    width: 38px;
    height: 38px;
    border-radius: 10px;
    background: var(--accent);
    color: var(--accent-text);
    display: grid;
    place-items: center;
    font-size: 21px;
    flex-shrink: 0;
    box-shadow: var(--shadow);
  }
  .compact .logo {
    width: 44px;
    height: 44px;
    font-size: 24px;
    border-radius: 12px;
  }
  .name {
    font-weight: 700;
    font-size: 15px;
    line-height: 1.2;
    white-space: nowrap;
  }
  .links {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
  }
  .compact .links {
    gap: 6px;
  }
  .navlink {
    display: flex;
    align-items: center;
    gap: 11px;
    padding: 9px 12px;
    border: none;
    background: transparent;
    border-radius: 8px;
    color: var(--text-2);
    font-weight: 550;
    text-align: left;
    white-space: nowrap;
  }
  .compact .navlink {
    justify-content: center;
    padding: 10px 0;
    border-radius: 10px;
  }
  .navlink:hover {
    background: var(--surface-2);
    color: var(--text);
  }
  .navlink.on {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .foot {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .syncbox {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 4px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface);
  }
  .syncbox .navlink {
    padding: 6px 8px;
    font-size: 13px;
    position: relative;
  }
  .compact .syncbox .navlink {
    padding: 8px 0;
  }
  .syncbox .light {
    color: var(--text-3);
    font-weight: 500;
  }
  .lamp {
    width: 9px;
    height: 9px;
    margin: 0 4px;
    border-radius: 50%;
    background: var(--text-3);
    flex-shrink: 0;
  }
  .compact .lamp {
    margin: 0;
  }
  .lamp.ok {
    background: #22b45a;
    box-shadow: 0 0 0 3px rgb(34 180 90 / 0.2);
  }
  .lamp.bad {
    background: #e0453a;
    box-shadow: 0 0 0 3px rgb(224 69 58 / 0.2);
  }
  .turn {
    display: inline-flex;
  }
  .go.busy .turn {
    animation: turn 1s linear infinite;
  }
  .go:disabled {
    cursor: default;
    opacity: 0.75;
  }
  .waiting {
    position: absolute;
    top: 5px;
    right: 6px;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--accent);
  }
  @keyframes turn {
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .go.busy .turn {
      animation: none;
    }
  }
  .toggle {
    color: var(--text-3);
    font-weight: 500;
    font-size: 13px;
  }
  .lang {
    display: flex;
    width: 100%;
    background: var(--surface-2);
    border: none;
    border-radius: 8px;
    padding: 3px;
    font: inherit;
    cursor: pointer;
  }
  .lang:hover {
    background: var(--surface-3);
  }
  .lang span {
    flex: 1;
    min-width: 0;
    padding: 5px 0;
    border-radius: 6px;
    color: var(--text-2);
    font-size: 12.5px;
    font-weight: 600;
    white-space: nowrap;
    text-align: center;
    transition:
      background 0.15s,
      color 0.15s;
  }
  .compact .lang span {
    font-size: 12px;
  }
  .lang span.on {
    background: var(--surface);
    color: var(--text);
    box-shadow: var(--shadow);
  }
  main {
    overflow-y: auto;
    min-width: 0;
  }
  main.alone {
    height: 100vh;
  }
  .keep.hidden {
    display: none;
  }

  /* ---------- a phone: the icon bar alone, sliding away when the page wants the room ---------- */
  /* (everything starts below the status bar, so nothing scrolls up behind the clock) */
  .shell.phone {
    grid-template-columns: calc(60px + env(safe-area-inset-left)) minmax(0, 1fr);
    height: 100dvh;
    padding-top: env(safe-area-inset-top);
    position: relative;
  }
  .shell.phone.railhidden {
    grid-template-columns: 0px minmax(0, 1fr);
  }
  .phone .sidebar {
    padding: 10px 8px calc(env(safe-area-inset-bottom) + 10px) calc(env(safe-area-inset-left) + 8px);
    gap: 16px;
  }
  .phone.railhidden .sidebar {
    padding-left: 0;
    padding-right: 0;
    border-right-width: 0;
  }
  .phone .logo {
    width: 40px;
    height: 40px;
    font-size: 22px;
  }
  .phone .links {
    gap: 4px;
  }
  .phone main {
    padding-bottom: env(safe-area-inset-bottom);
    padding-right: env(safe-area-inset-right);
  }
  .phone.railhidden main {
    padding-left: env(safe-area-inset-left);
  }
  .toast.phone {
    bottom: calc(env(safe-area-inset-bottom) + 16px);
  }
  .toast {
    position: fixed;
    bottom: 22px;
    left: 50%;
    transform: translateX(-50%);
    background: var(--text);
    color: var(--bg);
    padding: 10px 18px;
    border-radius: 10px;
    box-shadow: var(--shadow-lg);
    z-index: 100;
    animation: up 0.18s ease-out;
    max-width: min(560px, 90vw);
  }
  .toast.err {
    background: var(--accent);
    color: var(--accent-text);
  }
  @keyframes up {
    from {
      transform: translate(-50%, 8px);
      opacity: 0;
    }
  }
</style>
