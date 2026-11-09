<script lang="ts">
  import { onMount } from "svelte";
  import { api, type DictStatus } from "../lib/api";
  import { t } from "../lib/i18n.svelte";
  import { fmtNum } from "../lib/format";
  import { goto } from "../lib/nav.svelte";
  import { settings, saveSetting } from "../lib/settings.svelte";
  import { speak, voicesReady } from "../lib/speech";

  let voices = $state<SpeechSynthesisVoice[]>([]);
  let voicesChecked = $state(false);
  let dict = $state<DictStatus | null>(null);

  onMount(async () => {
    dict = await api.dictStatus();
    voices = await voicesReady();
    voicesChecked = true;
  });
</script>

<div class="page narrow">
  <div class="page-head"><h1>{t("settings.title")}</h1></div>

  <section class="group card-surface">
    <label class="row">
      <span>{t("settings.language")}</span>
      <select value={settings.language} onchange={(e) => saveSetting("language", e.currentTarget.value)}>
        <option value="en">English</option>
        <option value="zh">简体中文</option>
      </select>
    </label>
    <label class="row">
      <span>{t("settings.theme")}</span>
      <select value={settings.theme} onchange={(e) => saveSetting("theme", e.currentTarget.value)}>
        <option value="system">{t("settings.theme.system")}</option>
        <option value="light">{t("settings.theme.light")}</option>
        <option value="dark">{t("settings.theme.dark")}</option>
      </select>
    </label>
    <label class="row">
      <span>{t("settings.toneColors")}</span>
      <input
        type="checkbox"
        class="toggle"
        checked={settings.toneColors}
        onchange={(e) => saveSetting("toneColors", e.currentTarget.checked)}
      />
    </label>
  </section>

  <h2>{t("settings.study")}</h2>
  <section class="group card-surface">
    <div class="row col">
      <div class="line">
        <span>{t("settings.retention")}</span>
        <strong>{Math.round(settings.desiredRetention * 100)}%</strong>
      </div>
      <input
        type="range"
        min="0.8"
        max="0.97"
        step="0.01"
        value={settings.desiredRetention}
        onchange={(e) => saveSetting("desiredRetention", Number(e.currentTarget.value))}
        oninput={(e) => (settings.desiredRetention = Number(e.currentTarget.value))}
      />
      <small class="faint">{t("settings.retentionHint")}</small>
    </div>
    <label class="row">
      <span>{t("settings.newPerDay")}</span>
      <input
        type="number"
        min="0"
        max="500"
        class="num"
        value={settings.newPerDay}
        onchange={(e) => saveSetting("newPerDay", Math.max(0, Math.round(Number(e.currentTarget.value) || 0)))}
      />
    </label>
  </section>

  <h2>{t("settings.audio")}</h2>
  <section class="group card-surface">
    <label class="row">
      <span>{t("settings.autoplay")}</span>
      <input
        type="checkbox"
        class="toggle"
        checked={settings.autoplayAudio}
        onchange={(e) => saveSetting("autoplayAudio", e.currentTarget.checked)}
      />
    </label>
    <label class="row">
      <span>{t("settings.voice")}</span>
      <select value={settings.voice} onchange={(e) => saveSetting("voice", e.currentTarget.value)} disabled={!voices.length}>
        {#each voices as v (v.name)}<option value={v.name}>{v.name}</option>{/each}
      </select>
    </label>
    <div class="row">
      <span>{t("settings.rate")}</span>
      <div class="line">
        <input
          type="range"
          min="0.5"
          max="1.3"
          step="0.05"
          value={settings.speechRate}
          onchange={(e) => saveSetting("speechRate", Number(e.currentTarget.value))}
        />
        <button class="btn" onclick={() => speak("你好，欢迎学习中文。")}>{t("settings.test")}</button>
      </div>
    </div>
    {#if voicesChecked && !voices.length}
      <div class="notice small">{t("settings.noVoice")}</div>
    {/if}
  </section>

  <h2>{t("settings.dictionary")}</h2>
  <section class="group card-surface">
    <div class="row">
      <span>{dict?.entries ? t("dict.entries", { n: fmtNum(dict.entries) }) : t("dict.notInstalled")}</span>
      <button class="btn" onclick={() => goto("dictionary")}>{t("nav.dictionary")}</button>
    </div>
  </section>

  <h2>{t("settings.about")}</h2>
  <section class="group card-surface">
    <p class="muted">{t("settings.aboutText", { v: "0.1.0" })}</p>
  </section>
</div>

<style>
  .narrow {
    max-width: 720px;
  }
  h2 {
    margin: 26px 0 10px;
  }
  .group {
    border-radius: var(--radius);
    padding: 4px 18px;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 18px;
    padding: 12px 0;
    border-bottom: 1px solid var(--border);
  }
  .row:last-child {
    border-bottom: none;
  }
  .row.col {
    flex-direction: column;
    align-items: stretch;
    gap: 8px;
  }
  .row select {
    width: 220px;
  }
  .line {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  .num {
    width: 100px;
  }
  input[type="range"] {
    accent-color: var(--accent);
    width: 100%;
  }
  .line input[type="range"] {
    width: 180px;
  }
  .toggle {
    width: 18px;
    height: 18px;
    accent-color: var(--accent);
  }
</style>
