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
| Tones by pitch | with Whisper | not in the build (see below) |
| AI tutor | with your own Anthropic key | not in the build (see below) |
| Stories read by MiniMax | with your own MiniMax key | not in the build (see below) |
| Data | choose files and folders, save copies | the app's own data; backups and restore stay |
| Keeping devices the same | server or client | client only (iOS stops a background app listening) |
| GPU libraries | NVIDIA download | not offered |

**Left out of the build, not only hidden.** `build.rs` sets `cfg(appstore)` for every iPhone build.
With it, `lib.rs` compiles without `ai.rs`, `tutor.rs`, `story_audio.rs`, `tones.rs`,
`tone_check.rs` and `calibration.rs`, and `commands.rs` answers their commands with "not part of this version": nothing
in the phone's program talks to Anthropic or MiniMax, and none of the tone-checking numbers (fitted to the Tone
Perfect and speaker recordings) are in it. The pages are shared, so the tutor's screen is still among
the page files, never shown and with nothing behind it. `HANZI_APPSTORE=1 cargo check --lib` checks
this build on any computer.

**For the App Store** (`gen/apple/hanzi-cards_iOS/`): `Info.plist` has the reasons for the
microphone, speech recognition and local network (LINK) permissions, and
`ITSAppUsesNonExemptEncryption` = true (LINK encrypts with its own code, not Apple's: answer the
encryption questions in App Store Connect once). `PrivacyInfo.xcprivacy` is the privacy manifest:
nothing collected, nothing tracked. The privacy policy and support pages are `PRIVACY.md` and
`SUPPORT.md` at the top of the repository.

Device sync (LINK) is a page of its own in Settings on the phone, and a console on a computer: see `docs/link.md`.

## Layout on a phone (narrower than 640 px)

- The side bar is the icon column. It shows only when a page is pulled down past its top; a
  tap anywhere on the page, scrolling down, a study session or opening a story hides it.
- Settings, Decks and Reading: at the list, the side bar always shows; inside an item it
  stays away until you go back.
- Popups (editing a card or a deck, adding a story...) are pages of their own, with the way back
  on the left.
- The page never zooms: fields are 16 px (iOS zooms into smaller ones when tapped) and the
  viewport doesn't scale.
- One pane at a time: Settings (sections, then one section), Decks (decks, then one deck),
  Reading (collections, then stories, then a story), each with a back control.
- Fewer words: the subtitles under page titles are left out.
- The page reaches under the notch and home bar (`viewport-fit=cover`) and keeps clear of them
  with the safe-area insets.

## Siri and Shortcuts

"Start studying in Hanzi Cards" (and "Study Chinese with Hanzi Cards", "Study my cards in
Hanzi Cards") is an App Shortcut: `gen/apple/Sources/hanzi-cards/StudyIntent.swift`. It opens
the app and leaves the word `study` in `siri-action` in the app's data folder; the page looks
for it every 1.5 s while open (`commands::pending_action`; iOS doesn't tell the web view when
the app returns to the front) and starts a session with the learner's usual choices. After
adding a Swift file, run `xcodegen generate` in `gen/apple` and keep `Info.plist` as it was
(Tauri fills it in at build time).

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

On your own iPhone (team `3VRF7M4STV` is set in `tauri.conf.json` › `bundle.iOS`): plug it in,
turn on Developer Mode (Settings › Privacy & Security; the switch only appears after Xcode or
`devicectl` has tried to use the phone), add the Apple ID in Xcode › Settings › Accounts, then

```sh
npx tauri ios build --debug --target aarch64
xcrun devicectl device install app --device <UDID> "<DerivedData>/Build/Products/debug-iphoneos/Hanzi Cards.app"
xcrun devicectl device process launch --device <UDID> com.hanzicards.app
```

(`xcrun devicectl list devices` shows the UDID.) The very first signed build needs the phone
registered with the team: one `xcodebuild … -destination id=<UDID>
-allowProvisioningUpdates -allowProvisioningDeviceRegistration build` does it (its Rust step
fails outside Tauri; that's fine, the registration and profile are what it's for).

The first build takes several minutes (all the crates); later ones only rebuild what changed.
