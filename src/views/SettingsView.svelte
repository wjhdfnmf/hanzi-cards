<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { ask, open, save } from "@tauri-apps/plugin-dialog";
  import { getVersion } from "@tauri-apps/api/app";
  import { api, errorText, type AiSpending, type DataContents, type DataFile, type DataInfo, type DictStatus, type KeyStatus, type SpeechStatus, type WhisperDeviceInfo, type WhisperModel } from "../lib/api";
  import { recordUtterance } from "../lib/mic";
  import PitchCalibration from "../components/PitchCalibration.svelte";
  import { showToast } from "../lib/toast.svelte";
  import { t } from "../lib/i18n.svelte";
  import { fmtNum } from "../lib/format";
  import { goto } from "../lib/nav.svelte";
  import { settings, saveSetting } from "../lib/settings.svelte";
  import { outputDevices, routingReady, speak, voicesReady } from "../lib/speech";

  let voices = $state<SpeechSynthesisVoice[]>([]);
  let voicesChecked = $state(false);
  let dict = $state<DictStatus | null>(null);
  let appVersion = $state("");

  onMount(async () => {
    appVersion = await getVersion().catch(() => "");
    dict = await api.dictStatus();
    voices = await voicesReady();
    voicesChecked = true;
  });

  // ---------- audio output ----------

  let canChooseOutput = $state(false);
  let outputs = $state<MediaDeviceInfo[]>([]);
  /** The chosen device as the list knows it now (its id may have changed since it was saved). */
  const outputValue = $derived(
    outputs.find((d) => d.deviceId === settings.audioOutput)?.deviceId ??
      outputs.find((d) => settings.audioOutputLabel && d.label === settings.audioOutputLabel)?.deviceId ??
      settings.audioOutput,
  );

  async function refreshOutputs() {
    outputs = await outputDevices();
  }

  onMount(() => {
    routingReady.then((ok) => {
      canChooseOutput = ok && !!navigator.mediaDevices?.enumerateDevices;
      if (canChooseOutput) refreshOutputs();
    });
    // plugging in or removing headphones
    const changed = () => refreshOutputs();
    navigator.mediaDevices?.addEventListener?.("devicechange", changed);
    return () => navigator.mediaDevices?.removeEventListener?.("devicechange", changed);
  });

  async function chooseOutput(id: string) {
    const label = outputs.find((d) => d.deviceId === id)?.label ?? "";
    await saveSetting("audioOutputLabel", id ? label : "");
    await saveSetting("audioOutput", id);
  }

  /** Device names are only shown once the page may use the microphone: ask, then stop. */
  async function showOutputNames() {
    try {
      const s = await navigator.mediaDevices.getUserMedia({ audio: true });
      s.getTracks().forEach((tr) => tr.stop());
    } catch (e) {
      showToast(errorText(e), "err");
    }
    await refreshOutputs();
  }

  // ---------- speech recognition ----------

  let speech = $state<SpeechStatus | null>(null);
  let models = $state<WhisperModel[]>([]);
  /** Download progress (percent) by model name. */
  let progress = $state<Record<string, number>>({});
  let micState = $state<"idle" | "recording" | "thinking">("idle");
  let micText = $state("");

  let device = $state<WhisperDeviceInfo | null>(null);

  async function refreshSpeech() {
    [speech, models] = await Promise.all([api.speechStatus(), api.whisperModels()]);
    device = await api.whisperDeviceInfo().catch(() => null);
  }

  // NVIDIA's libraries for the graphics card (downloaded on request)
  let gpuProgress = $state<number | null>(null);
  onMount(() => {
    let unlisten: (() => void) | undefined;
    listen<{ received: number; total: number }>("gpu-download", (e) => {
      gpuProgress = e.payload.total ? Math.floor((e.payload.received / e.payload.total) * 100) : 0;
    }).then((f) => (unlisten = f));
    return () => unlisten?.();
  });

  async function downloadGpuLibs() {
    gpuProgress = 0;
    try {
      await api.gpuLibsDownload();
      showToast(t("settings.gpuReady"));
    } catch (e) {
      showToast(errorText(e), "err");
    } finally {
      gpuProgress = null;
      await refreshSpeech();
    }
  }

  async function removeGpuLibs() {
    try {
      await api.gpuLibsRemove();
    } catch (e) {
      showToast(errorText(e), "err");
    }
    await refreshSpeech();
  }

  async function setDevice(value: string) {
    await saveSetting("whisperDevice", value);
    device = await api.whisperDeviceInfo().catch(() => null);
  }

  onMount(() => {
    refreshSpeech();
    let unlisten: (() => void) | undefined;
    listen<{ name: string; received: number; total: number }>("whisper-download", (e) => {
      const { name, received, total } = e.payload;
      progress[name] = total ? Math.floor((received / total) * 100) : 0;
    }).then((f) => (unlisten = f));
    return () => unlisten?.();
  });

  const engineLabel = $derived(
    speech?.engine === "windows"
      ? t("settings.engine.windows")
      : speech?.engine === "whisper"
        ? t("settings.whisperModel", { name: speech.whisperModel ?? "" })
        : "",
  );

  async function setEngine(value: string) {
    await saveSetting("speechEngine", value);
    await refreshSpeech();
  }

  async function useModel(name: string) {
    await saveSetting("whisperModel", name);
    await refreshSpeech();
  }

  async function download(name: string) {
    progress[name] = 0;
    try {
      await api.whisperDownload(name);
      if (!speech?.whisperModel) await saveSetting("whisperModel", name);
    } catch (e) {
      showToast(errorText(e), "err");
    } finally {
      delete progress[name];
      await refreshSpeech();
    }
  }

  async function remove(name: string) {
    if (!(await ask(t("settings.delete") + ` Whisper ${name}?`, { kind: "warning" }))) return;
    try {
      await api.whisperDelete(name);
    } catch (e) {
      showToast(errorText(e), "err");
    }
    await refreshSpeech();
  }

  // ---------- HSK tags ----------

  let hskBusy = $state(false);
  async function tagAllHsk() {
    hskBusy = true;
    try {
      showToast(t("settings.hskTagged", { n: await api.hskTagAll() }));
    } catch (e) {
      showToast(errorText(e), "err");
    } finally {
      hskBusy = false;
    }
  }

  // ---------- AI tutor ----------

  let aiKey = $state<KeyStatus | null>(null);
  let spending = $state<AiSpending | null>(null);
  let showSpending = $state(false);
  const usd = (c: number) => `$${c.toFixed(c > 0 && c < 0.1 ? 3 : 2)}`;
  const when = (s: number) => new Date(s * 1000).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
  // ---------- your data (the cards database file) ----------

  let data = $state<DataInfo | null>(null);
  let dataBusy = $state(false);
  let showBackups = $state(false);

  async function refreshData() {
    data = await api.dataInfo().catch((e) => {
      showToast(errorText(e), "err");
      return null;
    });
  }
  onMount(refreshData);

  const mb = (bytes: number) => (bytes / 1048576).toFixed(1);
  const summary = (c: DataContents | null) =>
    c
      ? t(c.decks === 1 ? "settings.dataSummaryOne" : "settings.dataSummary", { cards: fmtNum(c.cards), decks: fmtNum(c.decks), reviews: fmtNum(c.reviews) })
      : t("settings.dataUnreadable");

  async function backupNow() {
    dataBusy = true;
    try {
      showToast(t("settings.dataBackedUp", { name: await api.dataBackup() }));
      await refreshData();
    } catch (e) {
      showToast(errorText(e), "err");
    } finally {
      dataBusy = false;
    }
  }

  /** Save a copy somewhere the user picks, e.g. to load into a fresh install. */
  async function saveCopy() {
    const stamp = new Date().toISOString().slice(0, 10);
    const path = await save({ defaultPath: `hanzi-cards-${stamp}.db`, filters: [{ name: t("settings.dataFiles"), extensions: ["db"] }] });
    if (!path) return;
    dataBusy = true;
    try {
      await api.dataSaveCopy(path);
      showToast(t("settings.dataSavedCopy", { path }));
    } catch (e) {
      showToast(errorText(e), "err");
    } finally {
      dataBusy = false;
    }
  }

  /** Switch to another database file, after showing what it holds. */
  async function useFile(path: string) {
    let file: DataFile;
    try {
      file = await api.dataInspect(path);
    } catch (e) {
      showToast(errorText(e), "err");
      return;
    }
    const message = t("settings.dataUseConfirm", {
      what: summary(file.contents),
      when: when(file.modified),
      current: summary(data?.current.contents ?? null),
    });
    if (!(await ask(message, { kind: "warning", okLabel: t("settings.dataUse") }))) return;
    dataBusy = true;
    try {
      const saved = await api.dataUseFile(path);
      showToast(t("settings.dataSwitched", { name: saved }));
      // every page shows the old database's cards: start over
      setTimeout(() => location.reload(), 1200);
    } catch (e) {
      showToast(errorText(e), "err");
      await refreshData();
    } finally {
      dataBusy = false;
    }
  }

  async function pickFile() {
    const path = await open({ multiple: false, directory: false, defaultPath: data?.folder });
    if (path) await useFile(path as string);
  }

  const modelName = (m: string) => (m.includes("haiku") ? "Haiku 5.5" : m.includes("opus") ? "Opus 5.5" : m);

  async function exportSpending() {
    const path = await save({ defaultPath: "ai-spending.csv", filters: [{ name: "CSV", extensions: ["csv"] }] });
    if (!path) return;
    try {
      await api.aiSpendingExport(path);
      showToast(t("settings.spendingExported"));
    } catch (e) {
      showToast(errorText(e), "err");
    }
  }
  let keyInput = $state("");
  let keyBusy = $state(false);

  onMount(async () => {
    aiKey = await api.aiKeyStatus().catch(() => null);
    spending = await api.aiSpending().catch(() => null);
  });

  async function saveKey() {
    keyBusy = true;
    try {
      await api.aiKeySet(keyInput);
      keyInput = "";
      aiKey = await api.aiKeyStatus();
      showToast(t("settings.aiSaved"));
    } catch (e) {
      showToast(errorText(e), "err");
    } finally {
      keyBusy = false;
    }
  }

  async function removeKey() {
    if (!(await ask(t("settings.aiRemove") + "?", { kind: "warning" }))) return;
    try {
      await api.aiKeyClear();
    } catch (e) {
      showToast(errorText(e), "err");
    }
    aiKey = await api.aiKeyStatus().catch(() => null);
  }

  async function micTest() {
    micState = "recording";
    micText = "";
    try {
      const pcm = await recordUtterance();
      if (!pcm) {
        micText = t("settings.micNothing");
        return;
      }
      micState = "thinking";
      micText = t("settings.micHeard", { text: await api.speechTranscribe(pcm) });
    } catch (e) {
      micText = errorText(e);
    } finally {
      micState = "idle";
    }
  }
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
    <label class="row">
      <span>
        {t("settings.hskAutotag")}
        <small class="faint block">{t("settings.hskAutotagHint")}</small>
      </span>
      <input
        type="checkbox"
        class="toggle"
        checked={settings.hskAutotag}
        onchange={(e) => saveSetting("hskAutotag", e.currentTarget.checked)}
      />
    </label>
    <div class="row">
      <span class="small muted">{t("settings.hskTagAllHint")}</span>
      <button class="btn small" disabled={hskBusy} onclick={tagAllHsk}>{t("settings.hskTagAll")}</button>
    </div>
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
    {#if canChooseOutput}
      <label class="row">
        <span>
          {t("settings.output")}
          <small class="faint block">{t("settings.outputHint")}</small>
        </span>
        <select value={outputValue} onchange={(e) => chooseOutput(e.currentTarget.value)}>
          <option value="">{t("settings.outputDefault")}</option>
          {#each outputs as d, i (d.deviceId)}
            <option value={d.deviceId}>{d.label || t("settings.outputUnnamed", { n: i + 1 })}</option>
          {/each}
          {#if settings.audioOutput && !outputs.some((d) => d.deviceId === settings.audioOutput || d.label === settings.audioOutputLabel)}
            <option value={settings.audioOutput}>{t("settings.outputMissing", { name: settings.audioOutputLabel || "?" })}</option>
          {/if}
        </select>
      </label>
      {#if outputs.length && outputs.every((d) => !d.label)}
        <div class="row">
          <span class="small muted">{t("settings.outputNamesHint")}</span>
          <button class="btn small" onclick={showOutputNames}>{t("settings.outputNames")}</button>
        </div>
      {/if}
    {/if}
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

  <h2>{t("settings.speech")}</h2>
  <section class="group card-surface">
    <label class="row">
      <span>{t("settings.speechEngine")}</span>
      <select value={settings.speechEngine} onchange={(e) => setEngine(e.currentTarget.value)}>
        <option value="auto">{t("settings.engine.auto")}</option>
        <option value="windows" disabled={!speech?.windows}>{t("settings.engine.windows")}</option>
        <option value="whisper" disabled={!speech?.whisperModels.length}>{t("settings.engine.whisper")}</option>
      </select>
    </label>
    <div class="row">
      <span class="small muted">
        {speech?.available ? t("settings.speechUsing", { engine: engineLabel }) : (speech?.reason ?? t("settings.speechNone"))}
      </span>
    </div>
    <label class="row">
      <span>
        {t("settings.whisperDevice")}
        {#if device}
          <small class="faint block">
            {device.using === "gpu"
              ? t("settings.whisperOnGpu", { gpu: device.gpu ?? "GPU" })
              : t("settings.whisperOnCpu", { why: settings.whisperDevice === "cpu" ? t("settings.whisperCpuChosen") : (device.gpuProblem ?? "") })}
          </small>
        {/if}
      </span>
      <select value={settings.whisperDevice} onchange={(e) => setDevice(e.currentTarget.value)}>
        <option value="auto">{t("settings.whisperDevice.auto")}</option>
        <option value="cpu">{t("settings.whisperDevice.cpu")}</option>
      </select>
    </label>
    {#if device?.libs?.nvidia}
      <div class="row">
        <span>
          {t("settings.gpuLibs")}
          <small class="faint block">
            {device.libs.installed
              ? device.libs.downloaded
                ? t("settings.gpuLibsInstalled")
                : t("settings.gpuLibsToolkit")
              : t("settings.gpuLibsHint", { gpu: device.gpu ?? "NVIDIA", mb: device.libs.downloadMb })}
          </small>
        </span>
        <span class="model-actions">
          {#if gpuProgress !== null}
            <progress max="100" value={gpuProgress}></progress>
            <span class="small">{t("settings.downloading", { p: gpuProgress })}</span>
          {:else if !device.libs.installed}
            <button class="btn small" onclick={downloadGpuLibs}>{t("settings.download")}</button>
          {:else if device.libs.downloaded}
            <button class="btn ghost small" onclick={removeGpuLibs}>{t("settings.delete")}</button>
          {/if}
        </span>
      </div>
    {/if}
    <div class="row col">
      <small class="faint">{t("settings.whisperHint")}</small>
      <div class="models">
        {#each models as m (m.name)}
          <div class="model">
            <strong
              >{t("settings.whisperModel", { name: m.name })}{#if device?.recommended === m.name}
                <span class="rec">{t("settings.recommended")}</span>{/if}</strong
            >
            <span class="small faint"
              >{t("settings.whisperSize", { n: m.sizeMb })}{#if m.gpuRecommended}<br />{t("settings.gpuRecommended")}{/if}</span
            >
            <span class="model-actions">
              {#if progress[m.name] !== undefined}
                <progress max="100" value={progress[m.name]}></progress>
                <span class="small">{t("settings.downloading", { p: progress[m.name] })}</span>
              {:else if m.installed}
                {#if speech?.whisperModel === m.name}
                  <span class="small in-use">{t("settings.inUse")}</span>
                {:else}
                  <button class="btn small" onclick={() => useModel(m.name)}>{t("settings.use")}</button>
                {/if}
                <button class="btn ghost small" onclick={() => remove(m.name)}>{t("settings.delete")}</button>
              {:else}
                <button class="btn small" disabled={Object.keys(progress).length > 0} onclick={() => download(m.name)}>
                  {t("settings.download")}
                </button>
              {/if}
            </span>
          </div>
        {/each}
      </div>
    </div>
    <label class="row">
      <span>
        {t("settings.pitchTones")}
        <small class="faint block">{t("settings.pitchTonesHint", { n: 5 })}</small>
      </span>
      <input
        type="checkbox"
        class="toggle"
        checked={settings.pitchTones}
        onchange={(e) => saveSetting("pitchTones", e.currentTarget.checked)}
      />
    </label>
    <div class="row col">
      <PitchCalibration />
    </div>
    <label class="row">
      <span>
        {t("settings.replayAnswers")}
        <small class="faint block">{t("settings.replayAnswersHint")}</small>
      </span>
      <input
        type="checkbox"
        class="toggle"
        checked={settings.replayAnswers}
        onchange={(e) => saveSetting("replayAnswers", e.currentTarget.checked)}
      />
    </label>
    {#if speech?.whisperModels.length}
      <div class="row">
        <button class="btn" disabled={micState !== "idle"} onclick={micTest}>
          {micState === "recording" ? t("settings.micTestHint") : micState === "thinking" ? t("speech.thinking") : t("settings.micTest")}
        </button>
        {#if micText}<span class="mic-text">{micText}</span>{/if}
      </div>
    {/if}
  </section>

  <h2>{t("settings.ai")}</h2>
  <section class="group card-surface">
    <div class="row col">
      <small class="faint">{t("settings.aiHint")}</small>
    </div>
    <div class="row">
      <span>{t("settings.aiKey")}</span>
      <span class="key-state small">
        {#if aiKey?.set}
          <span class="in-use">{t("settings.aiKeySet", { hint: aiKey.hint ?? "" })}</span>
          <button class="btn ghost small" onclick={removeKey}>{t("settings.aiRemove")}</button>
        {:else}
          <span class="muted">{t("settings.aiKeyNone")}</span>
        {/if}
      </span>
    </div>
    <div class="row">
      <input
        class="key-input"
        type="password"
        autocomplete="off"
        spellcheck="false"
        placeholder={t("settings.aiKeyPlaceholder")}
        bind:value={keyInput}
        onkeydown={(e) => e.key === "Enter" && keyInput.trim() && saveKey()}
      />
      <button class="btn" disabled={keyBusy || !keyInput.trim()} onclick={saveKey}>
        {keyBusy ? t("settings.aiChecking") : t("settings.aiSave")}
      </button>
    </div>
    <label class="row">
      <span>{t("settings.aiModel")}</span>
      <select class="wide" value={settings.tutorModel} onchange={(e) => saveSetting("tutorModel", e.currentTarget.value)}>
        <option value="claude-opus-5-5">{t("settings.aiModel.opus")}</option>
        <option value="claude-haiku-5-5">{t("settings.aiModel.haiku")}</option>
      </select>
    </label>
    {#if spending}
      <div class="row col">
        <div class="line">
          <span>{t("settings.spending")}</span>
          <span class="spend-actions">
            {#if spending.recent.length}
              <button class="btn ghost small" onclick={() => (showSpending = !showSpending)}>
                {showSpending ? t("settings.spendingHide") : t("settings.spendingShow")}
              </button>
              <button class="btn ghost small" onclick={exportSpending}>{t("settings.spendingExport")}</button>
            {/if}
          </span>
        </div>
        <div class="spend-tiles">
          <div><strong>{usd(spending.today)}</strong><small class="muted">{t("settings.spending.today")}</small></div>
          <div><strong>{usd(spending.last7Days)}</strong><small class="muted">{t("settings.spending.week")}</small></div>
          <div><strong>{usd(spending.thisMonth)}</strong><small class="muted">{t("settings.spending.month")}</small></div>
          <div><strong>{usd(spending.total)}</strong><small class="muted">{t("settings.spending.total", { n: spending.conversations })}</small></div>
        </div>
        {#if showSpending}
          <table class="spend-table">
            <thead>
              <tr>
                <th>{t("settings.spending.when")}</th>
                <th>{t("settings.spending.model")}</th>
                <th class="num-c">{t("settings.spending.calls")}</th>
                <th class="num-c">{t("settings.spending.tokens")}</th>
                <th class="num-c">{t("settings.spending.cost")}</th>
              </tr>
            </thead>
            <tbody>
              {#each spending.recent as c (`${c.sessionId}:${c.startedAt}`)}
                <tr>
                  <td>{when(c.startedAt)}</td>
                  <td>{modelName(c.model)}</td>
                  <td class="num-c">{c.calls}</td>
                  <td class="num-c">{fmtNum(c.inputTokens)} / {fmtNum(c.outputTokens)}</td>
                  <td class="num-c">{usd(c.cost)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
        <small class="faint">{t("settings.spendingHint")}</small>
      </div>
    {/if}
  </section>

  <h2>{t("settings.data")}</h2>
  <section class="group card-surface">
    <div class="row">
      <span class="data-current">
        {t("settings.dataCurrent")}
        <small class="faint block">
          {data ? `${summary(data.current.contents)} · ${t("settings.dataSize", { mb: mb(data.current.sizeBytes) })}` : "…"}
        </small>
        {#if data}<code class="data-path">{data.current.path}</code>{/if}
      </span>
      <button class="btn ghost small" onclick={() => api.dataShowInFolder().catch((e) => showToast(errorText(e), "err"))}>
        {t("settings.dataShow")}
      </button>
    </div>
    <div class="row">
      <span>
        {t("settings.dataBackup")}
        <small class="faint block">{t("settings.dataBackupHint")}</small>
      </span>
      <span class="key-state">
        <button class="btn ghost" disabled={dataBusy} onclick={saveCopy}>{t("settings.dataSaveCopy")}</button>
        <button class="btn" disabled={dataBusy} onclick={backupNow}>{t("settings.dataBackupNow")}</button>
      </span>
    </div>
    <div class="row">
      <span>
        {t("settings.dataOther")}
        <small class="faint block">{t("settings.dataOtherHint")}</small>
      </span>
      <button class="btn" disabled={dataBusy} onclick={pickFile}>{t("settings.dataChoose")}</button>
    </div>
    {#if data?.backups.length}
      <div class="row col">
        <div class="line">
          <span>{t("settings.dataBackups", { n: data.backups.length })}</span>
          <button class="btn ghost small" onclick={() => (showBackups = !showBackups)}>
            {showBackups ? t("settings.spendingHide") : t("settings.spendingShow")}
          </button>
        </div>
        {#if showBackups}
          <table class="spend-table">
            <thead>
              <tr>
                <th>{t("settings.dataSaved")}</th>
                <th>{t("settings.dataHolds")}</th>
                <th class="num-c">{t("settings.dataSizeCol")}</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {#each data.backups as b (b.path)}
                <tr>
                  <td title={b.name}>{when(b.modified)}</td>
                  <td>{summary(b.contents)}</td>
                  <td class="num-c">{mb(b.sizeBytes)} MB</td>
                  <td class="num-c">
                    {#if b.contents}
                      <button class="btn ghost small" disabled={dataBusy} onclick={() => useFile(b.path)}>{t("settings.dataRestore")}</button>
                    {/if}
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
      </div>
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
    <p class="muted">{t("settings.aboutText", { v: appVersion })}</p>
    <label class="row">
      <span>
        {t("settings.debugMode")}
        <small class="faint block">{import.meta.env.DEV ? t("settings.debugModeDev") : t("settings.debugModeHint")}</small>
      </span>
      <input
        type="checkbox"
        class="toggle"
        checked={settings.debugMode || import.meta.env.DEV}
        disabled={import.meta.env.DEV}
        onchange={(e) => saveSetting("debugMode", e.currentTarget.checked)}
      />
    </label>
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
  .models {
    display: flex;
    flex-direction: column;
    gap: 6px;
    width: 100%;
  }
  .model {
    display: grid;
    grid-template-columns: 175px 125px 1fr;
    align-items: center;
    gap: 10px;
  }
  .model-actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 6px;
  }
  .model progress {
    width: 140px;
  }
  .rec {
    margin-left: 4px;
    padding: 1px 6px;
    border-radius: 6px;
    font-size: 11px;
    font-weight: 600;
    background: var(--accent-soft);
    color: var(--accent);
  }
  .in-use {
    color: var(--ok);
    font-weight: 600;
  }
  .btn.small {
    height: 28px;
    padding: 0 10px;
    font-size: 12.5px;
  }
  .row select.wide {
    width: 320px;
  }
  .spend-actions {
    display: flex;
    gap: 6px;
  }
  .spend-tiles {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 8px;
  }
  .spend-tiles > div {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 8px 10px;
    border-radius: 8px;
    background: var(--surface-2);
  }
  .spend-tiles strong {
    font-size: 17px;
    font-variant-numeric: tabular-nums;
  }
  .spend-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12.5px;
  }
  .spend-table th,
  .spend-table td {
    text-align: left;
    padding: 4px 6px;
    border-bottom: 1px solid var(--border);
  }
  .spend-table .num-c {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .block {
    display: block;
    margin-top: 2px;
  }
  .key-state {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .key-input {
    flex: 1;
  }
  .data-current {
    min-width: 0;
  }
  .data-path {
    display: block;
    margin-top: 4px;
    font-size: 12px;
    overflow-wrap: anywhere;
    user-select: text;
  }
  .mic-text {
    font-size: 16px;
  }
</style>
