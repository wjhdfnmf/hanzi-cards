<script lang="ts">
  // Settings › Speech: calibrate tone detection. Shows the learner's voice range and the
  // detector's settings; the calibration records a sentence and the four tones of "ma", with
  // a live pitch trace while recording and the analysis (pitch track, the syllable's shape
  // against the tone shapes, how sure it is) after each one.
  import { onMount } from "svelte";
  import { api, errorText, type PitchAnalysis, type PitchEarlier, type PitchProfileInfo, type PitchVoice } from "../lib/api";
  import { recordUtterance } from "../lib/mic";
  import { LivePitch, hzToSt, noteName, stToHz, type LivePoint } from "../lib/livepitch";
  import { settings, saveSetting } from "../lib/settings.svelte";
  import { showToast } from "../lib/toast.svelte";
  import { t } from "../lib/i18n.svelte";

  interface Step {
    hanzi: string;
    pinyin: string;
    /** null: a sentence, used for the range only */
    tone: number | null;
  }
  const STEPS: Step[] = [
    { hanzi: "妈妈骑马，马慢，妈妈骂马。", pinyin: "Māma qí mǎ, mǎ màn, māma mà mǎ.", tone: null },
    { hanzi: "妈", pinyin: "mā", tone: 1 },
    { hanzi: "麻", pinyin: "má", tone: 2 },
    { hanzi: "马", pinyin: "mǎ", tone: 3 },
    { hanzi: "骂", pinyin: "mà", tone: 4 },
  ];

  let info = $state<PitchProfileInfo | null>(null);
  let calibrating = $state(false);
  let step = $state(0);
  let results = $state<(PitchAnalysis | null)[]>([]);
  let phase = $state<"idle" | "waiting" | "listening" | "analysing">("idle");
  let saving = $state(false);
  let justSaved = $state(false);
  let level = $state(0);
  /** "Speak now" shows for a moment when listening starts. */
  let speakCue = $state(false);
  /** The quiet wait before listening (ms). */
  const WAIT_MS = 1200;

  onMount(async () => {
    info = await api.pitchProfileInfo().catch(() => null);
  });

  const current = $derived(results[step] ?? null);
  /** The range to draw against: this calibration's so far, else the saved one. */
  const liveVoice = $derived<PitchVoice | null>(
    [...results.slice(0, step + 1)].reverse().find((r) => r?.voice)?.voice ?? info?.voice ?? null,
  );

  function start() {
    calibrating = true;
    justSaved = false;
    step = 0;
    results = STEPS.map(() => null);
  }

  function cancel() {
    abort?.abort();
    calibrating = false;
    phase = "idle";
  }

  // ---------- recording ----------

  let abort: AbortController | null = null;
  let stopper: AbortController | null = null;
  let canvas = $state<HTMLCanvasElement | null>(null);
  let livePoints: LivePoint[] = [];
  let frame = 0;

  async function record() {
    const s = STEPS[step];
    const thisAbort = new AbortController();
    abort = thisAbort;
    stopper = new AbortController();
    livePoints = [];
    const tracker = new LivePitch();
    phase = "waiting";
    results[step] = null;
    // if the microphone never delivers sound, don't sit on "Stay quiet" forever
    const watchdog = setTimeout(() => {
      if (phase === "waiting" && abort === thisAbort) {
        thisAbort.abort();
        showToast(t("calib.micStuck"), "err");
      }
    }, WAIT_MS + 4000);
    try {
      const pcm = await recordUtterance({
        // a quiet second before listening: the click on "Record" isn't part of it, and the
        // second half measures the room's noise
        skipMs: WAIT_MS,
        // mains hum heard in the quiet wait is filtered out of the live trace too
        onNoise: (quiet) => tracker.learnNoise(quiet),
        onReady: () => {
          phase = "listening";
          speakCue = true;
          setTimeout(() => (speakCue = false), 1600);
        },
        trimEndMs: 250,
        // the whole recording counts: nothing cut from the front, and it only stops by itself
        // after 2 s of quiet (pauses and a quiet low 3rd tone don't end it), or on "Stop"
        keepAll: true,
        raw: true,
        startTimeoutMs: 8000,
        silenceMs: 2000,
        maxMs: s.tone === null ? 15000 : 8000,
        signal: abort.signal,
        stopSignal: stopper.signal,
        onLevel: (l) => (level = l),
        onSamples: (x) => {
          livePoints.push(...tracker.push(x));
          if (livePoints.length > 600) livePoints.splice(0, livePoints.length - 600);
          cancelAnimationFrame(frame);
          frame = requestAnimationFrame(drawLive);
        },
      });
      level = 0;
      if (!pcm) {
        phase = "idle";
        if (!thisAbort.signal.aborted) showToast(t("calib.nothingHeard"), "err");
        return;
      }
      phase = "analysing";
      // debug mode only: kept in the app-data folder, to check the pitch tracker on real voices
      if (settings.debugMode || import.meta.env.DEV) api.pitchSaveRecording(pcm, `step${step + 1}-${s.tone ? `tone${s.tone}` : "sentence"}`).catch(() => {});
      const a = await api.pitchAnalyze(pcm, s.tone, keptSoFar(step));
      // a 4th tone can have as little as 0.1 s of clear pitch before it turns to creak: only
      // turn away what can't be measured at all (the shape needs 5 frames, 50 ms)
      if (a.voicedMs < 50) {
        showToast(t("calib.tooLittleVoice"), "err");
        phase = "idle";
        return;
      }
      results[step] = a;
      phase = "idle";
    } catch (e) {
      phase = "idle";
      showToast(errorText(e), "err");
    } finally {
      clearTimeout(watchdog);
    }
  }

  /** The recordings of the steps before `upTo`, as the analysis wants them. */
  function keptSoFar(upTo: number): PitchEarlier[] {
    return results
      .slice(0, upTo)
      .map((r, i) => (r ? { pitch: r.pitch, tone: STEPS[i].tone, length: r.lengthFrames } : null))
      .filter((r): r is PitchEarlier => !!r);
  }

  function next() {
    if (step < STEPS.length - 1) step += 1;
  }

  async function finish() {
    saving = true;
    try {
      info = await api.pitchCalibrationSave(keptSoFar(STEPS.length));
      calibrating = false;
      justSaved = true;
    } catch (e) {
      showToast(errorText(e), "err");
    } finally {
      saving = false;
    }
  }

  async function reset() {
    try {
      info = await api.pitchProfileReset();
      justSaved = false;
    } catch (e) {
      showToast(errorText(e), "err");
    }
  }

  // ---------- drawing helpers ----------

  const css = (name: string) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  const toneVar = (tone: number | null) => (tone ? `var(--tone-${tone})` : "var(--series-1)");

  /** Chao value (-2 = bottom of the voice .. +2 = top) of a pitch in semitones. */
  const chao = (st: number, v: PitchVoice) => (st - v.medianSt) / (v.spreadSt / 4);
  /** Semitones of a Chao value. */
  const fromChao = (c: number, v: PitchVoice) => v.medianSt + (c * v.spreadSt) / 4;

  /** The live trace: a scrolling pitch line over the voice's five levels, with the loudness. */
  function drawLive() {
    const c = canvas;
    if (!c) return;
    const dpr = window.devicePixelRatio || 1;
    const w = c.clientWidth;
    const h = c.clientHeight;
    if (c.width !== Math.round(w * dpr)) {
      c.width = Math.round(w * dpr);
      c.height = Math.round(h * dpr);
    }
    const g = c.getContext("2d")!;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, w, h);
    const v = liveVoice;
    const pts = livePoints;
    // vertical scale: the voice's range with room around it (75-400 Hz before there is one),
    // grown to fit everything heard, so a deep 3rd tone stays on the chart
    const heard = pts.filter((p) => p.st !== null).map((p) => p.st as number);
    const [baseLo, baseHi] = v ? [fromChao(-3, v), fromChao(3, v)] : [hzToSt(75), hzToSt(400)];
    const lo = Math.min(baseLo, ...heard.map((st) => st - 1));
    const hi = Math.max(baseHi, ...heard.map((st) => st + 1));
    const y = (st: number) => h - 14 - ((st - lo) / (hi - lo)) * (h - 24);
    // the five levels
    g.font = "10px system-ui, sans-serif";
    for (let k = -2; k <= 2; k++) {
      const st = v ? fromChao(k, v) : lo + ((k + 2.5) / 5) * (hi - lo);
      const yy = y(st);
      g.strokeStyle = css("--grid");
      g.lineWidth = 1;
      g.setLineDash(k === 0 ? [] : [3, 4]);
      g.beginPath();
      g.moveTo(22, yy);
      g.lineTo(w, yy);
      g.stroke();
      g.fillStyle = css("--text-3");
      g.fillText(String(k + 3), 6, yy + 3);
    }
    g.setLineDash([]);
    const span = 250; // 5 seconds of 20 ms points across the width
    const dx = (w - 26) / span;
    const x0 = w - pts.length * dx;
    // loudness as soft bars along the bottom
    const maxRms = Math.max(0.02, ...pts.map((p) => p.rms));
    g.fillStyle = css("--surface-3");
    pts.forEach((p, i) => {
      const bh = (p.rms / maxRms) * 12;
      g.fillRect(x0 + i * dx, h - bh, Math.max(1, dx - 0.5), bh);
    });
    // the pitch: a glowing line in the colour of the tone being asked for
    const colour = css(STEPS[step].tone ? `--tone-${STEPS[step].tone}` : "--series-1");
    // voice with no clear pitch (creak, consonants): dots along the bottom, so a gap in the
    // line reads as "voice, but no pitch to measure" rather than a fault
    g.fillStyle = colour;
    g.globalAlpha = 0.45;
    pts.forEach((p, i) => {
      if (p.st === null && p.rms >= maxRms * 0.15) {
        g.beginPath();
        g.arc(x0 + i * dx, h - 20, 1.6, 0, Math.PI * 2);
        g.fill();
      }
    });
    g.globalAlpha = 1;
    g.strokeStyle = colour;
    g.lineWidth = 3;
    g.lineCap = "round";
    g.lineJoin = "round";
    g.shadowColor = colour;
    g.shadowBlur = 10;
    g.beginPath();
    let drawing = false;
    pts.forEach((p, i) => {
      if (p.st === null) {
        drawing = false;
        return;
      }
      const xx = x0 + i * dx;
      if (drawing) g.lineTo(xx, y(p.st));
      else g.moveTo(xx, y(p.st));
      drawing = true;
    });
    g.stroke();
    g.shadowBlur = 0;
    // the latest point, with its frequency
    const last = [...pts].reverse().find((p) => p.st !== null);
    if (last?.st != null) {
      const hz = stToHz(last.st);
      g.fillStyle = colour;
      g.beginPath();
      g.arc(w - 4, y(last.st), 4, 0, Math.PI * 2);
      g.fill();
      g.fillStyle = css("--text-2");
      g.textAlign = "right";
      g.fillText(`${Math.round(hz)} Hz · ${noteName(hz)}`, w - 12, Math.max(12, y(last.st) - 8));
      g.textAlign = "left";
    }
  }

  $effect(() => {
    // clear the live canvas when moving to another step
    void step;
    livePoints = [];
    if (canvas) requestAnimationFrame(drawLive);
  });

  // ---------- the analysis chart (SVG) ----------

  const CW = 520;
  const CH = 200;
  const PAD = { l: 28, r: 54, t: 10, b: 22 };

  function analysisChart(a: PitchAnalysis) {
    const v = a.voice!;
    const n = a.pitch.length;
    const x = (i: number) => PAD.l + (i / Math.max(1, n - 1)) * (CW - PAD.l - PAD.r);
    // vertical range: the voice's five levels with room around them, grown to fit every
    // pitch (a deep 3rd tone can go well below the voice's usual bottom)
    const cs = a.pitch.filter((st): st is number => st !== null).map((st) => chao(st, v));
    const top = Math.max(3, ...cs.map((c) => c + 0.4), ...(a.contour ?? []).map((c) => c + 0.4));
    const bottom = Math.min(-3, ...cs.map((c) => c - 0.4), ...(a.contour ?? []).map((c) => c - 0.4));
    const y = (c: number) => PAD.t + ((top - c) / (top - bottom)) * (CH - PAD.t - PAD.b);
    // the pitch track as path pieces
    let d = "";
    let on = false;
    a.pitch.forEach((st, i) => {
      if (st === null) return void (on = false);
      d += `${on ? "L" : "M"}${x(i).toFixed(1)},${y(chao(st, v)).toFixed(1)}`;
      on = true;
    });
    const levels = [-2, -1, 0, 1, 2].map((k) => ({ k, y: y(k), hz: stToHz(fromChao(k, v)) }));
    const bars = a.level.map((l, i) => ({ x: x(i), h: l * 16 }));
    let seg = null as null | {
      x0: number;
      x1: number;
      m0: number;
      m1: number;
      creak: string;
      contour: string;
      dots: { x: number; y: number }[];
      template: string;
    };
    if (a.segment && a.contour && a.measured) {
      const [s, e] = a.segment;
      const [m0, m1] = a.measured;
      // the 5 points are spread over the measured frames
      const xs = [0.1, 0.3, 0.5, 0.7, 0.9].map((q) => x(m0 + q * (m1 - m0)));
      const dots = a.contour.map((c, i) => ({ x: xs[i], y: y(c) }));
      const tone = STEPS[step].tone;
      const tpl = a.profile.params.templates.find(([tt]) => tt === tone)?.[1];
      seg = {
        x0: x(s),
        x1: x(e),
        m0: x(m0),
        m1: x(Math.min(n - 1, m1)),
        // a creaky ending, counted as the bottom of the voice
        creak: a.creakFrames ? `M${x(e).toFixed(1)},${y(-2).toFixed(1)}L${x(Math.min(n - 1, e + a.creakFrames)).toFixed(1)},${y(-2).toFixed(1)}` : "",
        contour: dots.map((p, i) => `${i ? "L" : "M"}${p.x.toFixed(1)},${p.y.toFixed(1)}`).join(""),
        dots,
        template: tpl ? tpl.map((c, i) => `${i ? "L" : "M"}${xs[i].toFixed(1)},${y(c).toFixed(1)}`).join("") : "",
      };
    }
    return { d, levels, bars, seg, secs: (n * a.profile.params.hopMs) / 1000, plotTop: PAD.t, plotH: CH - PAD.t - PAD.b };
  }

  /** A small drawing of a tone shape (5 points, -2..+2) in a w×h box. */
  function shapePath(points: number[], w: number, h: number) {
    return points.map((c, i) => `${i ? "L" : "M"}${(4 + (i / 4) * (w - 8)).toFixed(1)},${(4 + ((2.2 - c) / 4.4) * (h - 8)).toFixed(1)}`).join("");
  }
  const toneShapes = $derived.by(() => {
    const tpl = info?.params.templates ?? [];
    // one shape per tone (the full 214 for tone 3)
    return [1, 2, 3, 4].map((tone) => ({ tone, points: tpl.find(([tt]) => tt === tone)?.[1] ?? [] }));
  });

  // ---------- the voice range (histogram of the profile) ----------

  function rangeChart(p: PitchProfileInfo) {
    const W = 520;
    const H = 110;
    const { profileLowSt: low, profileBinSt: step_ } = p.params;
    const maxBin = Math.max(1e-6, ...p.bins);
    // show 70-500 Hz
    const [lo, hi] = [hzToSt(70), hzToSt(500)];
    const x = (st: number) => 8 + ((st - lo) / (hi - lo)) * (W - 16);
    const base = H - 26;
    let d = `M${x(lo)},${base}`;
    p.bins.forEach((b, k) => {
      const st = low + (k + 0.5) * step_;
      if (st < lo || st > hi) return;
      d += `L${x(st).toFixed(1)},${(base - (b / maxBin) * (base - 8)).toFixed(1)}`;
    });
    d += `L${x(hi)},${base}Z`;
    const ticks = [80, 100, 150, 200, 300, 400].map((hz) => ({ x: x(hzToSt(hz)), hz }));
    const v = p.voice;
    return {
      W,
      H,
      d,
      base,
      ticks,
      band: v ? { x0: x(hzToSt(v.lowHz)), x1: x(hzToSt(v.highHz)), med: x(v.medianSt) } : null,
    };
  }

  const pct = (x: number) => `${Math.round(x * 100)}%`;
</script>

<div class="calib">
  <div class="head">
    <div>
      <strong>{t("calib.title")}</strong>
      <small class="faint block">
        {#if info?.ready}
          {t("calib.statusReady", { n: info.recordings })}
        {:else}
          {t("calib.statusNeeds", { n: info?.recordings ?? 0, need: info?.params.minRecordings ?? 5 })}
        {/if}
      </small>
    </div>
    {#if !calibrating}
      <span class="actions">
        {#if info?.recordings}
          <button class="btn ghost small" onclick={reset}>{t("calib.reset")}</button>
        {/if}
        <button class="btn small primary" onclick={start}>{info?.recordings ? t("calib.recalibrate") : t("calib.start")}</button>
      </span>
    {/if}
  </div>

  {#if justSaved && !settings.pitchTones}
    <div class="notice ok small turn-on">
      <span>{t("calib.savedTurnOn")}</span>
      <button class="btn small" onclick={() => saveSetting("pitchTones", true)}>{t("calib.turnOn")}</button>
    </div>
  {:else if justSaved}
    <div class="notice ok small">{t("calib.saved")}</div>
  {/if}

  {#if calibrating}
    {@const s = STEPS[step]}
    <div class="wizard">
      <ol class="dots" aria-label={t("calib.progress")}>
        {#each STEPS as st, i (i)}
          <li class:done={!!results[i]} class:here={i === step} style:--c={toneVar(st.tone)}>
            <button onclick={() => phase === "idle" && (step = i)} disabled={phase !== "idle" || (i > 0 && !results[i - 1] && i !== step)}>
              {st.tone ? st.hanzi : "…"}
            </button>
          </li>
        {/each}
      </ol>

      <div class="prompt" style:--c={toneVar(s.tone)}>
        <div class="say">{t(s.tone ? "calib.sayWord" : "calib.saySentence")}</div>
        <div class="hanzi" class:sentence={!s.tone}>{s.hanzi}</div>
        <div class="pinyin">{s.pinyin}{#if s.tone}<span class="tone-tag">{t("calib.toneN", { n: s.tone })}</span>{/if}</div>
        {#if s.tone}
          <svg class="shape-hint" viewBox="0 0 60 36" aria-hidden="true">
            <path d={shapePath(toneShapes[s.tone - 1]?.points ?? [], 60, 36)} />
          </svg>
        {/if}
      </div>

      <div class="live" class:active={phase === "listening"}>
        <canvas bind:this={canvas}></canvas>
        {#if phase === "listening"}
          <div class="meter" style:transform="scaleX({level})"></div>
        {/if}
        {#if phase === "waiting"}
          <div class="live-hint quiet">
            <div>
              <div class="quiet-title">🤫 {t("calib.stayQuiet")}</div>
              <div class="small muted">{t("calib.measuringNoise")}</div>
              <div class="quiet-bar"><span style:animation-duration="{WAIT_MS}ms"></span></div>
            </div>
          </div>
        {:else if phase === "listening" && speakCue}
          <div class="speak-now">🎙 {t("calib.speakNow")}</div>
        {:else if phase !== "listening" && !current}
          <div class="live-hint faint small">{t("calib.liveHint")}</div>
        {/if}
        {#if phase === "listening"}
          <div class="legend faint">{t("calib.legendNoPitch")}</div>
        {/if}
      </div>

      <div class="controls">
        {#if phase === "waiting"}
          <button class="btn" onclick={() => abort?.abort()}>{t("calib.cancel")}</button>
        {:else if phase === "listening"}
          <button class="btn" onclick={() => stopper?.abort()}><span class="rec-dot"></span>{t("calib.stop")}</button>
        {:else if phase === "analysing"}
          <button class="btn" disabled>{t("calib.analysing")}</button>
        {:else}
          <button class="btn" class:primary={!current} onclick={record}>{current ? t("calib.again") : t("calib.record")}</button>
        {/if}
        <span class="grow"></span>
        <button class="btn ghost" onclick={cancel}>{t("calib.cancel")}</button>
        {#if step < STEPS.length - 1}
          <button class="btn" class:primary={!!current} disabled={!current || phase !== "idle"} onclick={next}>{t("calib.next")}</button>
        {:else}
          <button class="btn" class:primary={!!current} disabled={!current || phase !== "idle" || saving} onclick={finish}>{t("calib.save")}</button>
        {/if}
      </div>

      {#if current?.voice}
        {@const ch = analysisChart(current)}
        <div class="analysis">
          <svg class="track" viewBox="0 0 {CW} {CH}" role="img" aria-label={t("calib.trackLabel")}>
            {#each ch.levels as lv (lv.k)}
              <line x1={PAD.l} x2={CW - PAD.r} y1={lv.y} y2={lv.y} class="grid" class:mid={lv.k === 0} />
              <text x={PAD.l - 8} y={lv.y + 3} class="axis" text-anchor="end">{lv.k + 3}</text>
              <text x={CW - PAD.r + 6} y={lv.y + 3} class="axis">{Math.round(lv.hz)} Hz</text>
            {/each}
            {#each ch.bars as b, i (i)}
              <rect x={b.x} y={CH - b.h - 2} width="2" height={b.h} class="lvl" />
            {/each}
            {#if ch.seg}
              <!-- the syllable, and inside it the part the tone was measured from -->
              <rect x={ch.seg.x0} y={ch.plotTop} width={Math.max(1, ch.seg.x1 - ch.seg.x0)} height={ch.plotH} class="seg" />
              <rect
                x={ch.seg.m0}
                y={ch.plotTop}
                width={Math.max(2, ch.seg.m1 - ch.seg.m0)}
                height={ch.plotH}
                class="measured"
                style:fill={toneVar(s.tone)}
                style:stroke={toneVar(s.tone)}
              />
              <text x={(ch.seg.m0 + ch.seg.m1) / 2} y={ch.plotTop + 11} class="measured-label" text-anchor="middle" style:fill={toneVar(s.tone)}
                >{t("calib.analysed")}</text
              >
              {#if ch.seg.template}<path d={ch.seg.template} class="template" style:stroke={toneVar(s.tone)} />{/if}
              <clipPath id="calib-measured"><rect x={ch.seg.m0} y="0" width={Math.max(2, ch.seg.m1 - ch.seg.m0)} height={CH} /></clipPath>
            {/if}
            <!-- the whole pitch track faint, the measured part strong -->
            <path d={ch.d} class="pitch" class:dim={!!ch.seg} style:stroke={toneVar(s.tone)} />
            {#if ch.seg}
              <path d={ch.d} class="pitch" clip-path="url(#calib-measured)" style:stroke={toneVar(s.tone)} />
              {#if ch.seg.creak}<path d={ch.seg.creak} class="creak" style:stroke={toneVar(s.tone)} />{/if}
              <path d={ch.seg.contour} class="contour" />
              {#each ch.seg.dots as p, i (i)}<circle cx={p.x} cy={p.y} r="4.5" class="dot" />{/each}
            {/if}
            <text x={PAD.l} y={CH - 6} class="axis">0 s</text>
            <text x={CW - PAD.r} y={CH - 6} class="axis" text-anchor="end">{ch.secs.toFixed(1)} s</text>
          </svg>

          <div class="side">
            {#if current.probs && s.tone}
              <div class="verdict" class:ok={current.heard === s.tone}>
                {#if current.heard === s.tone}
                  {t("calib.heardRight", { n: s.tone })}
                {:else}
                  {t("calib.heardOther", { n: current.heard ?? "?" })}
                {/if}
              </div>
              <div class="probs">
                {#each current.probs as p, i (i)}
                  <div class="prob" class:want={i + 1 === s.tone} style:--c="var(--tone-{i + 1})">
                    <svg viewBox="0 0 40 24" aria-hidden="true"><path d={shapePath(toneShapes[i]?.points ?? [], 40, 24)} /></svg>
                    <span class="bar"><span style:width={pct(p)}></span></span>
                    <span class="num">{pct(p)}</span>
                  </div>
                {/each}
              </div>
            {:else}
              <div class="verdict ok">{t("calib.rangeHeard")}</div>
            {/if}
            <dl class="stats">
              <dt>{t("calib.voiced")}</dt><dd>{(current.voicedMs / 1000).toFixed(2)} s</dd>
              {#if current.lengthFrames}
                <dt>{t("calib.length")}</dt>
                <dd>{(current.lengthFrames / 100).toFixed(2)} s <span class="faint">/ {(current.typicalLength / 100).toFixed(2)} s</span></dd>
              {/if}
              <dt>{t("calib.median")}</dt><dd>{Math.round(current.voice.medianHz)} Hz · {noteName(current.voice.medianHz)}</dd>
              <dt>{t("calib.range")}</dt><dd>{Math.round(current.voice.lowHz)}–{Math.round(current.voice.highHz)} Hz</dd>
              <dt>{t("calib.spread")}</dt><dd>{current.voice.spreadSt.toFixed(1)} {t("calib.semitones")}</dd>
              <dt>{t("calib.hum")}</dt><dd>{current.humHz.length ? current.humHz.map((f) => `${Math.round(f)} Hz`).join(", ") : t("calib.humNone")}</dd>
            </dl>
          </div>
        </div>
        {#if s.tone && current.advice.length}
          <ul class="advice" style:--c={toneVar(s.tone)}>
            {#each current.advice as adv (adv.key)}
              <li>
                {t(`calib.advice.${adv.key}` as Parameters<typeof t>[0], {
                  target: Math.round(adv.targetHz),
                  actual: Math.round(adv.actualHz),
                })}
              </li>
            {/each}
          </ul>
        {/if}
        {#if s.tone && current.heard !== s.tone}
          <p class="small muted tip">{t("calib.tip")}</p>
        {/if}
      {/if}
    </div>
  {/if}

  {#if info && !calibrating}
    {#if info.voice}
      {@const rc = rangeChart(info)}
      <div class="range">
        <div class="range-head small">
          <span>{t("calib.yourVoice")}</span>
          <strong>{Math.round(info.voice.medianHz)} Hz · {noteName(info.voice.medianHz)}</strong>
          <span class="faint">{t("calib.toneRange", { lo: Math.round(info.voice.lowHz), hi: Math.round(info.voice.highHz) })}</span>
        </div>
        <svg viewBox="0 0 {rc.W} {rc.H}" role="img" aria-label={t("calib.rangeLabel")}>
          <defs>
            <linearGradient id="calib-fill" x1="0" x2="0" y1="0" y2="1">
              <stop offset="0" stop-color="var(--series-1)" stop-opacity="0.55" />
              <stop offset="1" stop-color="var(--series-1)" stop-opacity="0.05" />
            </linearGradient>
          </defs>
          {#if rc.band}
            <rect x={rc.band.x0} y="4" width={rc.band.x1 - rc.band.x0} height={rc.base - 4} class="band" />
            {#each [1, 2, 3, 4, 5] as lv (lv)}
              {@const xx = rc.band.x0 + ((lv - 1) / 4) * (rc.band.x1 - rc.band.x0)}
              <line x1={xx} x2={xx} y1="4" y2={rc.base} class="chao" />
              <text x={xx} y="14" class="axis" text-anchor="middle">{lv}</text>
            {/each}
          {/if}
          <path d={rc.d} fill="url(#calib-fill)" class="hist" />
          {#if rc.band}<line x1={rc.band.med} x2={rc.band.med} y1="4" y2={rc.base} class="median" />{/if}
          <line x1="8" x2={rc.W - 8} y1={rc.base} y2={rc.base} class="grid" />
          {#each rc.ticks as tk (tk.hz)}
            <line x1={tk.x} x2={tk.x} y1={rc.base} y2={rc.base + 4} class="grid" />
            <text x={tk.x} y={rc.base + 15} class="axis" text-anchor="middle">{tk.hz}</text>
          {/each}
          <text x={rc.W - 8} y={rc.H - 1} class="axis" text-anchor="end">Hz</text>
        </svg>
      </div>
    {/if}

    <details class="params">
      <summary class="small">{t("calib.params")}</summary>
      <div class="param-grid small">
        <span>{t("calib.p.tracker")}</span><span>YIN · {info.params.windowMs} ms / {info.params.hopMs} ms</span>
        <span>{t("calib.p.f0")}</span><span>{info.params.f0MinHz}–{info.params.f0MaxHz} Hz</span>
        <span>{t("calib.p.thresholds")}</span><span>{info.params.yinFirst} / {info.params.yinGlobal}</span>
        <span>{t("calib.p.lowPass")}</span><span>{info.params.lowPassHz} Hz</span>
        <span>{t("calib.p.shapeWeight")}</span><span>× {info.params.shapeWeight}</span>
        <span>{t("calib.p.fallWeight")}</span><span>× {info.params.fallWeight}</span>
        <span>{t("calib.p.creak")}</span><span>{pct(info.params.creak)}</span>
        <span>{t("calib.p.decay")}</span><span>{info.params.profileDecay}</span>
        <span>{t("calib.p.min")}</span><span>{info.params.minRecordings}</span>
      </div>
      <div class="shapes">
        {#each toneShapes as sh (sh.tone)}
          <div class="shape" style:--c="var(--tone-{sh.tone})">
            <svg viewBox="0 0 60 36" aria-hidden="true">
              {#each [-2, 0, 2] as g (g)}<line x1="2" x2="58" y1={4 + ((2.2 - g) / 4.4) * 28} y2={4 + ((2.2 - g) / 4.4) * 28} class="grid" />{/each}
              <path d={shapePath(sh.points, 60, 36)} />
            </svg>
            <span class="small">{t("calib.toneN", { n: sh.tone })}</span>
          </div>
        {/each}
      </div>
      <p class="small faint">{t("calib.paramsHint")}</p>
    </details>
  {/if}
</div>

<style>
  .calib {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 12px 0 4px;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  .actions {
    display: flex;
    gap: 6px;
  }
  .block {
    display: block;
  }
  .turn-on {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .wizard {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 14px;
    border-radius: var(--radius-lg);
    background: var(--surface-2);
  }
  .dots {
    display: flex;
    gap: 8px;
    list-style: none;
    margin: 0;
    padding: 0;
    justify-content: center;
  }
  .dots button {
    min-width: 34px;
    height: 34px;
    border-radius: 999px;
    border: 2px solid var(--border);
    background: var(--surface);
    color: var(--text-2);
    font-family: var(--font-han);
    font-size: 15px;
    cursor: pointer;
  }
  .dots button:disabled {
    cursor: default;
    opacity: 0.6;
  }
  .dots .here button {
    border-color: var(--c);
    color: var(--c);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--c) 20%, transparent);
  }
  .dots .done button {
    background: var(--c);
    border-color: var(--c);
    color: var(--surface);
  }
  .prompt {
    position: relative;
    text-align: center;
  }
  .say {
    font-size: 12px;
    color: var(--text-3);
  }
  .hanzi {
    font-family: var(--font-han);
    font-size: 56px;
    line-height: 1.2;
    color: var(--c);
  }
  .hanzi.sentence {
    font-size: 26px;
    padding: 6px 0;
  }
  .pinyin {
    color: var(--text-2);
    font-size: 16px;
  }
  .tone-tag {
    margin-left: 8px;
    font-size: 11px;
    padding: 1px 7px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--c) 15%, transparent);
    color: var(--c);
  }
  .shape-hint {
    position: absolute;
    right: 8px;
    top: 18px;
    width: 60px;
    height: 36px;
  }
  .shape-hint path,
  .shape path {
    fill: none;
    stroke: var(--c);
    stroke-width: 3;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .live {
    position: relative;
    height: 150px;
    border-radius: var(--radius);
    background: var(--surface);
    border: 1px solid var(--border);
    overflow: hidden;
    transition: box-shadow 0.2s;
  }
  .live.active {
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 25%, transparent);
  }
  .live canvas {
    width: 100%;
    height: 100%;
    display: block;
  }
  .meter {
    position: absolute;
    left: 0;
    right: 0;
    top: 0;
    height: 3px;
    background: var(--accent);
    transform-origin: left;
    transition: transform 0.08s linear;
  }
  .live-hint {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    text-align: center;
    padding: 0 24px;
  }
  .quiet {
    background: color-mix(in srgb, var(--surface) 85%, transparent);
  }
  .quiet-title {
    font-size: 17px;
    font-weight: 600;
    color: var(--text);
  }
  .quiet-bar {
    margin: 10px auto 0;
    width: 180px;
    height: 4px;
    border-radius: 99px;
    background: var(--surface-3);
    overflow: hidden;
  }
  .quiet-bar span {
    display: block;
    height: 100%;
    background: var(--text-3);
    transform-origin: left;
    animation: fill linear forwards;
  }
  @keyframes fill {
    from {
      transform: scaleX(0);
    }
    to {
      transform: scaleX(1);
    }
  }
  .legend {
    position: absolute;
    left: 26px;
    bottom: 26px;
    font-size: 10px;
    pointer-events: none;
  }
  .speak-now {
    position: absolute;
    top: 10px;
    left: 50%;
    transform: translateX(-50%);
    padding: 4px 14px;
    border-radius: 999px;
    background: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
    font-size: 14px;
    animation: pop 1.6s ease-out forwards;
    pointer-events: none;
  }
  @keyframes pop {
    0% {
      opacity: 0;
      transform: translateX(-50%) scale(0.8);
    }
    12% {
      opacity: 1;
      transform: translateX(-50%) scale(1.05);
    }
    75% {
      opacity: 1;
      transform: translateX(-50%) scale(1);
    }
    100% {
      opacity: 0;
    }
  }
  .controls {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .grow {
    flex: 1;
  }
  .rec-dot {
    display: inline-block;
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--accent);
    margin-right: 7px;
    animation: pulse 1s ease-in-out infinite;
  }
  @keyframes pulse {
    50% {
      opacity: 0.3;
    }
  }
  .analysis {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 200px;
    gap: 12px;
    align-items: start;
  }
  @media (max-width: 760px) {
    .analysis {
      grid-template-columns: 1fr;
    }
  }
  svg text.axis {
    font-size: 10px;
    fill: var(--text-3);
  }
  svg .grid {
    stroke: var(--grid);
    stroke-width: 1;
    stroke-dasharray: 3 4;
  }
  svg .grid.mid {
    stroke-dasharray: none;
  }
  .track {
    width: 100%;
    background: var(--surface);
    border-radius: var(--radius);
    border: 1px solid var(--border);
  }
  .track .lvl {
    fill: var(--surface-3);
  }
  .track .seg {
    fill: color-mix(in srgb, var(--text) 4%, transparent);
  }
  .track .measured {
    fill-opacity: 0.12;
    stroke-opacity: 0.6;
    stroke-width: 1;
    stroke-dasharray: 4 3;
  }
  .track .measured-label {
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.02em;
  }
  .track .pitch {
    fill: none;
    stroke-width: 2.5;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .track .pitch.dim {
    opacity: 0.3;
  }
  .track .creak {
    fill: none;
    stroke-width: 2.5;
    stroke-dasharray: 2 4;
    stroke-linecap: round;
  }
  .track .template {
    fill: none;
    stroke-width: 10;
    stroke-linecap: round;
    stroke-linejoin: round;
    opacity: 0.18;
  }
  .track .contour {
    fill: none;
    stroke: var(--text);
    stroke-width: 2;
    stroke-dasharray: 4 3;
  }
  .track .dot {
    fill: var(--surface);
    stroke: var(--text);
    stroke-width: 2;
  }
  .side {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .verdict {
    font-weight: 600;
    color: var(--warn);
  }
  .verdict.ok {
    color: var(--ok);
  }
  .probs {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .prob {
    display: grid;
    grid-template-columns: 40px 1fr 36px;
    align-items: center;
    gap: 6px;
    opacity: 0.75;
  }
  .prob.want {
    opacity: 1;
  }
  .prob svg {
    width: 40px;
    height: 24px;
  }
  .prob svg path {
    fill: none;
    stroke: var(--c);
    stroke-width: 2.5;
    stroke-linecap: round;
  }
  .bar {
    height: 8px;
    border-radius: 99px;
    background: var(--surface-3);
    overflow: hidden;
  }
  .bar span {
    display: block;
    height: 100%;
    background: var(--c);
    border-radius: 99px;
    transition: width 0.4s ease;
  }
  .num {
    font-size: 12px;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .stats {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 2px 10px;
    margin: 0;
    font-size: 12px;
  }
  .stats dt {
    color: var(--text-3);
  }
  .stats dd {
    margin: 0;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .tip {
    margin: 0;
  }
  .advice {
    margin: 0;
    padding: 8px 12px 8px 28px;
    border-left: 3px solid var(--c);
    border-radius: 0 var(--radius) var(--radius) 0;
    background: color-mix(in srgb, var(--c) 8%, transparent);
    font-size: 13px;
  }
  .advice li + li {
    margin-top: 4px;
  }
  .range svg {
    width: 100%;
  }
  .range-head {
    display: flex;
    gap: 10px;
    align-items: baseline;
    flex-wrap: wrap;
  }
  .range .band {
    fill: color-mix(in srgb, var(--series-1) 10%, transparent);
  }
  .range .chao {
    stroke: color-mix(in srgb, var(--series-1) 35%, transparent);
    stroke-width: 1;
  }
  .range .hist {
    stroke: var(--series-1);
    stroke-width: 1.5;
  }
  .range .median {
    stroke: var(--accent);
    stroke-width: 2;
  }
  .params summary {
    cursor: pointer;
    color: var(--text-2);
  }
  .param-grid {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 3px 16px;
    margin: 8px 0;
  }
  .param-grid span:nth-child(odd) {
    color: var(--text-3);
  }
  .param-grid span:nth-child(even) {
    font-variant-numeric: tabular-nums;
  }
  .shapes {
    display: flex;
    gap: 12px;
    flex-wrap: wrap;
  }
  .shape {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
    color: var(--c);
  }
  .shape svg {
    width: 60px;
    height: 36px;
  }
</style>
