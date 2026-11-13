# Voice-interactive AI study: exploration

Status: phases 1-3 (Windows voice drill, Whisper, conversation tutor) are built; phase 4 is a plan. Written 2026-11-08.

## Goal

A "Speak" page where you study by talking: the app speaks, you answer out loud in
Mandarin, and it grades you, corrects you and keeps a conversation going, using the
words in your decks.

## What I checked on this PC

| Check | Result |
|---|---|
| Chinese text-to-speech in the app's WebView2 | **Works.** Voices: Microsoft Huihui, Yaoyao, Kangkang (zh-CN), plus English voices |
| Windows speech recognition languages | en-US and **zh-Hans-CN** installed (WinRT and System.Speech) |
| Microphone capture in the WebView (`getUserMedia`, `MediaRecorder` with WebM/Opus, `AudioWorklet`) | Available (not started, to avoid a permission prompt) |
| `SpeechRecognition` in the WebView | The object exists, but WebView2 doesn't ship the online recognition service behind it, so it can't be relied on (and it isn't in Linux's WebKitGTK at all) |
| Claude Messages API audio input | **None.** User content accepts text, image, document and tool blocks only (checked against the live API reference) |

So the pipeline is: **microphone → speech-to-text → app logic and/or Claude (text) →
text-to-speech**.

## Two modes

### 1. Voice drill (no AI needed; build first)

The existing study session, spoken. The app shows or says the prompt (English meaning,
or plays the audio), you say the Chinese, and the answer is checked and graded into FSRS
like a normal review.

- The expected answer is known, so recognition can be **constrained** to the card plus
  a few look-alike distractors. That is far more accurate for a learner's accent than
  open dictation, and it works offline.
- Grading: exact match → Good; matched only after a retry → Hard; wrong word or nothing
  understood → Again. Show what was heard (characters and pinyin) next to the answer.
- Adds a "speaking" direction next to recognition, recall and listening.

### 2. Conversation tutor (Claude)

Role-play short scenes ("ordering at a café", "meeting a classmate") built from the
decks you pick, mostly from words you already know plus a few due ones.

- Claude replies in simple Mandarin, sentence by sentence, and each sentence is spoken
  as soon as it streams in, to keep latency low.
- Tools the app gives Claude:
  - `grade_word(hanzi, correct)` feeds FSRS for words you used or missed.
  - `suggest_card(hanzi, pinyin, meaning)` queues new words. At the end you tick which to
    add, through the existing editor with dictionary auto-fill.
  - `show(pinyin | english)` reveals help on screen when you ask "什么意思?".
- Session summary at the end: mistakes, corrections and new words.

## Speech recognition options

| Option | Platforms | Offline | Mandarin quality | Notes |
|---|---|---|---|---|
| **Windows `Windows.Media.SpeechRecognition`** (Rust `windows` crate) | Windows only | List/grammar constraints: yes. Free dictation: uses Microsoft's online service (privacy setting) | Good with constraints | Ideal for voice drill on Windows; zh-Hans-CN already installed here |
| **Whisper** (whisper.cpp via `whisper-rs`) | Windows, macOS, Linux | Yes | Good open transcription; may "auto-correct" your pronunciation into the right word | Model download: base ≈ 140 MB, small ≈ 470 MB. Best default for the conversation tutor |
| Cloud speech services (e.g. Azure Speech) | All | No | Very good; some offer **pronunciation and tone scoring** for zh-CN | Paid, needs a separate key. Worth evaluating if tone feedback becomes the priority |
| WebView `SpeechRecognition` | Unreliable (see above) | — | — | Not recommended |

Recommendation:
- Use **Windows constrained recognition** for voice drill on Windows.
- Use **Whisper** for the conversation tutor and as the cross-platform fallback.
- Either way, audio stays on the device and only text leaves it.

Whisper and open dictation tend to snap to the nearest real word, so neither reliably
catches *tone* mistakes. Constrained recognition helps: put same-syllable, different-tone
distractors in the list (mǎi 买 vs mài 卖). Real tone scoring needs a pronunciation
assessment service.

## Speech output

Keep the current Web Speech synthesis (`speak()` in `src/lib/speech.ts`). The zh-CN
voices now work on this PC, and macOS ships Mandarin voices.
- Add a slower rate for drills.
- Speak streamed tutor replies sentence by sentence, splitting on 。！？.
- Linux voices (espeak) are poor; a cloud voice could be an optional upgrade later.

## Claude integration

- **Model:** Claude Opus 5.5 (`claude-opus-5-5`, $4 / $20 per million input / output
  tokens) at **low effort** for snappy conversational turns.
  - Thinking is always on for this model. Effort is the control, and the default is
    `medium`, so set `low` explicitly and raise it only if the tutoring quality needs it.
  - Claude Haiku 5.5 ($0.10 / $0.50) is the faster, much cheaper option if you prefer;
    it's your call.
- **Streaming** for every turn, so speech starts before the reply is finished.
- **Prompt caching:**
  - Keep the system prompt (tutor rules, scenario, your vocabulary list) stable.
  - Cache it, and append turns only, so each turn mostly re-reads cached tokens.
- **Rough cost:** a 10-minute conversation of about 20 turns with a cached vocabulary
  prompt is about **$0.10–0.25 on Opus 5.5** (mostly input tokens). An estimate, to be
  measured with `usage` once built.
- **Where the call happens:**
  - Make it from **Rust**, so the key never enters the web page.
  - There's no official Rust SDK, so use plain HTTPS to `POST /v1/messages` with SSE
    streaming (the app already has `ureq`; an async client such as `reqwest` would suit
    streaming better).
  - Stream text chunks to the page with Tauri events.
- **API key:**
  - Pasted once in Settings, stored in the OS keychain (`keyring` crate: Windows
    Credential Manager / macOS Keychain / Secret Service), never in the SQLite file or in
    localStorage.
  - A "Test key" button.
  - The AI page is hidden until a key is set.
- **Safety and robustness:**
  - Check `stop_reason` on every response (`refusal`, `max_tokens`).
  - Handle 429/5xx with retry and backoff.
  - Show cost per session from `usage`.

## Desktop specifics

- **Microphone permission:**
  - **Windows:** WebView2 shows a permission prompt the first time.
  - **macOS:** needs `NSMicrophoneUsageDescription` in the bundle's Info.plist.
  - **Linux:** WebKitGTK needs media-stream support enabled.
- **Push-to-talk** (hold Space) is simpler and more reliable than voice-activity
  detection. Add VAD later.
- **Capture:** `MediaRecorder` in the page sends WebM/Opus or 16 kHz PCM to Rust over
  a Tauri command; Rust runs Whisper or the Windows recognizer.

## Suggested phases

1. **Voice drill, Windows** (built): the "Meaning → Say it" study direction.
   - `src-tauri/src/speech.rs`: WinRT recognizer limited to the answer, other-tone
     distractors and other session words, graded into FSRS. Offline, no key.
   - Press Space or the microphone to speak, Enter to show the answer, Esc to stop.
   - The suggested rating is highlighted and Enter accepts it:
     - correct and clear → Good
     - correct but unsure, or right on the second try → Hard
     - wrong twice → Again
2. **Whisper backend** (built): `src-tauri/src/whisper.rs`.
   - OpenAI Whisper run with candle, a pure-Rust library, so the build needs no C++ toolchain.
   - Models (tiny 151 MB, base 290 MB, small 967 MB) are downloaded from Hugging Face in
     Settings › Speech recognition, which also has the engine choice and a microphone test.
   - The mel filterbank and token decoding are implemented in the app.
   - On this PC, base transcribes a short phrase in about 1 s on the CPU. Dependencies
     are optimised even in dev builds.
   - Recording happens in the page (`src/lib/mic.ts`): 16 kHz mono, starting when you
     speak and stopping after 0.9 s of silence. Only text leaves the recorder.
   - Judging (`speech::judge_text`) converts traditional to simplified and accepts the
     same characters or the same toned pinyin (homophones like 他/她). The right
     syllables with a wrong tone are reported as a tone mistake.
   - Accuracy work (2026-11-08), measured with the `whisper_bench` test on 60 synthetic
     clips (clean and with added noise; 40 card words, 10 sentences, 10 words not on any card):
     - Decoding favours the learner's card words: a token that continues a card word gets
       +1.0 log-probability (`whisper::Options::bias_words`). Stronger bias caused loops on noise.
     - Latin letters and digits (ASCII and full-width) are blocked: answers are Mandarin.
     - The drill also gives Whisper the session's words as a `<|startofprev|>` prompt; the
       tutor gives it the tutor's last line instead (a word list garbles sentences).
     - Result for single card words with Whisper base: 17 → 29 of 40 right (clean),
       10 → 22 (noisy). Words not on cards were unaffected (6/10 → 5/10).
     - Looping output ("祝福祝福祝福…") is collapsed and the length is capped by the audio length.
     - Beam search (5) was tried and dropped: slower, no gain on these clips.
     - "Automatic" now prefers Whisper when a model is downloaded: the Windows recognizer must
       pick from a list that includes wrong-tone look-alikes, which looked like "different words".
     - In debug mode (Settings › About) the last recording is kept as `last-recording.wav` in the
       app's data folder for checking; otherwise recordings aren't written to disk.
     - Whisper small (2026-11-09) beats base everywhere: drill answers 33/40 vs 29/40 (clean),
       29/40 vs 22/40 (noisy); tutor sentences 9/10. It is the model in use on this PC.
   - CPU speed (2026-11-09, Ryzen 9 5900X, Whisper small):
     - The encoder used to process a padded 30 s window for every answer (about 2 s). It now
       gets the speech plus 2 s, at least 10 s (`SPEECH_WINDOW` in `commands.rs`): about 3x
       faster with the same accuracy. Shorter windows (1-5 s) made Whisper invent text.
     - The decoder is our own (`whisper_net.rs`, same weights as candle's) with a key/value
       cache, so earlier tokens and the audio keys aren't recomputed for each new token. With
       the drill's word-list prompt this halved decoding time.
     - Net: a short answer takes about 0.8 s (tutor) / 1.1 s (drill), down from 2.9 / 5.6 s.
     - The model is loaded in the background when a speaking session or the tutor opens.
     - Tried and dropped: beam search, thread count (12/16/24 the same), building for
       AVX2/FMA (x86-64-v3: no measurable gain, and old CPUs couldn't run the app).
     - Next step for speed and accuracy: the GPU (candle's CUDA backend on NVIDIA cards needs
       the CUDA Toolkit to build), which would also make Whisper large-v3-turbo practical.
   - Not yet tested: macOS (needs the `NSMicrophoneUsageDescription` in
     `src-tauri/Info.plist`) and Linux (WebKitGTK must allow media streams).
3. **Conversation tutor** (built): the AI Tutor page, `src-tauri/src/ai.rs` and
   `src-tauri/src/tutor.rs`.
   - The learner's own Anthropic API key is checked with a free call (listing models) and
     stored in the system keychain (`keyring`: Windows Credential Manager, macOS Keychain,
     Linux Secret Service). It is never shown again; Settings shows its last four characters.
   - Claude is called over HTTPS from Rust (there is no Rust SDK), streaming, with Opus 5.5
     by default (Haiku 5.5 selectable in Settings), low effort and prompt caching. Server-side
     fallback is on for Opus: if a turn is declined it can be retried on another model.
   - The system prompt holds the scene and up to 400 of the learner's words from the chosen
     decks (due words marked). Replies are short, Chinese only (they are read aloud), tolerant
     of speech-recognition slips, with gentle 「更好的说法：…」 corrections.
   - Two strict tools: `grade_word` (the learner used one of their words, right or wrong) and
     `suggest_card` (a useful new word). They fill a live side panel.
   - Speech in: Whisper transcribes on this computer, only text is sent. Speech out: each
     sentence is spoken as soon as it has streamed in. Typing works too, as does English for
     questions. Pinyin (from the dictionary) and an English translation are one click away.
   - At the end: record the graded words as "Meaning → Say it" reviews (Good / Again) and add
     the suggested words as cards (tagged `ai-tutor`). The page shows the running cost.
   - Per-conversation options next to the setup (remembered): level (HSK 1 / 2 / 3-4), reply
     length, corrections (off / gentle / every mistake explained), English (never / when asked /
     hints for new words, which aren't read aloud), new words per turn, who leads, only my words,
     voice speed, model, and free-text requests.
   - Typing: a built-in pinyin keyboard (拼) offers dictionary words as you type pinyin, with
     tone digits and Chinese punctuation; Ctrl+Space or the button switches to plain English.
   - Spending record: every API call's tokens and estimated cost go into the `ai_usage` table
     (also cancelled or failed turns; output of a cancelled stream is estimated). Settings › AI
     tutor shows today / 7 days / month / all time, each conversation, and exports CSV.
   - Tested: everything up to the API call (key check rejects a bad key, the tutor page,
     options, pinyin typing and summary with mocked replies).
4. **Optional**: tone and pronunciation scoring through a cloud assessment service.

## Open questions for you

- Is Windows-only acceptable for the first version of voice drill, or should Whisper
  (cross-platform, about 140–470 MB download) come first?
- Are you happy to use your own Anthropic API key for the tutor, at roughly the cost
  above?
- Is tone feedback important enough to consider a paid pronunciation-assessment service?
