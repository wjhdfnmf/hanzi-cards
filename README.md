# Hanzi Cards (汉字卡)

A desktop flash card app for learning Mandarin Chinese (Simplified), for Windows, macOS and Linux.

- **Cards** with characters, pinyin (type `ni3 hao3` → `nǐ hǎo`), English meaning, notes and tags, organised into decks
- **Study** with FSRS-5 spaced repetition. Pick decks, tags, mode (due & new / new only / cram / difficult), session size, and directions:
  characters → meaning, meaning → characters, and listening
- **Dictionary**: CC-CEDICT (~120k entries), searchable by characters, pinyin (with or without tones) or English; one click adds a word as a card. The card editor auto-fills pinyin and meaning from it
- **Statistics**: reviews per day, retention, streaks, a 30-day forecast, card progress, an activity heatmap and the cards you forget most
- **Tone colours** for characters and pinyin, OS text-to-speech for pronunciation
- **CSV/TSV import & export** (works with Pleco and Anki text exports)
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

## Layout

```
src/                  Svelte front end
  views/              Study, Decks & Cards, Dictionary, Statistics, Settings
  components/         Card editor, tone-coloured Hanzi/Pinyin, charts, modal
  lib/                API wrappers, i18n (en / zh), pinyin helpers, speech, settings
src-tauri/src/
  commands.rs         IPC commands
  db.rs               schema, migrations, settings
  cards.rs            decks, cards, CSV import/export
  study.rs            session building, answering
  scheduler.rs        FSRS-5
  stats.rs            statistics
  dictionary.rs       CC-CEDICT import/download/search
  pinyin.rs           tone marks and search keys
```

## Licences of data

- CC-CEDICT © MDBG, [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/). Downloaded by the user at runtime, not bundled.
- FSRS default parameters from the [open-spaced-repetition](https://github.com/open-spaced-repetition) project.

## Roadmap ideas

Stroke-order animation and handwriting practice (Make Me a Hanzi data), example sentences (Tatoeba),
character decomposition, bundled HSK 3.0 word lists, sentence segmentation (`jieba-rs`), sync.
