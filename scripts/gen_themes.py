"""Theme palettes for Hanzi Cards: checks WCAG contrast for every pair the app relies on, then
writes the CSS (between markers in src/app.css) and src/lib/themes.ts from the same data."""
import os, sys, re
sys.stdout.reconfigure(encoding="utf-8")
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))  # the project folder

LIGHT_TONES = {"tone-1": "#d92b2b", "tone-2": "#1b7a34", "tone-3": "#2563c9", "tone-4": "#8a3cc2", "tone-5": "#6f6c66"}
DARK_TONES = {"tone-1": "#ff6b6b", "tone-2": "#5ccf7a", "tone-3": "#6aa5ff", "tone-4": "#c48af0", "tone-5": "#a3a19a"}
LIGHT_STATE = {"ok": "#13722f", "ok-soft": "#e4f4e8", "warn": "#9a5c00", "warn-soft": "#fdf1dc", "focus": "#2a78d6"}
DARK_STATE = {"ok": "#52c474", "ok-soft": "#173023", "warn": "#e6a640", "warn-soft": "#362a14", "focus": "#5b9cf0"}
LIGHT_SERIES = {"series-1": "#2a78d6", "series-2": "#eb6834", "series-3": "#1baf7a", "series-4": "#eda100",
                "seq-1": "#cde2fb", "seq-2": "#86b6ef", "seq-3": "#3987e5", "seq-4": "#1c5cab"}
DARK_SERIES = {"series-1": "#3987e5", "series-2": "#d95926", "series-3": "#199e70", "series-4": "#c98500",
               "seq-1": "#184f95", "seq-2": "#256abf", "seq-3": "#3987e5", "seq-4": "#86b6ef"}

def svg(s):
    s = re.sub(r"\s+", " ", s.strip())
    return "url(\"data:image/svg+xml," + s.replace("#", "%23").replace('"', "'").replace("<", "%3C").replace(">", "%3E") + "\")"

def blossom(cx, cy, r, rot, fill, op):
    petals = "".join(
        f'<ellipse cx="0" cy="{-r*0.95:.1f}" rx="{r*0.55:.1f}" ry="{r*0.95:.1f}" transform="rotate({rot + k*72})"/>' for k in range(5))
    return (f'<g transform="translate({cx} {cy})" fill="{fill}" fill-opacity="{op}">{petals}'
            f'<circle r="{r*0.28:.1f}" fill="#e0789a" fill-opacity="{op}"/></g>')

SAKURA_ART = svg(f"""<svg xmlns="http://www.w3.org/2000/svg" width="320" height="320" viewBox="0 0 320 320">
  {blossom(60, 70, 13, 10, "#f3a9bf", 0.32)}
  {blossom(230, 150, 17, 35, "#f3a9bf", 0.26)}
  {blossom(120, 255, 10, 60, "#f3a9bf", 0.3)}
  {blossom(285, 285, 8, 5, "#f3a9bf", 0.3)}
  <ellipse cx="170" cy="60" rx="4" ry="7" transform="rotate(40 170 60)" fill="#f3a9bf" fill-opacity="0.32"/>
  <ellipse cx="40" cy="190" rx="3.5" ry="6" transform="rotate(-30 40 190)" fill="#f3a9bf" fill-opacity="0.3"/>
  <ellipse cx="300" cy="40" rx="3" ry="5.5" transform="rotate(70 300 40)" fill="#f3a9bf" fill-opacity="0.28"/>
</svg>""")

INK_ART = svg("""<svg xmlns="http://www.w3.org/2000/svg" width="1600" height="360" viewBox="0 0 1600 360">
  <path d="M0 360 L0 250 C120 200 200 140 300 170 C380 195 430 120 520 110 C610 100 660 190 760 200 C860 210 900 130 1000 120 C1100 110 1160 200 1260 210 C1360 220 1440 160 1600 180 L1600 360 Z" fill="#1b1a17" fill-opacity="0.035"/>
  <path d="M0 360 L0 300 C160 270 260 230 380 250 C500 270 560 210 680 220 C800 230 860 290 980 280 C1100 270 1180 220 1300 240 C1420 260 1500 290 1600 280 L1600 360 Z" fill="#1b1a17" fill-opacity="0.05"/>
</svg>""")

WAVE_ART = svg("""<svg xmlns="http://www.w3.org/2000/svg" width="80" height="40" viewBox="0 0 80 40" fill="none" stroke="#c8d6ff" stroke-opacity="0.045" stroke-width="1.5">
  <circle cx="40" cy="40" r="38"/><circle cx="40" cy="40" r="28"/><circle cx="40" cy="40" r="18"/><circle cx="40" cy="40" r="8"/>
  <circle cx="0" cy="20" r="38"/><circle cx="0" cy="20" r="28"/><circle cx="0" cy="20" r="18"/><circle cx="0" cy="20" r="8"/>
  <circle cx="80" cy="20" r="38"/><circle cx="80" cy="20" r="28"/><circle cx="80" cy="20" r="18"/><circle cx="80" cy="20" r="8"/>
</svg>""")

def maple_leaf(cx, cy, size, rot, fill, op):
    """A stylised five-lobed maple leaf with a short stem."""
    pts = []
    import math
    # radius pattern around the leaf: deep notches between the lobes, small teeth on them
    for i in range(40):
        a = math.radians(i * 9 - 90)
        lobe = (i % 8)
        r = [1.0, 0.82, 0.66, 0.5, 0.42, 0.5, 0.66, 0.82][lobe]
        if 135 < (i * 9) % 360 < 225:  # flatter at the stem end
            r *= 0.75
        pts.append(f"{math.cos(a) * r * size:.1f},{math.sin(a) * r * size:.1f}")
    stem = f'<path d="M0 {size*0.55:.1f} L0 {size*1.25:.1f}" stroke="{fill}" stroke-opacity="{op}" stroke-width="{size*0.08:.1f}" stroke-linecap="round"/>'
    return (f'<g transform="translate({cx} {cy}) rotate({rot})">{stem}'
            f'<polygon points="{" ".join(pts)}" fill="{fill}" fill-opacity="{op}"/></g>')

MAPLE_ART = svg(f"""<svg xmlns="http://www.w3.org/2000/svg" width="360" height="360" viewBox="0 0 360 360">
  {maple_leaf(70, 80, 22, -20, "#d9733a", 0.16)}
  {maple_leaf(260, 140, 28, 25, "#c9562a", 0.13)}
  {maple_leaf(150, 280, 18, 160, "#e0913f", 0.16)}
  {maple_leaf(320, 320, 13, -60, "#d9733a", 0.15)}
</svg>""")

def plum_flower(cx, cy, r, fill, op):
    petals = "".join(
        f'<circle cx="{(r*0.62)*__import__("math").cos(__import__("math").radians(k*72-90)):.1f}" '
        f'cy="{(r*0.62)*__import__("math").sin(__import__("math").radians(k*72-90)):.1f}" r="{r*0.5:.1f}"/>' for k in range(5))
    return (f'<g transform="translate({cx} {cy})" fill="{fill}" fill-opacity="{op}">{petals}'
            f'<circle r="{r*0.22:.1f}" fill="#f4c542" fill-opacity="{op*1.6:.2f}"/></g>')

PLUM_ART = svg(f"""<svg xmlns="http://www.w3.org/2000/svg" width="420" height="420" viewBox="0 0 420 420">
  <path d="M-10 330 C60 300 110 250 150 200 C180 160 230 140 280 120 M150 200 C160 170 150 140 170 110 M280 120 C320 105 350 80 380 40"
        fill="none" stroke="#4a3a3a" stroke-opacity="0.09" stroke-width="5" stroke-linecap="round"/>
  {plum_flower(150, 200, 11, "#c43a5c", 0.2)}
  {plum_flower(172, 112, 9, "#c43a5c", 0.18)}
  {plum_flower(282, 118, 12, "#c43a5c", 0.2)}
  {plum_flower(362, 62, 8, "#c43a5c", 0.17)}
  {plum_flower(90, 268, 8, "#c43a5c", 0.17)}
  <circle cx="220" cy="160" r="3" fill="#c43a5c" fill-opacity="0.2"/>
  <circle cx="330" cy="96" r="2.5" fill="#c43a5c" fill-opacity="0.18"/>
</svg>""")

CLOUD = "M0 0 c0 -11 9 -20 20 -20 c8 0 14 4 17 10 c3 -4 8 -6 13 -6 c9 0 16 7 16 16 c0 2 0 3 -1 4 c5 1 9 6 9 11 c0 6 -5 11 -11 11 h-53 c-7 0 -13 -6 -13 -13 c0 -6 3 -10 3 -13 z"
DUNHUANG_ART = svg(f"""<svg xmlns="http://www.w3.org/2000/svg" width="300" height="220" viewBox="0 0 300 220" fill="none" stroke="#9b6a2f" stroke-opacity="0.13" stroke-width="2">
  <path transform="translate(30 60)" d="{CLOUD}"/>
  <path transform="translate(42 58) scale(0.55)" d="M0 0 c4 -8 14 -8 16 0 c1 5 -4 8 -8 6" />
  <path transform="translate(180 170)" d="{CLOUD}"/>
  <path transform="translate(192 168) scale(0.55)" d="M0 0 c4 -8 14 -8 16 0 c1 5 -4 8 -8 6" />
</svg>""")

def stars(seed, n, w, h):
    import random
    rnd = random.Random(seed)
    out = []
    for _ in range(n):
        x, y = rnd.uniform(0, w), rnd.uniform(0, h)
        r = rnd.choice([0.6, 0.8, 0.8, 1.0, 1.3])
        op = rnd.choice([0.16, 0.24, 0.32, 0.45])
        out.append(f'<circle cx="{x:.0f}" cy="{y:.0f}" r="{r}" fill="#f2efff" fill-opacity="{op}"/>')
    return "".join(out)

MOON_STARS = svg(f"""<svg xmlns="http://www.w3.org/2000/svg" width="360" height="360" viewBox="0 0 360 360">{stars(7, 30, 360, 360)}</svg>""")
MOON_ART = "radial-gradient(circle at 92% 6%, rgb(236 230 255 / 0.13) 0, rgb(200 180 255 / 0.05) 14%, transparent 34%), " + MOON_STARS

def pine(x, base, h, op):
    w = h * 0.42
    tiers = "".join(
        f'<path d="M{x} {base - h + t*h*0.22:.0f} L{x - w*(0.45 + t*0.18):.0f} {base - h*0.42 + t*h*0.2:.0f} L{x + w*(0.45 + t*0.18):.0f} {base - h*0.42 + t*h*0.2:.0f} Z"/>'
        for t in range(3))
    trunk = f'<rect x="{x - h*0.03:.0f}" y="{base - h*0.2:.0f}" width="{h*0.06:.0f}" height="{h*0.2:.0f}"/>'
    return f'<g fill="#9fe0b5" fill-opacity="{op}">{tiers}{trunk}</g>'

PINE_ART = svg(f"""<svg xmlns="http://www.w3.org/2000/svg" width="1600" height="260" viewBox="0 0 1600 260">
  {pine(90, 260, 170, 0.05)}{pine(190, 260, 120, 0.04)}{pine(330, 260, 210, 0.05)}{pine(470, 260, 140, 0.04)}
  {pine(1180, 260, 150, 0.04)}{pine(1300, 260, 220, 0.05)}{pine(1420, 260, 130, 0.04)}{pine(1530, 260, 190, 0.05)}
</svg>""")

HC_TONES = {"tone-1": "#ff8080", "tone-2": "#74e392", "tone-3": "#8cbcff", "tone-4": "#d6a3ff", "tone-5": "#cfcdc7"}

def light(**k):
    return {"scheme": "light", **LIGHT_TONES, **LIGHT_STATE, **LIGHT_SERIES, **k}

def dark(**k):
    return {"scheme": "dark", **DARK_TONES, **DARK_STATE, **DARK_SERIES, **k}

# key: (English name, Chinese name, emblem character or "flower", palette)
THEMES = {
    "sakura": ("Sakura", "樱花", "flower", light(
        bg="#fdf2f5", surface="#ffffff", **{"surface-2": "#fbe7ed", "surface-3": "#f5d6df"}, sidebar="#fff7f9",
        border="#f0d2db", text="#2a171e", **{"text-2": "#694651", "text-3": "#8f6b76"},
        accent="#b02f62", **{"accent-hover": "#962652", "accent-soft": "#fde3ec", "accent-text": "#ffffff"},
        art=SAKURA_ART, artsize="320px 320px", artpos="0 0", artrepeat="repeat",
        shadow="0 1px 2px rgb(140 40 80 / 0.06), 0 4px 16px rgb(140 40 80 / 0.06)",
        **{"seq-0": "#f6e3e9", "grid": "#f2dde4"})),
    "ink": ("Ink & paper", "水墨", "墨", light(
        bg="#f3f0e8", surface="#fbfaf5", **{"surface-2": "#ebe7dc", "surface-3": "#dfdacb"}, sidebar="#eeeae0",
        border="#d9d3c4", text="#1b1a17", **{"text-2": "#4b473f", "text-3": "#77726a"},
        accent="#a82127", **{"accent-hover": "#8e1a20", "accent-soft": "#f2e0dd", "accent-text": "#ffffff"},
        art=INK_ART, artsize="1600px auto", artpos="bottom center", artrepeat="no-repeat",
        shadow="0 1px 2px rgb(40 30 10 / 0.06), 0 4px 16px rgb(40 30 10 / 0.05)",
        **{"seq-0": "#e8e4d9", "grid": "#e2ddd0"})),
    "jade": ("Celadon", "青瓷", "玉", light(
        bg="#edf4f1", surface="#fbfdfc", **{"surface-2": "#e1ece7", "surface-3": "#d2e2db"}, sidebar="#f4f9f6",
        border="#cddfd7", text="#13221c", **{"text-2": "#415650", "text-3": "#687d76"},
        accent="#24705f", **{"accent-hover": "#1b5a4c", "accent-soft": "#d9eee7", "accent-text": "#ffffff"},
        art="none", artsize="auto", artpos="0 0", artrepeat="no-repeat",
        shadow="0 1px 2px rgb(20 60 50 / 0.06), 0 4px 16px rgb(20 60 50 / 0.05)",
        **{"seq-0": "#e0ebe6", "grid": "#d9e6e0"})),
    "matcha": ("Matcha", "抹茶", "茶", light(
        bg="#f3f1e3", surface="#fffef7", **{"surface-2": "#eae7d4", "surface-3": "#ddd9c2"}, sidebar="#f8f6ea",
        border="#dbd6bd", text="#1e2113", **{"text-2": "#4c5236", "text-3": "#737960"},
        accent="#4f6b1f", **{"accent-hover": "#405818", "accent-soft": "#e3ebcd", "accent-text": "#ffffff"},
        art="none", artsize="auto", artpos="0 0", artrepeat="no-repeat",
        shadow="0 1px 2px rgb(50 60 20 / 0.06), 0 4px 16px rgb(50 60 20 / 0.05)",
        **{"seq-0": "#e8e5d3", "grid": "#e2dfca"})),
    "lavender": ("Lavender", "薰衣草", "香", light(
        bg="#f4f2fa", surface="#ffffff", **{"surface-2": "#ebe7f6", "surface-3": "#ddd7ef"}, sidebar="#faf9fe",
        border="#dcd6ec", text="#1d1a2a", **{"text-2": "#4f4864", "text-3": "#776f8e"},
        accent="#5f45ad", **{"accent-hover": "#4f3896", "accent-soft": "#ebe5fb", "accent-text": "#ffffff"},
        art="none", artsize="auto", artpos="0 0", artrepeat="no-repeat",
        shadow="0 1px 2px rgb(60 40 120 / 0.06), 0 4px 16px rgb(60 40 120 / 0.05)",
        **{"seq-0": "#ebe7f5", "grid": "#e4dff1"})),
    "imperial": ("Imperial", "紫禁城", "金", dark(
        bg="#1a0e10", surface="#251517", **{"surface-2": "#301c1f", "surface-3": "#3d2427"}, sidebar="#130a0b",
        border="#43292c", text="#f7ece4", **{"text-2": "#d6c3b6", "text-3": "#a68f83"},
        accent="#e4b44f", **{"accent-hover": "#efc56c", "accent-soft": "#3d2c13", "accent-text": "#1a0e10"},
        art="none", artsize="auto", artpos="0 0", artrepeat="no-repeat",
        shadow="inset 0 1px 0 rgb(255 220 180 / 0.04), 0 1px 2px rgb(0 0 0 / 0.4), 0 6px 18px rgb(0 0 0 / 0.25)",
        **{"seq-0": "#2c1a1c", "grid": "#33201f"})),
    "indigo": ("Indigo", "蓝染", "藍", dark(
        bg="#0d1322", surface="#141c2f", **{"surface-2": "#1b253c", "surface-3": "#25304b"}, sidebar="#090e1a",
        border="#263049", text="#e9eef8", **{"text-2": "#b4bfd5", "text-3": "#8590a8"},
        accent="#79a8ff", **{"accent-hover": "#95baff", "accent-soft": "#1c2b4b", "accent-text": "#0d1322"},
        art=WAVE_ART, artsize="80px 40px", artpos="0 0", artrepeat="repeat",
        shadow="inset 0 1px 0 rgb(200 220 255 / 0.04), 0 1px 2px rgb(0 0 0 / 0.45), 0 6px 18px rgb(0 0 0 / 0.25)",
        **{"seq-0": "#1a2338", "grid": "#212b42"})),
    "lantern": ("Lantern", "灯笼", "灯", dark(
        bg="#16110d", surface="#201913", **{"surface-2": "#2a2119", "surface-3": "#362b20"}, sidebar="#100c09",
        border="#3a2f24", text="#f6ede1", **{"text-2": "#cfbea8", "text-3": "#9f8e78"},
        accent="#ff8f45", **{"accent-hover": "#ffa565", "accent-soft": "#3b2416", "accent-text": "#1a120c"},
        art="none", artsize="auto", artpos="0 0", artrepeat="no-repeat",
        shadow="inset 0 1px 0 rgb(255 220 180 / 0.035), 0 1px 2px rgb(0 0 0 / 0.4), 0 6px 18px rgb(0 0 0 / 0.25)",
        **{"seq-0": "#2a2119", "grid": "#30261c"})),
    "maple": ("Maple", "红叶", "枫", light(
        bg="#fbf2e9", surface="#fffdf9", **{"surface-2": "#f5e7d8", "surface-3": "#ecd8c2"}, sidebar="#fdf7f0",
        border="#e9d6c0", text="#2a190f", **{"text-2": "#5d4232", "text-3": "#876b58"},
        accent="#ad431b", **{"accent-hover": "#923716", "accent-soft": "#fbe2d2", "accent-text": "#ffffff"},
        art=MAPLE_ART, artsize="360px 360px", artpos="0 0", artrepeat="repeat",
        shadow="0 1px 2px rgb(120 60 20 / 0.07), 0 4px 16px rgb(120 60 20 / 0.06)",
        **{"seq-0": "#f2e4d4", "grid": "#eedecb"})),
    "plum": ("Plum & Snow", "梅雪", "梅", light(
        bg="#f2f4f7", surface="#ffffff", **{"surface-2": "#e9edf2", "surface-3": "#dce2ea"}, sidebar="#f8fafc",
        border="#d8dee7", text="#161a21", **{"text-2": "#454d5a", "text-3": "#6d7583"},
        accent="#981d43", **{"accent-hover": "#801838", "accent-soft": "#f6e0e7", "accent-text": "#ffffff"},
        art=PLUM_ART, artsize="420px 420px", artpos="0 0", artrepeat="repeat",
        shadow="0 1px 2px rgb(30 40 60 / 0.06), 0 4px 16px rgb(30 40 60 / 0.06)",
        **{"seq-0": "#e7ebf0", "grid": "#e1e6ed"})),
    "dunhuang": ("Dunhuang", "敦煌", "敦", light(
        bg="#f3e8d5", surface="#fdf8ee", **{"surface-2": "#ecdec4", "surface-3": "#e0ccac"}, sidebar="#f8efe0",
        border="#dac5a2", text="#2a1c0f", **{"text-2": "#5b442e", "text-3": "#836a4f"},
        accent="#1d578b", **{"accent-hover": "#174773", "accent-soft": "#dde7f2", "accent-text": "#ffffff"},
        art=DUNHUANG_ART, artsize="300px 220px", artpos="0 0", artrepeat="repeat",
        shadow="0 1px 2px rgb(110 70 20 / 0.07), 0 4px 16px rgb(110 70 20 / 0.06)",
        **{"seq-0": "#ece0ca", "grid": "#e6d7bc"})),
    "moon": ("Moon Night", "月夜", "月", dark(
        bg="#0f0e1a", surface="#171627", **{"surface-2": "#1f1d33", "surface-3": "#2a2742"}, sidebar="#0b0a14",
        border="#2c2944", text="#edecf8", **{"text-2": "#bdb9d7", "text-3": "#8d89a8"},
        accent="#c6adff", **{"accent-hover": "#d6c3ff", "accent-soft": "#2a2148", "accent-text": "#130f22"},
        art=MOON_ART, artsize="100% 100%, 360px 360px", artpos="0 0, 0 0", artrepeat="no-repeat, repeat",
        shadow="inset 0 1px 0 rgb(220 210 255 / 0.04), 0 1px 2px rgb(0 0 0 / 0.45), 0 6px 18px rgb(0 0 0 / 0.25)",
        **{"seq-0": "#1e1c31", "grid": "#25223a"})),
    "pine": ("Pine Forest", "松林", "松", dark(
        bg="#0e1612", surface="#15201a", **{"surface-2": "#1c2a22", "surface-3": "#26362d"}, sidebar="#0a110d",
        border="#27382e", text="#e9f2eb", **{"text-2": "#b6c8bb", "text-3": "#849b8a"},
        accent="#82d6a3", **{"accent-hover": "#9de2b8", "accent-soft": "#1d3a2a", "accent-text": "#0b1510"},
        art=PINE_ART, artsize="1600px auto", artpos="bottom center", artrepeat="no-repeat",
        shadow="inset 0 1px 0 rgb(200 255 220 / 0.03), 0 1px 2px rgb(0 0 0 / 0.45), 0 6px 18px rgb(0 0 0 / 0.25)",
        **{"seq-0": "#1b2821", "grid": "#213027"})),
    "contrast": ("High Contrast", "高对比", "Aa", dark(
        bg="#000000", surface="#0b0b0b", **{"surface-2": "#171717", "surface-3": "#262626"}, sidebar="#000000",
        border="#707070", text="#ffffff", **{"text-2": "#e8e8e8", "text-3": "#c2c2c2"},
        accent="#ffd60a", **{"accent-hover": "#ffe24d", "accent-soft": "#332b00", "accent-text": "#000000"},
        art="none", artsize="auto", artpos="0 0", artrepeat="no-repeat",
        shadow="0 0 0 1px #3a3a3a",
        **{"ok": "#6fe38f", "ok-soft": "#0f2a17", "warn": "#ffc04d", "warn-soft": "#332300", "focus": "#4fc3ff",
           "seq-0": "#1c1c1c", "grid": "#333333"}, **HC_TONES)),
}

# ---------- contrast ----------
def lum(hexc):
    h = hexc.lstrip("#")
    r, g, b = (int(h[i:i+2], 16) / 255 for i in (0, 2, 4))
    f = lambda c: c / 12.92 if c <= 0.03928 else ((c + 0.055) / 1.055) ** 2.4
    return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b)

def ratio(a, b):
    la, lb = sorted((lum(a), lum(b)), reverse=True)
    return (la + 0.05) / (lb + 0.05)

CHECKS = [  # (foreground, background, minimum)
    ("text", "bg", 7), ("text", "surface", 7), ("text", "surface-2", 7), ("text", "surface-3", 6), ("text", "sidebar", 7),
    ("text-2", "bg", 4.5), ("text-2", "surface", 4.5), ("text-2", "surface-2", 4.5),
    ("text-3", "bg", 3), ("text-3", "surface", 3),
    ("accent", "surface", 4.5), ("accent", "bg", 4.5), ("accent", "accent-soft", 4.5), ("accent", "sidebar", 4.5),
    ("accent-text", "accent", 4.5), ("accent-text", "accent-hover", 4.5),
    ("tone-1", "surface", 4.5), ("tone-2", "surface", 4.5), ("tone-3", "surface", 4.5), ("tone-4", "surface", 4.5), ("tone-5", "surface", 4.5),
    ("tone-1", "surface-2", 3.5), ("tone-3", "surface-2", 3.5),
    ("ok", "ok-soft", 4.5), ("warn", "warn-soft", 4.5), ("ok", "surface", 3), ("warn", "surface", 3),
    ("focus", "surface", 3),
]
bad = []
for key, (_, _, _, p) in THEMES.items():
    for fg, bgk, need in CHECKS:
        r = ratio(p[fg], p[bgk])
        if r < need:
            bad.append(f"{key}: {fg} on {bgk} = {r:.2f} (needs {need})")
if bad:
    print("CONTRAST FAILURES:\n  " + "\n  ".join(bad))
    sys.exit(1)
print("all contrast checks pass:", len(THEMES) * len(CHECKS), "pairs")
for key, (_, _, _, p) in THEMES.items():
    print(f"  {key:9s} text/bg {ratio(p['text'], p['bg']):.1f}  text-2/surface {ratio(p['text-2'], p['surface']):.1f}  accent/surface {ratio(p['accent'], p['surface']):.1f}  button {ratio(p['accent-text'], p['accent']):.1f}  worst tone {min(ratio(p[f'tone-{i}'], p['surface']) for i in range(1, 6)):.1f}")

if "--check" in sys.argv:
    sys.exit(0)

# ---------- CSS ----------
VARS = ["bg", "surface", "surface-2", "surface-3", "sidebar", "border", "text", "text-2", "text-3", "accent", "accent-hover",
        "accent-soft", "accent-text", "focus", "ok", "ok-soft", "warn", "warn-soft", "shadow", "tone-1", "tone-2", "tone-3",
        "tone-4", "tone-5", "series-1", "series-2", "series-3", "series-4", "seq-0", "seq-1", "seq-2", "seq-3", "seq-4", "grid"]
css = ["/* THEMES:BEGIN (generated by scripts/gen_themes.py; contrast-checked) */"]
for key, (_, _, _, p) in THEMES.items():
    lines = [f'  color-scheme: {p["scheme"]};'] + [f"  --{v}: {p[v]};" for v in VARS]
    lines += [f"  --page-art: {p['art']};", f"  --page-art-size: {p['artsize']};",
              f"  --page-art-pos: {p['artpos']};", f"  --page-art-repeat: {p['artrepeat']};"]
    css.append(f':root[data-theme="{key}"] {{\n' + "\n".join(lines) + "\n}")
css.append("/* THEMES:END */")
block = "\n".join(css)

p = os.path.join(ROOT, "src", "app.css")
s = open(p, encoding="utf-8").read()
if "/* THEMES:BEGIN" in s:
    s = re.sub(r"/\* THEMES:BEGIN.*?/\* THEMES:END \*/", lambda m: block, s, flags=re.S)
else:
    anchor = "*,\n*::before,\n*::after {"
    assert anchor in s
    s = s.replace(anchor, block + "\n\n" + anchor, 1)
open(p, "w", encoding="utf-8", newline="").write(s)

# ---------- TS list for the picker ----------
ts = ["// Generated by scripts/gen_themes.py with the CSS in app.css (contrast-checked palettes).",
      "",
      "export interface ThemeInfo {",
      "  key: string;",
      "  name: string;",
      "  zh: string;",
      "  /** A character shown on the swatch, or \"flower\" for drawn blossoms. */",
      "  emblem: string;",
      "  dark: boolean;",
      "  bg: string;",
      "  surface: string;",
      "  accent: string;",
      "  accentSoft: string;",
      "}",
      "",
      "export const CUSTOM_THEMES: ThemeInfo[] = ["]
for key, (name, zh, emblem, p) in THEMES.items():
    ts.append(f'  {{ key: "{key}", name: "{name}", zh: "{zh}", emblem: "{emblem}", dark: {"true" if p["scheme"] == "dark" else "false"}, '
              f'bg: "{p["bg"]}", surface: "{p["surface"]}", accent: "{p["accent"]}", accentSoft: "{p["accent-soft"]}" }},')
ts += ["];", "", "export const isCustomTheme = (t: string) => CUSTOM_THEMES.some((x) => x.key === t);", ""]
open(os.path.join(ROOT, "src", "lib", "themes.ts"), "w", encoding="utf-8", newline="").write("\n".join(ts))
print("wrote app.css themes and src/lib/themes.ts")
