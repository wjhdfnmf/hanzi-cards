# Building Hanzi Cards for iPhone

Written 2026-11-14, after the first builds on a Mac (Xcode 27.2, iOS 27.2 simulator).

**Where it stands:** the app builds for the iOS simulator and runs there. It hasn't been put on
a real iPhone yet: that needs an Apple development team set in `tauri.conf.json`
(`bundle › iOS › developmentTeam`) or `APPLE_DEVELOPMENT_TEAM`.

## One code base

The phone build is the same app, not a copy:

- **Rust:** `commands.rs` is the same everywhere. What a phone can't do is behind
  `#[cfg(mobile)]` / `#[cfg(target_os = "ios")]` with a stand-in that answers the same calls
  (`kokoro_mobile.rs`, the Apple branch in `speech.rs`).
- **Pages:** shared. `src/lib/device.svelte.ts` says whether the screen is a phone
  (`device.phone`, layout) and whether it's iOS (`ios`, what's offered).

## What is different on an iPhone

| | Computer | iPhone |
|---|---|---|
| Voice (text to speech) | system voices, or the built-in Kokoro voice | Apple's Chinese voices only |
| Spoken answers | Whisper, or Windows' recognizer | Apple's speech recognition, on the device only (`apple_speech.rs`) |
| Tones by pitch | with Whisper | not offered (Apple's recognizer can't be steered that way) |
| Data | choose files and folders, save copies | the app's own data; backups and restore stay |
| Keeping devices the same | server or client | client only (iOS stops a background app listening) |
| GPU libraries | NVIDIA download | not offered |

Device sync stays where it is on every platform: behind the hidden door in Settings.

## Layout on a phone (narrower than 640 px)

- The side bar is the icon column. It slides away while a page scrolls down, during a study
  session and while a story is read; scrolling up, a swipe in from the left edge or the tab at
  the edge brings it back.
- One pane at a time: Settings (sections, then one section), Decks (decks, then one deck),
  Reading (collections, then stories, then a story), each with a back control.
- Fewer words: the subtitles under page titles are left out.
- The page reaches under the notch and home bar (`viewport-fit=cover`) and keeps clear of them
  with the safe-area insets.

## Setting up a Mac

Needed: Xcode (accept its licence once: `sudo xcodebuild -license`), Rust with the iOS
targets, Node, and XcodeGen.

```sh
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
npm ci
```

Without Homebrew, `tauri ios init` stops on three tools it wants to install with it:

- **XcodeGen:** needed; unpack the release from github.com/yonaskolb/XcodeGen and put its
  `bin` on `PATH`.
- **libimobiledevice** (`idevicesyslog`) and **CocoaPods** (`pod`): only for real devices and
  pods, which this project doesn't use. A stub script named `idevicesyslog`, and one named
  `pod` that answers `--version`, on `PATH` for `init` are enough. Nothing calls them.

The Xcode project (`src-tauri/gen/apple`) is in git, so `init` is only needed again after a
Tauri upgrade.

## Building

```sh
npx tauri ios build --debug --target aarch64-sim   # an .app for the simulator
npx tauri ios dev                                  # the simulator, with the pages reloading live
```

The first build takes several minutes (all the crates); later ones only rebuild what changed.
