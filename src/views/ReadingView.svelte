<script lang="ts">
  import { onMount } from "svelte";
  import { open, ask, save } from "@tauri-apps/plugin-dialog";
  import { api, errorText, type Segment, type Story, type StoryFolder, type StoryInfo } from "../lib/api";
  import { t } from "../lib/i18n.svelte";
  import { settings, saveSetting } from "../lib/settings.svelte";
  import { showToast } from "../lib/toast.svelte";
  import Icon from "../components/Icon.svelte";
  import Modal from "../components/Modal.svelte";
  import Pinyin from "../components/Pinyin.svelte";
  import { hanziTones } from "../lib/pinyin";
  import { SIZE_MIN, SIZE_MAX, readingFont, clampSize } from "../lib/reading";
  import FontSelect from "../components/FontSelect.svelte";

  let folders = $state<StoryFolder[]>([]);
  let selected = $state<number | null>(null); // null = all stories
  let stories = $state<StoryInfo[]>([]);
  let loading = $state(true);

  const folder = $derived(folders.find((f) => f.id === selected) ?? null);
  const indent = (f: StoryFolder) => `${"  ".repeat(f.depth)}${f.name}`;

  // ---------- the open story ----------
  let story = $state<Story | null>(null);
  // raw: a long story is thousands of segments, only ever replaced whole
  let segments = $state.raw<Segment[] | null>(null);

  async function openStory(id: number) {
    try {
      story = await api.getStory(id);
      segments = null;
      hover = null;
      remember();
      const mine = id;
      const segs = await api.storySegments(id);
      if (story?.id === mine) segments = segs;
    } catch (e) {
      showToast(errorText(e), "err");
    }
  }

  function closeStory() {
    story = null;
    segments = null;
    hover = null;
    remember();
  }

  // Where you were (collection and story), so the page opens there next time.
  const LAST_KEY = "reading.last";
  function remember() {
    try {
      localStorage.setItem(LAST_KEY, JSON.stringify({ folder: selected, story: story?.id ?? null }));
    } catch {
      /* not essential */
    }
  }

  // ---------- text settings ----------
  const fontFamily = $derived(readingFont(settings.readingFont));
  const toneText = $derived(settings.toneColors && settings.readingToneColors);
  function setSize(n: number) {
    const v = clampSize(n);
    if (v !== settings.readingSize) saveSetting("readingSize", v);
  }

  // ---------- word lookup on hover ----------
  // One tooltip for the whole text (not one per word): placed next to the word under the pointer.
  let hover = $state<{ i: number; x: number; top: number; bottom: number } | null>(null);
  const hoverSeg = $derived(hover && segments ? segments[hover.i] : null);

  function over(e: MouseEvent) {
    const el = (e.target as HTMLElement).closest<HTMLElement>("[data-i]");
    if (!el) {
      hover = null;
      return;
    }
    const i = Number(el.dataset.i);
    if (hover?.i === i) return;
    const r = el.getBoundingClientRect();
    hover = { i, x: r.left + r.width / 2, top: r.top, bottom: r.bottom };
  }

  /** Keep the tooltip in the window: below the word, or above it when there's no room. */
  function place(node: HTMLElement, h: { x: number; top: number; bottom: number }) {
    const set = (h: { x: number; top: number; bottom: number }) => {
      const pad = 8;
      const w = node.offsetWidth;
      const ht = node.offsetHeight;
      const left = Math.min(window.innerWidth - pad - w, Math.max(pad, h.x - w / 2));
      const below = h.bottom + 6;
      const top = below + ht > window.innerHeight - pad ? Math.max(pad, h.top - 6 - ht) : below;
      node.style.left = `${left}px`;
      node.style.top = `${top}px`;
    };
    set(h);
    return { update: set };
  }

  // ---------- resizing the collection list ----------
  const SIDE_KEY = "reading.sideWidth";
  const SIDE_DEFAULT = 240;
  let sideWidth = $state(
    (() => {
      try {
        return Number(JSON.parse(localStorage.getItem(SIDE_KEY) ?? "null")?.w) || SIDE_DEFAULT;
      } catch {
        return SIDE_DEFAULT;
      }
    })(),
  );
  function saveSide() {
    try {
      localStorage.setItem(SIDE_KEY, JSON.stringify({ w: sideWidth }));
    } catch {
      /* not essential */
    }
  }
  function dragSide(e: PointerEvent) {
    e.preventDefault();
    const handle = e.currentTarget as HTMLElement;
    const x0 = e.clientX;
    const start = sideWidth;
    const max = Math.max(260, window.innerWidth * 0.6);
    handle.setPointerCapture(e.pointerId);
    document.body.classList.add("col-resizing");
    const move = (ev: PointerEvent) => (sideWidth = Math.round(Math.min(max, Math.max(160, start + ev.clientX - x0))));
    const up = () => {
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", up);
      handle.removeEventListener("pointercancel", up);
      document.body.classList.remove("col-resizing");
      saveSide();
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", up);
    handle.addEventListener("pointercancel", up);
  }

  // ---------- collection tree ----------
  const COLLAPSED_KEY = "reading.collapsed";
  let collapsed = $state<number[]>(
    (() => {
      try {
        const saved = JSON.parse(localStorage.getItem(COLLAPSED_KEY) ?? "[]");
        return Array.isArray(saved) ? saved.filter((x) => typeof x === "number") : [];
      } catch {
        return [];
      }
    })(),
  );
  function toggleCollapsed(id: number) {
    collapsed = collapsed.includes(id) ? collapsed.filter((x) => x !== id) : [...collapsed, id];
    try {
      localStorage.setItem(COLLAPSED_KEY, JSON.stringify(collapsed));
    } catch {
      /* not essential */
    }
  }
  const visibleFolders = $derived.by(() => {
    const parent = new Map(folders.map((f) => [f.id, f.parentId]));
    return folders.filter((f) => {
      for (let p = f.parentId; p !== null && p !== undefined; p = parent.get(p) ?? null) {
        if (collapsed.includes(p)) return false;
      }
      return true;
    });
  });
  function subtree(id: number): Set<number> {
    const out = new Set([id]);
    for (const f of folders) if (f.parentId !== null && out.has(f.parentId)) out.add(f.id);
    return out;
  }
  let allCount = $state(0); // every story, for "All stories"

  // ---------- loading ----------
  onMount(async () => {
    let last: { folder: number | null; story: number | null } = { folder: null, story: null };
    try {
      last = { ...last, ...JSON.parse(localStorage.getItem(LAST_KEY) ?? "null") };
    } catch {
      /* start at the top */
    }
    await loadFolders();
    if (last.folder !== null && folders.some((f) => f.id === last.folder)) selected = last.folder;
    await loadStories();
    if (last.story !== null) {
      try {
        await api.getStory(last.story); // still there?
        await openStory(last.story);
      } catch {
        remember();
      }
    }
  });

  async function loadFolders() {
    try {
      folders = await api.listStoryFolders();
    } catch (e) {
      showToast(errorText(e), "err");
    }
  }

  let request = 0;
  async function loadStories() {
    const mine = ++request;
    loading = true;
    try {
      const rows = await api.listStories(selected);
      if (mine !== request) return;
      stories = rows;
      allCount = selected === null ? rows.length : await api.storyTotal();
    } catch (e) {
      showToast(errorText(e), "err");
    } finally {
      if (mine === request) loading = false;
    }
  }
  async function refresh() {
    await loadFolders();
    await loadStories();
  }

  function choose(id: number | null) {
    selected = id;
    story = null;
    segments = null;
    hover = null;
    remember();
    loadStories();
  }

  // ---------- collections: add, edit, delete ----------
  let folderForm = $state<{ id: number | null; name: string; description: string; parentId: number | null } | null>(
    null,
  );

  async function saveFolder() {
    if (!folderForm) return;
    try {
      if (folderForm.id === null) {
        const id = await api.createStoryFolder(folderForm.name, folderForm.description, folderForm.parentId);
        if (folderForm.parentId !== null) collapsed = collapsed.filter((x) => x !== folderForm!.parentId);
        folderForm = null;
        await loadFolders();
        choose(id);
      } else {
        await api.updateStoryFolder(folderForm.id, folderForm.name, folderForm.description, folderForm.parentId);
        folderForm = null;
        await refresh();
      }
    } catch (e) {
      showToast(errorText(e), "err");
    }
  }

  async function removeFolder() {
    if (!folder) return;
    const subs = subtree(folder.id).size - 1;
    const message = subs
      ? t("reading.deleteFolderConfirmNested", { name: folder.path, d: subs, n: folder.totalStories })
      : t("reading.deleteFolderConfirm", { name: folder.path, n: folder.storyCount });
    if (!(await ask(message, { title: t("reading.deleteFolder"), kind: "warning" }))) return;
    try {
      await api.deleteStoryFolder(folder.id);
      await loadFolders();
      choose(null);
    } catch (e) {
      showToast(errorText(e), "err");
    }
  }

  // ---------- stories: add, edit, delete, import ----------
  let storyForm = $state<{ id: number | null; title: string; body: string; folderId: number | null } | null>(null);

  async function saveStory() {
    if (!storyForm) return;
    try {
      const f = storyForm;
      if (f.id === null) {
        const id = await api.createStory(f.folderId, f.title, f.body);
        storyForm = null;
        await refresh();
        await openStory(id);
      } else {
        await api.updateStory(f.id, f.folderId, f.title, f.body);
        storyForm = null;
        await refresh();
        await openStory(f.id);
      }
    } catch (e) {
      showToast(errorText(e), "err");
    }
  }

  async function removeStory() {
    if (!story) return;
    if (!(await ask(t("reading.deleteStoryConfirm", { name: story.title }), { title: t("reading.deleteStory"), kind: "warning" })))
      return;
    try {
      await api.deleteStory(story.id);
      closeStory();
      await refresh();
    } catch (e) {
      showToast(errorText(e), "err");
    }
  }

  let importMenu = $state(false);

  // ---------- export: files to keep or to share (the same files the import reads) ----------

  /** The open story as a text file. */
  async function exportStory() {
    if (!story) return;
    const name = story.title.replace(/[\\/:*?"<>|]/g, "_").trim() || "story";
    const path = await save({ defaultPath: `${name}.txt`, filters: [{ name: t("reading.importFiles"), extensions: ["txt"] }] });
    if (!path) return;
    try {
      await api.exportStory(story.id, path);
      showToast(t("reading.exportedStory", { path }));
    } catch (e) {
      showToast(errorText(e), "err");
    }
  }

  /** The collection shown (or every story) as a new folder inside one the learner picks. */
  async function exportCollection() {
    const into = await open({ directory: true, multiple: false, title: t("reading.exportInto") });
    if (!into) return;
    try {
      const r = await api.exportStoryFolder(selected, into as string);
      showToast(t("reading.exported", { s: r.stories, path: r.path }));
    } catch (e) {
      showToast(errorText(e), "err");
    }
  }

  /** A folder: it becomes a collection here, its subfolders collections inside it. */
  async function importFolder() {
    importMenu = false;
    const path = await open({ directory: true, multiple: false });
    if (!path) return;
    try {
      const r = await api.importStoryFolder(selected, path as string);
      showToast(t("reading.importedFolder", { s: r.stories, c: r.collections, k: r.skipped }));
      await refresh();
    } catch (e) {
      showToast(errorText(e), "err");
    }
  }

  // Key vocabulary lines: "word – pinyin – meaning"
  const vocab = $derived(
    (story?.vocab ?? "")
      .split("\n")
      .map((l) => l.split(/\s+[–—-]\s+/))
      .filter((p) => p[0]?.trim())
      .map(([word, py = "", ...meaning]) => ({ word: word.trim(), pinyin: py.trim(), meaning: meaning.join(" – ").trim() })),
  );
  let showVocab = $state(true);

  async function importFiles() {
    importMenu = false;
    const picked = await open({ multiple: true, filters: [{ name: t("reading.importFiles"), extensions: ["txt"] }] });
    if (!picked) return;
    const paths = Array.isArray(picked) ? picked : [picked];
    if (!paths.length) return;
    try {
      const n = await api.importStories(selected, paths as string[]);
      showToast(t("reading.imported", { n }));
      await refresh();
    } catch (e) {
      showToast(errorText(e), "err");
    }
  }

  const folderName = (id: number | null) => (id === null ? "" : (folders.find((f) => f.id === id)?.path ?? ""));
</script>

<div class="layout" style="--side:{sideWidth}px">
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="side-handle"
    title={t("decks.resizeHint")}
    onpointerdown={dragSide}
    ondblclick={() => {
      sideWidth = SIDE_DEFAULT;
      saveSide();
    }}
  ></div>
  <aside class="folderlist">
    <div class="fl-head">
      <h2>{t("reading.title")}</h2>
      <button
        class="btn ghost icon"
        title={t("reading.newFolder")}
        aria-label={t("reading.newFolder")}
        onclick={() => (folderForm = { id: null, name: "", description: "", parentId: null })}
      >
        <Icon name="plus" />
      </button>
    </div>
    <button class="folder" class:on={selected === null} onclick={() => choose(null)}>
      <span class="fname">{t("reading.allStories")}</span>
      <span class="faint small">{allCount}</span>
    </button>
    {#each visibleFolders as f (f.id)}
      <div class="folderrow" style="padding-left:{f.depth * 14}px">
        {#if f.childCount}
          <button
            class="twisty"
            class:open={!collapsed.includes(f.id)}
            aria-label={collapsed.includes(f.id) ? t("reading.expand") : t("reading.collapse")}
            aria-expanded={!collapsed.includes(f.id)}
            onclick={() => toggleCollapsed(f.id)}>▸</button
          >
        {:else}
          <span class="twisty-space"></span>
        {/if}
        <button class="folder" class:on={selected === f.id} onclick={() => choose(f.id)}>
          <span class="fname">{f.name}</span>
          <span class="faint small">{f.totalStories}</span>
        </button>
      </div>
    {/each}
  </aside>

  <section class="main">
    <!-- text settings, always at the top of the page -->
    <div class="textbar" role="toolbar" aria-label={t("reading.textSettings")}>
      <label class="tb-field">
        <span class="small muted">{t("reading.font")}</span>
        <FontSelect />
      </label>
      <div class="tb-field">
        <span class="small muted">{t("reading.size")}</span>
        <button
          class="btn ghost icon sz"
          title={t("reading.smaller")}
          aria-label={t("reading.smaller")}
          disabled={settings.readingSize <= SIZE_MIN}
          onclick={() => setSize(settings.readingSize - 2)}><span class="a small-a">A</span></button
        >
        <input
          type="range"
          min={SIZE_MIN}
          max={SIZE_MAX}
          step="1"
          value={settings.readingSize}
          oninput={(e) => (settings.readingSize = Number(e.currentTarget.value))}
          onchange={(e) => setSize(Number(e.currentTarget.value))}
          aria-label={t("reading.size")}
        />
        <button
          class="btn ghost icon sz"
          title={t("reading.larger")}
          aria-label={t("reading.larger")}
          disabled={settings.readingSize >= SIZE_MAX}
          onclick={() => setSize(settings.readingSize + 2)}><span class="a big-a">A</span></button
        >
        <span class="small muted num">{settings.readingSize}px</span>
      </div>
      <span class="sample han" style="font-family:{fontFamily}">汉字阅读</span>
    </div>

    {#if story}
      <div class="head">
        <div class="title">
          <button class="btn ghost back" onclick={closeStory}>
            <span class="flip"><Icon name="arrow" size={16} /></span>{folder?.name ?? t("reading.allStories")}
          </button>
          <h1 class="han">{story.title}</h1>
          {#if story.folderId !== null && story.folderId !== selected}
            <p class="small faint">{folderName(story.folderId)}</p>
          {/if}
        </div>
        <div class="tools">
          <button
            class="btn ghost icon"
            title={t("reading.editStory")}
            aria-label={t("reading.editStory")}
            onclick={() => story && (storyForm = { id: story.id, title: story.title, body: story.body, folderId: story.folderId })}
          >
            <Icon name="edit" />
          </button>
          <button class="btn ghost icon" title={t("reading.exportStory")} aria-label={t("reading.exportStory")} onclick={exportStory}>
            <Icon name="download" />
          </button>
          <button class="btn ghost icon" title={t("reading.deleteStory")} aria-label={t("reading.deleteStory")} onclick={removeStory}>
            <Icon name="trash" />
          </button>
        </div>
      </div>

      {#if vocab.length}
        <section class="vocab card-surface">
          <button class="vocab-head" aria-expanded={showVocab} onclick={() => (showVocab = !showVocab)}>
            <span class="twisty" class:open={showVocab} aria-hidden="true">▸</span>
            <span>{t("reading.keyVocab")}</span>
            <small class="faint">{vocab.length}</small>
          </button>
          {#if showVocab}
            <ul class="vocab-list">
              {#each vocab as v, i (i)}
                <li>
                  <span class="vw han" lang="zh-CN">{v.word}</span>
                  <span class="vp"><Pinyin text={v.pinyin} /></span>
                  <span class="vm muted">{v.meaning}</span>
                </li>
              {/each}
            </ul>
          {/if}
        </section>
      {/if}
      <p class="small faint hint">{t("reading.hoverHint")}</p>
      <!-- svelte-ignore a11y_mouse_events_have_key_events -->
      <article
        class="text card-surface"
        lang="zh-CN"
        style="font-family:{fontFamily}; font-size:{settings.readingSize}px; line-height:{settings.readingLineHeight}"
        onmouseover={over}
        onmouseleave={() => (hover = null)}
      >
        {#if segments}
          {#each segments as s, i (i)}{#if s.entry}<span class="w" class:on={hover?.i === i} data-i={i}
                >{#if toneText}{#each hanziTones(s.text, s.entry.pinyin) as c, k (k)}<span
                      class={c.tone ? `tone-${c.tone}` : ""}>{c.ch}</span
                    >{/each}{:else}{s.text}{/if}</span
              >{:else}{s.text}{/if}{/each}
        {:else}
          {story.body}
        {/if}
      </article>
    {:else}
      <div class="head">
        <div class="title">
          {#if folder && folder.depth > 0}
            <p class="crumbs small faint">{folder.path.split(" › ").slice(0, -1).join(" › ")}</p>
          {/if}
          <h1>{folder?.name ?? t("reading.allStories")}</h1>
          {#if folder?.description}<p class="muted">{folder.description}</p>{/if}
        </div>
        <div class="tools">
          {#if folder}
            <button
              class="btn ghost icon"
              title={t("reading.editFolder")}
              aria-label={t("reading.editFolder")}
              onclick={() =>
                (folderForm = { id: folder.id, name: folder.name, description: folder.description, parentId: folder.parentId })}
            >
              <Icon name="edit" />
            </button>
            <button class="btn ghost icon" title={t("reading.deleteFolder")} aria-label={t("reading.deleteFolder")} onclick={removeFolder}>
              <Icon name="trash" />
            </button>
            <button
              class="btn ghost icon"
              title={t("reading.newSubfolder")}
              aria-label={t("reading.newSubfolder")}
              onclick={() => (folderForm = { id: null, name: "", description: "", parentId: folder.id })}
            >
              <Icon name="folder" />
            </button>
          {/if}
          <div class="menuwrap">
            <button class="btn" aria-haspopup="menu" aria-expanded={importMenu} onclick={() => (importMenu = !importMenu)}>
              <Icon name="upload" size={16} />{t("reading.import")}
            </button>
            {#if importMenu}
              <div class="menu card-surface" role="menu">
                <button role="menuitem" onclick={importFiles} title={t("reading.importHint")}>{t("reading.importFilesItem")}</button>
                <button role="menuitem" onclick={importFolder} title={t("reading.importFolderHint")}>{t("reading.importFolder")}</button>
              </div>
            {/if}
          </div>
          <button class="btn" disabled={!stories.length && !folder?.totalStories} title={t("reading.exportHint")} onclick={exportCollection}>
            <Icon name="download" size={16} />{t("reading.export")}
          </button>
          <button class="btn primary" onclick={() => (storyForm = { id: null, title: "", body: "", folderId: selected })}>
            <Icon name="plus" size={16} />{t("reading.addStory")}
          </button>
        </div>
      </div>

      {#if !loading && stories.length === 0}
        <div class="empty card-surface">
          <Icon name="book" size={28} />
          <p class="muted">{t("reading.empty")}</p>
        </div>
      {:else}
        <div class="list">
          {#each stories as s (s.id)}
            <button class="story card-surface" onclick={() => openStory(s.id)}>
              <span class="st-title han">{s.title}</span>
              <span class="st-preview han muted">{s.preview}</span>
              <span class="st-meta small faint">
                {t("reading.chars", { n: s.chars })}{#if s.folderId !== null && s.folderId !== selected}
                  · {folderName(s.folderId)}{/if}
              </span>
            </button>
          {/each}
        </div>
      {/if}
    {/if}
  </section>
</div>

{#if hover && hoverSeg?.entry}
  {@const e = hoverSeg.entry}
  <div class="tip" role="tooltip" use:place={hover}>
    <div class="tip-head">
      <span class="tip-hz han" lang="zh-CN">{e.simplified}{#if e.traditional !== e.simplified}<span class="trad"
            >（{e.traditional}）</span
          >{/if}</span>
      {#if e.inCards}<span class="badge review">{t("reading.inCards")}</span>{/if}
    </div>
    <div class="tip-py"><Pinyin text={e.pinyin} /></div>
    <div class="tip-defs">{e.definitions.slice(0, 5).join("; ")}</div>
  </div>
{/if}

<svelte:window
  onscroll={() => (hover = null)}
  onresize={() => (hover = null)}
  onclick={(e) => {
    if (importMenu && !(e.target as HTMLElement).closest(".menuwrap")) importMenu = false;
  }}
/>

{#if folderForm}
  {@const blocked = folderForm.id !== null ? subtree(folderForm.id) : new Set<number>()}
  <Modal
    title={folderForm.id === null ? t("reading.newFolder") : t("reading.editFolder")}
    onclose={() => (folderForm = null)}
    width={440}
  >
    <form
      class="form"
      onsubmit={(e) => {
        e.preventDefault();
        saveFolder();
      }}
    >
      <label class="field">
        <span>{t("reading.folderName")}</span>
        <!-- svelte-ignore a11y_autofocus -->
        <input type="text" bind:value={folderForm.name} autofocus />
      </label>
      <label class="field">
        <span>{t("decks.parent")}</span>
        <select bind:value={folderForm.parentId}>
          <option value={null}>{t("decks.topLevel")}</option>
          {#each folders.filter((f) => !blocked.has(f.id)) as f (f.id)}<option value={f.id}>{indent(f)}</option>{/each}
        </select>
      </label>
      <label class="field">
        <span>{t("decks.description")}</span>
        <input type="text" bind:value={folderForm.description} />
      </label>
      <div class="formbtns">
        <button type="button" class="btn ghost" onclick={() => (folderForm = null)}>{t("common.cancel")}</button>
        <button type="submit" class="btn primary" disabled={!folderForm.name.trim()}>{t("common.save")}</button>
      </div>
    </form>
  </Modal>
{/if}

{#if storyForm}
  <Modal
    title={storyForm.id === null ? t("reading.addStory") : t("reading.editStory")}
    onclose={() => (storyForm = null)}
    width={680}
  >
    <form
      class="form"
      onsubmit={(e) => {
        e.preventDefault();
        saveStory();
      }}
    >
      <label class="field">
        <span>{t("reading.storyTitle")}</span>
        <!-- svelte-ignore a11y_autofocus -->
        <input type="text" bind:value={storyForm.title} autofocus />
      </label>
      <label class="field">
        <span>{t("reading.folder")}</span>
        <select bind:value={storyForm.folderId}>
          <option value={null}>{t("reading.noFolder")}</option>
          {#each folders as f (f.id)}<option value={f.id}>{indent(f)}</option>{/each}
        </select>
      </label>
      <label class="field">
        <span>{t("reading.storyText")}</span>
        <textarea class="han" rows="14" bind:value={storyForm.body} placeholder={t("reading.storyTextHint")}></textarea>
      </label>
      <div class="formbtns">
        <button type="button" class="btn ghost" onclick={() => (storyForm = null)}>{t("common.cancel")}</button>
        <button type="submit" class="btn primary" disabled={!storyForm.title.trim() || !storyForm.body.trim()}
          >{t("common.save")}</button
        >
      </div>
    </form>
  </Modal>
{/if}

<style>
  .layout {
    display: grid;
    grid-template-columns: var(--side) minmax(0, 1fr);
    height: 100%;
    position: relative;
  }
  .side-handle {
    position: absolute;
    top: 0;
    bottom: 0;
    left: calc(var(--side) - 4px);
    width: 8px;
    cursor: col-resize;
    z-index: 5;
    touch-action: none;
  }
  .side-handle:hover,
  .side-handle:active {
    background: linear-gradient(to right, transparent 3px, var(--accent) 3px, var(--accent) 5px, transparent 5px);
  }
  .folderlist {
    border-right: 1px solid var(--border);
    padding: 22px 12px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .fl-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 6px 10px 10px;
  }
  .folder {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    border: none;
    background: transparent;
    padding: 8px 10px;
    border-radius: 8px;
    text-align: left;
  }
  .folder:hover {
    background: var(--surface-2);
  }
  .folder.on {
    background: var(--surface-3);
    font-weight: 600;
  }
  .fname {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .folderrow {
    display: flex;
    align-items: center;
    gap: 2px;
  }
  .folderrow .folder {
    flex: 1;
    min-width: 0;
  }
  .twisty,
  .twisty-space {
    flex: 0 0 18px;
    width: 18px;
  }
  .twisty {
    border: none;
    background: transparent;
    padding: 0;
    color: var(--text-3);
    font-size: 12px;
    line-height: 1;
    transition: transform 0.12s;
  }
  .twisty.open {
    transform: rotate(90deg);
  }

  .main {
    overflow-y: auto;
    min-width: 0;
    padding: 0 28px 28px;
  }
  .textbar {
    position: sticky;
    top: 0;
    z-index: 4;
    display: flex;
    align-items: center;
    gap: 22px;
    flex-wrap: wrap;
    margin: 0 -28px 18px;
    padding: 10px 28px;
    background: var(--bg);
    border-bottom: 1px solid var(--border);
  }
  .tb-field {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .tb-field :global(select) {
    width: auto;
  }
  .tb-field input[type="range"] {
    width: 140px;
    accent-color: var(--accent);
  }
  .sz {
    width: 30px;
    height: 30px;
    padding: 0;
  }
  .a {
    font-weight: 650;
    line-height: 1;
  }
  .small-a {
    font-size: 12px;
  }
  .big-a {
    font-size: 18px;
  }
  .num {
    font-variant-numeric: tabular-nums;
    min-width: 36px;
  }
  .sample {
    margin-left: auto;
    font-size: 20px;
    color: var(--text-2);
  }

  .head {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 16px;
    margin-bottom: 14px;
    flex-wrap: wrap;
  }
  .tools {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }
  .crumbs {
    margin: 0 0 2px;
  }
  .back {
    margin: 0 0 6px -10px;
    color: var(--text-2);
  }
  .flip {
    display: inline-flex;
    transform: scaleX(-1);
  }
  .menuwrap {
    position: relative;
  }
  .menu {
    position: absolute;
    top: calc(100% + 4px);
    right: 0;
    z-index: 20;
    display: flex;
    flex-direction: column;
    padding: 4px;
    min-width: 180px;
    box-shadow: var(--shadow-lg);
  }
  .menu button {
    border: none;
    background: transparent;
    text-align: left;
    padding: 8px 10px;
    border-radius: 6px;
    white-space: nowrap;
  }
  .menu button:hover {
    background: var(--surface-2);
  }
  .vocab {
    max-width: 52em;
    margin-bottom: 12px;
    padding: 10px 16px;
  }
  .vocab-head {
    display: flex;
    align-items: baseline;
    gap: 8px;
    border: none;
    background: none;
    padding: 0;
    font: inherit;
    font-weight: 600;
    color: inherit;
    cursor: pointer;
  }
  .vocab-head .twisty {
    display: inline-block;
    width: 12px;
    color: var(--text-3);
    font-size: 12px;
    font-weight: normal;
    transition: transform 0.12s;
  }
  .vocab-head .twisty.open {
    transform: rotate(90deg);
  }
  .vocab-list {
    list-style: none;
    margin: 8px 0 2px;
    padding: 0;
    display: grid;
    grid-template-columns: max-content max-content minmax(0, 1fr);
    gap: 4px 14px;
  }
  .vocab-list li {
    display: contents;
  }
  .vw {
    font-size: 18px;
  }
  .vp {
    font-weight: 600;
    white-space: nowrap;
  }
  .hint {
    margin: 0 0 10px;
  }
  .text {
    padding: 28px 32px;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    max-width: 52em;
  }
  .w {
    border-radius: 4px;
    cursor: help;
  }
  .w.on {
    background: var(--accent-soft);
    box-shadow: 0 0 0 2px var(--accent-soft);
  }

  .empty {
    padding: 48px;
    text-align: center;
    color: var(--text-3);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
  }
  .list {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: 12px;
  }
  .story {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
    padding: 14px 16px;
    text-align: left;
    border: 1px solid var(--border);
    cursor: pointer;
    min-width: 0;
  }
  .story:hover {
    border-color: var(--text-3);
  }
  .st-title {
    font-size: 17px;
    font-weight: 600;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .st-preview {
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    font-size: 14px;
    line-height: 1.5;
  }

  .tip {
    position: fixed;
    z-index: 50;
    width: max-content;
    max-width: 320px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 10px 12px;
    background: var(--surface);
    color: var(--text);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: var(--shadow-lg);
    font-size: 14px;
    line-height: 1.4;
    pointer-events: none;
  }
  .tip-head {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .tip-hz {
    font-size: 22px;
  }
  .trad {
    font-size: 15px;
    color: var(--text-3);
  }
  .tip-py {
    font-weight: 600;
    font-size: 15px;
  }
  .tip-defs {
    color: var(--text-2);
  }

  .form {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .form textarea {
    font-size: 16px;
    line-height: 1.7;
    resize: vertical;
  }
  .formbtns {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 6px;
  }
</style>
