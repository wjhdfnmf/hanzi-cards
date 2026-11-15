# Support

Hanzi Cards is a free, open-source app for learning Mandarin with flash cards, for iPhone, Windows,
macOS and Linux.

## Getting help

- **Something isn't working, or you have a question or an idea:** open an issue at
  <https://github.com/wjhdfnmf/hanzi-cards/issues>. Say which device and which version of the app
  (Settings › About), and what you did just before.
- **How things work:** the [README](README.md) describes every page of the app.
  - [Importing cards and stories](docs/import-formats.md): the file formats the app reads.
  - [LINK](docs/link.md): keeping your cards the same on several devices.
- **Privacy:** the app collects nothing. See the [privacy policy](PRIVACY.md).

## Common questions

**Where are my cards kept?** On your device, in the app's own data folder. Nothing is stored
anywhere else unless you set up LINK with a computer or server of your own.

**How do I move my cards to another device?** Settings › LINK, on both devices: one holds the shared
copy (`host`), the others `join` it. Or export a deck to a file (Decks) and import it on the other
device.

**I deleted the app. Are my cards gone?** On an iPhone, yes, unless another of your devices has them
through LINK. On a computer the data folder stays when the app is removed unless you ask the
uninstaller to delete it, and the app makes backups of its own (Settings › Your data).

**Spoken answers don't work on my iPhone.** Check Settings › Privacy & Security › Microphone and
› Speech Recognition, and that Hanzi Cards is turned on in both. The app only uses speech
recognition that runs on the phone itself, which needs a recent iPhone and iOS; if the phone can't
do that for Chinese, the app says so and answers can be typed.

**LINK can't find my computer.** Both devices need to be on the same network (or the server's port,
47810 unless you changed it, must be reachable from outside), the computer's firewall must let the
app listen, and on an iPhone Settings › Privacy & Security › Local Network must have Hanzi Cards
turned on. `ping` in the LINK console says whether the server answers.
