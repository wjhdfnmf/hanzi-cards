# Voices for reading aloud: options and a recommendation

Written 2026-11-13. **Built since (same day): the Kokoro backup voice.** Settings › Audio has
"Built-in voice (Kokoro)": a 147 MB download, checked against a pinned SHA-256, unpacked into
the app's data folder, and used automatically when the computer has no Chinese voice (or
when chosen). Still to do from this plan: the choice of "stories and tutor only", stand-in
characters, keeping card audio, and trying it on Linux and macOS.

The rest of this page is the review as written before that. It leans on the
measurements already made on this computer (`docs/local-tts.md`, 130 test items judged by
Whisper, 2026-11-09 and 2026-11-11). Where a statement comes from documentation or a model
card and was not tried here, it says so.

## Recommendation

1. **Keep the system voice as the default wherever a Chinese one exists.** On this computer
   Yaoyao was the clearest voice of everything tested on single card words (52 of 60), it is
   instant, costs nothing, and it is the only option that tells the app which word it is
   saying, which the new story highlighting uses.
2. **Add one built-in voice as a download: Kokoro (Chinese + English), run on the processor
   through sherpa-onnx.** It is the fallback for computers with no Chinese voice (most Linux
   installs, some Windows and Mac setups), and an option everywhere for the tutor and stories,
   where it reads mixed Chinese and English properly and sounds the same on every computer.
3. **Do not ship a graphics-card voice now.** The best one measured (Qwen3-TTS 0.6B) still
   lost to Yaoyao on card words, took about 6 seconds a clip here, sometimes ran on babbling,
   and needs 3 to 6 GB of extra downloads. It would only make sense for preparing card audio
   in advance, and the gain isn't there yet.
4. **Settings gets one "Voice" choice with a recommendation**, built like the Whisper one:
   the app looks at what the computer has and says what it suggests and why.

## What each part of the app needs

| Where | What is read | What matters most |
|---|---|---|
| Flash cards (speaker button, Listening direction) | one word, often one syllable | the right tones and reading on a lone word; instant |
| Stories (Read aloud) | sentences, for minutes | natural flow; knowing which word is being said, for highlighting |
| AI tutor | sentences, Chinese mixed with English | natural flow; mixed languages; starting quickly |

Single words are the hard case. Every neural model tested is trained on sentences and rushes,
swallows or mis-tones a lone syllable. That is why no single engine wins everywhere.

## The options

### 1. System voices (what the app uses today)

| System | Chinese voices | Notes |
|---|---|---|
| Windows 10 and 11 | Huihui, Yaoyao, Kangkang | Older, flat-sounding, but clear. Measured here: Yaoyao 52/60 card words, 10/10 sentences |
| Windows 11 "natural" voices (Xiaoxiao and others) | much better sounding | Installed for Narrator. Edge's own browser can use them; whether an app's embedded web view can is **not confirmed**. Worth one test on a Windows 11 computer: if they appear in the app's voice list, they are the best free option there. This computer runs Windows 10, so I could not check |
| macOS | Tingting and others | Expected to be good; not heard yet (the Mac build is untested) |
| Linux | usually none | The speaker buttons are now switched off with a note when there is no voice |

Strengths: free, instant, nothing to download, word-by-word timing. Weaknesses: differs on
every computer, absent on Linux, reads a character with two readings (行, 长, 好) its default
way, reads English inside Chinese badly.

### 2. A built-in voice on the processor

Run by **sherpa-onnx**, which has an official Rust library and built on Windows without extra
setup in the earlier prototype. It adds roughly 20 to 30 MB to the app; the voice itself is a
download.

| Model | Size | Measured here | Verdict |
|---|---|---|---|
| **Kokoro v1.1 (Chinese + English)**, 103 speakers, Apache-2.0 | about 150 MB (8-bit) to 330 MB | 41/60 card words, 9 to 10 of 10 sentences, about 0.6 s per word on the processor | **The one to ship.** Good on sentences, acceptable on words |
| Matcha (Chinese + English), 1 speaker | about 125 MB with its vocoder | Not tested | Smaller; worth measuring beside Kokoro before deciding |
| MeloTTS (Chinese + English) | 170 MB | 4 to 13 of 60 card words; one clip in six silent | Rejected |
| ZipVoice (voice cloning) | — | 27/60; many clips cut short | Rejected |

Kokoro gives no word-by-word timing through this route, so stories read with it would mark
the sentence being read but not the word. The play button added today already works that way
when a voice reports no words.

### 3. A voice on the graphics card

| Model | Measured here | Cost to ship |
|---|---|---|
| Qwen3-TTS 0.6B CustomVoice (Apache-2.0; five native Chinese speakers, per its model card) | 49/60 card words, 10/10 sentences, about 6 s per clip on a shared RTX 3070; 7 of 120 single-word clips ran on for 5 to 41 s | PyTorch (2 to 3 GB) plus the model (1.2 GB or more). Its model card advertises streaming with very low delay on a graphics card; that was not what I measured, and I found no supported way to run it without Python |
| IndexTTS-2.5 | 42/60; 7 to 12 s per clip | Licence needs the publisher's agreement for commercial use |
| CosyVoice 3 (0.5B) | 37/60; about 2 s per word | The only one fast enough for a live tutor |

All three got every test sentence right, so for the tutor and stories the difference between
them is naturalness, which only listening can judge (`tts-samples/` has clips from each).
None beat the system voice on card words. A graphics-card voice also splits the audience:
it helps only people with an NVIDIA card, who are the ones already best served.

### 4. An online voice (not local)

Microsoft's Azure Speech service has the same natural Chinese voices as Windows 11's
Narrator, with word timing. It would work like the tutor does today: the learner brings
their own key, text is sent to the service, and it is paid for by use. I have not checked
current prices. It is the highest-quality route and the simplest to build, but it needs the
internet and sends the text off the computer, which the rest of the app avoids. The free
"read aloud" service that Edge uses is not an option: it is not offered for other programs.

## How Settings would work

One section, **Settings › Audio › Voice**:

- A single list: the system's Chinese voices by name, then "Built-in voice (Kokoro)" with its
  download size or "installed", each with a Play button to hear a sample.
- Under it, the recommendation in a sentence, as for Whisper.
- A second choice once the built-in voice is installed: use it for **everything**, or only
  for **stories and the tutor** (keeping the system voice for single card words, where it
  measured better).

The recommendation:

| The computer has | Suggested | Why (shown to the learner) |
|---|---|---|
| A Chinese system voice | System voice; the built-in one offered as an extra for stories and the tutor | Clearest on single words, instant, nothing to download |
| No Chinese voice | Built-in voice, offered once as a download, the way the Whisper model is | Without it nothing can be read aloud |
| No Chinese voice and a slow processor (the same test the Whisper recommendation uses) | Built-in voice, 8-bit version | Smaller and faster |
| Windows 11 with natural voices visible to the app (if the test above passes) | The natural system voice | Best quality at no cost |

When playing, the app falls back in order: the chosen voice, then any other that is
available, then the buttons are switched off with the note they show today.

## What building it takes

1. **Measure first** (a day): Kokoro full precision against 8-bit, two or three of its
   speakers, and Matcha, on the same 130 items; memory use; time to first sound. Check the
   Windows 11 natural voices in the app's web view on a Windows 11 computer.
2. **Engine** (the largest piece): add sherpa-onnx to the backend, a voice download with the
   size and checksum checks the Whisper download already has, and make the existing
   "speech as a sound file" path serve any engine. The page side already plays such files
   one sentence at a time (it does so today when an output device is chosen).
3. **Settings and the recommendation**, the one-time offer on the Study page, and the
   fallback order.
4. **Two cheap improvements that help every voice:**
   - Stand-in characters for cards whose character has two readings (speak 航 for 行 háng):
     measured to fix one or two of 20 such cards.
   - Keeping each card's audio after it is first made, so a slower voice is only slow once.
5. **Linux and Mac packaging** of the sherpa-onnx libraries, to be confirmed on each.

## What I could not verify

- Whether Windows 11's natural voices reach the app (needs a Windows 11 computer).
- How any of these sound on macOS.
- Kokoro beyond the 8-bit model and two speakers; Matcha at all.
- Current speeds of the graphics-card models with the card to themselves, and any
  Python-free way to run them.
- Azure's present prices and terms.
