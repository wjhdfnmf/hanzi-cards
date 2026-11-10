# Exploration: speaking Chinese with a local voice engine

**Decision (2026-11-09): the system voice (Yaoyao) on Windows; Kokoro on Linux** (to build and
test once WSL is set up).

Status: exploration + prototype measured 2026-11-09. **Result: not worth switching on
Windows for card words; one cheap fix (stand-in characters) is worth adding.**

## Prototype results (2026-11-09)

Prototype: a small program on the official `sherpa-onnx` Rust crate (2.0.1, static libraries
downloaded by its build script; it built on Windows without extra setup). Models:
`kokoro-int8-multi-lang-v1_1` (147 MB, Apache-2.0) and `vits-melo-tts-zh_en` (167 MB, MIT).
130 test items: 60 of the user's card words, 40 tone-set syllables (妈麻马骂 …), 20
single characters with two readings (行 háng/xíng …), 10 tutor-style sentences.

Judged by ear, Whisper large-v3-turbo's (GPU, no hints, Chinese only), exactly like a spoken
answer in Study (same characters or same toned pinyin = right). Every clip padded with 0.4 s
of silence and normalised to the same loudness; local voices resampled with sherpa-onnx's
filtered resampler.

| Voice | Card words /60 | Tone sets /40 | Two readings /20 | Sentences /10 | CPU per word |
|---|---|---|---|---|---|
| **Yaoyao** (Windows OneCore, the app's voice) | **52-53** | **21** | 16; **17 with stand-ins** | 10 | instant |
| Kangkang (Windows) | 50 | 18 | 16 | 10 | instant |
| Huihui (Windows desktop) | 47-49 | 14-17 | 11-13; **15 with stand-ins** | 10 | instant |
| Kokoro v1.1 int8, speaker 3 / 60 | 33-36; 41 with "。" | 5-7 | 4-7 | 9-10 | 0.55-0.65 s (RTF ~1.1) |
| MeloTTS zh_en | 4; 13 with "。" | 0 | 0-2 | 4-7 | 0.07 s (RTF 0.24) |

Caveats:
- Whisper is a weak judge of tones on single syllables (it also misses many tone-set items
  from the Windows voices), so the tone column compares voices only roughly.
- MeloTTS, re-checked after the user heard silent/cut-off clips: its archive's jieba `dict`
  folder had unpacked empty on Windows (fixed by copying Kokoro's identical files), its text
  processing is correct (汤 → "t ang", tone 1), but its audio is ~10x quieter and very short.
  With loudness normalised and slower speech (length 1.3-1.6) it still scored 9-14/60 card
  words, 0/40 tone sets, 5-7/10 sentences. The sherpa-onnx conversion has no BERT input
  (the original MeloTTS uses BERT features for prosody), the likely reason. Worse: about
  one clip in six (21-28 of 130, mostly single words) comes out empty or a near-silent blip
  (0.06-0.1 s), whatever the duration-noise setting (0.8, 0.3 or 0). Not usable here.
  `tts-samples/4-melotts-volume-fixed-slower` has the normalised clips.
- Kokoro was the 8-bit model on the CPU; the full-precision model (365 MB) wasn't tried.
- Naturalness can't be measured this way: **listen** to the samples in `tts-samples/` (not in
  git): the same words, tone sets, 行 háng/xíng and two sentences from each voice.

Conclusions:
1. **Windows: keep the system voice (Yaoyao).** It was the most intelligible on isolated card
   words, which is what flash cards play most, and it costs nothing.
2. **Stand-in characters work with any voice:** for a single-character card using a less
   common reading, speak a character that only has that reading (行 háng → 航, 长 zhǎng →
   掌, 好 hào → 耗, 还 huán → 环, 觉 jiào → 叫…), picked from the dictionary. It raised the
   two-reading items from 16 to 17 (Yaoyao) and 11-13 to 15 (Huihui). Cheap to build.
3. **Linux (no Chinese system voice):** Kokoro is a usable fallback: sentences come out right
   (9-10/10) and card words mostly (41/60 with "。"), at about real time on the CPU.
4. Worth a later look for the tutor (sentences, mixed English): Kokoro full precision, other
   Kokoro speakers, or GPU-class voices.

The original exploration follows.

## Today

The app speaks through the web view's speech synthesis (`src/lib/speech.ts`):

| | Voices | Notes |
|---|---|---|
| Windows 10 (this PC) | Microsoft Huihui, Yaoyao, Kangkang (OneCore) | Older non-neural voices; robotic; this app uses Yaoyao |
| Windows 11 | the same, plus "natural" Narrator voices for some languages | not reliably exposed to web views |
| macOS | Tingting, Meijia, … | decent |
| Linux (WebKitGTK) | often none, or no Chinese voice | the 🔊 buttons and the tutor's voice may be silent |

Problems beyond Linux:
- **Quality and tones:** the Windows 10 voices are clear but flat; their tones are fine in
  sentences but can be clipped on single syllables, which is what flash cards play most.
- **Wrong readings:** a voice reads characters with its default pronunciation. A card for
  行 háng, 长 zhǎng, 好 hào, 了 liǎo, 得 děi… can be read with the other reading, and a
  learner can't tell. The card's own pinyin is ignored.
- **Different everywhere:** each OS (and each Windows version) sounds different.
- **Control:** speed is the only setting; no "stop at the end of this sentence" events in
  WebKitGTK; `speechSynthesis` quirks (queue stalls after `cancel()` in Chromium).

## Local engines (checked 2026-11-09)

| Engine / model | Chinese | Mixed zh+en | Size | Licence | Notes |
|---|---|---|---|---|---|
| **sherpa-onnx** (k2-fsa) runtime | — | — | ~20-30 MB of libraries per OS | Apache-2.0 | Runs the models below on Windows, Linux and macOS (CPU; GPU optional). Has an **official Rust crate** (`sherpa-onnx`, 1.13.x, safe bindings over its C API, prebuilt libraries). The older community `sherpa-rs` crate is deprecated in its favour. |
| **Kokoro-82M multi-lang** (v1.0; v1.1-zh) | yes (v1.1-zh: 100 Chinese speakers) | yes | ~330 MB | Apache-2.0 (Chinese voices from LongMaoData, granted for this use; check for redistribution) | Small, fast on CPU, natural; sherpa-onnx documents a Chinese+English Kokoro model (v1.0) with lexicons for both languages. |
| **MeloTTS Chinese** (`vits-melo-tts-zh_en`) | yes | yes (one speaker) | ~170 MB | MIT | "Fast enough for CPU real-time"; one voice; in sherpa-onnx. |
| sherpa-onnx VITS zh models (aishell3, fanchen, zh-ll…) | yes | no | 100-200 MB | varies per model | Many voices; quality varies; some trained on fan data with unclear rights. |
| Piper `zh_CN-huayan` | yes | no | 60 MB | voice licence "Unknown" (aggregators say non-commercial); the maintained engine (piper1-gpl) is GPL-3 | Avoid. |
| CosyVoice / F5-TTS / Fish-Speech | excellent | yes | 0.5-2 GB | various | GPU-class models; overkill for card words; maybe later for the tutor with the GPU build. |

Sources: sherpa-onnx pre-trained model pages (VITS, Kokoro Chinese+English), the `sherpa-onnx`
crate on docs.rs, the Kokoro-82M-v1.1-zh and MeloTTS-Chinese model cards, Piper's
huayan model card.

## Would it be better on Windows too?

Probably yes, for this app:
- **Better tones and naturalness** than the Windows 10 voices, especially for single words
  (to be confirmed by listening: see the test plan).
- **The card's own pinyin:** sherpa-onnx's Chinese models read text through a lexicon
  (characters to pinyin phones). The app already knows the right pinyin for every card, so
  for a card it can hand the engine that reading (a one-word lexicon override, or phonemes
  built from the pinyin), fixing 多音字 readings. Sentences (the tutor) use the normal
  lexicon, which handles common words correctly.
- **The same voice on every computer**, offline, chosen in Settings (several Kokoro
  speakers, male and female).
- **Mixed Chinese and English** in tutor replies (Kokoro multi-lang, MeloTTS zh_en),
  instead of the English parts being read by a Chinese voice.
- **Full control:** speed, pauses, an exact "finished" event, instant stop, and caching:
  each card's audio is made once and replayed instantly.

Costs:
- A download of ~170-330 MB (one model), plus ~20-30 MB of libraries in the app.
- The first sentence after start takes about a second (loading); after that, generating a
  short sentence should take well under a second on the CPU (to measure).
- A new audio path: Rust makes 16-24 kHz audio, the page plays it with Web Audio (also
  gives "Stop" and "Say again" for free). The OS voices stay as a fallback and an option.

## Plan (if built)

1. **Prototype + measurements** (needs the model download, ~330 MB, your OK):
   - Build sherpa-onnx in a test with Kokoro multi-lang and MeloTTS zh_en.
   - **Round-trip check:** synthesize 100 card words, 30 tone pairs (妈/麻/马/骂…), 20
     polyphones with the card's pinyin, and 10 tutor-style sentences; transcribe them with
     Whisper large-v3-turbo; count how often the right characters/tones come back.
     Same test for the Windows voices (Huihui, Yaoyao) as the baseline.
   - Time per word and per sentence, CPU and GPU.
   - Save the clips so you can **listen** and pick (numbers can't judge naturalness).
2. **App:** a `tts` module (sherpa-onnx), Settings › Audio: engine "Built-in voice
   (download)" / "System voice", voice choice, speed; cached card audio; Web Audio playback
   for the 🔊 buttons, Study autoplay and the tutor (sentence by sentence).
3. **Linux** then gets speech without depending on WebKitGTK.

## Open questions for you

- Download size OK (one model, ~170-330 MB)?
- Prefer one voice (simpler) or a choice of speakers?
- Should the tutor speak its English hints/explanations in an English voice (mixed model)?
