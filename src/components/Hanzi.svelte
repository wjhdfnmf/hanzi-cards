<script module lang="ts">
  import { api, type DictEntry } from "../lib/api";

  // Dictionary lookups for single characters, shared by every <Hanzi>.
  const lookups = new Map<string, Promise<DictEntry[]>>();
  function lookupChar(ch: string): Promise<DictEntry[]> {
    let p = lookups.get(ch);
    if (!p) {
      p = api.dictLookup(ch).catch(() => {
        lookups.delete(ch); // try again next time (e.g. dictionary installed later)
        return [];
      });
      lookups.set(ch, p);
    }
    return p;
  }
</script>

<script lang="ts">
  // Chinese characters, optionally coloured by tone (when pinyin lines up with the characters).
  // With `define`, hovering a character shows its dictionary definition.
  import Pinyin from "./Pinyin.svelte";
  import { hanziTones, isHanzi, stripTones, toneOf } from "../lib/pinyin";
  import { settings } from "../lib/settings.svelte";
  import { t } from "../lib/i18n.svelte";

  let {
    hanzi,
    pinyin = "",
    colored = true,
    size = 0,
    define = false,
  }: { hanzi: string; pinyin?: string; colored?: boolean; size?: number; define?: boolean } = $props();

  const parts = $derived(hanziTones(hanzi, pinyin));
  const useColor = $derived(colored && settings.toneColors && pinyin !== "");

  let hover = $state<number | null>(null);
  let entries = $state<DictEntry[] | null>(null);

  async function enter(i: number) {
    const ch = parts[i].ch;
    if (!define || !isHanzi(ch)) return;
    hover = i;
    entries = null;
    const found = await lookupChar(ch);
    if (hover === i) entries = pick(found, parts[i].syl);
  }

  function leave() {
    hover = null;
    entries = null;
  }

  const sameSyl = (a: string, b: string) =>
    stripTones(a.toLowerCase()).replace(/:/g, "") === stripTones(b.toLowerCase()).replace(/:/g, "") &&
    toneOf(a) === toneOf(b);

  /** Readings to show: the one this card uses first, others after; at most three. */
  function pick(found: DictEntry[], syl: string): DictEntry[] {
    const seen = new Set<string>();
    const unique = found.filter((e) => {
      const k = e.pinyin.toLowerCase() + "|" + e.definitions.join(";");
      if (seen.has(k)) return false;
      seen.add(k);
      return true;
    });
    if (!syl) return unique.slice(0, 3);
    const match = unique.filter((e) => sameSyl(e.pinyin, syl));
    return match.length ? match.slice(0, 3) : unique.slice(0, 3);
  }

  /** Keep the tooltip inside the window. */
  function clamp(node: HTMLElement) {
    const r = node.getBoundingClientRect();
    const pad = 8;
    let dx = 0;
    if (r.left < pad) dx = pad - r.left;
    else if (r.right > window.innerWidth - pad) dx = window.innerWidth - pad - r.right;
    if (dx) node.style.setProperty("--dx", `${dx}px`);
    if (r.bottom > window.innerHeight - pad) node.classList.add("above");
  }
</script>

<span class="han hz" lang="zh-CN" style={size ? `font-size:${size}px` : ""}
  >{#each parts as p, i (i)}<span
      class={[useColor && p.tone ? `tone-${p.tone}` : "", define && isHanzi(p.ch) ? "def" : ""]}
      role={define ? "presentation" : undefined}
      onmouseenter={() => enter(i)}
      onmouseleave={leave}
      >{p.ch}{#if hover === i && entries}<span class="tip" role="tooltip" lang="en" use:clamp
          >{#if entries.length === 0}<span class="none">{t("dict.noResults")}</span>{:else}{#each entries as e (e.id)}<span
                class="entry"
                ><span class="py"><Pinyin text={e.pinyin} /></span><span class="defs"
                  >{e.definitions.slice(0, 4).join("; ")}</span
                ></span
              >{/each}{/if}</span
        >{/if}</span
    >{/each}</span
>

<style>
  .hz {
    line-height: 1.2;
    letter-spacing: 0.02em;
  }
  .def {
    position: relative;
    cursor: help;
    border-radius: 6px;
  }
  .def:hover {
    background: var(--surface-2);
  }
  .tip {
    --dx: 0px;
    position: absolute;
    top: calc(100% + 6px);
    left: 50%;
    transform: translateX(calc(-50% + var(--dx)));
    z-index: 20;
    width: max-content;
    max-width: 300px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 12px;
    background: var(--surface);
    color: var(--text);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: var(--shadow-lg);
    font-family: var(--font);
    font-size: 14px;
    font-weight: normal;
    line-height: 1.4;
    letter-spacing: normal;
    text-align: left;
    white-space: normal;
    pointer-events: none;
  }
  .tip:global(.above) {
    top: auto;
    bottom: calc(100% + 6px);
  }
  .entry {
    display: flex;
    flex-direction: column;
  }
  .py {
    font-weight: 600;
  }
  .defs {
    color: var(--text-2);
  }
  .none {
    color: var(--text-3);
  }
</style>
