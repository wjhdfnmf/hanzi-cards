<script lang="ts">
  // The console behind Settings: where syncing between devices is set up. It looks like an old
  // terminal on purpose; everything it does goes through the same commands as the side bar's
  // button (lib/sync.svelte.ts).
  import { onMount, tick } from "svelte";
  import { api, errorText } from "../lib/api";
  import { sync, refreshStatus, ping, syncNow, describe } from "../lib/sync.svelte";

  let { onclose }: { onclose: () => void } = $props();

  type Line = { text: string; kind?: "in" | "ok" | "err" | "dim" | "code" };
  let lines = $state<Line[]>([]);
  let entry = $state("");
  let working = $state(false);
  let booted = $state(false);
  let screen: HTMLDivElement | undefined = $state();
  let field: HTMLInputElement | undefined = $state();
  const past: string[] = [];
  let at = 0;

  const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));
  const calm = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  async function say(text: string | string[], kind?: Line["kind"], pace = 18) {
    for (const t of Array.isArray(text) ? text : [text]) {
      lines.push({ text: t, kind });
      await tick();
      screen?.scrollTo({ top: screen.scrollHeight });
      if (!calm && pace) await wait(pace);
    }
  }

  function role() {
    const s = sync.status;
    if (!s?.role) return "not set up";
    return s.role === "server" ? `server, ${s.online ? "online" : "offline"}, port ${s.port}` : `client of ${s.address}`;
  }

  onMount(async () => {
    await refreshStatus();
    await say(
      [
        "HANZI//LINK 1.0",
        "",
        "checking the lock ........ XChaCha20-Poly1305",
        "checking the line ........ TLS 1.3, pinned",
        `this device .............. ${role()}`,
        "",
      ],
      "dim",
      calm ? 0 : 90,
    );
    await say("You found the back room. Type `help`.", "ok");
    booted = true;
    await tick();
    field?.focus();
  });

  const HELP = [
    "  status                 what is set up here",
    "  host [port]            make this device the server (the port is optional)",
    "  join <address> <code>  pair this device with a server",
    "  code                   show the pairing code again, for another device",
    "  ping                   is the other side there?",
    "  ip                     this computer's local and public addresses",
    "  sync                   send what is new here, take what is new there",
    "  online | offline       (server) listen for your devices, or stop",
    "  forget                 undo the setup on this device (your cards stay)",
    "  clear | exit",
  ];

  function when(ts: number) {
    return ts ? new Date(ts * 1000).toLocaleString() : "never";
  }

  async function showCode(code: string) {
    await say("");
    await say(`  ${code}`, "code");
    await say("");
    await say(["Anyone with this code can read and change your cards: treat it like a password.", "On each other device, open this console and type:"], "dim");
  }

  async function run(raw: string) {
    const [name = "", ...args] = raw.trim().split(/\s+/);
    switch (name.toLowerCase()) {
      case "":
        return;
      case "help":
      case "?":
        return say(HELP);
      case "clear":
        lines = [];
        return;
      case "exit":
      case "quit":
        return onclose();
      case "status": {
        const s = await refreshStatus();
        if (!s?.role) return say("Nothing is set up. `host` makes this device the server; `join <address> <code>` pairs it with one.");
        await say(`role       ${s.role}`);
        if (s.role === "server") {
          await say(`listening  ${s.online ? `yes, port ${s.port}` : "no (type `online`)"}`);
          if (s.localAddress) await say(`address    ${s.localAddress}:${s.port}`);
          if (s.pending) await say("waiting    a device has left changes: type `sync` to take them in", "ok");
        } else {
          await say(`server     ${s.address}`);
        }
        await say(`last sync  ${when(s.lastSync)}${s.generation ? `  (box #${s.generation})` : ""}`);
        return;
      }
      case "host": {
        const port = args[0] ? Number(args[0]) : undefined;
        if (port !== undefined && !(Number.isInteger(port) && port >= 1024 && port <= 65535)) return say("The port is a number from 1024 to 65535.", "err");
        await say("making a certificate and keys ...", "dim");
        const code = await api.syncHost(port ?? null);
        const s = await refreshStatus();
        await say(`This device is now the server, listening on port ${s?.port}.`, "ok");
        await showCode(code);
        await say(`  join ${s?.localAddress ?? "<this computer's address>"}${s?.port && s.port !== 47810 ? `:${s.port}` : ""} <the code>`, "code");
        await say(["", "Your firewall may ask whether to let Hanzi Cards accept connections: allow it for private networks.", "Then type `sync` here once, so the box holds this device's cards."], "dim");
        return;
      }
      case "join": {
        if (args.length < 2) return say("join <address> <code>   for example: join 192.168.1.20 AB12C-…", "err");
        await say(`calling ${args[0]} ...`, "dim");
        await api.syncJoin(args[0], args.slice(1).join(""));
        await refreshStatus();
        await ping().catch(() => {});
        await say("Paired. The server showed the right certificate and took the code.", "ok");
        await say("Type `sync` to bring the two together. Nothing has been changed yet.", "dim");
        return;
      }
      case "code": {
        const code = await api.syncCode();
        const s = sync.status;
        await showCode(code);
        const where = s?.role === "server" ? `${s.localAddress ?? "<this computer's address>"}${s.port !== 47810 ? `:${s.port}` : ""}` : s?.address;
        await say(`  join ${where} <the code>`, "code");
        return;
      }
      case "ping": {
        const p = await ping();
        return p.ok
          ? say(sync.status?.role === "server" ? `● listening (box #${p.generation})` : `● the server answered in ${p.ms} ms (box #${p.generation})`, "ok")
          : say(`● ${p.error}`, "err");
      }
      case "ip": {
        await say("asking the network ... (the public address comes from asking an outside service what address this computer appears from)", "dim");
        const a = await api.syncAddresses();
        const port = a.port === 47810 ? "" : `:${a.port}`;
        await say(`  local    ${a.local ?? "not on a network"}`, a.local ? "ok" : "err");
        await say(`  public   ${a.public ?? `unknown (${a.publicError})`}`, a.public ? "ok" : "err");
        await say("");
        if (a.local) await say(`  same network:  join ${a.local}${port} <the code>`, "dim");
        if (a.public) await say([`  from outside:  join ${a.public}${port} <the code>`, `                 (your router must forward port ${a.port} to this computer)`], "dim");
        return;
      }
      case "sync": {
        await say("reading this device ... fetching the box ... merging ...", "dim");
        const r = await syncNow();
        await say(`Done. Box #${r.generation}.`, "ok");
        await say([
          `  came in    ${describe(r.received)}`,
          `  went out   ${describe(r.sent)}`,
          `  here now   ${r.cards.toLocaleString()} cards · ${r.reviews.toLocaleString()} answers · ${r.stories.toLocaleString()} stories`,
        ]);
        return;
      }
      case "online":
      case "offline": {
        const s = await api.syncOnline(name.toLowerCase() === "online");
        await refreshStatus();
        return say(s.online ? `● listening on port ${s.port}` : "○ offline: other devices can't reach this one", s.online ? "ok" : "dim");
      }
      case "forget": {
        if (args[0] !== "yes") {
          return say(["This removes the keys and the pairing from this device (and, on the server, the box).", "Your cards, stories and history stay exactly as they are.", "Type `forget yes` to go ahead."], "dim");
        }
        await api.syncForget();
        await refreshStatus();
        return say("Forgotten. This device is on its own again.", "ok");
      }
      // (a console needs a few of these)
      case "sudo":
        return say("Nice try.");
      case "ls":
        return say("cards/  stories/  secrets/ (no, not really)");
      case "whoami":
        return say("学生");
      case "你好":
      case "hello":
      case "hi":
        return say("你好！");
      default:
        return say(`Unknown command: ${name}. Type \`help\`.`, "err");
    }
  }

  async function submit() {
    const raw = entry;
    if (working || !raw.trim()) return;
    entry = "";
    past.push(raw);
    at = past.length;
    // (a pairing code is a password: it isn't echoed back onto the screen)
    const shown = /^\s*join\s+\S+\s+\S/i.test(raw) ? raw.trim().split(/\s+/).slice(0, 2).join(" ") + " ·····" : raw;
    await say(`link> ${shown}`, "in", 0);
    working = true;
    try {
      await run(raw);
    } catch (e) {
      await say(errorText(e), "err");
    } finally {
      working = false;
      await tick();
      field?.focus();
    }
  }

  function keys(e: KeyboardEvent) {
    if (e.key === "ArrowUp" && at > 0) {
      e.preventDefault();
      entry = past[--at];
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      at = Math.min(past.length, at + 1);
      entry = past[at] ?? "";
    }
  }
</script>

<svelte:window
  onkeydown={(e) => {
    if (e.key === "Escape") onclose();
  }}
/>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="veil" onclick={(e) => e.target === e.currentTarget && onclose()}>
  <div class="tube" role="dialog" tabindex="-1" aria-modal="true" aria-label="Console" onclick={() => !window.getSelection()?.toString() && field?.focus()}>
    <div class="bar">
      <span class="lamp" class:ok={sync.light === "ok"} class:bad={sync.light === "bad"}></span>
      <span class="title">HANZI//LINK</span>
      <button class="x" onclick={onclose} aria-label="Close">×</button>
    </div>
    <div class="screen" bind:this={screen}>
      {#each lines as l, i (i)}<div class="line {l.kind ?? ''}">{l.text || " "}</div>{/each}
      {#if booted}
        <form
          class="prompt"
          onsubmit={(e) => {
            e.preventDefault();
            submit();
          }}
        >
          <span>link&gt;</span>
          <input
            bind:this={field}
            bind:value={entry}
            onkeydown={keys}
            disabled={working}
            spellcheck="false"
            autocomplete="off"
            autocapitalize="off"
            aria-label="Command"
          />
          {#if working}<span class="spin" aria-hidden="true">▒</span>{/if}
        </form>
      {/if}
    </div>
  </div>
</div>

<style>
  .veil {
    position: fixed;
    inset: 0;
    z-index: 200;
    display: grid;
    place-items: center;
    background: rgb(0 0 0 / 0.72);
    backdrop-filter: blur(3px);
    animation: on 0.25s ease-out;
  }
  .tube {
    --ink: #7dffa0;
    --dim: #3fae63;
    position: relative;
    width: min(820px, 94vw);
    height: min(560px, 86vh);
    display: flex;
    flex-direction: column;
    border-radius: 14px;
    overflow: hidden;
    background: radial-gradient(120% 90% at 50% 40%, #06180d 0%, #020a05 70%, #010402 100%);
    border: 1px solid #1c5a33;
    box-shadow:
      0 0 0 6px #0a0f0c,
      0 0 0 7px #24302a,
      0 30px 80px rgb(0 0 0 / 0.7),
      0 0 60px rgb(60 255 130 / 0.12);
    color: var(--ink);
    font:
      14px/1.55 ui-monospace,
      "Cascadia Mono",
      "SF Mono",
      Consolas,
      "DejaVu Sans Mono",
      monospace;
    text-shadow: 0 0 6px rgb(90 255 150 / 0.45);
  }
  /* scan lines, and a slow band of light going down the glass */
  .tube::before,
  .tube::after {
    content: "";
    position: absolute;
    inset: 0;
    pointer-events: none;
    z-index: 2;
  }
  .tube::before {
    background: repeating-linear-gradient(to bottom, rgb(0 0 0 / 0) 0, rgb(0 0 0 / 0) 2px, rgb(0 0 0 / 0.22) 3px);
  }
  .tube::after {
    background: linear-gradient(to bottom, transparent, rgb(120 255 170 / 0.05) 50%, transparent);
    height: 35%;
    animation: sweep 7s linear infinite;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 12px;
    border-bottom: 1px solid #14432a;
    background: rgb(20 80 45 / 0.25);
    letter-spacing: 0.18em;
    font-size: 12px;
  }
  .title {
    flex: 1;
  }
  .lamp {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: #2c4a38;
  }
  .lamp.ok {
    background: #5dff8f;
    box-shadow: 0 0 8px #5dff8f;
  }
  .lamp.bad {
    background: #ff5d5d;
    box-shadow: 0 0 8px #ff5d5d;
  }
  .x {
    position: relative;
    z-index: 3;
    border: none;
    background: none;
    color: var(--dim);
    font: inherit;
    font-size: 18px;
    line-height: 1;
    cursor: pointer;
    padding: 0 4px;
  }
  .x:hover {
    color: var(--ink);
  }
  .screen {
    flex: 1;
    overflow-y: auto;
    padding: 14px 18px 18px;
    user-select: text;
    scrollbar-color: #1c5a33 transparent;
  }
  .line {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .line.dim {
    color: var(--dim);
  }
  .line.in {
    color: #c8ffd9;
  }
  .line.ok {
    color: #b6ffcd;
    font-weight: 600;
  }
  .line.err {
    color: #ff8a7a;
    text-shadow: 0 0 6px rgb(255 110 90 / 0.45);
  }
  .line.code {
    color: #ffe58a;
    text-shadow: 0 0 6px rgb(255 220 110 / 0.45);
    letter-spacing: 0.04em;
  }
  .prompt {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 2px;
  }
  .prompt input {
    position: relative;
    z-index: 3;
    flex: 1;
    min-width: 0;
    border: none;
    outline: none;
    background: transparent;
    color: #c8ffd9;
    font: inherit;
    text-shadow: inherit;
    caret-color: var(--ink);
    caret-shape: block;
    padding: 0;
    box-shadow: none;
  }
  .spin {
    animation: blink 0.5s steps(2) infinite;
  }
  @keyframes sweep {
    from {
      transform: translateY(-100%);
    }
    to {
      transform: translateY(320%);
    }
  }
  @keyframes blink {
    50% {
      opacity: 0;
    }
  }
  @keyframes on {
    from {
      opacity: 0;
      transform: scaleY(0.02);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .tube::after,
    .spin,
    .veil {
      animation: none;
    }
  }
</style>
