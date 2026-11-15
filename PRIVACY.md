# Privacy policy

*Hanzi Cards, for iPhone, Windows, macOS and Linux. Last changed 15 November 2026.*

**Hanzi Cards collects nothing.** There are no accounts, no analytics, no advertising and no
tracking, and the people who make the app run no server that it talks to. Your cards, your study
history and your stories are files on your own device.

## What stays on your device

- **Cards, decks, study progress, statistics, stories and settings.** They are kept in the app's own
  data folder. Removing the app removes them (on a computer, the folder can also be deleted by hand).
- **Your voice.** When you say an answer aloud, the recording is checked on the device and then
  dropped. On an iPhone this is done by Apple's speech recognition, which the app only uses in its
  on-device mode: the recording is not sent to Apple or to anyone else. On a computer it is done by a
  speech model that runs on the computer. Recordings are not kept, except on a computer when you turn
  on *Debug mode* yourself, which saves tone-calibration recordings in the app's data folder.

## When the app uses the network

Only when you ask for one of these. Each connection is to the service named, which sees what any
download sees (your internet address and the file asked for) and nothing about you or your cards.

| What you do | Where the app connects | What is sent |
|---|---|---|
| Keep your devices in step (**LINK**) | The computer or server **you** chose, by the address you typed | Your cards, progress, statistics and stories, encrypted on your device with a key only your devices have. The server stores them without being able to read them. |
| Ask LINK for your public address (`ip` in its console) | `api.ipify.org`, or `icanhazip.com` | Nothing but the request; the answer is your internet address. |
| Download the latest dictionary | `mdbg.net` (CC-CEDICT) | Nothing but the request. |
| Download a reading font | `github.com` | Nothing but the request. |
| *Computers only:* download a speech model, the built-in voice, or NVIDIA's libraries | `huggingface.co`, `github.com`, `developer.download.nvidia.com` | Nothing but the request. |
| *Computers only:* have a story's sound made (Reading › Make audio) | MiniMax (`api.minimax.io`, or `api.minimaxi.com` for mainland China), with **your own** API key | The text of that story, once. The sound that comes back is kept on your computer. MiniMax's own terms and privacy policy apply to it. The key is kept in your system's keychain. |
| *Computers only:* talk with the AI tutor | Anthropic (`api.anthropic.com`), with **your own** API key | What you say or type to the tutor, and the words from the decks you chose for the conversation. Anthropic's own terms and privacy policy apply to it. The key is kept in your system's keychain. |

The iPhone app has no AI tutor and no made story sound, and sends nothing to any AI service.

## Permissions on an iPhone

- **Microphone** and **Speech Recognition**: to check answers you say aloud, on the device.
- **Local Network**: to reach your own computer or server for LINK.

Each is asked for the first time it is needed, and the app works without them: answers can be typed,
and LINK is optional.

## Children

The app has nothing aimed at children and collects nothing from anyone, children included.

## Changes

This page is kept with the app's source code, and its history shows every change:
<https://github.com/wjhdfnmf/hanzi-cards/commits/main/PRIVACY.md>

## Questions

Open an issue at <https://github.com/wjhdfnmf/hanzi-cards/issues>.
