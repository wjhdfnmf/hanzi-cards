// A quick pitch tracker for the live display while recording (Settings › calibration).
// The real analysis runs in Rust (tones.rs) once the recording is done; this only has to
// draw a believable line as you speak: plain YIN on 40 ms windows every 20 ms.

const RATE = 16000;
const WIN = 640;
const HOP = 320;
const TAU_MIN = Math.floor(RATE / 500);
const TAU_MAX = Math.ceil(RATE / 70);

export interface LivePoint {
  /** Semitones above 100 Hz, or null when unvoiced. */
  st: number | null;
  rms: number;
}

// The same as the Rust tracker: a 1 kHz low-pass first, YIN thresholds 0.25 / 0.45, and
// frames quieter than 8% of the loudest so far count as silence.
const CUTOFF = 1000;
const FIRST = 0.25;
const GLOBAL = 0.45;

export class LivePitch {
  private buf = new Float32Array(0);
  private d = new Float32Array(TAU_MAX + 1);
  private loudest = 0;
  // second-order Butterworth low-pass (RBJ biquad), state kept between blocks
  private z = [0, 0, 0, 0];
  private coef = (() => {
    const w0 = (2 * Math.PI * CUTOFF) / RATE;
    const alpha = Math.sin(w0) / (2 * Math.SQRT1_2);
    const cos = Math.cos(w0);
    const a0 = 1 + alpha;
    return { b0: (1 - cos) / 2 / a0, b1: (1 - cos) / a0, b2: (1 - cos) / 2 / a0, a1: (-2 * cos) / a0, a2: (1 - alpha) / a0 };
  })();

  private lowPass(x: Float32Array): Float32Array {
    const { b0, b1, b2, a1, a2 } = this.coef;
    let [x1, x2, y1, y2] = this.z;
    const out = new Float32Array(x.length);
    for (let i = 0; i < x.length; i++) {
      const y = b0 * x[i] + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2;
      [x2, x1, y2, y1] = [x1, x[i], y1, y];
      out[i] = y;
    }
    this.z = [x1, x2, y1, y2];
    return out;
  }

  /** Mains hum found in the noise sample, removed before tracking (Hz). */
  hum: number[] = [];
  private notches: { b0: number; b1: number; b2: number; a1: number; a2: number; z: number[] }[] = [];

  /** Learn the room's hum from a stretch of quiet (the wait before speaking). */
  learnNoise(quiet: Float32Array) {
    this.hum = findHum(quiet);
    this.notches = this.hum.map((f) => {
      const w0 = (2 * Math.PI * f) / RATE;
      const alpha = Math.sin(w0) / (2 * (f / 3));
      const cos = Math.cos(w0);
      const a0 = 1 + alpha;
      return { b0: 1 / a0, b1: (-2 * cos) / a0, b2: 1 / a0, a1: (-2 * cos) / a0, a2: (1 - alpha) / a0, z: [0, 0, 0, 0] };
    });
  }

  private removeHum(x: Float32Array): Float32Array {
    if (!this.notches.length) return x;
    const out = new Float32Array(x);
    for (const n of this.notches) {
      let [x1, x2, y1, y2] = n.z;
      for (let i = 0; i < out.length; i++) {
        const y = n.b0 * out[i] + n.b1 * x1 + n.b2 * x2 - n.a1 * y1 - n.a2 * y2;
        [x2, x1, y2, y1] = [x1, out[i], y1, y];
        out[i] = y;
      }
      n.z = [x1, x2, y1, y2];
    }
    return out;
  }

  /** Feed samples; returns a point for every 20 ms they complete. */
  push(samples: Float32Array): LivePoint[] {
    const filtered = this.lowPass(this.removeHum(samples));
    const joined = new Float32Array(this.buf.length + filtered.length);
    joined.set(this.buf);
    joined.set(filtered, this.buf.length);
    const out: LivePoint[] = [];
    let start = 0;
    while (start + WIN + TAU_MAX <= joined.length) {
      out.push(this.estimate(joined, start));
      start += HOP;
    }
    this.buf = joined.slice(start);
    return out;
  }

  private estimate(x: Float32Array, start: number): LivePoint {
    let sum = 0;
    for (let j = 0; j < WIN; j++) sum += x[start + j] ** 2;
    const rms = Math.sqrt(sum / WIN);
    this.loudest = Math.max(this.loudest, rms);
    if (rms < Math.max(0.003, this.loudest * 0.08)) return { st: null, rms };
    const d = this.d;
    for (let tau = 1; tau <= TAU_MAX; tau++) {
      let s = 0;
      for (let j = 0; j < WIN; j++) {
        const diff = x[start + j] - x[start + j + tau];
        s += diff * diff;
      }
      d[tau] = s;
    }
    // cumulative mean normalised difference; first dip under the threshold
    let running = 0;
    let pick = -1;
    let best = -1;
    let bestVal = Infinity;
    const cmnd = (tau: number, r: number) => (r > 0 ? (d[tau] * tau) / r : 1);
    const values = new Float32Array(TAU_MAX + 2);
    for (let tau = 1; tau <= TAU_MAX; tau++) {
      running += d[tau];
      values[tau] = cmnd(tau, running);
    }
    for (let tau = TAU_MIN; tau <= TAU_MAX; tau++) {
      if (values[tau] < bestVal) [best, bestVal] = [tau, values[tau]];
      if (pick < 0 && values[tau] < FIRST) {
        while (tau < TAU_MAX && values[tau + 1] < values[tau]) tau++;
        pick = tau;
        break;
      }
    }
    if (pick < 0 && bestVal < GLOBAL) pick = best;
    if (pick < 0) return { st: null, rms };
    const [a, b, c] = [values[pick - 1], values[pick], values[pick + 1]];
    const den = a - 2 * b + c;
    const t = Math.abs(den) > 1e-9 && pick > 1 && pick < TAU_MAX ? pick + (0.5 * (a - c)) / den : pick;
    return { st: 12 * Math.log2(RATE / t / 100), rms };
  }
}

/** Power of `x` (Hann-windowed) at `f` Hz (Goertzel). */
function powerAt(x: Float32Array, f: number): number {
  const n = x.length;
  const w = 2 * Math.cos((2 * Math.PI * f) / RATE);
  let s1 = 0;
  let s2 = 0;
  for (let i = 0; i < n; i++) {
    const s0 = x[i] * (0.5 - 0.5 * Math.cos((2 * Math.PI * i) / n)) + w * s1 - s2;
    s2 = s1;
    s1 = s0;
  }
  return Math.max(0, s1 * s1 + s2 * s2 - w * s1 * s2);
}

/** Mains hum (50 or 60 Hz and harmonics) in a stretch of quiet: the same test as
 * tones::find_hum in Rust. */
export function findHum(quiet: Float32Array): number[] {
  const seg = quiet.length > RATE / 2 ? quiet.subarray(quiet.length - RATE / 2) : quiet;
  if (seg.length < RATE / 5) return [];
  let total = 0;
  for (let k = 20; k <= 250; k++) total += powerAt(seg, k * 2);
  if (total <= 0) return [];
  let best = { sum: 0, found: [] as number[] };
  for (const mains of [50, 60]) {
    let sum = 0;
    const found: number[] = [];
    for (let k = 1; k <= 8; k++) {
      const reach = Math.max(0.1, k * mains * 0.002);
      let [f, p] = [0, 0];
      for (let d = -5; d <= 5; d++) {
        const ff = k * mains + (d * reach) / 5;
        const pp = powerAt(seg, ff);
        if (pp > p) [f, p] = [ff, pp];
      }
      const around = [-12, -9, -6, 6, 9, 12].map((d) => powerAt(seg, f + d)).sort((a, b) => a - b);
      if (p >= total * 0.02 && p > around[2] * 20) {
        found.push(f);
        sum += p;
      }
    }
    if (sum > best.sum) best = { sum, found };
  }
  return best.found;
}

/** Semitones above 100 Hz → Hz. */
export const stToHz = (st: number) => 100 * 2 ** (st / 12);
/** Hz → semitones above 100 Hz. */
export const hzToSt = (hz: number) => 12 * Math.log2(hz / 100);

const NOTES = ["C", "C♯", "D", "D♯", "E", "F", "F♯", "G", "G♯", "A", "A♯", "B"];
/** The nearest musical note, e.g. "A3". */
export function noteName(hz: number): string {
  const midi = Math.round(69 + 12 * Math.log2(hz / 440));
  return `${NOTES[((midi % 12) + 12) % 12]}${Math.floor(midi / 12) - 1}`;
}
