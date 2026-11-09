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
}

export const SAMPLE_RATE = 16000;

/** Resolves with the recording, or null if nothing was said (or it was aborted). */
export async function recordUtterance(opts: RecordOptions = {}): Promise<Float32Array | null> {
  const { startTimeoutMs = 6000, silenceMs = 900, maxMs = 10000, signal, stopSignal, onLevel } = opts;
  if (signal?.aborted) return null;
  const stream = await navigator.mediaDevices.getUserMedia({
    audio: { channelCount: 1, echoCancellation: true, noiseSuppression: true, autoGainControl: true },
  });
  // The browser resamples the microphone to the context's rate.
  const ctx = new AudioContext({ sampleRate: SAMPLE_RATE });
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

    const finish = (keep: boolean) => {
      if (done) return;
      done = true;
      proc.onaudioprocess = null;
      proc.disconnect();
      source.disconnect();
      stream.getTracks().forEach((tr) => tr.stop());
      ctx.close();
      if (!keep || !started) return resolve(null);
      const out = new Float32Array(chunks.reduce((n, c) => n + c.length, 0));
      let at = 0;
      for (const c of chunks) {
        out.set(c, at);
        at += c.length;
      }
      resolve(out);
    };
    signal?.addEventListener("abort", () => finish(false), { once: true });
    stopSignal?.addEventListener("abort", () => finish(true), { once: true });

    proc.onaudioprocess = (e) => {
      const data = new Float32Array(e.inputBuffer.getChannelData(0));
      const ms = (data.length / SAMPLE_RATE) * 1000;
      elapsed += ms;
      let sum = 0;
      for (const v of data) sum += v * v;
      const rms = Math.sqrt(sum / data.length);
      onLevel?.(Math.min(1, rms * 8));

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
