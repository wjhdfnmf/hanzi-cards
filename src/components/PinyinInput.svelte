<script lang="ts">
  // A one-line text box with a built-in pinyin keyboard: in 拼 mode, letters typed before
  // the caret are pinyin and matching words are offered above the box (Space or Enter picks,
  // ↑↓ choose, a digit 1-5 after a syllable narrows the tone, punctuation becomes Chinese).
  // In EN mode it's an ordinary text box. Ctrl+Space or the button switches modes.
  import { tick } from "svelte";
  import { api, type Candidate } from "../lib/api";
  import { t } from "../lib/i18n.svelte";
  import Hanzi from "./Hanzi.svelte";
  import Pinyin from "./Pinyin.svelte";

  let {
    value = $bindable(""),
    pinyin = $bindable(true),
    placeholder = "",
    disabled = false,
    onsubmit,
  }: {
    value?: string;
    pinyin?: boolean;
    placeholder?: string;
    disabled?: boolean;
    onsubmit?: () => void;
  } = $props();

  type Run = { start: number; end: number; text: string };
  let input = $state<HTMLInputElement | null>(null);
  let candidates = $state<Candidate[]>([]);
  let index = $state(0);
  let activeRun: Run | null = null;
  let composing = false; // an OS input method is composing; leave it alone
  let seq = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;

  const RUN = /[a-zA-ZüÜ][a-zA-ZüÜ:'1-5]*$/;
  const PUNCT: Record<string, string> = { ",": "，", ".": "。", "?": "？", "!": "！", ":": "：", ";": "；", "(": "（", ")": "）" };

  function run(): Run | null {
    if (!input) return null;
    const caret = input.selectionStart ?? value.length;
    const m = value.slice(0, caret).match(RUN);
    return m ? { start: caret - m[0].length, end: caret, text: m[0] } : null;
  }

  function close() {
    seq++;
    clearTimeout(timer);
    candidates = [];
    activeRun = null;
  }

  function update() {
    const r = pinyin && !composing ? run() : null;
    if (!r) return close();
    const mine = ++seq;
    clearTimeout(timer);
    timer = setTimeout(async () => {
      const list = await api.dictCandidates(r.text, 8).catch(() => []);
      if (mine !== seq) return;
      activeRun = r;
      candidates = list;
      index = 0;
    }, 60);
  }

  async function setCaret(at: number) {
    await tick();
    input?.focus();
    input?.setSelectionRange(at, at);
  }

  async function pick(c: Candidate) {
    const r = activeRun;
    if (!r) return;
    const rest = r.text.slice(c.consumed); // pinyin this word didn't use stays for the next pick
    value = value.slice(0, r.start) + c.entry.simplified + rest + value.slice(r.end);
    close();
    await setCaret(r.start + c.entry.simplified.length + rest.length);
    if (rest) update();
  }

  function insert(text: string) {
    if (!input) return;
    const a = input.selectionStart ?? value.length;
    const b = input.selectionEnd ?? a;
    value = value.slice(0, a) + text + value.slice(b);
    setCaret(a + text.length);
  }

  export function toggle() {
    pinyin = !pinyin;
    close();
    try {
      localStorage.setItem("tutor.typePinyin", String(pinyin));
    } catch {}
    input?.focus();
    if (pinyin) update();
  }

  export function focus() {
    input?.focus();
  }

  async function onkeydown(e: KeyboardEvent) {
    if (composing || e.isComposing) return;
    if (e.key === " " && e.ctrlKey) {
      e.preventDefault();
      toggle();
      return;
    }
    const n = candidates.length;
    if (n) {
      if (e.key === "ArrowDown") index = (index + 1) % n;
      else if (e.key === "ArrowUp") index = (index - 1 + n) % n;
      else if (e.key === " " || (e.key === "Enter" && !e.ctrlKey && !e.metaKey)) pick(candidates[index]);
      else if (e.key === "Escape") close();
      else if (pinyin && PUNCT[e.key]) {
        // like an input method: punctuation commits the highlighted word first
        e.preventDefault();
        await pick(candidates[index]);
        insert(PUNCT[e.key]);
        return;
      } else return;
      e.preventDefault();
      e.stopPropagation();
      return;
    }
    if (pinyin && PUNCT[e.key] && !e.ctrlKey && !e.altKey && !e.metaKey) {
      e.preventDefault();
      insert(PUNCT[e.key]);
    } else if (e.key === "Enter" && !e.isComposing) {
      e.preventDefault();
      onsubmit?.();
    }
  }
</script>

<div class="pinyin-input" class:disabled>
  <button
    type="button"
    class="mode"
    class:py={pinyin}
    {disabled}
    title={t("pyinput.toggle")}
    aria-label={t("pyinput.toggle")}
    aria-pressed={pinyin}
    onclick={toggle}>{pinyin ? "拼" : "EN"}</button
  >
  <input
    bind:this={input}
    bind:value
    type="text"
    autocomplete="off"
    spellcheck="false"
    lang={pinyin ? "zh-CN" : "en"}
    placeholder={pinyin ? t("pyinput.placeholderPinyin") : placeholder}
    {disabled}
    role="combobox"
    aria-expanded={candidates.length > 0}
    aria-controls="pyinput-candidates"
    oninput={update}
    onclick={update}
    {onkeydown}
    onkeyup={(e) => ["ArrowLeft", "ArrowRight", "Home", "End"].includes(e.key) && update()}
    onblur={close}
    oncompositionstart={() => {
      composing = true;
      close();
    }}
    oncompositionend={() => {
      composing = false;
      update();
    }}
  />
  {#if candidates.length}
    <ul class="cands" id="pyinput-candidates" role="listbox" aria-label={t("editor.candidates")}>
      {#each candidates as c, i (`${c.entry.id}:${c.entry.simplified}`)}
        <li
          role="option"
          aria-selected={i === index}
          class:on={i === index}
          onmousedown={(e) => {
            e.preventDefault();
            pick(c);
          }}
          onmouseenter={() => (index = i)}
        >
          <span class="han-c"><Hanzi hanzi={c.entry.simplified} pinyin={c.entry.pinyin} size={20} /></span>
          <span class="py-c"><Pinyin text={c.entry.pinyin} /></span>
          <span class="def">{c.entry.definitions.slice(0, 2).join("; ")}</span>
        </li>
      {/each}
      <li class="help" role="presentation">{t("pyinput.keys")}</li>
    </ul>
  {/if}
</div>

<style>
  .pinyin-input {
    position: relative;
    flex: 1;
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }
  .pinyin-input input {
    flex: 1;
    min-width: 0;
  }
  .mode {
    flex-shrink: 0;
    width: 38px;
    height: 34px;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--text-2);
    font-weight: 700;
    font-size: 13px;
    cursor: pointer;
  }
  .mode.py {
    background: var(--accent-soft);
    color: var(--accent);
    border-color: transparent;
    font-size: 15px;
  }
  .mode:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .cands {
    position: absolute;
    left: 44px;
    right: 0;
    bottom: calc(100% + 6px);
    margin: 0;
    padding: 4px;
    list-style: none;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: var(--shadow-lg);
    z-index: 20;
    max-height: 320px;
    overflow-y: auto;
  }
  .cands li[role="option"] {
    display: grid;
    grid-template-columns: auto auto 1fr;
    align-items: baseline;
    gap: 10px;
    padding: 5px 8px;
    border-radius: 6px;
    cursor: pointer;
  }
  .cands li.on {
    background: var(--accent-soft);
  }
  .py-c {
    font-size: 13px;
  }
  .def {
    font-size: 12.5px;
    color: var(--text-2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .help {
    padding: 4px 8px 2px;
    font-size: 11.5px;
    color: var(--text-3, var(--text-2));
  }
</style>
