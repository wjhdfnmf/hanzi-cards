<script lang="ts">
  import { onMount } from "svelte";
  import { t, i18n } from "./lib/i18n.svelte";
  import { nav, goto, type View } from "./lib/nav.svelte";
  import { settings, loadSettings, saveSetting } from "./lib/settings.svelte";
  import { refreshFonts, ensureFont, fontFiles } from "./lib/fonts.svelte";
  import { toast } from "./lib/toast.svelte";
  import { errorText } from "./lib/api";
  import Icon from "./components/Icon.svelte";
  import StudyView from "./views/StudyView.svelte";
  import DecksView from "./views/DecksView.svelte";
  import ReadingView from "./views/ReadingView.svelte";
  import DictionaryView from "./views/DictionaryView.svelte";
  import StatsView from "./views/StatsView.svelte";
  import SettingsView from "./views/SettingsView.svelte";
  import TutorView from "./views/TutorView.svelte";

  let startupError = $state("");
  let tutorOpened = $state(false);
  $effect(() => {
    if (nav.view === "tutor") tutorOpened = true;
  });

  onMount(async () => {
    try {
      await loadSettings();
      await refreshFonts();
    } catch (e) {
      startupError = errorText(e);
    }
  });

  // the Reading font, when it's one the app downloaded
  $effect(() => {
    void fontFiles.list;
    if (settings.loaded) ensureFont(settings.readingFont).catch(() => {});
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

<div class="shell" class:compact>
  <nav class="sidebar">
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

  <main>
    {#if startupError}
      <div class="page"><div class="notice err">{startupError}</div></div>
    {:else if settings.loaded}
      {#key nav.view}
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
      <div class="keep" class:hidden={nav.view !== "study"}><StudyView /></div>
      {#if tutorOpened}
        <div class="keep" class:hidden={nav.view !== "tutor"}><TutorView /></div>
      {/if}
    {/if}
  </main>
</div>

{#if toast.message}
  {#key toast.id}
    <div class="toast {toast.kind}" role="status">{toast.message}</div>
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
  .keep.hidden {
    display: none;
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
