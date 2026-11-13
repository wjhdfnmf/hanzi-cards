# Hanzi Cards (汉字卡)

A desktop flash card app for learning Mandarin Chinese (Simplified), for Windows, macOS and Linux.

- **Cards** with characters, pinyin, English meaning, notes and tags, organised into **nested decks**
  (e.g. Class › Class 9; a deck includes its subdecks when listing, studying and exporting)
- **Duplicates**: the same word can be in several decks but is studied once, with progress shared
  between the copies (a copy added later takes it up too), and counted once in the due and new counts; the editor warns about duplicates, and imports skip same-deck duplicates and
  ask before adding words that are already in other decks. In the card list a word that several decks
  hold with the same pinyin and meaning is one row (a chip says how many other decks have it, and
  editing the row keeps the copies the same); copies that differ are listed separately, and "Show
  copies" lists every card
- **Fast card entry**: type pinyin in the characters field and pick from a ranked list, like an input method
  (`nihao` → 你好, `woaini` → 我爱你; add a tone number to narrow: `ni3`). Pinyin and meaning fill in from the
  dictionary and update as you add characters. In the pinyin field, a digit after a syllable sets its tone
  (`ni3` → `nǐ`, `5` = neutral)
- **Study** with FSRS-6 spaced repetition. Pick decks, tags, mode (due & new / new only / cram / difficult), session size, and directions:
  characters → meaning, meaning → characters, listening, and **speaking**: say the answer and it is checked
  by speech recognition, either Windows' built-in Mandarin recognizer or OpenAI Whisper run locally
  (downloaded in Settings). Audio never leaves the computer. Optionally, the pitch of your voice decides
  between words that differ only in tone (妈 mā / 马 mǎ), which Whisper on its own tends to miss
- **AI conversation tutor** (optional, needs your own Anthropic API key): a spoken role-play with Claude
  built from your decks' words. Speak (Whisper, on this computer) or type; it answers aloud, notes which of
  your words you used correctly, suggests new words, and at the end records reviews and adds cards. Each
  conversation can be tuned (level, corrections, English, reply length…), typing has a built-in pinyin
  keyboard, and Settings keeps a record of what the tutor has cost
- **HSK tags**: new cards are tagged with their HSK 3.0 level (2026 syllabus, with the 2021 lists filling gaps;
  combinations of list words get the highest level of their parts). The cards table sorts by any column
- **Dictionary**: CC-CEDICT (~120k entries), searchable by characters, pinyin (with or without tones) or English; one click adds a word as a card. The card editor auto-fills pinyin and meaning from it
- **Statistics** for all decks or one, over the last week, month, quarter, year or all time. The
  simple view has the headline figures (time studied, reviews, how much you remembered against your
  target, new words, streak, what is due), a few sentences on what they mean, time or reviews over
  time, card progress, an activity heatmap and a 30-day forecast. The advanced view adds progress by
  HSK level and by deck, true retention for young and mature words, how you answered, when you study
  (by hour and weekday), time by study direction, how long memories last and how hard words are, and
  the words you forget most. Time on the Reading and AI Tutor pages is counted with flashcard time
- **Tone colours** for characters and pinyin, OS text-to-speech for pronunciation
- **Pleco flashcards** (`.xml`): import categories as (nested) decks (re-importing an updated file skips cards you
  already have; pinyin is checked against the dictionary, with Pleco's reading choosing between readings),
  and export any deck back to Pleco (decks become categories, pinyin as tone numbers)
- **CSV/TSV import & export** (also works with Anki and Pleco text exports); a 6th `deck` column holds
  the subdeck path, e.g. `Class 9` or `Lesson 1/Food`. Every file format the app reads and writes is
  described in [docs/import-formats.md](docs/import-formats.md)
- **Reading**: stories in nested collections (type them, or import text files or a whole folder); export a
  story or a whole collection as plain text files, to keep or to share with other learners (importing
  them gives the same stories back); hover a word for its pinyin and meaning; choose the typeface (free Chinese fonts can be downloaded), size and
  spacing. Stories are kept in a database of their own
- **Your data** (Settings): shows which database files hold your cards and your stories; back them up, save
  a copy anywhere, or switch to a backup or a database from another computer. The app also backs up by
  itself, in a folder of its own that it keeps tidy. See [Data safety](#data-safety)
- English interface by default, Simplified Chinese optional; light and dark themes, and 16 illustrated
  ones in three styles (soft & floral, bold & deep, calm & classic), each with light and dark choices

## Stack

| Layer | Tech |
|---|---|
| Shell | [Tauri 2](https://v2.tauri.app) (system webview: WebView2 / WKWebView / WebKitGTK) |
| Core | Rust: `rusqlite` (bundled SQLite), FSRS-6 scheduler, CC-CEDICT parser/search |
| UI | Svelte 5 + TypeScript + Vite |

Data lives in three SQLite files in the OS app-data folder (`com.hanzicards.app/`):

| File | Holds | Backups |
|---|---|---|
| `hanzi-cards.db` | cards, decks, study progress, settings, the tutor's spending record | `backups/decks` |
| `stories.db` | Reading: stories and collections | `backups/reading` |
| `dictionary.db` | CC-CEDICT (about 25 MB) | none needed: it is made again from the copy that ships with the app |

## Data safety

Your cards and stories are the one thing the app can't make again, so they are guarded in layers:

- **One program at a time.** Only one copy of the app runs (starting it again brings the open window
  forward), and it holds its two databases exclusively: no other program can open them while it runs,
  and if another program already has one, the app says so and waits rather than sharing the file.
- **Every change goes straight into the database file.** The cards and reading databases use SQLite's
  rollback journal with full sync, not a write-ahead log: there is no second file holding recent work
  that could be lost or get out of step with the first. (The dictionary, which can be rebuilt, does use
  a log, so the app keeps reading it while it is being updated.)
- **Checked at every start.** A database that fails SQLite's integrity check is never opened, upgraded or
  studied on. The app opens on a recovery page instead, where a backup can be put in place; the damaged
  file is kept, untouched, in a `damaged-<time>` folder.
- **Backups, verified.** Each backup is read back and checked before it counts; a failed one is
  removed, not left looking like a backup. Yours and the app's are kept apart (below).
- **The dictionary is separate.** It is five times the size of everything else and holds nothing of yours,
  so it has its own file: card backups stay small (a megabyte or two), and a damaged dictionary is simply
  thrown away and rebuilt.
- **Tests never touch the real files.** The benchmark tests that need a learner's cards work on a private
  copy (`db::open_copy`).

### Backups and their cleanup

| | Where | Made | Kept |
|---|---|---|---|
| Your backups | `backups/decks/`, `backups/reading/` | when you press **Back up now** | until you delete them (Settings › Your data has a Delete button for each) |
| Daily | `auto/` inside those folders | at the first start on each day the app is used | one for each of the last 7 days of use, then one a week (the newest of that week) for the 4 weeks before |
| Before an update | `auto/` | the first time a new version starts, before it changes anything | the last 3 |
| Replaced by switching | `auto/` | when you switch to another database | the last 3 |

The app only ever deletes files in `auto/`, and only as the table says: each time it makes an automatic
backup it removes the ones of the same sort that are no longer kept. So automatic backups can't pile
up: at most 17 per database, about 1.6 MB each for 14,000 cards (the dictionary isn't in them), a few
tens of megabytes in all. Nothing is cleaned up while a database is failing its check. Automatic
backups made by earlier versions, which sat among your own, are moved into `auto/` at the first start
and then follow the same rules.

## Security

- The page shows everything you type or import as plain text (it never builds HTML from it), and a
  content security policy lets it load and run only the app's own files and talk only to the app.
- Every database query passes values as parameters. Text is stored without control characters and within
  generous length limits; imported files are size-limited before they are read; the page can save only
  the settings the app knows (`src-tauri/src/input.rs`).
- CSV exports mark cells that a spreadsheet would run as a formula (`=…`, `@…`) as text; importing the
  file takes the mark off again.
- The Anthropic API key is kept in the system keychain, never in a file. Speech is recognised on this
  computer; recordings are not written to disk (only in debug mode, for checking recognition).
- Downloads (dictionary, Whisper models, fonts, NVIDIA's libraries) use HTTPS with time limits.
  NVIDIA's archives and Whisper's model weights are checked against the SHA-256 their publishers give,
  and a download that is cut short or doesn't match is thrown away.

## Develop

Prerequisites: Rust (stable), Node.js 20+, and the [Tauri platform prerequisites](https://v2.tauri.app/start/prerequisites/)
(Windows: MSVC C++ build tools + WebView2; Linux: `webkit2gtk-4.1`, `libayatana-appindicator3` etc.; macOS: Xcode command line tools).

```bash
npm install
npm run tauri dev      # run the app with hot reload
npm run tauri build    # produce installers in src-tauri/target/release/bundle
npm run check          # type-check the front end
cd src-tauri && cargo test   # Rust unit tests (scheduler, cards, study, stats, dictionary, data safety, input checks…)
```

### GPU (optional, NVIDIA)

With the CUDA Toolkit installed, `scripts/dev-gpu.ps1` runs the app with Whisper on the graphics card
(`--features gpu`); a spoken answer then takes about 0.2 s instead of 1-3 s. See `docs/gpu-whisper-plan.md`.

### Windows installer

```powershell
scripts\dev-gpu.ps1 build    # -> src-tauri\target\release\bundle\nsis\Hanzi Cards_<version>_x64-setup.exe
```

The installer (NSIS, per-user, English or Simplified Chinese) contains the app and CC-CEDICT, which is
imported on first start (into `dictionary.db`). Nothing personal is in it: a new install starts with
the 12-word sample deck "Getting Started", made by the app itself, and an empty Reading page. It is
built with GPU support but runs on the processor by default; nothing large is bundled:

- **Whisper models** are downloaded from Hugging Face in Settings › Speech recognition. The Study page
  offers the one that suits the computer, once: Large-v3 Turbo on a working graphics card with 3 GB or
  more of its own memory (Small on a smaller card); on the processor, Small with 8 or more cores and
  8 GB of memory, Base on a more modest computer, Tiny on a very weak one. Settings says why.
- **NVIDIA's libraries** (cuBLAS, cuBLASLt, cuRAND; about 505 MB from NVIDIA's official redistributable
  archives, SHA-256 checked) are offered in Settings only when an NVIDIA card is found. The app
  delay-loads them, so it starts and works without them on any PC.

Building it needs the CUDA Toolkit and Visual Studio C++ build tools (the script finds them); the
plain `npm run tauri build` makes a CPU-only installer.

**Signing.** The installer is not signed, so Windows SmartScreen warns on first run ("Windows
protected your PC"; More info › Run anyway) and Smart App Control on Windows 11 may block it. To sign
it, get a signing identity (Microsoft's Artifact Signing, about $10 a month, or an OV certificate
from a certificate authority) and give Tauri the command or certificate in `tauri.conf.json` under
`bundle › windows` (`signCommand`, or `certificateThumbprint` + `digestAlgorithm` + `timestampUrl`):
see [Tauri's Windows signing guide](https://v2.tauri.app/distribute/sign/windows/). Signing shows
your name as the publisher and lets reputation carry from one release to the next; it does not
remove the warning for a brand-new publisher on day one (Microsoft's
[SmartScreen notes](https://learn.microsoft.com/windows/apps/package-and-deploy/smartscreen-reputation)).
Only the Microsoft Store avoids the warning entirely.

### Linux packages

```bash
npm run tauri build -- --bundles deb,rpm,appimage
```

This makes a `.deb` (Debian, Ubuntu, Mint), an `.rpm` (Fedora, openSUSE) and an AppImage (any
distribution) in `src-tauri/target/release/bundle/`. On Ubuntu, building needs `build-essential
pkg-config file libssl-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev`.
They can be built on Windows inside WSL (Ubuntu).

**Arch Linux:** `packaging/arch/PKGBUILD` (`hanzi-cards-bin`) repackages the release's `.deb`:
`cd packaging/arch && makepkg -si`. The AppImage also runs on Arch. (The PKGBUILD names the last
published release; it is updated, with the new checksum, once a release's `.deb` is on GitHub.)

Chinese characters need a CJK font (`fonts-noto-cjk` on Debian/Ubuntu, `noto-fonts-cjk` on Arch);
the `.deb` and `.rpm` recommend one, and desktop installs usually have one already.
On Linux, Whisper runs on the processor (no GPU build), and the AI tutor's key is kept in the
Secret Service keyring (GNOME Keyring, KWallet, KeePassXC). Reading aloud uses the webview's speech
support, which most distributions' WebKitGTK doesn't include yet, so cards may stay silent there.
The microphone needs GStreamer's PulseAudio or PipeWire source (`gst-plugins-good` or
`gst-plugin-pipewire`).

## Layout

```
src/                  Svelte front end
  views/              Study, AI Tutor, Decks & Cards, Reading, Dictionary, Statistics, Settings
  components/         Card editor, tone-coloured Hanzi/Pinyin, charts, modal, recovery page
  lib/                API wrappers, i18n (en / zh), pinyin helpers, speech, settings
src-tauri/src/
  commands.rs         IPC commands
  db.rs               schema, migrations, settings; how the databases are held (exclusive, journal)
  data.rs             the database files: integrity check, backups, switching, recovery
  input.rs            limits and cleaning for everything typed, imported or saved
  stories.rs          Reading: stories and collections (their own database)
  ai.rs, tutor.rs     Claude API client and the conversation tutor; billing.rs records its cost
  net.rs              downloads (one HTTP agent with time limits)
  decks.rs            nested decks: tree, paths, CRUD
  cards.rs            cards, duplicate checks, CSV import/export
  study.rs            session building, answering
  scheduler.rs        FSRS-6
  stats.rs            statistics
  dictionary.rs       CC-CEDICT import/download/search (its own file, dictionary.db)
  pinyin.rs           tone marks, syllables, search keys, tone sandhi
  speech.rs           checking spoken answers; whisper.rs runs Whisper
  tones.rs            pitch tracking and tone shapes; tone_check.rs picks words by tone
  pleco.rs            Pleco flashcard XML import/export
```

## Licence

Hanzi Cards' own code is under the [MIT licence](LICENSE). The data below keeps its own licences.

## Licences of data

- CC-CEDICT © MDBG, [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/). An unmodified copy ships with the app (`src-tauri/resources/cedict_1_0_ts_utf-8_mdbg.txt.gz`, imported on first start); the Dictionary page can download the latest release.
- FSRS default parameters from the [open-spaced-repetition](https://github.com/open-spaced-repetition) project.
- `src-tauri/data/char_freq.tsv`: character reading frequencies from the Unicode Unihan database (`kHanyuPinlu`),
  © Unicode, Inc., [Unicode License](https://www.unicode.org/license.txt). Regenerate with `scripts/gen_char_freq.py`.
- `src-tauri/data/hsk.tsv`: HSK 3.0 word levels (2026 syllabus and 2021 lists) from
  [complete-hsk-vocabulary](https://github.com/drkameleon/complete-hsk-vocabulary) © Yanis Zafirópulos, MIT licence
  (reproduced at the top of the file).
- Tone checking by pitch (`tones.rs`, `tone_check.rs`) was measured and tuned on the Tone Perfect
  recordings, which are not included in this repository or the app:
  Catherine Ryu, Mandarin Tone Perception & Production Team, and Michigan State University Libraries.
  *Tone Perfect: Multimodal Database for Mandarin Chinese.* Accessed 1 January 2022. https://tone.lib.msu.edu/

## Roadmap ideas

Stroke-order animation and handwriting practice (Make Me a Hanzi data), example sentences (Tatoeba),
character decomposition, bundled HSK 3.0 word lists, sentence segmentation (`jieba-rs`), sync.
