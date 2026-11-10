"""Convert the Tone Perfect dataset into the tone bench format.

Tone Perfect: Catherine Ryu, Mandarin Tone Perception & Production Team, and Michigan State
University Libraries. Tone Perfect: Multimodal Database for Mandarin Chinese. Accessed 1 January
2022. https://tone.lib.msu.edu/

Reads the two zips as they come from MSU (nothing is unpacked) and writes:

  <out>/all/clean/N.wav, noisy/N.wav   every clip: 16 kHz mono 16-bit, 44-byte header,
                                       150 ms of silence added at each end (the clips are
                                       trimmed tight to the voice)
  <out>/all/meta.tsv                   N, syllable, tone, speaker, lexical gap (0/1)
  <out>/whisper/<speaker>-g<k>/        40-line folders for whisper_tone_bench: ten syllables
                                       whose four tones are all real words, each tone written
                                       with a character that is (almost) only read that way

The noisy copy has pink noise mixed in at a fixed level (about -26 dBFS RMS; every clip is
peak-normalised to -0.1 dB, so that is roughly 12-16 dB below the speech).

Usage:
    python scripts/tone_perfect_prep.py TonePerfectDataset tone-data/tp
"""

import concurrent.futures as cf
import random
import re
import subprocess
import sys
import wave
import zipfile
from pathlib import Path

PAD = "adelay=150:all=1,apad=pad_dur=0.15"
MONO = "aresample=16000,pan=mono|c0=0.5*c0+0.5*c1"
WHISPER_SYLLABLES = 30  # per speaker, in groups of ten
CHAR_FREQ = Path(__file__).resolve().parent.parent / "src-tauri" / "data" / "char_freq.tsv"


def convert(mp3: bytes, noisy: bool) -> bytes:
    if noisy:
        args = ["-filter_complex", f"[0:a]{MONO},{PAD}[s];anoisesrc=r=16000:a=0.24:c=pink:seed=7[n];[s][n]amix=inputs=2:duration=first:normalize=0[o]", "-map", "[o]"]
    else:
        args = ["-af", f"{MONO},{PAD}"]
    cmd = ["ffmpeg", "-hide_banner", "-loglevel", "error", "-f", "mp3", "-i", "pipe:0", *args, "-ac", "1", "-ar", "16000", "-f", "s16le", "pipe:1"]
    return subprocess.run(cmd, input=mp3, capture_output=True, check=True).stdout


def write_wav(path: Path, pcm: bytes):
    path.parent.mkdir(parents=True, exist_ok=True)
    with wave.open(str(path), "wb") as w:  # the wave module writes exactly a 44-byte header
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(16000)
        w.writeframes(pcm)


def readings():
    """char -> {reading: count} from char_freq.tsv (readings like 'ma1')."""
    out = {}
    for line in CHAR_FREQ.read_text(encoding="utf-8").splitlines():
        if line.startswith("#") or "\t" not in line:
            continue
        ch, rs = line.split("\t")
        out[ch] = {r.split(":")[0]: int(r.split(":")[1]) for r in rs.split()}
    return out


def main(src: Path, out: Path):
    mp3 = zipfile.ZipFile(src / "tone_perfect_all_mp3.zip")
    xml = zipfile.ZipFile(src / "tone_perfect_all_xml.zip")
    rx = re.compile(r"tone_perfect/(.+?)([1-4])_([FM]V\d)_MP3\.mp3")
    clips = sorted(rx.fullmatch(n).groups() for n in mp3.namelist() if rx.fullmatch(n))  # (syllable, tone, speaker)

    info = {}  # (syllable, tone) -> (gap, [simplified chars])
    for s, t, v in clips:
        if (s, t) in info:
            continue
        x = xml.read(f"tone_perfect_xml/{s}{t}_{v}_CUSTOM.xml").decode("utf-8")
        gap = "<lexical_gap>Yes</lexical_gap>" in x
        info[(s, t)] = (gap, re.findall(r"<simplified>(.+?)</simplified>", x))

    # every clip, clean and noisy
    all_dir = out / "all"
    # zipfile reads aren't thread-safe on one handle: read the bytes first
    blobs = [(i, c, mp3.read(f"tone_perfect/{c[0]}{c[1]}_{c[2]}_MP3.mp3")) for i, c in enumerate(clips)]
    def job2(i, c, data):
        write_wav(all_dir / "clean" / f"{i}.wav", convert(data, False))
        write_wav(all_dir / "noisy" / f"{i}.wav", convert(data, True))
    with cf.ThreadPoolExecutor(20) as ex:
        list(ex.map(lambda a: job2(*a), blobs))
    meta = [f"{i}\t{s}\t{t}\t{v}\t{int(info[(s, t)][0])}" for i, (s, t, v) in enumerate(clips)]
    (all_dir / "meta.tsv").write_text("\n".join(meta) + "\n", encoding="utf-8")

    # whisper subsets: syllables with four real tones, each with an unambiguous character
    freq = readings()
    def pick(s, t):
        best = None
        for ch in info[(s, t)][1]:
            r = freq.get(ch, {})
            total = sum(r.values())
            n = r.get(f"{s}{t}", 0)
            if total and n / total >= 0.85 and (best is None or n > best[1]):
                best = (ch, n)
        return best and best[0]
    index = {c: i for i, c in enumerate(clips)}
    speakers = sorted({c[2] for c in clips})
    usable = []
    for s in sorted({c[0] for c in clips}):
        chars = [pick(s, str(t)) if not info[(s, str(t))][0] else None for t in range(1, 5)]
        complete = all((s, str(t), v) in index for t in range(1, 5) for v in speakers)
        if all(chars) and len(set(chars)) == 4 and complete:
            usable.append((s, chars))
    random.Random(20261109).shuffle(usable)
    chosen = usable[:WHISPER_SYLLABLES]
    for v in speakers:
        for g in range(0, len(chosen), 10):
            d = out / "whisper" / f"{v}-g{g // 10 + 1}"
            lines = []
            for s, chars in chosen[g : g + 10]:
                for t in range(1, 5):
                    i = index[(s, str(t), v)]
                    n = len(lines)
                    lines.append(f"{chars[t - 1]}\t{t}")
                    for kind in ("clean", "noisy"):
                        (d / kind).mkdir(parents=True, exist_ok=True)
                        (d / kind / f"{n}.wav").write_bytes((all_dir / kind / f"{i}.wav").read_bytes())
            (d / "list.tsv").write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"{len(clips)} clips -> {all_dir}; {len(usable)} syllables usable for Whisper, {len(chosen)} used:")
    print(" ".join(f"{s}:{''.join(c)}" for s, c in chosen))


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8")
    main(Path(sys.argv[1]), Path(sys.argv[2]))
