"""Build the native-speaker tone recording kit.

Writes, from the item lists below:
  docs/tone-recordings/items.tsv          one row per recording: block, position, item, tones
  docs/tone-recordings/speaker-sheet.html the bilingual reading sheet the speaker records from

Usage:
    python scripts/tone_sheet.py

The shuffle is seeded, so re-running gives the same blocks. Change SEED only before anyone has
recorded: items.tsv is what maps each recorded block back to its items.
See docs/native-tone-recordings.md for the plan.
"""

import html
import random
import unicodedata
from pathlib import Path

SEED = 20261109
BLOCK = 10
OUT = Path(__file__).resolve().parent.parent / "docs" / "tone-recordings"

# Part A: single syllables in all four tones. The first ten sets are the same, in the same
# order, as the synthetic test voice's list, so the existing benches run on them unchanged.
SETS = [
    ("妈麻马骂", "mā má mǎ mà"),
    ("八拔把爸", "bā bá bǎ bà"),
    ("衣姨椅意", "yī yí yǐ yì"),
    ("汤糖躺烫", "tāng táng tǎng tàng"),
    ("通同桶痛", "tōng tóng tǒng tòng"),
    ("方房访放", "fāng fáng fǎng fàng"),
    ("西习洗细", "xī xí xǐ xì"),
    ("烟言眼燕", "yān yán yǎn yàn"),
    ("汪王往忘", "wāng wáng wǎng wàng"),
    ("夫服府父", "fū fú fǔ fù"),
    ("诗时使是", "shī shí shǐ shì"),
    ("屋无五雾", "wū wú wǔ wù"),
    ("七骑起气", "qī qí qǐ qì"),
    ("呼湖虎户", "hū hú hǔ hù"),
    ("猪竹主住", "zhū zhú zhǔ zhù"),
    ("眯迷米密", "mī mí mǐ mì"),
    ("秃图土兔", "tū tú tǔ tù"),
    ("窗床闯创", "chuāng chuáng chuǎng chuàng"),
    ("温文稳问", "wēn wén wěn wèn"),
    ("千钱浅欠", "qiān qián qiǎn qiàn"),
]

# Part B: two-syllable words. Pinyin is as a dictionary shows it (syllables joined by "-");
# the tones are as a native speaker says them: 3-3 sandhi, 不/一 changes, 5 = neutral.
WORDS = [
    ("飞机", "fēi-jī", "11"), ("星期", "xīng-qī", "11"),
    ("中国", "Zhōng-guó", "12"), ("新闻", "xīn-wén", "12"),
    ("铅笔", "qiān-bǐ", "13"), ("身体", "shēn-tǐ", "13"),
    ("工作", "gōng-zuò", "14"), ("音乐", "yīn-yuè", "14"),
    ("明天", "míng-tiān", "21"), ("昨天", "zuó-tiān", "21"),
    ("学习", "xué-xí", "22"), ("银行", "yín-háng", "22"),
    ("苹果", "píng-guǒ", "23"), ("词典", "cí-diǎn", "23"),
    ("学校", "xué-xiào", "24"), ("同意", "tóng-yì", "24"),
    ("老师", "lǎo-shī", "31"), ("北京", "Běi-jīng", "31"),
    ("旅行", "lǚ-xíng", "32"), ("美国", "Měi-guó", "32"),
    ("你好", "nǐ hǎo", "23"), ("水果", "shuǐ-guǒ", "23"), ("可以", "kě-yǐ", "23"),
    ("考试", "kǎo-shì", "34"), ("好看", "hǎo-kàn", "34"),
    ("汽车", "qì-chē", "41"), ("面包", "miàn-bāo", "41"),
    ("问题", "wèn-tí", "42"), ("复杂", "fù-zá", "42"),
    ("跳舞", "tiào-wǔ", "43"), ("电脑", "diàn-nǎo", "43"),
    ("再见", "zài-jiàn", "44"), ("电话", "diàn-huà", "44"),
    ("妈妈", "mā-ma", "15"), ("谢谢", "xiè-xie", "45"), ("朋友", "péng-you", "25"),
    ("东西", "dōng-xi", "15"), ("喜欢", "xǐ-huan", "35"), ("漂亮", "piào-liang", "45"),
    ("不是", "bú shì", "24"), ("不要", "bú yào", "24"), ("不好", "bù hǎo", "43"), ("不来", "bù lái", "42"),
    ("一样", "yí-yàng", "24"), ("一起", "yì-qǐ", "43"), ("一天", "yì tiān", "41"), ("第一", "dì-yī", "41"),
]

# Part C: real words said with a wrong tone on purpose (the learner's case). The speaker sees
# only the pinyin to say; the word and its right tones are kept for scoring.
WRONG = [
    ("老师", "31", "lāo-shī", "11"),
    ("中国", "12", "zhōng-guǒ", "13"),
    ("学习", "22", "xué-xì", "24"),
    ("明天", "21", "míng-tiàn", "24"),
    ("苹果", "23", "pīng-guǒ", "13"),
    ("好看", "34", "hǎo-kān", "31"),
    ("工作", "14", "gòng-zuò", "44"),
    ("电脑", "43", "dián-nǎo", "23"),
    ("问题", "42", "wén-tí", "22"),
    ("北京", "31", "běi-jìng", "34"),
    ("音乐", "14", "yīn-yué", "12"),
    ("身体", "13", "shēn-tí", "12"),
    ("汽车", "41", "qí-chē", "21"),
    ("再见", "44", "zǎi-jiàn", "34"),
    ("学校", "24", "xuě-xiào", "34"),
    ("考试", "34", "kǎo-shí", "32"),
    ("旅行", "32", "lǚ-xìng", "34"),
    ("飞机", "11", "fēi-jí", "12"),
    ("跳舞", "43", "tiāo-wǔ", "13"),
    ("银行", "22", "yǐn-háng", "32"),
]

# Part D: short sentences, tones as spoken.
SENTENCES = [
    ("他是老师。", "tā shì lǎo-shī", "1431"),
    ("我们去学校。", "wǒ-men qù xué-xiào", "35424"),
    ("今天天气很热。", "jīn-tiān tiān-qì hěn rè", "111434"),
    ("你喜欢喝茶吗？", "nǐ xǐ-huan hē chá ma", "235125"),
    ("她不想去。", "tā bù xiǎng qù", "1434"),
    ("明天见。", "míng-tiān jiàn", "214"),
    ("我爸爸在家。", "wǒ bà-ba zài jiā", "34541"),
    ("这个苹果不贵。", "zhè-ge píng-guǒ bú guì", "452324"),
    ("我们一起吃饭吧。", "wǒ-men yì-qǐ chī-fàn ba", "3543145"),
    ("谢谢你的帮助。", "xiè-xie nǐ de bāng-zhù", "453514"),
]

MARKS = {"̄": 1, "́": 2, "̌": 3, "̀": 4}


def tone_of(syllable):
    for ch in unicodedata.normalize("NFD", syllable):
        if ch in MARKS:
            return MARKS[ch]
    return 5


def syllables(pinyin):
    return [s for w in pinyin.split() for s in w.split("-")]


def items():
    """Every item once, in canonical order: id, part, text, pinyin shown, tones said, right tones."""
    out = []
    for chars, py in SETS:
        for ch, s in zip(chars, py.split()):
            out.append(("A", ch, s, str(tone_of(s)), str(tone_of(s))))
    for text, py, said in WORDS:
        out.append(("B", text, py, said, said))
    for text, py, said in SENTENCES:
        out.append(("D", text, py, said, said))
    for text, right, py, said in WRONG:
        out.append(("C", text, py, said, right))
    rows = []
    count = {}
    for part, text, py, said, right in out:
        assert len(syllables(py)) == len(said) == len(text.rstrip("。？")), (text, py, said)
        count[part] = count.get(part, 0) + 1
        rows.append({"id": f"{part}{count[part]:02d}", "part": part, "text": text, "pinyin": py, "said": said, "right": right})
    return rows


def shuffled(rows, rng):
    """Shuffle so that no two syllables of one Part A set follow each other."""
    def base(r):
        return r["id"] if r["part"] != "A" else ("A", (int(r["id"][1:]) - 1) // 4)

    for _ in range(10000):
        order = rows[:]
        rng.shuffle(order)
        if all(base(a) != base(b) for a, b in zip(order, order[1:])):
            return order
    raise RuntimeError("no shuffle found")


def blocks(rows):
    rng = random.Random(SEED)
    ab = [r for r in rows if r["part"] in "AB"]
    plan = []  # (pass, [rows]) per block
    first = shuffled(ab, rng)
    plan += [(1, first[i : i + BLOCK]) for i in range(0, len(first), BLOCK)]
    wrong = shuffled([r for r in rows if r["part"] == "C"], rng)
    plan += [(1, wrong[i : i + BLOCK]) for i in range(0, len(wrong), BLOCK)]
    plan.append((1, [r for r in rows if r["part"] == "D"]))
    second = shuffled(ab, rng)
    plan += [(2, second[i : i + BLOCK]) for i in range(0, len(second), BLOCK)]
    return plan


def pinyin_html(py):
    words = []
    for w in py.split():
        words.append("".join(f'<span class="t{tone_of(s)}">{html.escape(s)}</span>' for s in w.split("-")))
    return " ".join(words)


PART_LABEL = {
    "AB": ("字和词", "Syllables and words"),
    "C": ("故意读错的声调", "Wrong tones on purpose"),
    "D": ("句子", "Sentences"),
}


def block_html(n, pass_no, rows):
    part = rows[0]["part"]
    key = "AB" if part in "AB" else part
    zh, en = PART_LABEL[key]
    cells = []
    for i, r in enumerate(rows, 1):
        if part == "C":
            main = f'<span class="py-big">{pinyin_html(r["pinyin"])}</span>'
            sub = ""
        else:
            cls = "hz sent" if part == "D" else "hz"
            main = f'<span class="{cls}" lang="zh-Hans">{html.escape(r["text"])}</span>'
            sub = f'<span class="py">{pinyin_html(r["pinyin"])}</span>'
        cells.append(f'<li><span class="n">{i}</span>{main}{sub}</li>')
    wide = " wide" if part == "D" else ""
    return f"""
<section class="block" id="b{n}">
  <header>
    <h3><span lang="zh-Hans">第 {n} 组</span> <span class="en">Block {n}</span></h3>
    <p class="meta"><span lang="zh-Hans">第{"一" if pass_no == 1 else "二"}轮 · {zh}</span> <span class="en">Pass {pass_no} · {en}</span></p>
    <label class="done"><input type="checkbox" id="done{n}" data-block="{n}"> <span lang="zh-Hans">录好了</span></label>
  </header>
  <p class="say"><span lang="zh-Hans">开始录音，先说「第{n}组」</span> <span class="en">Start recording, then say “dì {n} zǔ”</span></p>
  <ol class="items{wide}">{"".join(cells)}</ol>
</section>"""


def main():
    rows = items()
    plan = blocks(rows)
    OUT.mkdir(parents=True, exist_ok=True)

    lines = ["block\tpass\tpos\tid\tpart\ttext\tpinyin\ttones_said\ttones_right"]
    for n, (p, rs) in enumerate(plan, 1):
        for i, r in enumerate(rs, 1):
            lines.append(f"{n}\t{p}\t{i}\t{r['id']}\t{r['part']}\t{r['text']}\t{r['pinyin']}\t{r['said']}\t{r['right']}")
    (OUT / "items.tsv").write_text("\n".join(lines) + "\n", encoding="utf-8")

    pass1 = [block_html(n, p, rs) for n, (p, rs) in enumerate(plan, 1) if p == 1]
    pass2 = [block_html(n, p, rs) for n, (p, rs) in enumerate(plan, 1) if p == 2]
    n1 = len(pass1)
    page = TEMPLATE.replace("{{PASS1}}", "".join(pass1)).replace("{{PASS2}}", "".join(pass2))
    page = page.replace("{{N1}}", str(n1)).replace("{{N2A}}", str(n1 + 1)).replace("{{N}}", str(len(plan)))
    page = page.replace("{{C1}}", str(next(n for n, (p, rs) in enumerate(plan, 1) if rs[0]["part"] == "C")))
    (OUT / "speaker-sheet.html").write_text(page, encoding="utf-8")
    print(f"{len(rows)} items, {len(plan)} blocks ({n1} in pass 1) -> {OUT}")


TEMPLATE = Path(__file__).with_name("tone_sheet_template.html").read_text(encoding="utf-8")

if __name__ == "__main__":
    main()
