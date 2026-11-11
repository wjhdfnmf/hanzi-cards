"""Cut a speaker's block recordings (from the speaker sheet) into the tone bench format.

Each block file is one recording: "第N组", a pause, then the block's items with ~2 s gaps.
This finds the voiced stretches, expects 1 + k of them (the announcement plus k items), and
writes the clips of every block whose count is right. Blocks whose count is off are listed,
with where the stretches were, to fix by hand or record again; nothing is guessed.

Writes (audio stays out of git: use a folder under tone-data/):

  <out>/clean/N.wav, noisy/N.wav   16 kHz mono 16-bit, 44-byte header, 150 ms padding each
                                   side, peak-normalised; noisy has pink noise mixed in
  <out>/list.tsv                   text<TAB>tones as said (what tone_bench reads)
  <out>/meta.tsv                   N, block, pass, pos, id, part, pinyin, tones said, right tones
  <out>/blocks.tsv                 per block: file, segments found/expected, status

Usage:
    python scripts/tone_cut.py <folder of "Block N.*" files> tone-data/<speaker> [--pass 1]
"""

import argparse
import array
import math
import random
import re
import subprocess
import sys
import wave
from pathlib import Path

RATE = 16000
FRAME = 160  # 10 ms
ITEMS = Path(__file__).resolve().parent.parent / "docs" / "tone-recordings" / "items.tsv"


def decode(path: Path) -> array.array:
    cmd = ["ffmpeg", "-hide_banner", "-loglevel", "error", "-i", str(path), "-ac", "1", "-ar", str(RATE), "-f", "s16le", "pipe:1"]
    pcm = array.array("h")
    pcm.frombytes(subprocess.run(cmd, capture_output=True, check=True).stdout)
    return pcm


def frame_db(pcm: array.array) -> list[float]:
    out = []
    for i in range(0, len(pcm) - FRAME + 1, FRAME):
        s = sum(x * x for x in pcm[i : i + FRAME]) / FRAME
        out.append(10 * math.log10(s + 1.0))
    return out


def voiced_stretches(db: list[float], gap_ms: int, min_ms: int, skip_ms: int):
    """Voiced stretches as (start_frame, end_frame), split at silences >= gap_ms."""
    ordered = sorted(db)
    noise = ordered[len(ordered) // 10]
    loud = ordered[int(len(ordered) * 0.99)]
    # well above the room, and not so low that breaths count: within 35 dB of the loudest
    threshold = max(noise + 10, loud - 35)
    on = [d > threshold for d in db]
    skip = skip_ms // 10
    for i in range(min(skip, len(on))):
        on[i] = False  # the tap that starts the recording
    stretches, start, last = [], None, -10**9
    for i, v in enumerate(on):
        if not v:
            continue
        if start is None:
            start = i
        elif i - last > gap_ms // 10:
            stretches.append((start, last + 1))
            start = i
        last = i
    if start is not None:
        stretches.append((start, last + 1))
    return [s for s in stretches if (s[1] - s[0]) * 10 >= min_ms], noise, threshold


def clip(pcm: array.array, a: int, b: int, noisy: bool, rng: random.Random) -> bytes:
    # 60 ms margin around the stretch (soft onsets and tails sit under the threshold), then pad
    lo = max(0, a * FRAME - 960)
    hi = min(len(pcm), b * FRAME + 960)
    seg = pcm[lo:hi]
    peak = max(1, max(abs(x) for x in seg))
    gain = 32400 / peak
    pad = [0.0] * int(0.15 * RATE)
    samples = pad + [x * gain for x in seg] + pad
    if noisy:
        # pink-ish noise (Paul Kellet's filter) at about -26 dBFS RMS, like the other benches
        b0 = b1 = b2 = 0.0
        for i in range(len(samples)):
            w = rng.uniform(-1, 1)
            b0 = 0.99765 * b0 + w * 0.0990460
            b1 = 0.96300 * b1 + w * 0.2965164
            b2 = 0.57000 * b2 + w * 1.0526913
            samples[i] += (b0 + b1 + b2 + w * 0.1848) * 0.05 * 32767 / 3.5
    out = array.array("h", (max(-32768, min(32767, int(round(x)))) for x in samples))
    return out.tobytes()


def write_wav(path: Path, pcm: bytes):
    path.parent.mkdir(parents=True, exist_ok=True)
    with wave.open(str(path), "wb") as w:  # exactly a 44-byte header
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(pcm)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("src", type=Path)
    ap.add_argument("out", type=Path)
    ap.add_argument("--pass", dest="pass_", default="1", help="which pass these blocks are from")
    ap.add_argument("--gap-ms", type=int, default=600)
    ap.add_argument("--min-ms", type=int, default=120)
    ap.add_argument("--skip-ms", type=int, default=300)
    args = ap.parse_args()

    rows = [l.split("\t") for l in ITEMS.read_text(encoding="utf-8").splitlines()[1:]]
    blocks: dict[int, list[list[str]]] = {}
    for r in rows:
        blocks.setdefault(int(r[0]), []).append(r)

    files = {}
    for f in args.src.iterdir():
        m = re.fullmatch(r"block\s*0*(\d+)", f.stem, re.I)
        if m:
            files[int(m.group(1))] = f
    if not files:
        sys.exit(f"no 'Block N' files in {args.src}")

    rng = random.Random(7)
    list_lines, meta_lines, block_lines = [], ["n\tblock\tpass\tpos\tid\tpart\tpinyin\ttones_said\ttones_right"], ["block\tfile\tfound\texpected\tstatus"]
    n = 0
    for b in sorted(files):
        items = blocks.get(b)
        if not items:
            print(f"block {b}: not in items.tsv, skipped")
            continue
        if items[0][1] != args.pass_:
            print(f"block {b}: belongs to pass {items[0][1]}, not {args.pass_}; skipped")
            continue
        pcm = decode(files[b])
        db = frame_db(pcm)
        st, noise, thr = voiced_stretches(db, args.gap_ms, args.min_ms, args.skip_ms)
        want = 1 + len(items)
        where = " ".join(f"{a / 100:.2f}-{e / 100:.2f}" for a, e in st)
        if len(st) != want:
            status = "count off"
            print(f"block {b:2}: {len(st)} stretches, expected {want} (noise {noise:.0f} dB, threshold {thr:.0f} dB) -> {where}")
        else:
            status = "ok"
            print(f"block {b:2}: ok ({want}; noise {noise:.0f} dB, threshold {thr:.0f} dB)")
            for (a, e), item in zip(st[1:], items):
                for set_, noisy in (("clean", False), ("noisy", True)):
                    write_wav(args.out / set_ / f"{n}.wav", clip(pcm, a, e, noisy, rng))
                _, pass_, pos, iid, part, text, pinyin, said, right = item
                list_lines.append(f"{text}\t{said}")
                meta_lines.append("\t".join([str(n), str(b), pass_, pos, iid, part, pinyin, said, right]))
                n += 1
        block_lines.append(f"{b}\t{files[b].name}\t{len(st)}\t{want}\t{status}\t{where}")

    args.out.mkdir(parents=True, exist_ok=True)
    (args.out / "list.tsv").write_text("\n".join(list_lines) + "\n", encoding="utf-8")
    (args.out / "meta.tsv").write_text("\n".join(meta_lines) + "\n", encoding="utf-8")
    (args.out / "blocks.tsv").write_text("\n".join(block_lines) + "\n", encoding="utf-8")
    print(f"{n} clips written to {args.out}")


if __name__ == "__main__":
    main()
