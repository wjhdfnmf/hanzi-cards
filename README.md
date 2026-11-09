# Hanzi Cards (汉字卡)

A desktop flash card app for learning Mandarin Chinese (Simplified), for Windows, macOS and Linux.

- **Cards** with characters, pinyin, English meaning, notes and tags, organised into **nested decks**
  (e.g. Class › Class 9; a deck includes its subdecks when listing, studying and exporting)
- **Duplicates**: the same word can be in several decks but is studied once, with progress shared
  between the copies; the editor warns about duplicates, and imports skip same-deck duplicates and
  ask before adding words that are already in other decks
- **Fast card entry**: type pinyin in the characters field and pick from a ranked list, like an input method
  (`nihao` → 你好, `woaini` → 我爱你; add a tone number to narrow: `ni3`). Pinyin and meaning fill in from the
  dictionary and update as you add characters. In the pinyin field, a digit after a syllable sets its tone
  (`ni3` → `nǐ`, `5` = neutral)
- **Study** with FSRS-5 spaced repetition. Pick decks, tags, mode (due & new / new only / cram / difficult), session size, and directions:
  characters → meaning, meaning → characters, listening, and **speaking**: say the answer and it is checked
  by speech recognition, either Windows' built-in Mandarin recognizer or OpenAI Whisper run locally
  (downloaded in Settings). Audio never leaves the computer
- **AI conversation tutor** (optional, needs your own Anthropic API key): a spoken role-play with Claude
  built from your decks' words. Speak (Whisper, on this computer) or type; it answers aloud, notes which of
  your words you used correctly, suggests new words, and at the end records reviews and adds cards. Each
  conversation can be tuned (level, corrections, English, reply length…), typing has a built-in pinyin
  keyboard, and Settings keeps a record of what the tutor has cost
- **HSK tags**: new cards are tagged with their HSK 3.0 level (2026 syllabus, with the 2021 lists filling gaps;
  combinations of list words get the highest level of their parts). The cards table sorts by any column
- **Dictionary**: CC-CEDICT (~120k entries), searchable by characters, pinyin (with or without tones) or English; one click adds a word as a card. The card editor auto-fills pinyin and meaning from it
- **Statistics**: reviews per day, retention, streaks, a 30-day forecast, card progress, an activity heatmap and the cards you forget most
- **Tone colours** for characters and pinyin, OS text-to-speech for pronunciation
- **Pleco flashcards** (`.xml`): import categories as (nested) decks (re-importing an updated file skips cards you
  already have; pinyin is checked against the dictionary, with Pleco's reading choosing between readings),
  and export any deck back to Pleco (decks become categories, pinyin as tone numbers)
- **CSV/TSV import & export** (also works with Anki and Pleco text exports); a 6th `deck` column holds
  the subdeck path, e.g. `Class 9` or `Lesson 1/Food`
- English interface by default, Simplified Chinese optional; light and dark themes

## Stack

| Layer | Tech |
|---|---|
| Shell | [Tauri 2](https://v2.tauri.app) (system webview: WebView2 / WKWebView / WebKitGTK) |
| Core | Rust: `rusqlite` (bundled SQLite), FSRS-5 scheduler, CC-CEDICT parser/search |
| UI | Svelte 5 + TypeScript + Vite |

Data lives in a single SQLite file in the OS app-data folder (`com.hanzicards.app/hanzi-cards.db`).

## Develop

Prerequisites: Rust (stable), Node.js 20+, and the [Tauri platform prerequisites](https://v2.tauri.app/start/prerequisites/)
(Windows: MSVC C++ build tools + WebView2; Linux: `webkit2gtk-4.1`, `libayatana-appindicator3` etc.; macOS: Xcode command line tools).

```bash
npm install
npm run tauri dev      # run the app with hot reload
npm run tauri build    # produce installers in src-tauri/target/release/bundle
npm run check          # type-check the front end
cd src-tauri && cargo test   # Rust unit tests (scheduler, pinyin, cards, study, stats, dictionary)
```

### GPU (optional, NVIDIA)

With the CUDA Toolkit installed, `scripts/dev-gpu.ps1` runs the app with Whisper on the graphics card
(`--features gpu`); a spoken answer then takes about 0.2 s instead of 1-3 s. See `docs/gpu-whisper-plan.md`.

## Layout

```
src/                  Svelte front end
  views/              Study, Decks & Cards, Dictionary, Statistics, Settings
  components/         Card editor, tone-coloured Hanzi/Pinyin, charts, modal
  lib/                API wrappers, i18n (en / zh), pinyin helpers, speech, settings
src-tauri/src/
  commands.rs         IPC commands
  db.rs               schema, migrations, settings
  decks.rs            nested decks: tree, paths, CRUD
  cards.rs            cards, duplicate checks, CSV import/export
  study.rs            session building, answering
  scheduler.rs        FSRS-5
  stats.rs            statistics
  dictionary.rs       CC-CEDICT import/download/search
  pinyin.rs           tone marks, syllables and search keys
  pleco.rs            Pleco flashcard XML import/export
```

## Licences of data

- CC-CEDICT © MDBG, [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/). Downloaded by the user at runtime, not bundled.
- FSRS default parameters from the [open-spaced-repetition](https://github.com/open-spaced-repetition) project.
- `src-tauri/data/char_freq.tsv`: character reading frequencies from the Unicode Unihan database (`kHanyuPinlu`),
  © Unicode, Inc., [Unicode License](https://www.unicode.org/license.txt). Regenerate with `scripts/gen_char_freq.py`.
- `src-tauri/data/hsk.tsv`: HSK 3.0 word levels (2026 syllabus and 2021 lists) from
  [complete-hsk-vocabulary](https://github.com/drkameleon/complete-hsk-vocabulary) © Yanis Zafirópulos, MIT licence
  (reproduced at the top of the file).

## Roadmap ideas

Stroke-order animation and handwriting practice (Make Me a Hanzi data), example sentences (Tatoeba),
character decomposition, bundled HSK 3.0 word lists, sentence segmentation (`jieba-rs`), sync.
