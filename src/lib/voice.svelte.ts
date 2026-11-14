// Which voices this computer has: a Chinese system voice, the built-in one (Kokoro, a download
// in Settings › Audio), or neither. With neither the speaker buttons can't do anything, so
// they are shown switched off, with a note saying why.
import { api } from "./api";
import { setVoices, voicesReady } from "./speech";

export const voice = $state({ available: true, system: true, builtIn: false });

/** Look again (at start, when the system's voices change, after the built-in one is installed or removed). */
export async function checkVoices() {
  const system = (await voicesReady()).length > 0;
  const builtIn = await api
    .kokoroStatus()
    .then((s) => s.installed)
    .catch(() => false);
  voice.system = system;
  voice.builtIn = builtIn;
  voice.available = system || builtIn;
  setVoices(system, builtIn);
}
// (after this module and speech.ts have both finished loading: each imports the other)
setTimeout(checkVoices, 0);
if (typeof window !== "undefined" && "speechSynthesis" in window) {
  window.speechSynthesis.addEventListener("voiceschanged", checkVoices);
}
