# Import and export formats

What Hanzi Cards reads and writes, so that files can be prepared by hand, produced by other
programs, kept as plain-text copies, and passed between learners.

Everything is plain text in **UTF-8** (UTF-16 with a byte-order mark, which Excel and Notepad
call "Unicode text", is read too). Nothing below needs the app to be online.

| What | Where in the app | File |
|---|---|---|
| [Stories](#stories-reading) | Reading › Import / Export | `.txt`, one story each; folders of them |
| [Cards as a table](#cards-csv-or-tsv) | Decks & Cards › Import / Export | `.csv`, `.tsv`, `.txt` |
| [Pleco flashcards](#pleco-flashcards) | Decks & Cards › Import / Export | `.xml` |
| [The dictionary](#the-dictionary) | Dictionary, or Settings › Dictionary | CC-CEDICT `.u8`, `.txt`, `.gz` |
| [A whole database](#whole-databases) | Settings › Your data | `.db` |

---

## Stories (Reading)

### One story, one text file

The simplest story file is just the text. The file's name is the title:

```
小猫找妈妈.txt
```
```text
小猫找不到妈妈了。
它问小狗："你看见我妈妈了吗？"
```

To give a story a title of its own and a list of key vocabulary, use the sectioned form.
This is also exactly what **Export** writes:

```text
小猫找妈妈

--- Key Vocab ---
小猫 – xiǎo māo – kitten
找 – zhǎo – to look for
看见 – kàn jiàn – to see

--- Story ---
小猫找不到妈妈了。
它问小狗："你看见我妈妈了吗？"

小狗说："没看见。"
```

The rules:

- **Title**: the first line that isn't empty, above the first `--- … ---` line. If there is
  none, the file name is the title.
- **`--- Story ---`** starts the story. A file is read in the sectioned form only if it has
  this line (any capitals: `--- story ---` works); otherwise the whole file is the story.
- **`--- Key Vocab ---`** (any heading with "vocab" in it) starts the vocabulary: one word per
  line as `word – pinyin – meaning`, with a dash (`–`, `—` or `-`) that has a space on each
  side between the parts. Shown above the story, and can be hidden.
- Any other `--- Heading ---` section (notes, a translation, questions) is **left out**. That
  also means a line of that shape inside a story ends the story: write such a line another
  way.
- Blank lines separate paragraphs. Line breaks are kept as they are.
- The app adds pinyin and meanings itself when you point at a word (from the dictionary), so
  a story needs no annotation.
- Limits: 5 MB a file, a million characters a story, 200 characters a title.

### Order: numbers in front of names

A number at the start of a file name sets the story's place in its collection and is not part
of the title:

```
01_小猫找妈妈.txt      → "小猫找妈妈", first
02_下雨了.txt          → "下雨了", second
10 The Fox.txt         → "The Fox", tenth
```

`_`, `.`, a space or `-` may follow the number. Stories without a number come first, in
alphabetical order (numbers inside titles sort naturally: "Story 9" before "Story 10"). A
title that itself begins with a number is exported as `00_…` so the number stays in the title.

### A folder is a collection

**Import › Folder…** takes a folder of `.txt` files and makes it a collection, with its
subfolders as collections inside it:

```
Graded readers/
├── 01_Newbie/
│   ├── 01_小猫找妈妈.txt
│   └── 02_下雨了.txt
├── 02_Elementary/
│   └── 01_去北京.txt
└── About this set.txt
```

- Folder names follow the same number rule (`01_Newbie` is the collection "Newbie", first).
- Folders named after a level sort themselves even without numbers: newbie, beginner,
  elementary, pre-intermediate, intermediate, upper-intermediate, advanced, native.
- Only `.txt` files are read. Other files, and links to elsewhere, are skipped; a file that
  isn't text is skipped and counted, without stopping the rest.
- Importing the same folder again adds only what is new: a story whose title is already in
  that collection is skipped. So a set can be re-imported after its author adds to it.
- Up to 12 levels of folders and 5,000 stories in one folder.

**Import › Text files…** adds chosen files to the collection that is open.

### Exporting, and sharing stories

- **A story**: open it and press the download button. It is saved as one `.txt` file in the
  sectioned form above.
- **A collection**: open it and press **Export**, then choose where. A new folder is made
  there, named after the collection, holding its stories as `.txt` files and its
  subcollections as folders, all numbered so their order survives. From "All stories" the
  export is everything, in a folder called `Hanzi Cards stories`. An existing folder is never
  written into: a second export makes `Name (2)`.

To share stories, send the file or the folder (zipped, for a folder) and have the other
person use **Import**. Exporting and importing give back the same titles, text, vocabulary and
order; it is the same format in both directions, so stories written by hand, exported from
the app, or produced by a script are all equally good.

What an export does **not** carry: the collection's description, your reading font and size
(those are settings), and anything of your study progress. Stories are yours to share only if
their text is: a story copied from a book or a site keeps its author's rights.

---

## Cards: CSV or TSV

**Decks & Cards › Import** (with a deck chosen) and **Export › CSV**.

Columns, in this order; only the first is required:

| # | Column | Notes |
|---|---|---|
| 1 | characters | Simplified characters. A row without any Chinese character is skipped. |
| 2 | pinyin | Tone marks (`nǐ hǎo`) or tone numbers (`ni3 hao3`, `ni3hao3`; `5` or `0` = neutral; `v` or `u:` = ü), converted to marks. |
| 3 | meaning | English. Several senses separated by `;`. |
| 4 | notes | Free text. |
| 5 | tags | Separated by `;` (or `,` inside one quoted field). |
| 6 | deck | A subdeck path below the deck you import into: `Class 9` or `Lesson 1/Food`. Decks that don't exist are made. |

```csv
hanzi,pinyin,meaning,notes,tags,deck
你好,ni3 hao3,hello,,greetings,Lesson 1
谢谢,xiè xie,thank you; thanks,"said often, costs nothing",greetings;polite,Lesson 1
米饭,mi3fan4,cooked rice,,food,Lesson 2/Food
```

- **Separator**: a tab if the first line has one (TSV, and the plain-text exports of Anki and
  Pleco), otherwise a comma. Fields with commas, quotes or line breaks go in double quotes,
  with `""` for a quote, as usual for CSV.
- **Header**: optional. A first row whose first field is `hanzi`, `character(s)`,
  `simplified`, `chinese`, `word` or `汉字` is taken as the header and skipped.
- Lines starting with `#` are comments.
- **Duplicates**: a word already in the same deck is skipped. A word already in *another*
  deck is asked about first (it would be studied once, with shared progress, either way).
- New cards get their HSK level as a tag (`HSK1` … `HSK7-9`) unless that is off in Settings.
- Limits: 50 MB a file; 200 characters for the characters, 400 pinyin, 2,000 meaning, 10,000
  notes (longer fields are cut, the row is kept).

**Export** writes the same six columns with a header, tags joined by `;`, and the deck path
below the exported deck (the full path when exporting all cards). Two things to know:

- Study progress is not exported: a CSV is your cards, not your history. To move everything,
  use a [whole database](#whole-databases).
- A cell that a spreadsheet would run as a formula (starting with `=` or `@`, or with `+`/`-`
  followed by a function call) is written with an apostrophe in front, which spreadsheets
  show as plain text. Importing the file into Hanzi Cards takes the apostrophe off again.

---

## Pleco flashcards

**Decks & Cards › Import** with a Pleco `.xml` export (Pleco: *Import/Export › Export Cards*,
"XML File", with card definitions and categories included), and **Export › Pleco**.

- Each Pleco **category** becomes a deck; `/` in a category name nests decks (`Class/Class 9`
  is the deck "Class 9" inside "Class"). A card in several categories goes into each deck,
  and is studied once. Cards without a category go into the deck you choose, or a deck called
  "Pleco".
- **Pinyin** comes as Pleco writes it (tone numbers) and is converted to tone marks. With
  "check against the dictionary" on, the dictionary's pinyin is used, with Pleco's reading
  choosing between readings (行 háng / xíng); a missing definition is filled in too.
- Importing an updated export again skips the cards you already have.
- **Export** writes decks as categories, pinyin as tone numbers, and the traditional form
  from the dictionary where there is one. Study progress is not included.
- Limit: 100 MB a file. The file must be UTF-8.

---

## The dictionary

The app ships with CC-CEDICT and can download the current release itself. A file can also be
given: the CC-CEDICT text file (`cedict_ts.u8`) or its `.gz` archive, one entry per line:

```text
傳統 传统 [chuan2 tong3] /tradition/traditional/convention/
```

Traditional, simplified, pinyin with tone numbers in square brackets, then the senses between
slashes. Lines starting with `#` are comments. Importing a file replaces the whole dictionary
(it lives in its own file, `dictionary.db`, and is never part of a backup).

---

## Whole databases

To move everything, cards with their study history or all your stories, to another computer:
**Settings › Your data › Save a copy…** writes one `.db` file, and **Use another database ›
Choose file…** on the other computer switches to it (the one in use there is backed up first).
Cards and reading are separate files and are moved separately. These are SQLite files and are
meant for Hanzi Cards itself, not for editing.
