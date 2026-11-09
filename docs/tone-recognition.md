# Exploration: better tone recognition

Status: exploration with two prototypes, written 2026-11-09. Nothing here changes the app yet.

## The problem

Today a spoken answer is judged by what Whisper writes. Whisper is a speech-to-text model
with a strong sense of which words are likely, so a learner who says the right syllables with
a wrong tone is often "corrected" to the right word (it writes 妈妈 for a flat-toned māma
attempt and for a perfect one alike). The tone check in `speech::judge_text` only catches
tone errors when Whisper happens to write a different-tone word. The learner needs to know
*which syllable* had *which tone*, and ideally to see it.

## What was tried (prototypes in the code, test clips in the scratchpad)

Test material: 60 clips from the Windows "Huihui" Chinese voice: 10 sets of four syllables
that differ only in tone (妈麻马骂, 八拔把爸, 衣姨椅意, 汤糖躺烫, 通同桶痛, 方房访放, 西习洗细,
烟言眼燕, 汪王往忘, 夫服府父) and 20 two-syllable words with sandhi and neutral tones
(你好 → ní hǎo, 不是 → bú shì, 谢谢, 妈妈, …), clean and with added noise.

### 1. Pitch tracking + tone shapes (`src-tauri/src/tones.rs`)

- YIN pitch tracker, 10 ms frames, after a 1 kHz low-pass filter (which raised the voiced
  frames found noticeably), octave-jump repair and median smoothing.
- Splits the voiced part into the expected number of syllables at loudness dips.
- Each syllable's pitch becomes 5 points on the speaker's own scale (their median and
  spread, from earlier recordings), compared with Chao's tone shapes 55, 35, 214/21, 51.
- Unit tests pass on synthetic level, rising and falling glides.
- On the test voice: about **45% of syllables** right (chance is 25%). The tracker works;
  the test voice does not: its rising tones are low and dipping or even falling (糖, 王), its
  third tones drop an octave into creak, and its fourth tones end in creak with no pitch.
  Tuning shapes to this voice would only fit its quirks.

### 2. Asking Whisper about tones (`whisper::score`)

- Instead of asking Whisper *what* was said, ask how likely the expected word is against the
  same syllables with other tones (妈 vs 麻 马 骂; 学生 vs other xuesheng words), by scoring
  each candidate token by token (cheap with the cached decoder).
- Whisper small, single syllables (4 choices): right tone wins **23/40** clean, 16/40 noisy
  (chance 10/40). Two-syllable words: **11/11**, but there the wrong-tone versions are rare
  words or not words, so this measures word knowledge more than hearing.
- So Whisper is clearly tone-sensitive, but not enough to grade a single syllable, and its
  word knowledge hides tone errors inside real words, which is exactly the learner's case.

### The real limitation: no trustworthy test voice

Both numbers are limited by the test material: an old concatenative voice that doesn't say
its tones the way people do. Any further tuning needs **real people saying known tones**,
including a learner saying wrong ones.

## Options

| | What | Effort | Notes |
|---|---|---|---|
| A | **Pitch graph**: after each spoken answer, draw the learner's pitch per syllable over the target tone shape (and over the TTS voice's) | small | Works now (the tracker is fine); the learner judges the shape. Visual pitch feedback is the classic, well-studied aid for tone learning. |
| B | **Personal calibration**: say mā má mǎ mà once (checked against the shapes in A); classify later syllables by nearest personal template | small-medium | Avoids needing a dataset; adapts to the learner's voice and range. |
| C | **Trained tone classifier**: small network on pitch contours (like Tonara, MIT, ~94% on new speakers; ToneNet; others ~99% on isolated syllables) | medium | Needs real multi-speaker data: Tone Perfect (MSU, 410 syllables × 6 speakers; licence to check before use) or AISHELL-3 (Apache 2.0, needs syllable alignment). Runs in candle, tiny. |
| D | **Combine**: grade a syllable's tone only when the pitch classifier and Whisper's score agree, otherwise show the graph and say "not sure" | small, after B or C | Fewer false "wrong tone" verdicts, which are what makes learners distrust a tool. |
| E | **Cloud assessment** (Azure Pronunciation Assessment, zh-CN) | small code, ongoing cost | Gives syllable/phoneme accuracy scores; no documented tone-error output; audio leaves the computer; another account. |

Also needed in every option: **expected tones after sandhi** (3-3 → 2-3, 不 before a 4th tone →
bú, 一 → yí/yì, neutral-tone syllables from the dictionary's tone 5 not graded). The test list
already encodes these by hand; the app needs the rules in `pinyin.rs`.

## Tone Perfect: the dataset to use (checked 2026-11-09)

- **What it is:** Michigan State University's Tone Perfect (tone.lib.msu.edu; Catherine Ryu,
  Mandarin Tone Perception & Production Team, and MSU Libraries, 2017): every Mandarin
  syllable in all four tones, said by six native speakers (three female, three male).
  1,640 toned syllables, 8,940 MP3 recordings (not every syllable has all six speakers).
- **Labels for free:** github.com/MatthewScholefield/mandarin-sound-table (MIT) has
  `src/pinyins.json` mapping each toned syllable (e.g. "pàng") to its recording ids, one per
  speaker, and plays them from `tone.lib.msu.edu/tone/{id}/PROXY_MP3/download`.
- **Access:** the site and those audio links are behind an automated-traffic check, so they
  can't be fetched by a script (and shouldn't be). The project's "Using and Citing Data" page
  (tone.lib.msu.edu/cite) asks people to **request the dataset with a form**; that's the way
  to get it in bulk.
- **Terms:** described as open access, with a citation requested; no licence text could be
  read (the pages were blocked). Ask in the request whether the recordings may be used to
  train a tone model that ships inside the app; evaluating and tuning locally is the minimum.
- **What it enables:** a real benchmark (leave-one-speaker-out: tune on five voices, test on
  the sixth), training option C, checking B's templates against real voices, and re-measuring
  Whisper's tone scoring. Isolated syllables only, so the sandhi and two-syllable cases
  still need connected speech (the learner's own recordings).
- **When it arrives:** unpack it anywhere; I'll convert the MP3s to 16 kHz WAV, build
  `list.tsv` from the ids with `pinyins.json`, and run `tone_bench` / `whisper_tone_bench`.

## Recommendation

1. **Build A + the sandhi rules now.** The pitch graph is useful on day one, needs no data, and
   is honest: it shows what was heard instead of guessing a verdict.
2. **Collect real recordings** with a small "tone check" page: the app shows a syllable (or
   one of your card words), you say it, and the recording is saved with what you meant to
   say (opt-in, stays on this computer). Twenty minutes of your own speech is enough to
   measure options B, C and D on a real learner voice instead of a robot. In parallel,
   **request Tone Perfect** (above) for six native voices.
3. Then **B** (personal templates), measured on those recordings, and **D** to combine it with
   Whisper's score. Consider **C** if B isn't reliable enough and a usable dataset licence
   is confirmed.
4. With the GPU build (`gpu-whisper-plan.md`), re-measure Whisper's tone scoring with
   large-v3-turbo: a bigger model may hear tones much better.

## How to re-run the measurements

```bash
cd src-tauri
TONES_DIR=<folder with list.tsv, clean/, noisy/> cargo test --lib tone_bench -- --ignored --nocapture
WHISPER_DIR="$APPDATA/com.hanzicards.app" TONES_DIR=<same> cargo test --lib whisper_tone_bench -- --ignored --nocapture
```

`list.tsv` lines are `text<TAB>tones as spoken` (sandhi applied, 5 = neutral, not graded),
with `clean/N.wav` and `noisy/N.wav` (16 kHz mono 16-bit) for line N.
