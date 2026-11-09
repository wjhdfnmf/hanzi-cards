"""Build src-tauri/data/char_freq.tsv from the Unicode Unihan database.

Uses the kHanyuPinlu field: per-reading frequency counts for ~3,800 common
characters, from 《現代漢語頻率詞典》 (Beijing Language Institute, 1986).
Unihan is distributed under the Unicode License (https://www.unicode.org/license.txt).

Usage:
    curl -LO https://www.unicode.org/Public/UCD/latest/ucd/Unihan.zip
    unzip Unihan.zip Unihan_Readings.txt
    python scripts/gen_char_freq.py Unihan_Readings.txt > src-tauri/data/char_freq.tsv

Output: one character per line, `<char>\t<reading>:<count> <reading>:<count>...`
with readings as lower-case numbered pinyin (ü as v, neutral tone 5), e.g. `了\tle5:30101 liao3:654`.
"""

import re
import sys
import unicodedata

TONE_BY_MARK = {"̄": 1, "́": 2, "̌": 3, "̀": 4}


def numbered(syllable: str) -> str:
    tone = 5
    out = []
    for ch in unicodedata.normalize("NFD", syllable.lower()):
        if ch in TONE_BY_MARK:
            tone = TONE_BY_MARK[ch]
        elif ch == "̈":  # combining diaeresis: u + ̈ -> v
            out[-1] = "v"
        else:
            out.append(ch)
    return "".join(out) + str(tone)


def main(path: str) -> None:
    print("# Character reading frequencies from Unihan kHanyuPinlu. Unicode License. See scripts/gen_char_freq.py")
    with open(path, encoding="utf-8") as f:
        for line in f:
            parts = line.rstrip("\n").split("\t")
            if len(parts) != 3 or parts[1] != "kHanyuPinlu":
                continue
            char = chr(int(parts[0][2:], 16))
            readings = re.findall(r"([^\s(]+)\((\d+)\)", parts[2])
            print(char + "\t" + " ".join(f"{numbered(s)}:{n}" for s, n in readings))


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8", newline="\n")
    main(sys.argv[1])
