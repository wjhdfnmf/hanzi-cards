# LINK: keeping devices in step

LINK keeps one learner's cards, decks, progress, statistics and stories the same on several
devices: computers and iPhones. It needs no account and no outside service: one of your own
devices (or a small standalone program) keeps the latest copy, and the others sync with it.

## How it works

- **One server, any number of clients.** The server keeps a single *box*: the latest copy of
  everything, encrypted. A sync is: read this device, fetch the box, open it, merge the two,
  write the result here, seal it and put it back. If another device synced in between, the
  server says so and the steps are done again.
- **The server can't read the box.** Each device encrypts the copy before it leaves
  (XChaCha20-Poly1305). A standalone server never has the key at all; a computer that is the
  server has it only because it studies too.
- **Only your server is trusted.** Devices talk over TLS 1.3, and a client accepts only the
  server whose certificate it was paired with. The pairing code carries that certificate's
  fingerprint, the server's password and the seed of the key to the box. Treat it like a
  password.
- **Merging keeps the latest of both**, the same in either order (`sync/src/merge.rs`): the
  last edit of a card or deck wins, deleted things stay deleted unless edited afterwards, every
  answer from everywhere is kept, a word's schedule comes from its last review, and *Reset
  statistics* / *Reset study progress* stay reset on every device.
- **Nothing is left half-changed.** Databases are checked first, backed up before anything
  is written, written in one transaction each, and checked again. Clocks more than 5 minutes
  apart are refused (the merge compares times).
- **Keys stay in the keychain** (the server's password, the key to the box, a server's
  private key), never in a file.

The port is 47810 unless chosen otherwise.

## On a computer: the console

On Windows, macOS and Linux, LINK is set up in a console: Settings, then the empty space at
the bottom of the list of sections. The side bar then shows a light (is the other side
there?) and a Sync button.

| Command | What it does |
|---|---|
| `status` | What is set up on this device |
| `host [port]` | Make this computer the server; shows the pairing code and the line to type on the others |
| `join <address> <code>` | Pair this device with a server |
| `code` | Show the pairing code again, for another device |
| `ip` | This computer's addresses: on the same network, and from outside (the router must forward the port) |
| `ping` | Is the other side there? |
| `sync` | Send what is new here, take what is new there |
| `online` / `offline` | Server: listen for your devices, or stop |
| `forget` | Undo the setup on this device (`forget yes`); cards and history stay |

A typical setup: on the computer that is usually on, `host`. Allow Hanzi Cards through the
firewall for private networks when asked, then `sync` once so the box holds its cards. On each
other device, `join <address> <code>` with what `host` printed, then `sync`.

## On an iPhone: Settings › LINK

An iPhone is always a client: iOS stops an app listening as soon as it is in the background,
so it can't be the server. Settings › LINK has:

- **Pairing:** the computer's address and the pairing code (from `host` or `code` on the
  computer). Pair connects, checks the server's certificate and syncs once.
- **Once paired:** the computer's address with its light, when it was last in step, *Sync
  now*, *Check connection*, and *Unpair* (cards and progress stay on the phone).
- **Sync automatically** (on by default): the phone syncs when the app opens or comes back to
  the front, and again as you leave it, so a session studied on the train is on the computer
  by the time you sit down. Leaving is handled by the app itself (`sync::sync_on_leaving`,
  when iOS sends the window's *Suspended* event), not the page, because iOS may stop the page
  moments later. A sync that is cut short changes nothing, and the next one catches up.

The side bar shows the same light and Sync button as on a computer once the phone is paired.

The phone must be able to reach the computer: on the same Wi-Fi, or from outside with the
router forwarding the port to the computer, or over a VPN that both are on.

## A standalone server

`sync/src/bin/hanzi-sync-server.rs` is a small program for a computer that is always on
(a home server, a Raspberry Pi):

    hanzi-sync-server [--dir <folder>] [--port <n>] [--bind <address>] [--new-code]

The first run makes its certificate and prints its code, once. Join it from the first device;
that device then shows the full code (with the key) for the others. `--new-code` replaces the
code.

## Where the code is

- `sync/`: the shared crate (snapshot, merge, seal, store, net) and the standalone server.
- `src-tauri/src/sync.rs`: the app's side (config, keychain, commands, the server runtime,
  syncing on leaving); `sync_data.rs` reads and writes the databases.
- `src/lib/sync.svelte.ts`: the light, the Sync button and automatic syncing on a phone;
  `components/Console.svelte` the console; Settings › LINK in `views/SettingsView.svelte`.
