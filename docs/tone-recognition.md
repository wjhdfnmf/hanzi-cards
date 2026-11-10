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

Citation: Catherine Ryu, Mandarin Tone Perception & Production Team, and Michigan State
University Libraries. *Tone Perfect: Multimodal Database for Mandarin Chinese.* Accessed 1 January
2022. https://tone.lib.msu.edu/

- **What it is:** Michigan State University's Tone Perfect (tone.lib.msu.edu; Catherine Ryu,
  Mandarin Tone Perception & Production Team, and MSU Libraries, 2017): every Mandarin
  syllable in all four tones, said by six native speakers (three female, three male).
  1,640 toned syllables, 9,839 MP3 recordings (all six speakers for all but one).
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
- **Received 2026-11-09** (`TonePerfectDataset/`, git-ignored, two zips, 293 MB):
  - `tone_perfect_all_mp3.zip`: **9,839 clips** = 410 syllables × 4 tones × 6 speakers (FV1-3,
    MV1-3), only `jiao1_FV3` missing. Named `{syllable}{tone}_{speaker}_MP3.mp3`, ü written
    `v` (lv, nv, lve, nve). MP3 44.1 kHz stereo 320 kbps, 0.5-1.3 s (median 0.7), trimmed
    tight to the voice and peak-normalised to -0.1 dB.
  - `tone_perfect_all_xml.zip`: two files per clip. `_DC.xml` is just id/speaker/title.
    `_CUSTOM.xml` has pinyin with marks, initial, final, gender, example characters
    (simplified and traditional), and **`lexical_gap`**: 2,382 clips (24%) are tone/syllable
    pairs that aren't real Mandarin syllables, said anyway. No labels are needed from
    `pinyins.json`: the filename is the label.
  - No licence or readme in either zip, so the open question about shipping a model trained on
    it still stands.
  - To use: convert to 16 kHz mono WAV, add ~150 ms padding (clips start on the voice), build
    `list.tsv`, and run `tone_bench` / `whisper_tone_bench` per speaker.

## Results on Tone Perfect (2026-11-09)

Six native speakers, 410 syllables × 4 tones, 9,815 clips scored (the `ma` syllables are held
out as the calibration set). Prepared with `scripts/tone_perfect_prep.py`; noisy = pink noise
about 13-17 dB below the speech.

**Pitch tracker + shapes** (`tone_perfect_bench`), share of syllables whose tone is right:

| Templates | Clean | Noisy | T1 / T2 / T3 / T4 (clean) |
|---|---|---|---|
| Chao shapes, no data (before the creak fix) | 83.1% | 83.4% | 99 / 91 / 94 / **49** |
| **Chao shapes, no data** | **89.7%** | 87.4% | 99 / 91 / 94 / 75 |
| Speaker's own mā má mǎ mà only (option B as planned) | 85.2% | 75.4% | 100 / 97 / 72 / 72 |
| Speaker's own average over many syllables | 92.1% | 91.2% | 99 / 97 / 93 / 79 |
| Other five speakers' average (like a trained model) | 89.5% | 88.7% | 99 / 95 / 86 / 78 |

- On real voices the tracker works: **45% on the synthetic voice → 90% here**, untuned. Every
  speaker is 86-93%. Non-words (lexical gaps) score the same, so it hears pitch, not words.
- **4th tones were the weak spot**: half were heard as 1st tones. The fall ends in creak, which
  is loud but has no pitch the tracker can follow, so only the high start was kept and it
  looked level. `contour()` now counts a loud, pitchless tail as the bottom of the speaker's
  range (`TONE_CREAK`, default 0.1 of the loudest frame; 0 turns it off): 4th tones 49% → 75%,
  1st tones unchanged at 99%. Loosening YIN's thresholds or the octave-jump rule helped less
  (≤ 85%).
- Remaining errors: 4th → 1st (16%), 2nd → 3rd (9%, a low, late rise).
- **One calibration set is worse than no calibration**: four clips are too few to average out
  one syllable's quirks. Personal templates pay off only when built from many syllables (92%),
  e.g. averaged from the learner's answers that were graded right over time.

**Whisper's tone scoring** (`whisper_tone_bench`, 30 real syllables whose four tones each have
a character read almost only that way, × 6 speakers = 720 clips; the right character must beat
the other three tones):

| Model | Clean | Noisy |
|---|---|---|
| small | 47% | 45% |
| large-v3-turbo | 53% | 56% |

Chance is 25%. Its misses lean to frequent characters (抛 袍 炮 → 跑), so word frequency
still outweighs what it hears. On isolated syllables **pitch beats Whisper by a wide margin**,
and the bigger model barely helps. Whisper can't be the tone judge; at most it can
cross-check (option D).

**In the app: "Use pitch to tell tones apart"** (Settings › Speech recognition, off by default;
`tone_check.rs`). After Whisper transcribes a spoken answer, if it wrote the answer or a word
with the same syllables, the candidates (the answer, up to 7 same-syllable words from the
dictionary, and Whisper's own word) are re-scored as Whisper's log-probability + 2 × the
pitch's log-probability, with expected tones after sandhi (`pinyin::spoken_tones`). The
learner's pitch range is a histogram of their recordings (setting `pitch_profile`); choosing
starts after 5 recordings.

End to end (`tone_select_e2e`): ten cards (跑 同 影 书 位 题 去 想 臭 气), Tone Perfect clips of
each card's syllable in all four tones as a learner aiming for the card's word, Whisper with the
drill's prompt and card-word bias. Share of answers ending with the tone actually said:

| Model | Whisper's word | With pitch | Where pitch could choose |
|---|---|---|---|
| small | 79/234 (34%) | **207/234 (88%)** | 197/214 (92%) |
| large-v3-turbo | 114/234 (49%) | **189/234 (81%)** | 184/199 (92%) |

The rest are answers where Whisper wrote other syllables or nothing (嗯, 有, a song title for a
lone syllable); the pitch weight makes no difference between 1 and 1000 there.

What these don't cover: syllables in words and phrases (sandhi, neutral tones, coarticulation)
and learner-style errors. Those need the native recordings (`native-tone-recordings.md`).

To re-run: `python scripts/tone_perfect_prep.py TonePerfectDataset tone-data/tp`, then
`TP_DIR=../tone-data/tp/all cargo test --lib tone_perfect_bench -- --ignored --nocapture`
(set `CARGO_PROFILE_DEV_OPT_LEVEL=3` or it's slow), and `whisper_tone_bench` with
`TONES_DIR` on each `tone-data/tp/whisper/<speaker>-g<k>` folder. With the GPU build the test
process can hang after printing its results; stop it then.

## Recommendation

1. **Build A + the sandhi rules now.** The pitch graph is useful on day one, needs no data, and
   is honest: it shows what was heard instead of guessing a verdict.
2. **Collect real recordings** with a small "tone check" page: the app shows a syllable (or
   one of your card words), you say it, and the recording is saved with what you meant to
   say (opt-in, stays on this computer). Twenty minutes of your own speech is enough to
   measure options B, C and D on a real learner voice instead of a robot. In parallel,
   **request Tone Perfect** (above) for six native voices. Native-speaker recordings of our
   own list: see `native-tone-recordings.md`.
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
