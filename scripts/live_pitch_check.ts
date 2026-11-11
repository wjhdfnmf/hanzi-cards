// Run the live pitch display's tracker (src/lib/livepitch.ts) over saved calibration
// recordings, the way the app feeds it (2048-sample blocks), and report its breaks: points
// with no pitch while the voice is loud. Compare with the Rust tracker's report from
//   CALIB_DIR=<folder> cargo test --lib calibration_recordings -- --ignored --nocapture
//
//   node scripts/live_pitch_check.ts <folder with .wav files>
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { LivePitch, stToHz } from "../src/lib/livepitch.ts";

const dir = process.argv[2];
for (const name of readdirSync(dir).filter((f) => f.endsWith(".wav")).sort()) {
  const bytes = readFileSync(join(dir, name));
  const pcm = new Float32Array((bytes.length - 44) / 2);
  for (let i = 0; i < pcm.length; i++) pcm[i] = bytes.readInt16LE(44 + i * 2) / 32768;
  const tracker = new LivePitch();
  // the app learns the hum from the quiet wait before speaking; here, the quietest half second
  let quiet = 0;
  let quietE = Infinity;
  for (let s = 0; s + 8000 <= pcm.length; s += 800) {
    let e = 0;
    for (let j = s; j < s + 8000; j++) e += pcm[j] ** 2;
    if (e < quietE) [quiet, quietE] = [s, e];
  }
  tracker.learnNoise(pcm.subarray(quiet, quiet + 8000));
  console.log(`\n  hum: ${tracker.hum.map((f) => f.toFixed(1)).join(", ") || "none"}`);
  const pts = [];
  for (let at = 0; at < pcm.length; at += 2048) pts.push(...tracker.push(pcm.subarray(at, at + 2048)));
  const loud = Math.max(...pts.map((p) => p.rms));
  const speaking = pts.map((p, i) => (p.rms >= loud * 0.08 ? i : -1)).filter((i) => i >= 0);
  const [first, last] = [speaking[0] ?? 0, speaking.at(-1) ?? 0];
  const pitched = pts.filter((p) => p.st !== null).length;
  console.log(`\n${name}: ${pts.length} points (20 ms), pitched ${pitched}/${speaking.length} loud (${Math.round((100 * pitched) / Math.max(1, speaking.length))}%)`);
  for (let i = first; i <= last; i++) {
    if (pts[i].st === null && pts[i].rms >= loud * 0.15) {
      const s = i;
      while (i <= last && pts[i].st === null && pts[i].rms >= loud * 0.15) i++;
      console.log(`  break ${(s * 0.02).toFixed(2)}-${(i * 0.02).toFixed(2)} s (${(i - s) * 20} ms)`);
    }
  }
  const line = [];
  for (let i = first; i <= last; i += 3) line.push(pts[i].st === null ? "." : Math.round(stToHz(pts[i].st as number)));
  console.log(`  Hz: ${line.join(" ")}`);
}
