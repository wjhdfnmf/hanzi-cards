<script lang="ts">
  import { onMount, tick } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { ask, open, save } from "@tauri-apps/plugin-dialog";
  import { getVersion } from "@tauri-apps/api/app";
  import { api, errorText, type AiSpending, type DataContents, type DataFile, type DataInfo, type DbKind, type DictStatus, type KeyStatus, type SpeechStatus, type WhisperDeviceInfo, type WhisperModel } from "../lib/api";
  import { recordUtterance } from "../lib/mic";
  import PitchCalibration from "../components/PitchCalibration.svelte";
  import { showToast } from "../lib/toast.svelte";
  import { t } from "../lib/i18n.svelte";
  import { fmtNum } from "../lib/format";
  import { goto, nav } from "../lib/nav.svelte";
  import { settings, saveSetting } from "../lib/settings.svelte";
  import { sync, refreshStatus, ping, syncNow, summary as syncSummary } from "../lib/sync.svelte";
  import { device as layout, ios, trackLevel } from "../lib/device.svelte";
  import Back from "../components/Back.svelte";
  import Icon from "../components/Icon.svelte";
  import { SIZE_MIN, SIZE_MAX, LINE_MIN, LINE_MAX, readingFont, clampSize } from "../lib/reading";
  import FontSelect from "../components/FontSelect.svelte";
  import { CUSTOM_THEMES, THEME_GROUPS, isCustomTheme } from "../lib/themes";
  import Modal from "../components/Modal.svelte";
  import licenceText from "../../LICENSE?raw";
  import { fontFiles, downloadFont, removeFont } from "../lib/fonts.svelte";
  import { installedFonts } from "../lib/reading";

  // Downloadable fonts: a picture of each (shipped with the app) to see before downloading.
  const fontSamples = import.meta.glob<string>("../assets/font-samples/*.png", {
    eager: true,
    query: "?url",
    import: "default",
  });
  const sampleOf = (key: string) => fontSamples[`../assets/font-samples/${key}.png`];
  const systemFonts = installedFonts();

  // The font list folds away (closed at first); whether it's open is remembered.
  const FONTS_OPEN_KEY = "settings.fontsOpen";
  let fontsOpen = $state(
    (() => {
      try {
        return localStorage.getItem(FONTS_OPEN_KEY) === "true";
      } catch {
        return false;
      }
    })(),
  );
  function setFontsOpen(open: boolean) {
    fontsOpen = open;
    try {
      localStorage.setItem(FONTS_OPEN_KEY, String(open));
    } catch {
      /* not essential */
    }
  }

  async function getFont(key: string) {
    try {
      await downloadFont(key);
      await saveSetting("readingFont", key);
    } catch (e) {
      showToast(errorText(e), "err");
    }
  }

  async function dropFont(key: string) {
    try {
      if (settings.readingFont === key) await saveSetting("readingFont", "sans");
      await removeFont(key);
    } catch (e) {
      showToast(errorText(e), "err");
    }
  }
  import { BUILT_IN, outputDevices, routingReady, sampleBuiltIn, speak, voicesReady } from "../lib/speech";
  import { voice, checkVoices } from "../lib/voice.svelte";

  let voices = $state<SpeechSynthesisVoice[]>([]);
  let voicesChecked = $state(false);
  let dict = $state<DictStatus | null>(null);
  let appVersion = $state("");

  onMount(async () => {
    appVersion = await getVersion().catch(() => "");
    dict = await api.dictStatus().catch(() => null);
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
    try {
      [speech, models] = await Promise.all([api.speechStatus(), api.whisperModels()]);
    } catch (e) {
      showToast(errorText(e), "err");
    }
    device = await api.whisperDeviceInfo().catch(() => null);
  }

  // The built-in voice (Kokoro): a download, the backup where the computer has no Chinese voice
  let builtInMb = $state(147);
  let voiceProgress = $state<number | null>(null);
  onMount(() => {
    api.kokoroStatus().then((s) => (builtInMb = s.downloadMb)).catch(() => {});
    let unlisten: (() => void) | undefined;
    listen<{ received: number; total: number }>("voice-download", (e) => {
      voiceProgress = e.payload.total ? Math.floor((e.payload.received / e.payload.total) * 100) : 0;
    }).then((f) => (unlisten = f));
    return () => unlisten?.();
  });
  async function downloadVoice() {
    voiceProgress = 0;
    try {
      await api.kokoroDownload();
      await checkVoices();
      sampleBuiltIn("你好，欢迎学习中文。");
    } catch (e) {
      showToast(errorText(e), "err");
    } finally {
      voiceProgress = null;
    }
  }
  async function removeVoice() {
    try {
      await api.kokoroRemove();
      if (settings.voice === BUILT_IN) await saveSetting("voice", "");
      await checkVoices();
    } catch (e) {
      showToast(errorText(e), "err");
    }
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

  /** Why the marked model is the one suggested for this computer. */
  const recWhy = $derived.by(() => {
    if (!device) return "";
    const gb = (mb: number | null) => (mb === null ? "?" : String(Math.round(mb / 1024)));
    const key = `settings.recWhy.${device.recommendedReason}` as Parameters<typeof t>[0];
    return t(key, { name: device.recommended, cores: device.cores, ram: gb(device.ramMb), vram: gb(device.gpuMemoryMb) });
  });

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
        : speech?.engine === "apple"
          ? t("settings.engine.apple")
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
  // ---------- your data (two database files: cards & decks, and reading) ----------

  const DB_KINDS: DbKind[] = ["cards", "reading"];
  let data = $state<Record<DbKind, DataInfo | null>>({ cards: null, reading: null });
  let dataBusy = $state(false);
  /** Which backup lists are open: "cards", "cards-auto", "reading", "reading-auto". */
  let showBackups = $state<Record<string, boolean>>({});

  async function refreshData() {
    const [cards, reading] = await Promise.all(
      DB_KINDS.map((k) =>
        api.dataInfo(k).catch((e) => {
          showToast(errorText(e), "err");
          return null;
        }),
      ),
    );
    data = { cards, reading };
  }
  onMount(refreshData);

  const mb = (bytes: number) => (bytes / 1048576).toFixed(1);
  const summary = (c: DataContents | null, kind: DbKind) =>
    !c
      ? t(kind === "cards" ? "settings.dataUnreadable" : "settings.dataUnreadableReading")
      : kind === "reading"
        ? t("settings.dataSummaryReading", { stories: fmtNum(c.stories), collections: fmtNum(c.collections) })
        : t(c.decks === 1 ? "settings.dataSummaryOne" : "settings.dataSummary", { cards: fmtNum(c.cards), decks: fmtNum(c.decks), reviews: fmtNum(c.reviews) });

  // ---------- LINK (an iPhone): pairing with the computer that keeps the box ----------
  let linkAddress = $state("");
  let linkCode = $state("");
  let linkBusy = $state(false);
  let linkError = $state("");
  async function linkJoin() {
    linkBusy = true;
    linkError = "";
    try {
      await api.syncJoin(linkAddress.trim(), linkCode.replace(/\s+/g, ""));
      await refreshStatus();
      await ping().catch(() => {});
      linkCode = "";
      const r = await syncNow();
      showToast(t("link.paired", { cards: fmtNum(r.cards) }));
    } catch (e) {
      linkError = errorText(e);
    } finally {
      linkBusy = false;
    }
  }
  async function linkSync() {
    linkBusy = true;
    linkError = "";
    try {
      showToast(syncSummary(await syncNow()));
    } catch (e) {
      linkError = errorText(e);
    } finally {
      linkBusy = false;
    }
  }
  async function linkCheck() {
    const p = await ping().catch(() => null);
    linkError = p && !p.ok ? p.error : "";
  }
  async function linkAutomatic(on: boolean) {
    sync.status = await api.syncSetAutomatic(on);
  }
  async function linkForget() {
    if (!(await ask(t("link.forgetConfirm"), { title: t("link.forget"), kind: "warning" }))) return;
    await api.syncForget();
    linkError = "";
    await refreshStatus();
  }
  $effect(() => {
    if (section === "link")
      refreshStatus().then((s) => {
        if (s?.role === "client") linkCheck();
      });
  });

  // ---------- starting every card over ----------
  async function resetProgress() {
    if (!(await ask(t("settings.resetProgressConfirm"), { title: t("settings.resetProgress"), kind: "warning" }))) return;
    resetting = true;
    try {
      const n = await api.resetProgress();
      showToast(t("settings.resetProgressDone", { n }));
      sync.epoch++;
      await refreshData().catch(() => {});
    } catch (e) {
      showToast(errorText(e), "err");
    } finally {
      resetting = false;
    }
  }

  // ---------- starting the statistics over ----------
  let resetting = $state(false);
  async function resetStatistics() {
    if (!(await ask(t("settings.resetStatsConfirm"), { title: t("settings.resetStats"), kind: "warning" }))) return;
    resetting = true;
    try {
      const n = await api.resetStatistics();
      showToast(t("settings.resetStatsDone", { n }));
      sync.epoch++; // (the Study page's counts start afresh too)
      await refreshData().catch(() => {});
    } catch (e) {
      showToast(errorText(e), "err");
    } finally {
      resetting = false;
    }
  }

  async function backupNow(kind: DbKind) {
    dataBusy = true;
    try {
      showToast(t("settings.dataBackedUp", { name: await api.dataBackup(kind) }));
      await refreshData();
    } catch (e) {
      showToast(errorText(e), "err");
    } finally {
      dataBusy = false;
    }
  }

  /** Save a copy somewhere the user picks, e.g. to load into a fresh install. */
  async function saveCopy(kind: DbKind) {
    const stamp = new Date().toISOString().slice(0, 10);
    const name = kind === "cards" ? `hanzi-cards-${stamp}.db` : `hanzi-reading-${stamp}.db`;
    const path = await save({ defaultPath: name, filters: [{ name: t("settings.dataFiles"), extensions: ["db"] }] });
    if (!path) return;
    dataBusy = true;
    try {
      await api.dataSaveCopy(kind, path);
      showToast(t("settings.dataSavedCopy", { path }));
    } catch (e) {
      showToast(errorText(e), "err");
    } finally {
      dataBusy = false;
    }
  }

  /** Delete one backup file, after saying which. */
  async function removeBackup(kind: DbKind, b: DataFile) {
    const message = t("settings.dataDeleteConfirm", { name: b.name, what: summary(b.contents, kind), when: when(b.modified) });
    if (!(await ask(message, { kind: "warning", okLabel: t("settings.dataDelete") }))) return;
    dataBusy = true;
    try {
      await api.dataDeleteBackup(kind, b.path);
      showToast(t("settings.dataDeleted"));
    } catch (e) {
      showToast(errorText(e), "err");
    } finally {
      dataBusy = false;
      await refreshData();
    }
  }

  /** Switch to another database file, after showing what it holds. */
  async function useFile(kind: DbKind, path: string) {
    let file: DataFile;
    try {
      file = await api.dataInspect(kind, path);
    } catch (e) {
      showToast(errorText(e), "err");
      return;
    }
    const message = t("settings.dataUseConfirm", {
      what: summary(file.contents, kind),
      when: when(file.modified),
      current: summary(data[kind]?.current.contents ?? null, kind),
    });
    if (!(await ask(message, { kind: "warning", okLabel: t("settings.dataUse") }))) return;
    dataBusy = true;
    try {
      const saved = await api.dataUseFile(kind, path);
      showToast(t("settings.dataSwitched", { name: saved }));
      // every page shows the old database's contents: start over
      setTimeout(() => location.reload(), 1200);
    } catch (e) {
      showToast(errorText(e), "err");
      await refreshData();
    } finally {
      dataBusy = false;
    }
  }

  async function pickFile(kind: DbKind) {
    const path = await open({ multiple: false, directory: false, defaultPath: data[kind]?.backupFolder ?? data[kind]?.folder });
    if (path) await useFile(kind, path as string);
  }

  // ---------- themes ----------
  const LAST_CUSTOM_KEY = "settings.lastCustomTheme";
  const themeMode = $derived(isCustomTheme(settings.theme) ? "custom" : settings.theme);
  function chooseThemeMode(mode: string) {
    if (mode !== "custom") return saveSetting("theme", mode);
    // Custom: the theme chosen last time, else the first
    let last = "";
    try {
      last = localStorage.getItem(LAST_CUSTOM_KEY) ?? "";
    } catch {
      /* the first one */
    }
    return saveSetting("theme", isCustomTheme(last) ? last : CUSTOM_THEMES[0].key);
  }
  function chooseCustom(key: string) {
    try {
      localStorage.setItem(LAST_CUSTOM_KEY, key);
    } catch {
      /* not essential */
    }
    return saveSetting("theme", key);
  }

  // ---------- studying: advanced ----------
  const ADV_KEY = "settings.studyAdvanced";
  let advOpen = $state(
    (() => {
      try {
        return localStorage.getItem(ADV_KEY) === "true";
      } catch {
        return false;
      }
    })(),
  );
  function setAdvOpen(v: boolean) {
    advOpen = v;
    try {
      localStorage.setItem(ADV_KEY, String(v));
    } catch {
      /* not essential */
    }
  }
  /** The research-based setup (successive relearning, FSRS-6 with fuzz). */
  const STUDY_DEFAULTS = {
    newRecalls: 3,
    relearnRecalls: 1,
    againMinutes: 1,
    learnGaps: "5 15",
    relearnMinutes: 10,
    minGapCards: 5,
    fuzz: true,
    maxIntervalDays: 36500,
    leechLapses: 8,
    newOrder: "hsk",
  } as const;
  const isDefaultStudy = $derived(
    (Object.keys(STUDY_DEFAULTS) as (keyof typeof STUDY_DEFAULTS)[]).every((k) => settings[k] === STUDY_DEFAULTS[k]),
  );
  async function restoreStudyDefaults() {
    for (const k of Object.keys(STUDY_DEFAULTS) as (keyof typeof STUDY_DEFAULTS)[]) {
      if (settings[k] !== STUDY_DEFAULTS[k]) await saveSetting(k, STUDY_DEFAULTS[k] as never);
    }
    showToast(t("settings.adv.restored"));
  }
  /** A whole number in [lo, hi] from a number box (the default for nonsense). */
  const int = (v: string, lo: number, hi: number, fallback: number) => {
    const n = Math.round(Number(v));
    return Number.isFinite(n) ? Math.min(hi, Math.max(lo, n)) : fallback;
  };
  /** "5, 15" → "5 15": minutes between 0.5 and 1440, at most 6 of them. */
  function cleanGaps(v: string): string {
    const list = v
      .split(/[\s,;]+/)
      .map(Number)
      .filter((m) => Number.isFinite(m) && m > 0)
      .map((m) => Math.min(1440, Math.max(0.5, m)))
      .slice(0, 6);
    return list.length ? list.join(" ") : STUDY_DEFAULTS.learnGaps;
  }

  // ---------- dictionary ----------
  let dictBusy = $state(false);
  /** Download the latest CC-CEDICT, or import a file of it. */
  async function installDict(fromFile: boolean) {
    let path: string | null = null;
    if (fromFile) {
      const picked = await open({ multiple: false, filters: [{ name: "CC-CEDICT", extensions: ["u8", "txt", "gz"] }] });
      if (!picked) return;
      path = picked as string;
    }
    dictBusy = true;
    try {
      const n = path ? await api.dictImportFile(path) : await api.dictDownload();
      dict = await api.dictStatus();
      showToast(t("dict.installed", { n: fmtNum(n) }));
    } catch (e) {
      showToast(errorText(e), "err");
    } finally {
      dictBusy = false;
    }
  }

  // ---------- about ----------
  let showLicence = $state(false);
  /** What the app is built on and the data it uses, with each one's licence. */
  const CREDITS: { name: string; what: Parameters<typeof t>[0]; licence: string }[] = [
    { name: "CC-CEDICT © MDBG", what: "settings.about.cedict", licence: "CC BY-SA 4.0" },
    { name: "FSRS-6 · open-spaced-repetition", what: "settings.about.fsrs", licence: "MIT" },
    { name: "complete-hsk-vocabulary © Yanis Zafirópulos", what: "settings.about.hsk", licence: "MIT" },
    { name: "Unicode Unihan (kHanyuPinlu)", what: "settings.about.unihan", licence: "Unicode License" },
    // (the computers' speech checking: an iPhone uses Apple's own)
    ...(ios
      ? []
      : [
          { name: "Tone Perfect · Michigan State University", what: "settings.about.toneperfect" as const, licence: "—" },
          { name: "Whisper · OpenAI", what: "settings.about.whisper" as const, licence: "MIT" },
        ]),
    { name: "LXGW WenKai, Noto Serif SC, ZCOOL…", what: "settings.about.fonts", licence: "SIL OFL 1.1" },
    { name: "Tauri · Svelte · SQLite", what: "settings.about.stack", licence: "MIT / Apache-2.0 · public domain" },
  ];

  // ---------- sections (the side bar) ----------
  type Section = "general" | "reading" | "study" | "audio" | "speech" | "ai" | "data" | "link" | "dictionary" | "about";
  const SECTIONS: { key: Section; label: Parameters<typeof t>[0] }[] = [
    { key: "general", label: "settings.general" },
    { key: "reading", label: "settings.reading" },
    { key: "study", label: "settings.study" },
    { key: "audio", label: "settings.audio" },
    { key: "speech", label: "settings.speech" },
    { key: "ai", label: "settings.ai" },
    { key: "data", label: "settings.data" },
    // (an iPhone: keeping in step with a computer, as a page of its own; computers keep the console)
    ...(ios ? [{ key: "link" as Section, label: "settings.link" as Parameters<typeof t>[0] }] : []),
    { key: "dictionary", label: "settings.dictionary" },
    { key: "about", label: "settings.about" },
  ];
  const SECTION_KEY = "settings.section";
  // (no AI tutor on an iPhone, for now)
  if (ios) SECTIONS.splice(SECTIONS.findIndex((x) => x.key === "ai"), 1);
  const isSection = (k: unknown): k is Section => SECTIONS.some((x) => x.key === k);
  // On a phone the list of sections and a section are two screens: a section opened from
  // another page shows straight away, otherwise the list comes first.
  let sectionOpen = $state(isSection(nav.settingsSection));
  trackLevel(() => (sectionOpen ? "detail" : "list"));
  let section = $state<Section>(
    (() => {
      if (isSection(nav.settingsSection)) return nav.settingsSection; // opened from another page
      try {
        const saved = localStorage.getItem(SECTION_KEY);
        if (isSection(saved)) return saved;
      } catch {
        /* the first section */
      }
      return "general";
    })(),
  );
  nav.settingsSection = null;
  let mainEl = $state<HTMLElement | null>(null);
  function chooseSection(k: Section) {
    section = k;
    sectionOpen = true;
    // (after it's shown: on a phone the pane was hidden, and kept the last section's place)
    tick().then(() => mainEl?.scrollTo({ top: 0 }));
    try {
      localStorage.setItem(SECTION_KEY, k);
    } catch {
      /* not essential */
    }
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

<div class="settings-layout" class:phone={layout.phone} class:open={sectionOpen}>
  <nav class="secnav" aria-label={t("settings.title")}>
    <h2 class="secnav-title">{t("settings.title")}</h2>
    {#each SECTIONS as x (x.key)}
      <button class="sec" class:on={section === x.key} aria-current={section === x.key ? "page" : undefined} onclick={() => chooseSection(x.key)}>
        {t(x.label)}
      </button>
    {/each}
    <!-- (nothing to see here) -->
    {#if !ios}<button class="sec door" tabindex="-1" aria-hidden="true" onclick={() => (sync.consoleOpen = true)}>&nbsp;</button>{/if}
  </nav>
  <div class="secmain" bind:this={mainEl}>
  <!-- a phone: the way back stays at the top while the section scrolls under it -->
  {#if layout.phone}<div class="backbar"><Back label={t("settings.title")} onclick={() => (sectionOpen = false)} /></div>{/if}
  <div class="page narrow">
  <div class="page-head">
    <h1>{t(SECTIONS.find((x) => x.key === section)?.label ?? "settings.title")}</h1>
  </div>

  {#if section === "general"}

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
      <select value={themeMode} onchange={(e) => chooseThemeMode(e.currentTarget.value)}>
        <option value="system">{t("settings.theme.system")}</option>
        <option value="light">{t("settings.theme.light")}</option>
        <option value="dark">{t("settings.theme.dark")}</option>
        <option value="custom">{t("settings.theme.custom")}</option>
      </select>
    </label>
    {#if themeMode === "custom"}
      <div class="row col">
        <div class="themegroups" role="radiogroup" aria-label={t("settings.theme.custom")}>
          {#each THEME_GROUPS as g (g)}
          <div class="themegroup" role="group" aria-label={t(`settings.theme.group.${g}`)}>
          <div class="tg-name">{t(`settings.theme.group.${g}`)}</div>
          <div class="swatches">
          {#each CUSTOM_THEMES.filter((x) => x.group === g) as th (th.key)}
            <button
              class="swatch"
              class:on={settings.theme === th.key}
              role="radio"
              aria-checked={settings.theme === th.key}
              title={`${th.name} · ${th.zh}`}
              onclick={() => chooseCustom(th.key)}
            >
              <span class="sw-square" style="background:{th.bg}; --sw-accent:{th.accent}; --sw-soft:{th.accentSoft}; --sw-surface:{th.surface}">
                {#if th.emblem === "flower"}
                  <svg class="sw-flowers" viewBox="0 0 64 64" aria-hidden="true">
                    {#each [{ x: 22, y: 24, r: 9, a: 10 }, { x: 44, y: 40, r: 11, a: 40 }, { x: 18, y: 48, r: 6, a: 70 }] as f, i (i)}
                      <g transform="translate({f.x} {f.y}) rotate({f.a})">
                        {#each [0, 72, 144, 216, 288] as rot (rot)}
                          <ellipse cx="0" cy={-f.r * 0.95} rx={f.r * 0.55} ry={f.r * 0.95} transform="rotate({rot})" fill="#f8b9cc" />
                        {/each}
                        <circle r={f.r * 0.3} fill="#e2749a" />
                      </g>
                    {/each}
                  </svg>
                {:else}
                  <span class="sw-card"><span class="sw-glyph han" lang="zh-CN">{th.emblem}</span></span>
                {/if}
                <span class="sw-bar"></span>
              </span>
              <span class="sw-name">{th.name}</span>
              <span class="sw-zh han faint" lang="zh-CN">{th.zh}</span>
            </button>
          {/each}
          </div>
          </div>
          {/each}
        </div>
      </div>
    {/if}
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

  {/if}

  {#if section === "reading"}
  <section class="group card-surface">
    <label class="row">
      <span>{t("settings.readingFont")}</span>
      <FontSelect />
    </label>
    {#if !ios}<p class="row small faint fonthint">{t("settings.readingFontHint")}</p>{/if}
    {#if fontFiles.list.length}
      {@const nDownloaded = fontFiles.list.filter((f) => f.downloaded).length}
      {@const nSystem = fontFiles.list.filter((f) => !f.downloaded && systemFonts.has(f.key)).length}
      {@const nBusy = Object.keys(fontFiles.progress).length}
      <div class="row col">
        <button
          class="fonttoggle"
          aria-expanded={fontsOpen}
          aria-controls="font-downloads"
          onclick={() => setFontsOpen(!fontsOpen)}
        >
          <span class="twisty" class:open={fontsOpen} aria-hidden="true"><Icon name="chevron-right" size={14} /></span>
          <span class="fonttitle">{t("settings.readingDownloads")}</span>
          <small class="faint">
            {t("settings.readingDownloadsSummary", { n: fontFiles.list.length, d: nDownloaded, s: nSystem })}{#if nBusy}
              · {t("settings.readingDownloading")}{/if}
          </small>
        </button>
        {#if fontsOpen}
        {#if !ios}<small class="faint">{t("settings.readingDownloadsHint")}</small>{/if}
        <div class="fontlist" id="font-downloads">
          {#each fontFiles.list as f (f.key)}
            {@const busy = fontFiles.progress[f.key] !== undefined}
            {@const system = systemFonts.has(f.key) && !f.downloaded}
            <div class="fontrow" class:chosen={settings.readingFont === f.key && (f.downloaded || system)}>
              <div class="fontname">
                <span>{t(`reading.font.${f.key}` as Parameters<typeof t>[0])}</span>
                <small class="faint">{(f.sizeBytes / 1e6).toFixed(1)} MB · SIL OFL</small>
              </div>
              <img class="fontsample" src={sampleOf(f.key)} alt="春眠不觉晓，处处闻啼鸟。" />
              <!-- two fixed slots: status (or Remove) on the left, the button on the right, so
                   Use / Download line up in every row -->
              <div class="fontact">
                <span class="fa-status">
                  {#if busy}
                    <span class="small muted">{fontFiles.progress[f.key]}%</span>
                  {:else if system}
                    <span class="small faint">{t("settings.readingFontSystem")}</span>
                  {:else if f.downloaded}
                    <button class="btn ghost small" onclick={() => dropFont(f.key)}>{t("settings.readingFontRemove")}</button>
                  {/if}
                </span>
                <span class="fa-main">
                  {#if busy}
                    <!-- downloading -->
                  {:else if (system || f.downloaded) && settings.readingFont === f.key}
                    <span class="small inuse">{t("settings.readingFontInUse")}</span>
                  {:else if system || f.downloaded}
                    <button class="btn small" onclick={() => saveSetting("readingFont", f.key)}>{t("settings.readingFontUse")}</button>
                  {:else}
                    <button class="btn small" onclick={() => getFont(f.key)}>{t("settings.readingFontDownload")}</button>
                  {/if}
                </span>
              </div>
            </div>
          {/each}
        </div>
        {/if}
      </div>
    {/if}
    <div class="row col">
      <div class="line">
        <span>{t("settings.readingSize")}</span>
        <strong>{settings.readingSize}px</strong>
      </div>
      <input
        type="range"
        min={SIZE_MIN}
        max={SIZE_MAX}
        step="1"
        value={settings.readingSize}
        oninput={(e) => (settings.readingSize = Number(e.currentTarget.value))}
        onchange={(e) => saveSetting("readingSize", clampSize(Number(e.currentTarget.value)))}
      />
    </div>
    <div class="row col">
      <div class="line">
        <span>{t("settings.readingLineHeight")}</span>
        <strong>{settings.readingLineHeight.toFixed(1)}</strong>
      </div>
      <input
        type="range"
        min={LINE_MIN}
        max={LINE_MAX}
        step="0.1"
        value={settings.readingLineHeight}
        oninput={(e) => (settings.readingLineHeight = Number(e.currentTarget.value))}
        onchange={(e) => saveSetting("readingLineHeight", Number(e.currentTarget.value))}
      />
    </div>
    <div class="row col">
      <p
        class="reading-preview"
        lang="zh-CN"
        style="font-family:{readingFont(settings.readingFont)}; font-size:{settings.readingSize}px; line-height:{settings.readingLineHeight}"
      >
        {t("settings.readingPreview")}
      </p>
    </div>
    <label class="row">
      <span>
        {t("settings.readingToneColors")}
        <small class="faint block">{t("settings.readingToneColorsHint")}</small>
      </span>
      <input
        type="checkbox"
        class="toggle"
        checked={settings.readingToneColors}
        onchange={(e) => saveSetting("readingToneColors", e.currentTarget.checked)}
      />
    </label>
    <label class="row">
      <span>
        {t("settings.readingStyle")}
        <small class="faint block">{t("settings.readingStyleHint")}</small>
      </span>
      <select value={settings.readingStyle} onchange={(e) => saveSetting("readingStyle", e.currentTarget.value)}>
        <option value="plain">{t("settings.readingStyle.plain")}</option>
        <option value="books">{t("settings.readingStyle.books")}</option>
        <option value="cloth">{t("settings.readingStyle.cloth")}</option>
        <option value="thread">{t("settings.readingStyle.thread")}</option>
        <option value="poster">{t("settings.readingStyle.poster")}</option>
      </select>
    </label>
  </section>

  {/if}

  {#if section === "study"}
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
        {t("settings.answerButtons")}
        <small class="faint block">{t("settings.answerButtonsHint")}</small>
      </span>
      <select value={settings.answerButtons} onchange={(e) => saveSetting("answerButtons", e.currentTarget.value)}>
        <option value="2">{t("settings.answerButtons2")}</option>
        <option value="4">{t("settings.answerButtons4")}</option>
      </select>
    </label>
    <label class="row">
      <span>
        {t("settings.oneDirection")}
        <small class="faint block">{t("settings.oneDirectionHint")}</small>
      </span>
      <input
        type="checkbox"
        class="toggle"
        checked={settings.oneDirectionPerSession}
        onchange={(e) => saveSetting("oneDirectionPerSession", e.currentTarget.checked)}
      />
    </label>
  </section>


  <h2>
    <button class="fonttoggle advtoggle" aria-expanded={advOpen} onclick={() => setAdvOpen(!advOpen)}>
      <span class="twisty" class:open={advOpen} aria-hidden="true"><Icon name="chevron-right" size={14} /></span>
      <span class="fonttitle">{t("settings.adv.title")}</span>
      <small class="faint">{isDefaultStudy ? t("settings.adv.usingDefaults") : t("settings.adv.customised")}</small>
    </button>
  </h2>
  {#if advOpen}
    <!-- (a phone: the settings, without the essay on how scheduling works) -->
    {#if !layout.phone}
    <section class="group card-surface how">
      <h3>{t("settings.adv.howTitle")}</h3>
      <ul>
        <li>{t("settings.adv.how1")}</li>
        <li>{t("settings.adv.how2")}</li>
        <li>{t("settings.adv.how3")}</li>
        <li>{t("settings.adv.how4")}</li>
        <li>{t("settings.adv.how5")}</li>
        <li>{t("settings.adv.how6")}</li>
      </ul>
    </section>
    {/if}

    <section class="group card-surface">
      <label class="row">
        <span>
          {t("settings.adv.newRecalls")}
          <small class="faint block">{t("settings.adv.newRecallsHint")}</small>
        </span>
        <input type="number" min="1" max="6" class="num" value={settings.newRecalls}
          onchange={(e) => saveSetting("newRecalls", int(e.currentTarget.value, 1, 6, 3))} />
      </label>
      <label class="row">
        <span>
          {t("settings.adv.newOrder")}
          <small class="faint block">{t("settings.adv.newOrderHint")}</small>
        </span>
        <select value={settings.newOrder} onchange={(e) => saveSetting("newOrder", e.currentTarget.value)}>
          <option value="hsk">{t("settings.adv.newOrder.hsk")}</option>
          <option value="deck">{t("settings.adv.newOrder.deck")}</option>
          <option value="random">{t("settings.adv.newOrder.random")}</option>
        </select>
      </label>
      <label class="row">
        <span>
          {t("settings.adv.gaps")}
          <small class="faint block">{t("settings.adv.gapsHint")}</small>
        </span>
        <input type="text" class="num gaps" value={settings.learnGaps}
          onchange={(e) => { const v = cleanGaps(e.currentTarget.value); e.currentTarget.value = v; saveSetting("learnGaps", v); }} />
      </label>
      <label class="row">
        <span>
          {t("settings.adv.again")}
          <small class="faint block">{t("settings.adv.againHint")}</small>
        </span>
        <input type="number" min="1" max="1440" class="num" value={settings.againMinutes}
          onchange={(e) => saveSetting("againMinutes", int(e.currentTarget.value, 1, 1440, 1))} />
      </label>
      <label class="row">
        <span>
          {t("settings.adv.relearnRecalls")}
          <small class="faint block">{t("settings.adv.relearnRecallsHint")}</small>
        </span>
        <input type="number" min="1" max="4" class="num" value={settings.relearnRecalls}
          onchange={(e) => saveSetting("relearnRecalls", int(e.currentTarget.value, 1, 4, 1))} />
      </label>
      <label class="row">
        <span>
          {t("settings.adv.relearn")}
          <small class="faint block">{t("settings.adv.relearnHint")}</small>
        </span>
        <input type="number" min="1" max="1440" class="num" value={settings.relearnMinutes}
          onchange={(e) => saveSetting("relearnMinutes", int(e.currentTarget.value, 1, 1440, 10))} />
      </label>
      <label class="row">
        <span>
          {t("settings.adv.minGap")}
          <small class="faint block">{t("settings.adv.minGapHint")}</small>
        </span>
        <input type="number" min="1" max="50" class="num" value={settings.minGapCards}
          onchange={(e) => saveSetting("minGapCards", int(e.currentTarget.value, 1, 50, 5))} />
      </label>
      <label class="row">
        <span>
          {t("settings.adv.fuzz")}
          <small class="faint block">{t("settings.adv.fuzzHint")}</small>
        </span>
        <input type="checkbox" class="toggle" checked={settings.fuzz} onchange={(e) => saveSetting("fuzz", e.currentTarget.checked)} />
      </label>
      <label class="row">
        <span>
          {t("settings.adv.maxDays")}
          <small class="faint block">{t("settings.adv.maxDaysHint")}</small>
        </span>
        <input type="number" min="1" max="36500" class="num" value={settings.maxIntervalDays}
          onchange={(e) => saveSetting("maxIntervalDays", int(e.currentTarget.value, 1, 36500, 36500))} />
      </label>
      <label class="row">
        <span>
          {t("settings.adv.leech")}
          <small class="faint block">{t("settings.adv.leechHint")}</small>
        </span>
        <input type="number" min="0" max="99" class="num" value={settings.leechLapses}
          onchange={(e) => saveSetting("leechLapses", int(e.currentTarget.value, 0, 99, 8))} />
      </label>
      <div class="row">
        <span class="small muted">{t("settings.adv.restoreHint")}</span>
        <button class="btn small" disabled={isDefaultStudy} onclick={restoreStudyDefaults}>{t("settings.adv.restore")}</button>
      </div>
    </section>
  {/if}
  {/if}

  {#if section === "audio"}
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
    <!-- (an iPhone: its own Chinese voice, its own volume buttons, its own speaker) -->
    {#if !ios}
    <label class="row">
      <span>{t("settings.voice")}</span>
      <select value={settings.voice} onchange={(e) => saveSetting("voice", e.currentTarget.value)} disabled={!voices.length && !voice.builtIn}>
        {#each voices as v (v.name)}<option value={v.name}>{v.name}</option>{/each}
        {#if voice.builtIn}<option value={BUILT_IN}>{t("settings.builtIn")}</option>{/if}
      </select>
    </label>
    {/if}
    <!-- (an iPhone uses its own Chinese voices: no built-in voice there) -->
    {#if !ios}
    <div class="row">
      <span>
        {t("settings.builtIn")}
        <small class="faint block">{t(voice.system ? "settings.builtIn.extra" : "settings.builtIn.need")}</small>
      </span>
      {#if voice.builtIn}
        <span class="rowbtns">
          <span class="badge review">{t("settings.builtIn.installed")}</span>
          <button class="btn small" onclick={() => sampleBuiltIn("你好，欢迎学习中文。")}>{t("settings.builtIn.sample")}</button>
          <button class="btn ghost small" onclick={removeVoice}>{t("settings.builtIn.remove")}</button>
        </span>
      {:else if voiceProgress !== null}
        <span class="small muted">{t("settings.builtIn.downloading", { p: voiceProgress })}</span>
      {:else}
        <button class="btn small" class:primary={!voice.system} onclick={downloadVoice}>{t("settings.builtIn.download", { n: builtInMb })}</button>
      {/if}
    </div>
    {/if}
    {#if canChooseOutput && !ios}
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
    {#if !ios}
    <div class="row">
      <span>
        {t("settings.volume")}
        <small class="faint block">{t("settings.volumeHint")}</small>
      </span>
      <div class="line">
        <input
          type="range"
          min="0"
          max="1"
          step="0.05"
          value={settings.speechVolume}
          aria-label={t("settings.volume")}
          oninput={(e) => (settings.speechVolume = Number(e.currentTarget.value))}
          onchange={(e) => {
            saveSetting("speechVolume", Number(e.currentTarget.value));
            speak("你好");
          }}
        />
        <strong class="vol">{Math.round(settings.speechVolume * 100)}%</strong>
      </div>
    </div>
    {/if}
    {#if voicesChecked && !voices.length}
      <div class="notice small">{t("settings.noVoice")}</div>
    {/if}
  </section>

  {/if}

  {#if section === "speech"}
  <section class="group card-surface">
    <!-- (an iPhone has Apple's recognizer only: nothing to choose or download) -->
    {#if !ios}
    <label class="row">
      <span>{t("settings.speechEngine")}</span>
      <select value={settings.speechEngine} onchange={(e) => setEngine(e.currentTarget.value)}>
        <option value="auto">{t("settings.engine.auto")}</option>
        <option value="windows" disabled={!speech?.windows}>{t("settings.engine.windows")}</option>
        <option value="whisper" disabled={!speech?.whisperModels.length}>{t("settings.engine.whisper")}</option>
      </select>
    </label>
    {/if}
    <div class="row">
      <span class="small muted">
        {speech?.available ? t("settings.speechUsing", { engine: engineLabel }) : (speech?.reason ?? t("settings.speechNone"))}
      </span>
    </div>
    {#if !ios}
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
      {#if device && recWhy}
        <small class="faint">{recWhy}</small>
      {/if}
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
    <!-- (pitch helps Whisper choose between tones; Apple's recognizer can't be steered that way) -->
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
    {/if}
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
    {#if speech?.whisperModels.length || speech?.engine === "apple"}
      <div class="row mic-row">
        <button class="btn" disabled={micState !== "idle"} onclick={micTest}>
          {micState === "recording" ? t("settings.micTestHint") : micState === "thinking" ? t("speech.thinking") : t("settings.micTest")}
        </button>
        {#if micText}<span class="mic-text">{micText}</span>{/if}
      </div>
    {/if}
  </section>

  {/if}

  {#if section === "ai"}
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

  {/if}

  {#if section === "data"}
  {#snippet dbBlock(kind: DbKind)}
    {@const d = data[kind]}
    <h2>{t(kind === "cards" ? "settings.dataCards" : "settings.dataReading")}</h2>
    <section class="group card-surface">
      <div class="row">
        <span class="data-current">
          {t(kind === "cards" ? "settings.dataCurrent" : "settings.dataCurrentReading")}
          <small class="faint block">
            {d ? `${summary(d.current.contents, kind)} · ${t("settings.dataSize", { mb: mb(d.current.sizeBytes) })}` : "…"}
          </small>
          {#if d && !ios}<code class="data-path">{d.current.path}</code>{/if}
        </span>
      </div>
      <div class="row">
        <span>
          {t("settings.dataBackup")}
          <small class="faint block">{t(kind === "cards" ? "settings.dataBackupHint" : "settings.dataBackupHintReading")}</small>
        </span>
        <span class="key-state">
          {#if !ios}<button class="btn ghost" disabled={dataBusy} onclick={() => saveCopy(kind)}>{t("settings.dataSaveCopy")}</button>{/if}
          <button class="btn" disabled={dataBusy} onclick={() => backupNow(kind)}>{t("settings.dataBackupNow")}</button>
        </span>
      </div>
      <!-- (an iPhone's app has its own data, nowhere else to keep it: no other files or folders) -->
      {#if !ios}
      <div class="row">
        <span>
          {t("settings.dataOther")}
          <small class="faint block">{t("settings.dataOtherHint")}</small>
        </span>
        <button class="btn" disabled={dataBusy} onclick={() => pickFile(kind)}>{t("settings.dataChoose")}</button>
      </div>
      {/if}
      <!-- the learner's own backups, then the app's own: two folders, two lists -->
      {#each [{ auto: false, list: d?.backups ?? [], folder: d?.backupFolder ?? "" }, { auto: true, list: d?.autoBackups ?? [], folder: d?.autoFolder ?? "" }] as group (group.auto)}
        {@const key = group.auto ? `${kind}-auto` : kind}
        {#if d && (group.list.length || group.auto)}
          <div class="row col">
            <div class="line">
              <span>
                {t(group.auto ? "settings.dataAutoBackups" : "settings.dataBackups", { n: group.list.length })}
                <small class="faint block">
                  {group.auto ? t("settings.dataAutoHint", { ...d.keep, mb: mb(group.list.reduce((s, b) => s + b.sizeBytes, 0)) }) : t("settings.dataOwnHint")}
                </small>
                {#if !ios}<small class="faint block"><code class="data-path">{group.folder}</code></small>{/if}
              </span>
              {#if group.list.length}
                <button class="btn ghost small" onclick={() => (showBackups[key] = !showBackups[key])}>
                  {showBackups[key] ? t("settings.dataHideBackups") : t("settings.dataShowBackups")}
                </button>
              {/if}
            </div>
            {#if showBackups[key] && group.list.length}
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
                  {#each group.list as b (b.path)}
                    <tr>
                      <td title={b.name}>
                        {when(b.modified)}
                        {#if b.auto}<small class="faint block">{t(`settings.dataSort.${b.auto}`)}</small>{/if}
                      </td>
                      <td>{summary(b.contents, kind)}</td>
                      <td class="num-c">{mb(b.sizeBytes)} MB</td>
                      <td class="num-c">
                        {#if b.contents}
                          <button class="btn ghost small" disabled={dataBusy} onclick={() => useFile(kind, b.path)}
                            >{t("settings.dataRestore")}</button
                          >
                        {/if}
                        <button class="btn ghost small" disabled={dataBusy} onclick={() => removeBackup(kind, b)}
                          >{t("settings.dataDelete")}</button
                        >
                      </td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            {/if}
          </div>
        {/if}
      {/each}
      {#if kind === "cards"}
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
      {/if}
    </section>
  {/snippet}
  {#if !ios}
  <section class="group card-surface">
    <div class="row">
      <span class="data-current">
        {t("settings.dataFolder")}
        <small class="faint block">{t("settings.dataFolderHint")}</small>
        <code class="data-path">{data.cards?.folder ?? data.reading?.folder ?? "…"}</code>
      </span>
      <button class="btn" onclick={() => api.dataShowInFolder().catch((e) => showToast(errorText(e), "err"))}>
        {t("settings.dataShow")}
      </button>
    </div>
  </section>
  {/if}
  {@render dbBlock("cards")}
  {@render dbBlock("reading")}
  <h2>{t("settings.statsData")}</h2>
  <section class="group card-surface">
    <div class="row">
      <span>
        {t("settings.resetStats")}
        <small class="faint block">{t("settings.resetStatsHint")}</small>
      </span>
      <button class="btn danger" disabled={resetting} onclick={resetStatistics}>{t("settings.resetStatsButton")}</button>
    </div>
    <div class="row">
      <span>
        {t("settings.resetProgress")}
        <small class="faint block">{t("settings.resetProgressHint")}</small>
      </span>
      <button class="btn danger" disabled={resetting} onclick={resetProgress}>{t("settings.resetStatsButton")}</button>
    </div>
  </section>
  {/if}

  {#if section === "link"}
  {@const s = sync.status}
  {#if s?.role === "client"}
    <section class="group card-surface">
      <div class="row">
        <span>{t("link.computer")}</span>
        <span class="link-state">
          <span class="lamp" class:ok={sync.light === "ok"} class:bad={sync.light === "bad"}></span>
          <code>{s.address}</code>
        </span>
      </div>
      <div class="row">
        <span>{t("link.last")}</span>
        <span class="muted">{s.lastSync ? new Date(s.lastSync * 1000).toLocaleString() : t("link.never")}</span>
      </div>
      <div class="row">
        <span>{t("link.automatic")}<small class="faint block">{t("link.automaticHint")}</small></span>
        <input type="checkbox" class="toggle" checked={s.automatic} onchange={(e) => linkAutomatic(e.currentTarget.checked)} />
      </div>
      <div class="row">
        <button class="btn primary" disabled={linkBusy || sync.busy} onclick={linkSync}>
          {sync.busy ? t("sync.syncing") : t("link.syncNow")}
        </button>
        <button class="btn ghost" disabled={linkBusy} onclick={linkCheck}>{t("link.check")}</button>
      </div>
      {#if linkError}<div class="notice err small">{linkError}</div>{/if}
    </section>
    <section class="group card-surface">
      <div class="row">
        <span>{t("link.forget")}</span>
        <button class="btn danger" onclick={linkForget}>{t("link.forgetButton")}</button>
      </div>
    </section>
  {:else}
    <section class="group card-surface">
      <div class="row col">
        <p class="link-intro">{t("link.intro")}</p>
      </div>
      <label class="row col">
        <span>{t("link.address")}</span>
        <input type="text" bind:value={linkAddress} placeholder="192.168.1.20" autocapitalize="off" autocorrect="off" spellcheck="false" inputmode="url" />
      </label>
      <label class="row col">
        <span>{t("link.code")}</span>
        <input type="text" bind:value={linkCode} placeholder="AB12C-…" autocapitalize="characters" autocorrect="off" spellcheck="false" />
      </label>
      <div class="row">
        <button class="btn primary" disabled={linkBusy || !linkAddress.trim() || !linkCode.trim()} onclick={linkJoin}>
          {linkBusy ? t("link.pairing") : t("link.pair")}
        </button>
      </div>
      {#if linkError}<div class="notice err small">{linkError}</div>{/if}
    </section>
  {/if}
  {/if}

  {#if section === "dictionary"}
  <h2>CC-CEDICT</h2>
  <section class="group card-surface">
    <div class="row">
      <span class="data-current">
        {dict?.entries ? t("dict.entries", { n: fmtNum(dict.entries) }) : t("dict.notInstalled")}
        <small class="faint block">
          {dict?.importedAt ? t("settings.dictUpdated", { when: when(dict.importedAt) }) : t("settings.dictAbout")}
        </small>
      </span>
      <button class="btn" onclick={() => goto("dictionary")}>{t("settings.dictOpen")}</button>
    </div>
    <div class="row">
      <span>
        {t("settings.dictUpdate")}
        <small class="faint block">{t("settings.dictUpdateHint")}</small>
      </span>
      <button class="btn" disabled={dictBusy} onclick={() => installDict(false)}>
        {dictBusy ? t("settings.dictWorking") : dict?.entries ? t("settings.dictUpdateBtn") : t("dict.download")}
      </button>
    </div>
    <div class="row">
      <span>
        {t("settings.dictImport")}
        <small class="faint block">{t("settings.dictImportHint")}</small>
      </span>
      <button class="btn ghost" disabled={dictBusy} onclick={() => installDict(true)}>{t("dict.importFile")}</button>
    </div>
    <p class="small faint dict-licence">{t("dict.attribution")}</p>
  </section>

  {/if}

  {#if section === "about"}
  <section class="about-hero card-surface">
    <div class="about-logo han" aria-hidden="true">文</div>
    <div class="about-id">
      <div class="about-name">{t("app.name")} <span class="han about-zh">汉字卡</span></div>
      <div class="faint small">{t("settings.about.version", { v: appVersion || "…" })}</div>
      <p class="about-tag">{t("settings.about.tagline")}</p>
      <div class="about-chips">
        <span class="chip">{t("settings.about.chipOpen")}</span>
        <span class="chip">{t("settings.about.chipLocal")}</span>
        <span class="chip">{t("settings.about.chipPlatforms")}</span>
      </div>
    </div>
    <div class="about-motto han" lang="zh-CN" title={t("settings.about.mottoMeaning")}>
      学而时习之
      <small>{t("settings.about.mottoMeaning")}</small>
    </div>
  </section>

  <h2>{t("settings.about.licence")}</h2>
  <section class="group card-surface">
    <div class="row">
      <span>
        {t("settings.about.licenceMit")}
        <small class="faint block">{t("settings.about.licenceHint")}</small>
      </span>
      <button class="btn" onclick={() => (showLicence = true)}>{t("settings.about.readLicence")}</button>
    </div>
  </section>

  <h2>{t("settings.about.credits")}</h2>
  <section class="group card-surface">
    <ul class="credits">
      {#each CREDITS as c (c.name)}
        <li>
          <span class="credit-name">{c.name}</span>
          <span class="faint small">{t(c.what)}</span>
          <span class="credit-lic small">{c.licence}</span>
        </li>
      {/each}
    </ul>
  </section>

  <!-- (debug mode keeps recordings for tuning Whisper and the pitch model: neither is on an iPhone) -->
  {#if !ios}
  <h2>{t("settings.about.troubleshooting")}</h2>
  <section class="group card-surface">
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
  {/if}
  <p class="small faint about-foot">{t("settings.about.thanks")}</p>
  {/if}
  </div>
  </div>
</div>

{#if showLicence}
  <Modal title={t("settings.about.licence")} onclose={() => (showLicence = false)} width={620}>
    <pre class="licence">{licenceText}</pre>
  </Modal>
{/if}

<style>
  .about-hero {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    align-items: center;
    gap: 20px;
    padding: 22px 24px;
    border-radius: var(--radius-lg);
    background:
      radial-gradient(circle at 100% 0%, var(--accent-soft), transparent 60%),
      var(--surface);
  }
  .about-logo {
    width: 64px;
    height: 64px;
    border-radius: 16px;
    background: var(--accent);
    color: var(--accent-text);
    display: grid;
    place-items: center;
    font-size: 36px;
    box-shadow: var(--shadow);
  }
  .about-name {
    font-size: 22px;
    font-weight: 700;
    line-height: 1.2;
  }
  .about-zh {
    font-weight: 500;
    color: var(--text-2);
    margin-left: 4px;
  }
  .about-tag {
    margin: 8px 0 10px;
    color: var(--text-2);
  }
  .about-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .about-motto {
    font-family: var(--font-han-serif);
    font-size: 26px;
    writing-mode: vertical-rl;
    letter-spacing: 0.15em;
    color: var(--accent);
    display: flex;
    align-items: center;
    gap: 8px;
    padding-left: 8px;
    border-left: 1px solid var(--border);
  }
  .about-motto small {
    writing-mode: horizontal-tb;
    font-family: var(--font);
    font-size: 11px;
    letter-spacing: normal;
    color: var(--text-3);
    display: none;
  }
  .themegroups {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 2px 0 6px;
  }
  .tg-name {
    font-size: 12px;
    font-weight: 650;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--text-3);
    padding: 0 4px 2px;
  }
  .swatches {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(92px, 1fr));
    gap: 12px;
  }
  .swatch {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
    padding: 6px 4px 8px;
    border: none;
    background: transparent;
    border-radius: var(--radius);
    font: inherit;
    color: inherit;
    cursor: pointer;
  }
  .swatch:hover {
    background: var(--surface-2);
  }
  .sw-square {
    position: relative;
    width: 64px;
    height: 64px;
    border-radius: 14px;
    overflow: hidden;
    display: grid;
    place-items: center;
    box-shadow: inset 0 0 0 1px rgb(0 0 0 / 0.08), var(--shadow);
    margin-bottom: 4px;
  }
  .swatch.on .sw-square {
    outline: 3px solid var(--accent);
    outline-offset: 3px;
  }
  .sw-card {
    width: 38px;
    height: 38px;
    border-radius: 9px;
    background: var(--sw-surface);
    display: grid;
    place-items: center;
    box-shadow: 0 1px 3px rgb(0 0 0 / 0.18);
  }
  .sw-glyph {
    font-size: 22px;
    line-height: 1;
    color: var(--sw-accent);
  }
  .sw-flowers {
    width: 64px;
    height: 64px;
  }
  .sw-bar {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 7px;
    background: var(--sw-accent);
  }
  .sw-name {
    font-size: 12.5px;
    font-weight: 600;
  }
  .sw-zh {
    font-size: 12px;
  }
  .advtoggle {
    font-size: inherit;
    font-weight: inherit;
  }
  .how h3 {
    margin: 14px 0 6px;
    font-size: 14px;
  }
  .how ul {
    margin: 0 0 14px;
    padding-left: 20px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    color: var(--text-2);
    line-height: 1.5;
  }
  .row input.gaps {
    width: 110px;
  }
  .dict-licence {
    margin: 0;
    padding: 10px 0 12px;
  }
  /* one set of columns for every row (subgrid), so each column starts at the same place */
  .credits {
    list-style: none;
    margin: 0;
    padding: 6px 0;
    display: grid;
    grid-template-columns: minmax(0, 1.25fr) minmax(0, 1.5fr) max-content;
    column-gap: 20px;
  }
  .credits li {
    display: grid;
    grid-column: 1 / -1;
    grid-template-columns: subgrid;
    align-items: baseline;
    padding: 9px 0;
    border-bottom: 1px solid var(--border);
  }
  .credits li:last-child {
    border-bottom: none;
  }
  .credit-name {
    font-weight: 550;
  }
  .credit-lic {
    color: var(--text-2);
    white-space: nowrap;
    text-align: right;
  }
  .licence {
    margin: 0;
    white-space: pre-wrap;
    font-size: 12.5px;
    line-height: 1.5;
    max-height: 60vh;
    overflow-y: auto;
  }
  .about-foot {
    text-align: center;
    margin: 22px 0 8px;
  }
  .settings-layout {
    display: grid;
    grid-template-columns: 200px minmax(0, 1fr);
    height: 100%;
  }
  .secnav {
    border-right: 1px solid var(--border);
    padding: 22px 12px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .secnav-title {
    margin: 0;
    padding: 0 10px 10px;
  }
  .sec {
    border: none;
    background: transparent;
    padding: 8px 10px;
    border-radius: 8px;
    text-align: left;
    color: inherit;
    font: inherit;
  }
  .sec:hover {
    background: var(--surface-2);
  }
  /* the last place in the list, the size of the others, with nothing to show */
  .door {
    margin-top: auto;
    opacity: 0;
    cursor: default;
  }
  .sec.on {
    background: var(--surface-3);
    font-weight: 600;
  }
  .vol {
    min-width: 44px;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .secmain {
    overflow-y: auto;
    min-width: 0;
  }
  /* a phone: the list, or the section chosen from it */
  .settings-layout.phone {
    grid-template-columns: minmax(0, 1fr);
  }
  .phone .secnav {
    border-right: none;
    padding: 18px 14px;
  }
  .phone .sec {
    padding: 13px 12px;
    font-size: 16px;
  }
  .phone .sec.on {
    background: transparent;
    font-weight: inherit;
  }
  .phone.open .secnav,
  .phone:not(.open) .secmain {
    display: none;
  }
  /* a font: its name and button on one line, its sample under them */
  .phone .fontrow {
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 6px 10px;
  }
  .phone .fontsample {
    grid-column: 1 / -1;
    grid-row: 2;
    justify-self: start;
  }
  /* a picker under its label, the full width, rather than squeezing the label */
  .phone .row:has(select) {
    flex-direction: column;
    align-items: stretch;
    gap: 8px;
  }
  /* fewer words: the explanations under each setting go (the figures, like what a database
     holds, stay) */
  .phone .row > span:not(.data-current) > small.faint.block,
  .phone .row .line > span > small.faint.block {
    display: none;
  }
  .phone .spend-tiles {
    grid-template-columns: repeat(2, 1fr);
  }
  /* the list of sections: larger, like a page title, with rows like an iPhone's Settings */
  .phone .secnav-title {
    font-size: 24px;
    padding: 0 12px 8px;
  }
  .phone .sec:not(.door) {
    display: flex;
    justify-content: space-between;
    border-bottom: 1px solid var(--border);
    border-radius: 0;
  }
  .phone .sec:not(.door)::after {
    content: "›";
    color: var(--text-3);
    font-size: 20px;
    line-height: 1;
  }
  /* About: the logo and motto above, the name and words below; credits one under another */
  .phone .about-hero {
    grid-template-columns: auto minmax(0, 1fr);
    gap: 14px 16px;
    padding: 18px;
  }
  .phone .about-id {
    grid-column: 1 / -1;
    grid-row: 2;
  }
  .phone .about-motto {
    justify-self: end;
    writing-mode: horizontal-tb;
    font-size: 22px;
    padding-left: 0;
    border-left: none;
  }
  .phone .credits {
    display: flex;
    flex-direction: column;
  }
  .phone .credits li {
    display: flex;
    flex-wrap: wrap;
    gap: 2px 10px;
  }
  .phone .credit-lic {
    margin-left: auto;
  }
  .phone .credits li > .faint {
    flex-basis: 100%;
    order: 3;
  }
  .link-state {
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }
  .link-intro {
    margin: 0;
    color: var(--text-2);
  }
  .lamp {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--text-3);
  }
  .lamp.ok {
    background: #22b45a;
    box-shadow: 0 0 0 3px rgb(34 180 90 / 0.2);
  }
  .lamp.bad {
    background: #e0453a;
    box-shadow: 0 0 0 3px rgb(224 69 58 / 0.2);
  }
  .row.col > input {
    width: 100%;
  }
  .backbar {
    position: sticky;
    top: 0;
    z-index: 5;
    padding: 8px 16px 2px;
    /* frosted, like an iPhone's own bars: the theme's picture shows through, softened, and
       what scrolls under it stays readable */
    background: color-mix(in srgb, var(--bg) 62%, transparent);
    -webkit-backdrop-filter: blur(14px) saturate(1.4);
    backdrop-filter: blur(14px) saturate(1.4);
    border-bottom: 1px solid color-mix(in srgb, var(--border) 60%, transparent);
  }
  /* as wide as the window allows, up to a comfortable reading width */
  .narrow {
    max-width: 880px;
  }
  h2 {
    margin: 26px 0 10px;
  }
  .group {
    border-radius: var(--radius);
    padding: 4px 18px;
  }
  .fonttoggle {
    display: flex;
    align-items: baseline;
    gap: 8px;
    flex-wrap: wrap;
    width: 100%;
    border: none;
    background: none;
    padding: 0;
    font: inherit;
    color: inherit;
    text-align: left;
    cursor: pointer;
  }
  .fonttoggle:hover .fonttitle {
    color: var(--accent);
  }
  .fonttoggle .twisty {
    display: inline-block;
    width: 12px;
    color: var(--text-3);
    font-size: 12px;
    transition: transform 0.12s;
  }
  .fonttoggle .twisty.open {
    transform: rotate(90deg);
  }
  .fontlist {
    display: flex;
    flex-direction: column;
    margin-top: 6px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }
  .fontrow {
    display: grid;
    grid-template-columns: 200px minmax(0, 1fr) 190px;
    align-items: center;
    gap: 12px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
  }
  .fontrow:last-child {
    border-bottom: none;
  }
  .fontrow.chosen {
    background: var(--accent-soft);
  }
  .fontname {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .fontsample {
    height: 24px;
    width: auto;
    max-width: 100%;
    object-fit: contain;
    object-position: left center;
  }
  /* the pictures are black text: make it light on the dark theme */
  :global(:root[data-theme="dark"]) .fontsample {
    filter: invert(1);
  }
  @media (prefers-color-scheme: dark) {
    :global(:root:not([data-theme="light"])) .fontsample {
      filter: invert(1);
    }
  }
  .fontact {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 92px;
    align-items: center;
    gap: 8px;
    white-space: nowrap;
  }
  .fa-status {
    justify-self: end;
  }
  .fa-main {
    justify-self: stretch;
    display: grid;
  }
  .fa-main .btn {
    width: 100%;
  }
  .inuse {
    text-align: center;
    color: var(--ok);
    font-weight: 600;
  }
  .fonthint {
    margin: 0;
  }
  /* the font menu (a component, so .row select doesn't reach it) */
  .row :global(select) {
    width: 220px;
  }
  .reading-preview {
    margin: 0;
    padding: 12px 16px;
    background: var(--surface-2);
    border-radius: var(--radius);
    overflow-wrap: anywhere;
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
  .mic-row {
    flex-wrap: wrap;
    justify-content: flex-start;
  }
  .mic-text {
    font-size: 16px;
    flex: 1 1 220px;
  }
  .rowbtns {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    justify-content: flex-end;
  }
</style>
