// Record one spoken answer from the microphone as 16 kHz mono samples (what Whisper
// expects). Recording starts when you start talking and stops after a short silence.

export interface RecordOptions {
  /** Give up if nothing is said within this time. */
  startTimeoutMs?: number;
  /** Stop after this much silence once speech has started. */
  silenceMs?: number;
  /** Hard limit on the recording. */
  maxMs?: number;
  /** Cancel: discard the recording. */
  signal?: AbortSignal;
  /** Finish now and keep what was recorded (e.g. the learner clicked "done"). */
  stopSignal?: AbortSignal;
  /** Called with the input level (0..1, roughly) about 8 times a second. */
  onLevel?: (level: number) => void;
  /** Called with each block of samples as it arrives (for live displays). */
  onSamples?: (samples: Float32Array) => void;
  /** Throw away the first this-many ms (the click that started the recording). With
   * `keepAll`, the second half of it measures the room's noise: ask for quiet meanwhile. */
  skipMs?: number;
  /** Called once the skipped start is over: the time to start speaking. */
  onReady?: () => void;
  /** With `keepAll`: the room's noise from the wait (just before `onReady`). */
  onNoise?: (quiet: Float32Array) => void;
  /** When `stopSignal` ends it, drop this much from the end (the click on "Stop"). */
  trimEndMs?: number;
  /** Keep everything from the start (after `skipMs`): no waiting for speech to begin, nothing
   * cut from the front, and it only stops on its own after `silenceMs` of quiet once
   * something was said (or at `maxMs`). For calibration, where every bit of voice counts. */
  keepAll?: boolean;
  /** The microphone as it is: no echo cancellation, noise suppression or automatic gain,
   * which are made for calls and can chop up and flatten the pitch of a voice. */
  raw?: boolean;
}

export const SAMPLE_RATE = 16000;

/** A recording as a 16-bit mono WAV, for playing it back (kept in memory only). */
export function wavBlob(pcm: Float32Array, rate = SAMPLE_RATE): Blob {
  const buf = new ArrayBuffer(44 + pcm.length * 2);
  const v = new DataView(buf);
  const text = (at: number, s: string) => [...s].forEach((c, i) => v.setUint8(at + i, c.charCodeAt(0)));
  text(0, "RIFF");
  v.setUint32(4, 36 + pcm.length * 2, true);
  text(8, "WAVEfmt ");
  v.setUint32(16, 16, true);
  v.setUint16(20, 1, true); // PCM
  v.setUint16(22, 1, true); // mono
  v.setUint32(24, rate, true);
  v.setUint32(28, rate * 2, true);
  v.setUint16(32, 2, true);
  v.setUint16(34, 16, true);
  text(36, "data");
  v.setUint32(40, pcm.length * 2, true);
  for (let i = 0; i < pcm.length; i++) v.setInt16(44 + i * 2, Math.max(-1, Math.min(1, pcm[i])) * 0x7fff, true);
  return new Blob([buf], { type: "audio/wav" });
}

/** Resolves with the recording, or null if nothing was said (or it was aborted). */
export async function recordUtterance(opts: RecordOptions = {}): Promise<Float32Array | null> {
  const { startTimeoutMs = 6000, silenceMs = 900, maxMs = 10000, signal, stopSignal, onLevel, onSamples } = opts;
  const { skipMs = 0, onReady, onNoise, trimEndMs = 0, keepAll = false, raw = false } = opts;
  if (signal?.aborted) return null;
  const stream = await navigator.mediaDevices.getUserMedia({
    audio: { channelCount: 1, echoCancellation: !raw, noiseSuppression: !raw, autoGainControl: !raw },
  });
  // The browser resamples the microphone to the context's rate.
  const ctx = new AudioContext({ sampleRate: SAMPLE_RATE });
  // a context made after an await can start paused (no recent click): nothing would arrive
  if (ctx.state === "suspended") await ctx.resume().catch(() => {});
  const source = ctx.createMediaStreamSource(stream);
  const proc = ctx.createScriptProcessor(2048, 1, 1);

  return new Promise((resolve) => {
    const chunks: Float32Array[] = [];
    let elapsed = 0;
    let speechMs = 0;
    let silentFor = 0;
    let started = false;
    let floor = 0;
    let done = false;

    let skipped = 0;
    let quietest = Infinity;
    let noise = 0;
    const quietChunks: Float32Array[] = [];
    let loudest = 0;

    const finish = (keep: boolean, trimMs = 0) => {
      if (done) return;
      done = true;
      proc.onaudioprocess = null;
      proc.disconnect();
      source.disconnect();
      stream.getTracks().forEach((tr) => tr.stop());
      ctx.close();
      if (!keep || !started) return resolve(null);
      const total = chunks.reduce((n, c) => n + c.length, 0);
      const out = new Float32Array(total);
      let at = 0;
      for (const c of chunks) {
        out.set(c, at);
        at += c.length;
      }
      const trim = Math.min(Math.floor((trimMs / 1000) * SAMPLE_RATE), Math.floor(total / 2));
      resolve(trim > 0 ? out.slice(0, total - trim) : out);
    };
    signal?.addEventListener("abort", () => finish(false), { once: true });
    stopSignal?.addEventListener("abort", () => finish(true, trimEndMs), { once: true });

    proc.onaudioprocess = (e) => {
      const data = new Float32Array(e.inputBuffer.getChannelData(0));
      const ms = (data.length / SAMPLE_RATE) * 1000;
      // the start is thrown away (e.g. the mouse click that started the recording)
      if (skipped < skipMs) {
        skipped += ms;
        onLevel?.(0);
        // after the click has passed, the rest of the wait measures the room's noise
        if (keepAll && skipped > skipMs * 0.4) {
          let s = 0;
          for (const v of data) s += v * v;
          noise = Math.max(noise, Math.sqrt(s / data.length));
          quietChunks.push(data);
        }
        if (skipped >= skipMs) {
          if (quietChunks.length && onNoise) {
            const all = new Float32Array(quietChunks.reduce((n, c) => n + c.length, 0));
            let at = 0;
            for (const c of quietChunks) {
              all.set(c, at);
              at += c.length;
            }
            onNoise(all);
          }
          onReady?.();
        }
        return;
      }
      elapsed += ms;
      let sum = 0;
      for (const v of data) sum += v * v;
      const rms = Math.sqrt(sum / data.length);
      onLevel?.(Math.min(1, rms * 8));
      onSamples?.(data);

      if (keepAll) {
        // Speech is anything well above the quietest moment so far; stop after a long quiet
        // spell once there has been some.
        chunks.push(data);
        quietest = Math.min(quietest, rms);
        loudest = Math.max(loudest, rms);
        const speech = rms > Math.max(0.01, noise * 2.5, quietest * 3, loudest * 0.08);
        if (speech) started = true;
        if (started) silentFor = speech ? 0 : silentFor + ms;
        if ((started && silentFor >= silenceMs) || elapsed >= maxMs) finish(true);
        else if (!started && elapsed > startTimeoutMs) finish(false);
        return;
      }

      // Learn the room's noise level for the first 300 ms.
      if (elapsed <= 300) {
        floor = Math.max(floor, rms);
        chunks.push(data);
        return;
      }
      const threshold = Math.max(0.015, floor * 2.5);
      chunks.push(data);
      if (!started) {
        if (rms > threshold) started = true;
        else {
          // keep about 400 ms before speech starts
          while (chunks.length > 4) chunks.shift();
          if (elapsed > startTimeoutMs) finish(false);
        }
        return;
      }
      speechMs += ms;
      silentFor = rms < threshold * 0.7 ? silentFor + ms : 0;
      if (silentFor >= silenceMs || speechMs >= maxMs) finish(true);
    };
    source.connect(proc);
    proc.connect(ctx.destination); // the processor only runs when connected; it outputs silence
  });
}
