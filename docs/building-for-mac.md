# Building Hanzi Cards for macOS

Written 2026-11-12, against version 0.3.1 of the code. Nothing here has been built or run on a
Mac yet: it is a plan made from the code, Tauri's documentation and GitHub's. Each place that
needs a real Mac to confirm says so.

## Recommendation

**Build on GitHub, test on your Mac.** GitHub's macOS runners can build the app for both Apple
Silicon and Intel without you owning the build setup, and the repository is public, so the
runners are free. That fits how you want to work. What GitHub cannot do is look at the app:
somebody has to open it on a Mac once per release and try the microphone, the voices and the
themes. So the Mac's job is testing (and fixing what the test finds, with Claude there if you
like), not building.

The first build will show whether anything fails to compile on macOS. The Linux build already
exercises every "not Windows" path in the code, so I expect few surprises, but that is a
prediction, not a result.

## What the project already has

| Needed on macOS | State |
|---|---|
| App icon (`icons/icon.icns`) | Present |
| Microphone permission text (`src-tauri/Info.plist`, `NSMicrophoneUsageDescription`) | Present |
| API key storage (`keyring` with `apple-native`) | Present in `Cargo.toml` |
| Memory detection for the Whisper recommendation (`sysctl hw.memsize`) | Present |
| Opening the data folder in Finder (`open`) | Present |
| Chinese fonts in the font stacks (PingFang SC, Kaiti SC) | Present; both ship with macOS |
| One-instance lock, storage design, backups | Platform-neutral |

## What has to be added

1. **Bundle targets.** `tauri.conf.json` lists only `nsis`, so on a Mac the build must name its
   bundles, as the Linux build does: `npm run tauri build -- --bundles app,dmg`.
2. **A `macOS` section in `tauri.conf.json`:**

   ```json
   "macOS": {
     "minimumSystemVersion": "13.0",
     "signingIdentity": "-",
     "entitlements": "Entitlements.plist"
   }
   ```

   - `minimumSystemVersion`: the pages use newer CSS (`color-mix`, among others), which the
     system web view has from macOS 13. The exact floor needs a check on an older Mac if you
     want to go lower.
   - `signingIdentity: "-"` is ad-hoc signing (see Signing below). A real identity in the
     environment overrides it.
3. **`src-tauri/Entitlements.plist`** with `com.apple.security.device.audio-input` set to true.
   A signed app runs with the hardened runtime, and without this entitlement the microphone is
   refused even though the permission text is there. (From Apple's and Tauri's documentation;
   to confirm on a Mac.)
4. **The workflow file** below.

## What will behave differently on a Mac

| Feature | On macOS | Confidence |
|---|---|---|
| Reading aloud | Uses the web view's speech with macOS voices. A Chinese voice (Tingting) is normally installed; better ones are a download in System Settings › Accessibility › Spoken Content | Expected to work; voice quality to be heard |
| Choosing the output device | Not available (Windows-only today; Safari's engine has no `setSinkId`) | Certain |
| Windows speech recognition | Not available; Whisper is the only engine, as on Linux | Certain |
| Whisper speed | Runs on the processor. Apple Silicon is fast enough for Small or Base. `candle` has an `accelerate` feature (Apple's maths library) that should speed it up, and a `metal` one for the graphics chip; neither is wired in | To measure on your Mac before adding |
| NVIDIA speed-up | Never offered (no NVIDIA on Macs); the Settings section hides itself already when no card is found | Expected |
| Themes and layout | Safari's engine instead of Chromium's. The decorations use `clip-path`, `color-mix`, SVG backgrounds and text-decoration tricks, all supported, but pixel differences are likely | To look at |
| Data location | `~/Library/Application Support/com.hanzicards.app/` | Certain |

## Signing: the real decision

macOS checks who made an app that came from the internet. There are three levels.

| | Cost | What a user sees on first open |
|---|---|---|
| Unsigned | Free | On Apple Silicon: "is damaged and can't be opened". Not usable |
| **Ad-hoc signed** (`signingIdentity: "-"`) | Free | "Apple could not verify…": they open System Settings › Privacy & Security and press **Open Anyway**, once. Tauri's documentation says the same: ad-hoc signing does not remove this step |
| **Developer ID + notarization** | Apple Developer Program, $99 a year | The ordinary "downloaded from the internet, open?" prompt |

Ad-hoc is the Mac equivalent of today's unsigned Windows installer: it works, with a warning to
click through. Start there. If you later join the developer programme, the same workflow signs
and notarizes once these repository secrets exist (names from Tauri's guide):

- Signing: `APPLE_CERTIFICATE` (the certificate as base64 `.p12`), `APPLE_CERTIFICATE_PASSWORD`,
  `APPLE_SIGNING_IDENTITY`. The certificate type for apps outside the App Store is **Developer
  ID Application**; only the account holder can create it.
- Notarization: `APPLE_API_ISSUER`, `APPLE_API_KEY`, `APPLE_API_KEY_PATH` (an App Store Connect
  API key; the option that suits unattended builds), or `APPLE_ID`, `APPLE_PASSWORD`,
  `APPLE_TEAM_ID`.

A free Apple account cannot notarize.

## The workflow

Save as `.github/workflows/macos.yml`. It runs when you ask for it (Actions › macOS build › Run
workflow) with the tag of a release that already exists, builds both architectures, checks the
signature, and adds the two `.dmg` files to that release. So the release procedure stays as it
is (Windows and Linux built here, release created here), and the Mac files join it a few
minutes later.

```yaml
name: macOS build

on:
  workflow_dispatch:
    inputs:
      tag:
        description: "Release tag to build and upload to (for example v0.3.1)"
        required: true

permissions:
  contents: write

jobs:
  build:
    runs-on: macos-latest          # an Apple Silicon machine; it builds the Intel version too
    strategy:
      fail-fast: false
      matrix:
        include:
          - target: aarch64-apple-darwin   # Apple Silicon
          - target: x86_64-apple-darwin    # Intel
    steps:
      - uses: actions/checkout@v4
        with:
          ref: ${{ inputs.tag }}

      - uses: actions/setup-node@v4
        with:
          node-version: lts/*
          cache: npm

      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}

      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: src-tauri
          key: ${{ matrix.target }}

      - run: npm ci

      - name: Build
        run: npm run tauri build -- --target ${{ matrix.target }} --bundles app,dmg

      - name: Check the signature
        run: codesign --verify --deep --strict --verbose=2 "src-tauri/target/${{ matrix.target }}/release/bundle/macos/Hanzi Cards.app"

      - name: Add the disk image to the release
        env:
          GH_TOKEN: ${{ github.token }}
        run: gh release upload "${{ inputs.tag }}" src-tauri/target/${{ matrix.target }}/release/bundle/dmg/*.dmg --clobber
```

Notes on it:

- The two disk images are named by Tauri with their architecture (`…_aarch64.dmg`,
  `…_x64.dmg`), so they don't collide. One universal image is possible
  (`--target universal-apple-darwin`) at about twice the size; two files and a line in the
  release notes saying which Mac needs which is simpler.
- A first run without touching a release: replace the last step with
  `actions/upload-artifact@v4` and download the `.dmg` from the run's page.
- The Rust tests can run in the same job (`cargo test --no-default-features` in `src-tauri`),
  which would be the first time they run on macOS. Worth adding once the build is green.

## What to test on the Mac, in order

1. It opens (after **Open Anyway**), imports the dictionary, shows the sample deck.
2. Study a card with **Say it**: the microphone prompt appears once, and an answer is heard.
   This is the step most likely to need a fix (entitlement, permission text).
3. Reading aloud speaks Chinese with a Chinese voice.
4. Download a Whisper model in Settings and time a spoken answer.
5. The AI tutor: key saved to the Keychain, a short conversation.
6. Look through three or four themes, the Statistics page, a long deck, the Reading page.
7. Back up, switch database, restore: the storage design is the same code, but macOS file
   locking is its own implementation.
8. Close and reopen: one instance only, data intact.

## If you would rather build on the Mac itself

It is the same project:

```bash
xcode-select --install
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
# Node 20 or newer, from nodejs.org or Homebrew
git clone https://github.com/wjhdfnmf/hanzi-cards.git && cd hanzi-cards
npm install
npm run tauri dev                              # run it
npm run tauri build -- --bundles app,dmg       # src-tauri/target/release/bundle/dmg/
```

A build made and run on the same Mac is not quarantined, so it opens without any warning: that
is the quickest way to do the test list above. Claude on the Mac starts without this
computer's notes (the release steps and the database safety rules live in this machine's
memory, not in the repository), so point it at this file and at the README's Data safety
section first.

## Suggested order of work

1. Add the `macOS` section, `Entitlements.plist` and the workflow; run it without the upload
   step. Fix whatever fails to compile.
2. Download the Apple Silicon image on your Mac and go down the test list.
3. Fix what the test finds (most likely on the Mac, where it can be tried at once).
4. Turn the upload step on, add a macOS section to the release notes and the README.
5. Later, if wanted: the `accelerate` feature for Whisper, and Developer ID signing.

Sources: Tauri's [macOS code signing guide](https://v2.tauri.app/distribute/sign/macos/) and
[tauri-action](https://github.com/tauri-apps/tauri-action) (their example builds both
architectures on `macos-latest`).
