<script lang="ts">
  import { onMount } from "svelte";
  import { t, i18n } from "./lib/i18n.svelte";
  import { nav, goto, type View } from "./lib/nav.svelte";
  import { settings, loadSettings, saveSetting } from "./lib/settings.svelte";
  import { toast } from "./lib/toast.svelte";
  import { errorText } from "./lib/api";
  import Icon from "./components/Icon.svelte";
  import StudyView from "./views/StudyView.svelte";
  import DecksView from "./views/DecksView.svelte";
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
    } catch (e) {
      startupError = errorText(e);
    }
  });

  const items: { view: View; icon: string; key: Parameters<typeof t>[0] }[] = [
    { view: "study", icon: "study", key: "nav.study" },
    { view: "tutor", icon: "chat", key: "nav.tutor" },
    { view: "decks", icon: "decks", key: "nav.decks" },
    { view: "dictionary", icon: "dictionary", key: "nav.dictionary" },
    { view: "stats", icon: "stats", key: "nav.stats" },
    { view: "settings", icon: "settings", key: "nav.settings" },
  ];
</script>

<div class="shell">
  <nav class="sidebar">
    <div class="brand">
      <div class="logo han">文</div>
      <div>
        <div class="name">{t("app.name")}</div>
        <div class="faint small">{i18n.lang === "zh" ? "Hanzi Cards" : "汉字卡"}</div>
      </div>
    </div>

    <div class="links">
      {#each items as it (it.view)}
        <button class="navlink" class:on={nav.view === it.view} onclick={() => goto(it.view)}>
          <Icon name={it.icon} />
          <span>{t(it.key)}</span>
        </button>
      {/each}
    </div>

    <div class="lang" role="group" aria-label="Language">
      <button class:on={i18n.lang === "en"} onclick={() => saveSetting("language", "en")}>EN</button>
      <button class:on={i18n.lang === "zh"} onclick={() => saveSetting("language", "zh")}>中文</button>
    </div>
  </nav>

  <main>
    {#if startupError}
      <div class="page"><div class="notice err">{startupError}</div></div>
    {:else if settings.loaded}
      {#key nav.view}
        {#if nav.view === "study"}<StudyView />
        {:else if nav.view === "tutor"}<!-- kept alive below -->
        {:else if nav.view === "decks"}<DecksView />
        {:else if nav.view === "dictionary"}<DictionaryView />
        {:else if nav.view === "stats"}<StatsView />
        {:else}<SettingsView />{/if}
      {/key}
      <!-- The tutor page stays alive once opened, so a conversation survives switching pages
           until it's ended or the app closes. -->
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
  }
  .sidebar {
    background: var(--surface);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    padding: 18px 12px;
    gap: 22px;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 6px;
  }
  .logo {
    width: 38px;
    height: 38px;
    border-radius: 10px;
    background: var(--accent);
    color: #fff;
    display: grid;
    place-items: center;
    font-size: 21px;
    flex-shrink: 0;
  }
  .name {
    font-weight: 700;
    font-size: 15px;
    line-height: 1.2;
  }
  .links {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
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
  }
  .navlink:hover {
    background: var(--surface-2);
    color: var(--text);
  }
  .navlink.on {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .lang {
    display: flex;
    background: var(--surface-2);
    border-radius: 8px;
    padding: 3px;
  }
  .lang button {
    flex: 1;
    border: none;
    background: transparent;
    padding: 5px;
    border-radius: 6px;
    color: var(--text-2);
    font-size: 12.5px;
    font-weight: 600;
  }
  .lang button.on {
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
    color: #fff;
  }
  @keyframes up {
    from {
      transform: translate(-50%, 8px);
      opacity: 0;
    }
  }
  @media (max-width: 900px) {
    .shell {
      grid-template-columns: 64px 1fr;
    }
    .brand > div:last-child,
    .navlink span {
      display: none;
    }
    .navlink {
      justify-content: center;
    }
    .lang {
      flex-direction: column;
    }
  }
</style>
