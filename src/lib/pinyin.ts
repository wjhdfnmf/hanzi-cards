// Pinyin helpers for the UI: numbered -> tone marks, and tone detection for colouring.

const MARKS: Record<string, string[]> = {
  a: ["ā", "á", "ǎ", "à"],
  e: ["ē", "é", "ě", "è"],
  i: ["ī", "í", "ǐ", "ì"],
  o: ["ō", "ó", "ǒ", "ò"],
  u: ["ū", "ú", "ǔ", "ù"],
  ü: ["ǖ", "ǘ", "ǚ", "ǜ"],
  A: ["Ā", "Á", "Ǎ", "À"],
  E: ["Ē", "É", "Ě", "È"],
  I: ["Ī", "Í", "Ǐ", "Ì"],
  O: ["Ō", "Ó", "Ǒ", "Ò"],
  U: ["Ū", "Ú", "Ǔ", "Ù"],
  Ü: ["Ǖ", "Ǘ", "Ǚ", "Ǜ"],
};

const TONE_OF = new Map<string, number>();
for (const marks of Object.values(MARKS)) marks.forEach((m, i) => TONE_OF.set(m, i + 1));

function applyTone(syllable: string, tone: number): string {
  const s = syllable.replace(/u:/g, "ü").replace(/U:/g, "Ü").replace(/v/g, "ü").replace(/V/g, "Ü");
  if (tone < 1 || tone > 4) return s;
  const lower = s.toLowerCase();
  let idx = lower.indexOf("a");
  if (idx < 0) idx = lower.indexOf("e");
  if (idx < 0) idx = lower.indexOf("ou");
  if (idx < 0) {
    for (let i = lower.length - 1; i >= 0; i--) {
      if ("aeiouü".includes(lower[i])) {
        idx = i;
        break;
      }
    }
  }
  if (idx < 0) return s;
  const mark = MARKS[s[idx]]?.[tone - 1];
  return mark ? s.slice(0, idx) + mark + s.slice(idx + 1) : s;
}

/** "ni3 hao3" -> "nǐ hǎo"; text without tone numbers is returned unchanged. */
export function numberedToMarks(input: string): string {
  return input.replace(/([a-zA-ZüÜ]+(?::[a-zA-ZüÜ]*)?)([1-5])/g, (_m, syl: string, t: string) =>
    applyTone(syl, Number(t)),
  );
}

/** Tone (1-4, or 5 for neutral) of a syllable written with tone marks. */
export function toneOf(syllable: string): number {
  for (const ch of syllable) {
    const t = TONE_OF.get(ch);
    if (t) return t;
  }
  const m = syllable.match(/([1-5])$/);
  return m ? Number(m[1]) : 5;
}

const SYLLABLE_RE =
  /(?:[bpmfdtnlgkhjqxrwyz]|[zcs]h)?(?:[iuüv]?(?:[aāáǎàoōóǒòeēéěè](?:ng|n|i|o|u|r)?|[iīíǐìuūúǔùüǖǘǚǜv](?:ng|n|e|ē|é|ě|è|i|ī|í|ǐ|ì|u|ū|ú|ǔ|ù)?)|ng|n|m|r)(?:r(?![aeiouāáǎàēéěèīíǐìōóǒòūúǔù]))?/giy;

/** Split pinyin into syllables. Handles spaced and run-together forms ("nǐhǎo"). */
export function syllables(pinyin: string): string[] {
  const out: string[] = [];
  for (const word of pinyin.trim().split(/[\s'’·\-]+/)) {
    if (!word) continue;
    SYLLABLE_RE.lastIndex = 0;
    const parts: string[] = [];
    let pos = 0;
    while (pos < word.length) {
      SYLLABLE_RE.lastIndex = pos;
      const m = SYLLABLE_RE.exec(word);
      if (!m || m[0].length === 0) {
        parts.length = 0;
        break;
      }
      parts.push(m[0]);
      pos += m[0].length;
    }
    if (parts.length) out.push(...parts);
    else out.push(word);
  }
  return out;
}

const HANZI_RE = /[\u3400-\u4dbf\u4e00-\u9fff\uf900-\ufaff\u3007]/;
export const isHanzi = (ch: string) => HANZI_RE.test(ch);
export const hasHanzi = (s: string) => [...s].some(isHanzi);

/**
 * Pair each Chinese character with a tone, when the pinyin lines up
 * (same number of syllables as characters). Non-hanzi chars get tone 0.
 */
export function hanziTones(hanzi: string, pinyin: string): { ch: string; tone: number }[] {
  const chars = [...hanzi];
  const sylls = syllables(pinyin);
  const hanCount = chars.filter(isHanzi).length;
  const aligned = sylls.length === hanCount;
  let k = 0;
  return chars.map((ch) => {
    if (!isHanzi(ch)) return { ch, tone: 0 };
    const tone = aligned ? toneOf(sylls[k]) : 0;
    k++;
    return { ch, tone };
  });
}
