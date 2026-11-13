// Reading fonts the app downloads itself (into its data folder) and loads with FontFace, so they
// work without being installed in the system.
import { listen } from "@tauri-apps/api/event";
import { api, type FontFile } from "./api";
import { READING_FONTS, installedFonts } from "./reading";

export const fontFiles = $state({
  /** The downloadable fonts and whether each is downloaded. */
  list: [] as FontFile[],
  /** Download progress (0-100) by key while downloading. */
  progress: {} as Record<string, number>,
});

const faces = new Map<string, Promise<FontFace>>();

export const isDownloadable = (key: string) => fontFiles.list.some((f) => f.key === key);
export const isDownloaded = (key: string) => fontFiles.list.some((f) => f.key === key && f.downloaded);

export async function refreshFonts() {
  installedFonts(); // measure system fonts before any downloaded one is added to the page
  try {
    fontFiles.list = await api.fontsList();
  } catch {
    fontFiles.list = [];
  }
}

/** Load a downloaded font into the page (once); does nothing for other fonts. */
export function ensureFont(key: string): Promise<void> {
  if (!isDownloaded(key)) return Promise.resolve();
  let p = faces.get(key);
  if (!p) {
    p = (async () => {
      const font = READING_FONTS.find((f) => f.key === key);
      if (!font) throw new Error(`unknown font ${key}`);
      const data = await api.fontData(key);
      // Noto Serif SC is a variable font with every weight in one file
      const face = new FontFace(font.names[0], data, key === "notoserif" ? { weight: "200 900" } : {});
      await face.load();
      document.fonts.add(face);
      return face;
    })();
    p.catch(() => faces.delete(key)); // try again next time
    faces.set(key, p);
  }
  return p.then(() => undefined);
}

export async function downloadFont(key: string) {
  fontFiles.progress[key] = 0;
  const unlisten = await listen<{ name: string; received: number; total: number }>("font-download", (e) => {
    const { name, received, total } = e.payload;
    if (name === key) fontFiles.progress[key] = total ? Math.floor((received / total) * 100) : 0;
  });
  try {
    await api.fontDownload(key);
    await refreshFonts();
    await ensureFont(key);
  } finally {
    unlisten();
    delete fontFiles.progress[key];
  }
}

export async function removeFont(key: string) {
  await api.fontDelete(key);
  const p = faces.get(key);
  faces.delete(key);
  if (p) {
    try {
      document.fonts.delete(await p);
    } catch {
      /* wasn't loaded */
    }
  }
  await refreshFonts();
}
