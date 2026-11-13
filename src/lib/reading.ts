// Text settings for the Reading page (also shown in Settings).

export type FontGroup = "sans" | "serif" | "script" | "decorative" | "traditional";

export interface ReadingFont {
  key: string;
  group: FontGroup;
  /** Font names on Windows, macOS and Linux; the first one installed wins. */
  names: string[];
  generic: "sans-serif" | "serif";
}

/** "sans" is the app's own Chinese font (always there); the rest are shown when installed. */
export const READING_FONTS: ReadingFont[] = [
  { key: "sans", group: "sans", names: [], generic: "sans-serif" },
  { key: "hei", group: "sans", names: ["SimHei", "Heiti SC", "STHeiti", "WenQuanYi Zen Hei"], generic: "sans-serif" },
  { key: "dengxian", group: "sans", names: ["DengXian", "等线"], generic: "sans-serif" },
  {
    key: "notosans",
    group: "sans",
    names: ["Noto Sans SC", "Noto Sans CJK SC", "Source Han Sans SC", "Source Han Sans CN"],
    generic: "sans-serif",
  },
  { key: "yuan", group: "sans", names: ["YouYuan", "Yuanti SC", "STYuanti"], generic: "sans-serif" },
  { key: "song", group: "serif", names: ["SimSun", "Songti SC", "STSong", "NSimSun", "AR PL UMing CN"], generic: "serif" },
  {
    key: "notoserif",
    group: "serif",
    names: ["Noto Serif SC", "Noto Serif CJK SC", "Source Han Serif SC", "Source Han Serif CN"],
    generic: "serif",
  },
  { key: "fangsong", group: "serif", names: ["FangSong", "STFangsong", "FangSong_GB2312"], generic: "serif" },
  { key: "kai", group: "script", names: ["KaiTi", "Kaiti SC", "STKaiti", "KaiTi_GB2312", "AR PL UKai CN"], generic: "serif" },
  { key: "wenkai", group: "script", names: ["LXGW WenKai", "LXGW WenKai Screen", "霞鹜文楷"], generic: "serif" },
  { key: "mashanzheng", group: "script", names: ["Ma Shan Zheng"], generic: "serif" },
  { key: "lishu", group: "script", names: ["LiSu", "STLiti", "Libian SC"], generic: "serif" },
  { key: "xingkai", group: "script", names: ["STXingkai", "Xingkai SC"], generic: "serif" },
  { key: "zhimangxing", group: "script", names: ["Zhi Mang Xing"], generic: "serif" },
  { key: "caoshu", group: "script", names: ["Liu Jian Mao Cao"], generic: "serif" },
  { key: "longcang", group: "script", names: ["Long Cang"], generic: "sans-serif" },
  { key: "handwriting", group: "script", names: ["HanziPen SC", "Hannotate SC", "Wawati SC"], generic: "sans-serif" },
  { key: "kuaile", group: "decorative", names: ["ZCOOL KuaiLe"], generic: "sans-serif" },
  { key: "xiaowei", group: "decorative", names: ["ZCOOL XiaoWei"], generic: "serif" },
  { key: "huangyou", group: "decorative", names: ["ZCOOL QingKe HuangYou"], generic: "sans-serif" },
  {
    key: "jhenghei",
    group: "traditional",
    names: ["Microsoft JhengHei", "PingFang TC", "Noto Sans TC", "Noto Sans CJK TC"],
    generic: "sans-serif",
  },
  {
    key: "ming",
    group: "traditional",
    names: ["PMingLiU", "MingLiU", "Songti TC", "Noto Serif TC", "Noto Serif CJK TC"],
    generic: "serif",
  },
];

export const FONT_GROUPS: FontGroup[] = ["sans", "serif", "script", "decorative", "traditional"];

export const SIZE_MIN = 14;
export const SIZE_MAX = 48;
export const LINE_MIN = 1.3;
export const LINE_MAX = 2.6;

/** The CSS font-family for a font's key (the app's own font for an unknown key). */
export function readingFont(key: string): string {
  const f = READING_FONTS.find((x) => x.key === key);
  if (!f || !f.names.length) return "var(--font-han)";
  return [...f.names.map((n) => `"${n}"`), "var(--font-han)", f.generic].join(", ");
}

export const clampSize = (n: number) => Math.round(Math.min(SIZE_MAX, Math.max(SIZE_MIN, n)));

// ---------- which fonts are installed ----------
// A browser can't list fonts without asking, so measure: text in a missing font falls back to
// the generic family and measures the same as it; an installed font measures differently from
// at least one of the generic families.
let installedCache: Set<string> | null = null;

function hasFont(ctx: CanvasRenderingContext2D, name: string): boolean {
  const sample = "mmwwlli 永和九年汉字阅读 ABC123";
  const measure = (family: string) => {
    ctx.font = `40px ${family}`;
    const m = ctx.measureText(sample);
    return `${m.width.toFixed(2)}|${m.actualBoundingBoxAscent.toFixed(2)}|${m.actualBoundingBoxDescent.toFixed(2)}`;
  };
  return ["monospace", "sans-serif", "serif"].some((base) => measure(`"${name}", ${base}`) !== measure(base));
}

/** Keys of the fonts this computer has ("sans" always). */
export function installedFonts(): Set<string> {
  if (installedCache) return installedCache;
  const out = new Set<string>(["sans"]);
  try {
    const ctx = document.createElement("canvas").getContext("2d");
    if (!ctx) throw new Error("no canvas");
    for (const f of READING_FONTS) if (f.names.some((n) => hasFont(ctx, n))) out.add(f.key);
  } catch {
    for (const f of READING_FONTS) out.add(f.key); // can't tell: offer them all
  }
  installedCache = out;
  return out;
}
