<script lang="ts">
  // Shown instead of the app's pages when a database couldn't be opened at startup (it's
  // damaged). Nothing has been changed at that point: the learner picks a backup to use, or
  // another file, or starts empty; the damaged files are kept in a "damaged-…" folder.
  import { onMount } from "svelte";
  import { ask, open } from "@tauri-apps/plugin-dialog";
  import { api, errorText, type DataContents, type DataFile, type DataInfo, type DbKind } from "../lib/api";
  import { t } from "../lib/i18n.svelte";
  import { fmtNum } from "../lib/format";

  let { kind, reason, message }: { kind: DbKind; reason: "damaged" | "in_use" | "error"; message: string } = $props();
  /** Backups and starting empty are for a damaged file; one that is only busy just needs letting go. */
  const damaged = $derived(reason === "damaged");
  // what the last try to open it said (the first time: what startup said)
  let latest = $state("");
  const shownMessage = $derived(latest || message);

  let info = $state<DataInfo | null>(null);
  let busy = $state(false);
  let error = $state("");
  let done = $state(false);

  onMount(async () => {
    try {
      info = await api.dataInfo(kind);
    } catch (e) {
      error = errorText(e);
    }
  });

  // backups that can't be read (an empty or damaged file) are no use here
  const backups = $derived(
    [...(info?.backups ?? []), ...(info?.autoBackups ?? [])].filter((b) => b.contents).sort((a, b) => b.modified - a.modified),
  );
  const when = (s: number) => new Date(s * 1000).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
  const mb = (bytes: number) => t("settings.dataSize", { mb: (bytes / 1_048_576).toFixed(1) });
  const summary = (c: DataContents) =>
    kind === "reading"
      ? t("settings.dataSummaryReading", { stories: fmtNum(c.stories), collections: fmtNum(c.collections) })
      : t(c.decks === 1 ? "settings.dataSummaryOne" : "settings.dataSummary", {
          cards: fmtNum(c.cards),
          decks: fmtNum(c.decks),
          reviews: fmtNum(c.reviews),
        });

  /** The database is in place: load the app again, now with it. */
  function restart() {
    done = true;
    setTimeout(() => location.reload(), 900);
  }

  async function use(path: string) {
    error = "";
    let file: DataFile;
    try {
      file = await api.dataInspect(kind, path);
    } catch (e) {
      error = errorText(e);
      return;
    }
    const what = file.contents ? summary(file.contents) : file.name;
    if (!(await ask(t("recovery.useConfirm", { what, when: when(file.modified) }), { kind: "warning", okLabel: t("recovery.use") }))) return;
    busy = true;
    try {
      await api.dataUseFile(kind, path);
      restart();
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }

  /** Another program had the file: see whether it's free now. */
  async function retry() {
    error = "";
    busy = true;
    try {
      if (await api.startupRetry()) return restart();
      const now = await api.startupProblem();
      // still the same trouble: say what this try found; anything else (the other database's
      // turn, or a different reason): start the page over with it
      if (now && now.kind === kind && now.reason === reason) latest = now.message;
      else restart();
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }

  async function choose() {
    const path = await open({ multiple: false, directory: false, defaultPath: info?.backupFolder ?? info?.folder });
    if (path) await use(path as string);
  }

  async function startFresh() {
    error = "";
    const text = t(kind === "reading" ? "recovery.freshConfirmReading" : "recovery.freshConfirmCards");
    if (!(await ask(text, { kind: "warning", okLabel: t("recovery.fresh") }))) return;
    busy = true;
    try {
      await api.dataStartFresh(kind);
      restart();
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="page recovery">
  <h1>{t(kind === "reading" ? "recovery.titleReading" : "recovery.titleCards")}</h1>
  <p>{t(damaged ? "recovery.explain" : "recovery.explainInUse")}</p>
  <p class="faint small detail">{shownMessage}</p>

  {#if done}
    <div class="notice">{t("recovery.done")}</div>
  {:else}
    {#if error}<div class="notice err">{error}</div>{/if}

    {#if !damaged}
      <div class="actions">
        <button class="btn primary" disabled={busy} onclick={retry}>{t("recovery.retry")}</button>
        <button class="btn" onclick={() => api.dataShowInFolder().catch((e) => (error = errorText(e)))}>{t("recovery.openFolder")}</button>
      </div>
    {:else}
    <h2>{t("recovery.backups")}</h2>
    {#if !info && !error}
      <p class="faint">…</p>
    {:else if !backups.length}
      <p class="muted">{t("recovery.noBackups")}</p>
    {:else}
      <ul class="backups">
        {#each backups as b (b.path)}
          <li class="card-surface">
            <div class="what">
              <div class="when">{when(b.modified)}</div>
              <div class="muted small">{b.contents ? summary(b.contents) : ""} · {mb(b.sizeBytes)}</div>
              <div class="faint small name">{b.name}</div>
            </div>
            <button class="btn primary" disabled={busy} onclick={() => use(b.path)}>{t("recovery.use")}</button>
          </li>
        {/each}
      </ul>
    {/if}

    <div class="actions">
      <button class="btn" disabled={busy} onclick={choose}>{t("recovery.choose")}</button>
      <button class="btn" onclick={() => api.dataShowInFolder().catch((e) => (error = errorText(e)))}>{t("recovery.openFolder")}</button>
      <button class="btn ghost" disabled={busy} onclick={startFresh}>{t("recovery.fresh")}</button>
    </div>
    {/if}
  {/if}
</div>

<style>
  .recovery {
    max-width: 720px;
  }
  h1 {
    margin-bottom: 8px;
  }
  h2 {
    font-size: 15px;
    margin: 22px 0 10px;
  }
  .detail {
    overflow-wrap: anywhere;
  }
  .backups {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .backups li {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 12px 14px;
  }
  .what {
    flex: 1;
    min-width: 0;
  }
  .when {
    font-weight: 600;
  }
  .name {
    overflow-wrap: anywhere;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 22px;
  }
</style>
