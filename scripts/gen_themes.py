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

# ---------- Sakura: a spring-term notebook ----------
#
# Cherry blossom is the start of the school year, so this theme is a learner's notebook in
# April: blush practice paper ruled in 田字格 (the squares Chinese is learned to write in), a
# branch in blossom reaching in from the corner, petals on the page. The rest is in SAKURA_CSS:
# titles under a stroke of highlighter, the flashcard taped in with washi tape, and a
# teacher's flower mark (a hanamaru) around the 好 when a session is done.

def sakura_petal(r):
    """One petal pointing up from the flower's centre, with the notch a cherry petal has at its tip."""
    return (f"M0 0 C{-0.62*r:.1f} {-0.30*r:.1f} {-0.68*r:.1f} {-0.90*r:.1f} {-0.15*r:.1f} {-1.0*r:.1f} "
            f"L0 {-0.83*r:.1f} L{0.15*r:.1f} {-1.0*r:.1f} C{0.68*r:.1f} {-0.90*r:.1f} {0.62*r:.1f} {-0.30*r:.1f} 0 0 Z")

def sakura(cx, cy, r, rot, op, fill="#f8b9cc", heart="#e2749a"):
    """A five-petalled blossom with its stamens."""
    import math
    petals = "".join(f'<path d="{sakura_petal(r)}" transform="rotate({rot + k*72})"/>' for k in range(5))
    stamens = "".join(
        f'<circle cx="{0.30*r*math.cos(math.radians(rot + 36 + k*72 - 90)):.1f}" cy="{0.30*r*math.sin(math.radians(rot + 36 + k*72 - 90)):.1f}" r="{max(0.8, r*0.055):.1f}"/>'
        for k in range(5))
    return (f'<g transform="translate({cx} {cy})" fill-opacity="{op}"><g fill="{fill}">{petals}</g>'
            f'<g fill="{heart}"><circle r="{r*0.15:.1f}"/>{stamens}</g></g>')

def loose_petal(cx, cy, r, rot, op, fill="#f8b9cc"):
    return f'<path d="{sakura_petal(r)}" transform="translate({cx} {cy}) rotate({rot})" fill="{fill}" fill-opacity="{op}"/>'

def bud(cx, cy, r, rot, op):
    return (f'<g transform="translate({cx} {cy}) rotate({rot})" fill-opacity="{op}">'
            f'<ellipse cx="0" cy="{-r*0.7:.1f}" rx="{r*0.5:.1f}" ry="{r*0.8:.1f}" fill="#f29bb7"/>'
            f'<path d="M{-r*0.45:.1f} {-r*0.1:.1f} Q0 {-r*0.75:.1f} {r*0.45:.1f} {-r*0.1:.1f} L0 {r*0.25:.1f} Z" fill="#a8726f"/></g>')

# The branch comes in from the top right corner of the page.
SAKURA_BRANCH = svg(f"""<svg xmlns="http://www.w3.org/2000/svg" width="560" height="400" viewBox="0 0 560 400">
  <g fill="none" stroke="#9c6b6c" stroke-linecap="round" stroke-opacity="0.34">
    <path d="M572 -8 C508 34 452 52 392 92 C352 119 318 146 262 176" stroke-width="7"/>
    <path d="M392 92 C386 126 372 148 352 176" stroke-width="4.5"/>
    <path d="M452 55 C446 34 432 16 414 2" stroke-width="4"/>
    <path d="M330 136 C306 122 282 116 250 118" stroke-width="3.5"/>
    <path d="M262 176 C238 186 216 204 196 226" stroke-width="3.5"/>
    <path d="M352 176 C346 196 334 212 316 226" stroke-width="3"/>
  </g>
  {sakura(262, 176, 25, 12, 0.62)}
  {sakura(352, 178, 21, 40, 0.58)}
  {sakura(250, 118, 18, 64, 0.56)}
  {sakura(318, 226, 16, 20, 0.52)}
  {sakura(416, 82, 22, 30, 0.6)}
  {sakura(470, 46, 17, 5, 0.55)}
  {sakura(414, 4, 15, 50, 0.5)}
  {sakura(520, 30, 13, 24, 0.5)}
  {sakura(302, 148, 13, 80, 0.5, fill="#fbd0dc")}
  {sakura(384, 130, 12, 10, 0.48, fill="#fbd0dc")}
  {bud(196, 226, 9, -140, 0.55)}
  {bud(226, 204, 7, -110, 0.5)}
  {bud(338, 200, 7, 160, 0.5)}
  {bud(438, 28, 7, -40, 0.5)}
  {loose_petal(176, 268, 14, 150, 0.5)}
  {loose_petal(244, 266, 11, 40, 0.42)}
  {loose_petal(118, 318, 13, 100, 0.42)}
  {loose_petal(290, 306, 10, -30, 0.4)}
  {loose_petal(64, 262, 11, 200, 0.36)}
</svg>""")

# Petals that have drifted further down the page.
SAKURA_PETALS = svg(f"""<svg xmlns="http://www.w3.org/2000/svg" width="640" height="640" viewBox="0 0 640 640">
  {loose_petal(90, 110, 17, 30, 0.34)}
  {loose_petal(520, 210, 14, -50, 0.3)}
  {loose_petal(250, 330, 12, 160, 0.26, fill="#fbd0dc")}
  {loose_petal(430, 470, 18, 70, 0.32)}
  {loose_petal(110, 540, 13, -100, 0.28, fill="#fbd0dc")}
  {loose_petal(590, 590, 11, 20, 0.26)}
</svg>""")

# 田字格: a square with a dashed cross, the paper characters are practised on.
SAKURA_GRID = svg("""<svg xmlns="http://www.w3.org/2000/svg" width="80" height="80" viewBox="0 0 80 80" fill="none" stroke="#e39ab0">
  <path d="M0 0.5 H80 M0.5 0 V80" stroke-opacity="0.19"/>
  <path d="M0 40 H80 M40 0 V80" stroke-opacity="0.17" stroke-dasharray="3 5"/>
</svg>""")

SAKURA_ART = f"{SAKURA_BRANCH}, {SAKURA_PETALS}, {SAKURA_GRID}"

# The flower a teacher draws in red around good work, for the 好 at the end of a session.
SAKURA_STAMP = svg(f"""<svg xmlns="http://www.w3.org/2000/svg" width="112" height="112" viewBox="0 0 112 112">
  <g transform="translate(56 59)" stroke="#cf2f6a" stroke-width="3" stroke-linejoin="round">
    {"".join(f'<path d="{sakura_petal(52)}" transform="rotate({k*72})" fill="#ffe3ec"/>' for k in range(5))}
  </g>
  <circle cx="56" cy="59" r="27" fill="#fffafb"/>
</svg>""")

# A small blossom pinned to the corner of the logo.
SAKURA_BADGE = svg(f"""<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24">
  <g transform="translate(12 12)" stroke="#ffffff" stroke-width="1.2">
    {"".join(f'<path d="{sakura_petal(10)}" transform="rotate({k*72})" fill="#f9a8c2"/>' for k in range(5))}
  </g>
  <circle cx="12" cy="12" r="2" fill="#d6457a"/>
</svg>""")

# Petals falling over the finished-session box (tiles 240 px high, so one loop is one tile).
SAKURA_FALL = svg(f"""<svg xmlns="http://www.w3.org/2000/svg" width="240" height="240" viewBox="0 0 240 240">
  {loose_petal(40, 46, 13, 30, 0.8, fill="#f7a9c0")}
  {loose_petal(196, 96, 15, -40, 0.75, fill="#f9c3d2")}
  {loose_petal(104, 170, 11, 200, 0.7, fill="#f7a9c0")}
  {loose_petal(200, 224, 12, -110, 0.7, fill="#f9c3d2")}
</svg>""")

S = ':root[data-theme="sakura"]'
SAKURA_CSS = f"""
/* Sakura: the parts of the notebook that aren't colours (see SAKURA_ART in gen_themes.py) */

/* page titles under a stroke of highlighter */
{S} h1 {{
  text-decoration: underline;
  text-decoration-color: rgb(255 202 92 / 0.62);
  text-decoration-thickness: 0.4em;
  text-underline-offset: -0.16em;
  text-decoration-skip-ink: none;
}}

/* buttons you can press: a pill with an edge that gives way */
{S} .btn.primary {{
  border-radius: 999px;
  box-shadow: 0 2px 0 var(--accent-hover);
  transition: background 0.12s, transform 0.06s, box-shadow 0.06s;
}}
{S} .btn.primary:active:not(:disabled) {{
  transform: translateY(2px);
  box-shadow: none;
}}

/* the app's mark: a seal with a blossom pinned to it */
{S} .logo {{
  position: relative;
  border-radius: 14px;
  background: linear-gradient(145deg, #cf3f78, #a3225a);
  font-family: var(--font-han-serif);
}}
{S} .logo::after {{
  content: "";
  position: absolute;
  top: -7px;
  right: -7px;
  width: 20px;
  height: 20px;
  background: {SAKURA_BADGE} center / contain no-repeat;
  transform: rotate(14deg);
}}
{S} .navlink.on {{
  box-shadow: inset 3px 0 0 var(--accent);
}}
{S} .compact .navlink.on {{
  box-shadow: none;
}}

/* studying: a card taped into the notebook, and a bar that fills like a branch coming into bloom */
{S} .flash {{
  border-radius: 20px;
}}
{S} .flash::before {{
  content: "";
  position: absolute;
  top: -12px;
  left: 50%;
  width: 116px;
  height: 26px;
  transform: translateX(-50%) rotate(-1.6deg);
  background: repeating-linear-gradient(135deg, rgb(244 143 177 / 0.6) 0 9px, rgb(250 190 208 / 0.6) 9px 18px);
  clip-path: polygon(0 0, 3% 25%, 0 50%, 3% 75%, 0 100%, 100% 100%, 97% 75%, 100% 50%, 97% 25%, 100% 0);
  filter: drop-shadow(0 1px 1px rgb(140 40 80 / 0.18));
}}
{S} .topbar .bar {{
  height: 8px;
  border-radius: 8px;
}}
{S} .topbar .bar div {{
  border-radius: 8px;
  background: linear-gradient(90deg, #f6a5bf, var(--accent));
}}

/* done: the teacher's flower around the 好, and petals coming down */
{S} .seal {{
  width: 108px;
  height: 108px;
  padding-top: 6px;
  border-radius: 0;
  background: {SAKURA_STAMP} center / contain no-repeat;
  color: var(--accent);
  font-family: var(--font-han-serif);
  font-size: 40px;
  font-weight: 700;
}}
{S} .donebox {{
  position: relative;
  overflow: hidden;
}}
{S} .donebox > * {{
  position: relative;
  z-index: 1;
}}
{S} .donebox::before,
{S} .donebox::after {{
  content: "";
  position: absolute;
  inset: 0;
  z-index: 0;
  pointer-events: none;
  background: {SAKURA_FALL} 0 0 / 240px 240px repeat;
  opacity: 0.55;
  animation: sakura-fall 14s linear infinite, sakura-sway 5s ease-in-out infinite alternate;
}}
{S} .donebox::after {{
  background-size: 204px 204px;
  opacity: 0.35;
  animation: sakura-fall-small 19s linear infinite, sakura-sway 7s ease-in-out infinite alternate-reverse;
}}
@keyframes sakura-fall {{
  to {{
    background-position: 0 240px;
  }}
}}
@keyframes sakura-fall-small {{
  to {{
    background-position: 0 204px;
  }}
}}
@keyframes sakura-sway {{
  from {{
    transform: translateX(-10px);
  }}
  to {{
    transform: translateX(10px);
  }}
}}
@media (prefers-reduced-motion: reduce) {{
  {S} .donebox::before,
  {S} .donebox::after {{
    animation: none;
  }}
}}
"""

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
        # blush practice paper, plum-black ink, blossom rose; charts in rose, new leaf, spring sky and pencil
        bg="#fdf4f2", surface="#fffdfc", **{"surface-2": "#fbeaec", "surface-3": "#f6d9de"}, sidebar="#fff8f6",
        border="#f0d5d9", text="#2b1820", **{"text-2": "#644753", "text-3": "#8c6a76"},
        accent="#b92d65", **{"accent-hover": "#9d2356", "accent-soft": "#fde4ec", "accent-text": "#ffffff"},
        art=SAKURA_ART, artsize="min(46vw, 560px) auto, 640px 640px, 80px 80px",
        artpos="right top, 0 0, 0 0", artrepeat="no-repeat, repeat, repeat",
        shadow="0 1px 2px rgb(150 50 90 / 0.07), 0 6px 20px rgb(150 50 90 / 0.07)",
        **{"series-1": "#d2477f", "series-2": "#3f8f52", "series-3": "#3d8fd1", "series-4": "#c2850d",
           "seq-0": "#f8e6ea", "seq-1": "#f9cfdc", "seq-2": "#f09dbb", "seq-3": "#d9588c", "seq-4": "#a3225a",
           "grid": "#f3dfe3"},
        extra={"radius": "14px", "radius-lg": "20px"}, css=SAKURA_CSS)),
    "ink": ("Ink & paper", "水墨", "墨", light(
        bg="#f3f0e8", surface="#fbfaf5", **{"surface-2": "#ebe7dc", "surface-3": "#dfdacb"}, sidebar="#eeeae0",
        border="#d9d3c4", text="#1b1a17", **{"text-2": "#4b473f", "text-3": "#77726a"},
        accent="#a82127", **{"accent-hover": "#8e1a20", "accent-soft": "#f2e0dd", "accent-text": "#ffffff"},
        art=INK_ART, artsize="max(1600px, 100%) auto", artpos="bottom center", artrepeat="no-repeat",
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
        art=PINE_ART, artsize="max(1600px, 100%) auto", artpos="bottom center", artrepeat="no-repeat",
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

# ---------- every theme's own details ----------
#
# A theme is more than its colours. Each one gets, from the same small kit: a mark under page
# titles, a shape for its buttons and logo, something on the flashcard, a progress bar, a
# mark of its own around the 好 when a session is done (and, for some, something drifting
# over that box), chart colours that belong to it, and its page art. Sakura was done first
# and by hand (above); the rest are described here.

def spin(n, inner, start=0.0):
    """`inner` (drawn pointing up from the centre) repeated `n` times round a circle."""
    return "".join(f'<g transform="rotate({start + k * 360 / n:.1f})">{inner}</g>' for k in range(n))

def seal_svg(body):
    """The mark round the 好 on the finished-session box: 112 px square, origin at its centre."""
    return svg(f'<svg xmlns="http://www.w3.org/2000/svg" width="112" height="112" viewBox="-56 -56 112 112">{body}</svg>')

def tile(body, w=240, h=None):
    h = h or w
    return svg(f'<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">{body}</svg>')

def dots(seed, n, colors, rmin, rmax, size=240, op=(0.5, 0.9)):
    import random
    rnd = random.Random(seed)
    return "".join(
        f'<circle cx="{rnd.uniform(8, size - 8):.0f}" cy="{rnd.uniform(8, size - 8):.0f}" r="{rnd.uniform(rmin, rmax):.1f}" '
        f'fill="{rnd.choice(colors)}" fill-opacity="{rnd.uniform(*op):.2f}"/>' for _ in range(n))

def star4(cx, cy, r, fill, op):
    k = r * 0.22
    return (f'<path transform="translate({cx} {cy})" d="M0 {-r} L{k} {-k} L{r} 0 L{k} {k} L0 {r} L{-k} {k} L{-r} 0 L{-k} {-k} Z" '
            f'fill="{fill}" fill-opacity="{op}"/>')

def tea_leaf(cx, cy, r, rot, fill, op):
    """A pointed leaf with its midrib."""
    return (f'<g transform="translate({cx} {cy}) rotate({rot})" fill-opacity="{op}" stroke-opacity="{op}">'
            f'<path d="M0 {-r} C{r*0.62:.1f} {-r*0.45:.1f} {r*0.62:.1f} {r*0.45:.1f} 0 {r} C{-r*0.62:.1f} {r*0.45:.1f} {-r*0.62:.1f} {-r*0.45:.1f} 0 {-r} Z" fill="{fill}"/>'
            f'<path d="M0 {-r*0.8:.1f} V{r*0.8:.1f}" stroke="#ffffff" stroke-width="{max(0.8, r*0.07):.1f}" fill="none"/></g>')

def lavender_sprig(x, base, h, lean, op, stem="#7aa184", flower="#9a7fe0"):
    """A stem with its spike of florets: pairs up the top half, closing to a tip."""
    top = base - h
    florets = []
    for i in range(7):
        t = i / 6                                   # 0 at the tip, 1 at the bottom of the spike
        cx, cy = x + lean * (1 - t * 0.5), top + t * h * 0.5
        w = 3.2 + 2.6 * min(1, t * 2.2)             # the spike widens from its tip
        if i == 0:
            florets.append(f'<ellipse cx="{cx:.1f}" cy="{cy - 3:.1f}" rx="3.4" ry="6"/>')
            continue
        for side in (-1, 1):
            fx = cx + side * w
            florets.append(f'<ellipse cx="{fx:.1f}" cy="{cy:.1f}" rx="3.8" ry="6.6" transform="rotate({side * 32} {fx:.1f} {cy:.1f})"/>')
    leaves = (f'<path d="M{x + lean*0.12:.1f} {base - h*0.2:.1f} q-13 -9 -17 -27 q14 7 17 27 z M{x + lean*0.2:.1f} {base - h*0.32:.1f} q13 -8 16 -25 q-13 6 -16 25 z" fill="{stem}"/>')
    return (f'<g fill-opacity="{op}" stroke-opacity="{op}"><path d="M{x} {base} Q{x + lean*0.2:.1f} {base - h*0.5:.1f} {x + lean:.1f} {top:.1f}" '
            f'fill="none" stroke="{stem}" stroke-width="2" stroke-linecap="round"/>{leaves}<g fill="{flower}">{"".join(florets)}</g></g>')

def lantern(cx, top, r, string, op, glow=False):
    """A round lantern hanging on a string from the top of the page."""
    cy = top + string + r
    return (f'<g fill-opacity="{op}" stroke-opacity="{op}"><path d="M{cx} {top} V{cy - r}" stroke="#c9963f" stroke-width="1.5"/>'
            + (f'<circle cx="{cx}" cy="{cy}" r="{r*2.6:.0f}" fill="#ff8f45" fill-opacity="{op*0.10:.3f}"/>' if glow else "") +
            f'<rect x="{cx - r*0.4:.1f}" y="{cy - r*1.02:.1f}" width="{r*0.8:.1f}" height="{r*0.2:.1f}" rx="2" fill="#e2b04c"/>'
            f'<ellipse cx="{cx}" cy="{cy}" rx="{r}" ry="{r*0.86:.1f}" fill="#e2521f"/>'
            f'<ellipse cx="{cx}" cy="{cy}" rx="{r*0.36:.1f}" ry="{r*0.86:.1f}" fill="none" stroke="#f7a15e" stroke-width="1"/>'
            f'<ellipse cx="{cx}" cy="{cy}" rx="{r*0.7:.1f}" ry="{r*0.86:.1f}" fill="none" stroke="#f7a15e" stroke-width="1"/>'
            f'<rect x="{cx - r*0.4:.1f}" y="{cy + r*0.82:.1f}" width="{r*0.8:.1f}" height="{r*0.2:.1f}" rx="2" fill="#e2b04c"/>'
            f'<path d="M{cx} {cy + r} v{r*0.9:.1f} M{cx - r*0.18:.1f} {cy + r} v{r*0.7:.1f} M{cx + r*0.18:.1f} {cy + r} v{r*0.7:.1f}" stroke="#e2b04c" stroke-width="1.5" stroke-linecap="round"/></g>')

# ----- page art for the themes that had none (and one new one each for the two new themes) -----

# Celadon: rings spreading from the corner, like the glaze pooling in a bowl.
JADE_ART = tile("".join(f'<circle cx="520" cy="520" r="{r}" fill="none" stroke="#24705f" stroke-opacity="{0.10 - i*0.008:.3f}" stroke-width="1.5"/>'
                        for i, r in enumerate(range(70, 520, 48))), 520)
# Matcha: tea leaves on the table.
MATCHA_ART = tile(tea_leaf(70, 80, 20, 35, "#6f8f2c", 0.16) + tea_leaf(300, 130, 26, -50, "#5d7d25", 0.13)
                  + tea_leaf(170, 300, 16, 110, "#86a53a", 0.15) + tea_leaf(380, 360, 21, 15, "#6f8f2c", 0.12)
                  + tea_leaf(60, 390, 13, -20, "#86a53a", 0.13), 440)
# Lavender: a field of it along the bottom of the page.
LAVENDER_ART = tile("".join(lavender_sprig(x, 240, h, lean, op) for x, h, lean, op in [
    (60, 150, 14, 0.2), (120, 190, -10, 0.16), (190, 130, 8, 0.18), (270, 170, -14, 0.14), (350, 200, 12, 0.18),
    (430, 140, -8, 0.15), (1150, 150, 10, 0.15), (1230, 195, -12, 0.18), (1310, 135, 9, 0.14), (1390, 180, -10, 0.18),
    (1470, 205, 14, 0.16), (1540, 150, -8, 0.2)]), 1600, 240)
# Imperial: the lattice of a palace window, in gold.
IMPERIAL_ART = svg("""<svg xmlns="http://www.w3.org/2000/svg" width="72" height="72" viewBox="0 0 72 72" fill="none" stroke="#e4b44f" stroke-opacity="0.07" stroke-width="1.5">
  <path d="M0 0.75 H72 M0.75 0 V72"/><rect x="18" y="18" width="36" height="36"/><path d="M36 0 V18 M36 54 V72 M0 36 H18 M54 36 H72"/><rect x="29" y="29" width="14" height="14"/>
</svg>""")
# Lantern: two of them lit in the corner.
LANTERN_ART = ("radial-gradient(circle at 88% 0%, rgb(255 143 69 / 0.16) 0, rgb(255 143 69 / 0.05) 22%, transparent 42%), "
               + tile(lantern(150, 0, 34, 58, 0.62) + lantern(60, 0, 24, 112, 0.5) + lantern(232, 0, 18, 96, 0.45), 380, 270))
# Slate: squared paper, as for drawing to scale.
SLATE_ART = svg("""<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100" viewBox="0 0 100 100" fill="none" stroke="#1f5fae">
  <path d="M20 0 V100 M40 0 V100 M60 0 V100 M80 0 V100 M0 20 H100 M0 40 H100 M0 60 H100 M0 80 H100" stroke-opacity="0.055"/>
  <path d="M0 0.5 H100 M0.5 0 V100" stroke-opacity="0.13"/>
</svg>""")
# Rouge: petals on the evening air, a little gold dust, and a glow in the corner.
ROUGE_ART = ("radial-gradient(circle at 8% 0%, rgb(255 143 179 / 0.10) 0, transparent 38%), "
             + tile(loose_petal(70, 80, 15, 25, 0.2, fill="#e0608c") + loose_petal(300, 150, 19, -50, 0.16, fill="#f08fb0")
                    + loose_petal(170, 300, 13, 140, 0.2, fill="#e0608c") + loose_petal(380, 350, 16, 200, 0.15, fill="#f08fb0")
                    + loose_petal(50, 380, 11, -100, 0.17, fill="#e0608c") + loose_petal(230, 40, 9, 70, 0.16, fill="#f08fb0")
                    + star4(130, 170, 5, "#e8b96a", 0.3) + star4(340, 60, 3.5, "#e8b96a", 0.26) + star4(260, 380, 4.5, "#e8b96a", 0.28)
                    + star4(400, 240, 3, "#e8b96a", 0.24) + star4(30, 200, 3, "#e8b96a", 0.24), 440))

# ----- what sits on the flashcard's corner -----
INK_SEAL = tile('<rect x="2" y="2" width="40" height="40" rx="5" fill="#a82127"/>'
                '<text x="22" y="32" font-size="27" text-anchor="middle" fill="#fbfaf5" font-family="KaiTi, STKaiti, Kaiti SC, serif">学</text>', 44)
LAVENDER_CORNER = tile(lavender_sprig(30, 104, 84, 22, 0.75) + lavender_sprig(48, 104, 62, 34, 0.6), 96, 108)
PLUM_CORNER = tile('<path d="M118 6 C92 18 70 22 46 40 C34 49 24 56 8 60 M70 24 C66 38 58 46 50 56" fill="none" stroke="#4a3a3a" stroke-opacity="0.4" stroke-width="3" stroke-linecap="round"/>'
                   + plum_flower(46, 40, 10, "#c43a5c", 0.75) + plum_flower(84, 22, 8, "#c43a5c", 0.7) + plum_flower(50, 58, 7, "#c43a5c", 0.65)
                   + '<circle cx="14" cy="59" r="3.5" fill="#c43a5c" fill-opacity="0.7"/>', 120, 72)
CLOUD_CORNER = tile(f'<g fill="none" stroke="#b07a2c" stroke-opacity="0.6" stroke-width="2"><path transform="translate(10 44)" d="{CLOUD}"/></g>', 96, 64)
CRESCENT = tile('<circle cx="30" cy="30" r="28" fill="#c6adff" fill-opacity="0.12"/><path d="M38 9 A22 22 0 1 0 49 41 A17 17 0 1 1 38 9 Z" fill="#efeaff" fill-opacity="0.9"/>'
                + star4(12, 14, 4, "#efeaff", 0.8) + star4(50, 12, 2.5, "#efeaff", 0.6), 60)
LANTERN_MINI = tile(lantern(28, 0, 17, 18, 0.95), 56, 92)
ROUGE_CORNER = tile(sakura(42, 34, 21, 12, 0.92, fill="#e0608c", heart="#f3cf8f") + loose_petal(14, 62, 10, -35, 0.8, fill="#f08fb0")
                    + loose_petal(66, 66, 8, 50, 0.7, fill="#e0608c") + star4(72, 14, 4, "#e8b96a", 0.9) + star4(10, 22, 3, "#e8b96a", 0.7), 84, 76)
TEA_CORNER = tile(tea_leaf(26, 34, 20, -35, "#6f8f2c", 0.7) + tea_leaf(52, 30, 15, 25, "#86a53a", 0.65), 76, 62)

# ----- the mark round the 好, and its colour -----
SEALS = {
    "ink": (seal_svg('<rect x="-42" y="-42" width="84" height="84" rx="9" fill="#a82127"/>'
                     '<rect x="-35" y="-35" width="70" height="70" rx="4" fill="none" stroke="#fbfaf5" stroke-opacity="0.85" stroke-width="2"/>'), "#fbfaf5"),
    "jade": (seal_svg('<circle r="47" fill="#a9d3c4" stroke="#24705f" stroke-width="2.5"/><circle r="38" fill="none" stroke="#ffffff" stroke-opacity="0.65" stroke-width="3"/>'
                      '<circle r="26" fill="#f6fbf9" stroke="#24705f" stroke-width="2"/>'), "#24705f"),
    "matcha": (seal_svg('<circle r="47" fill="#4b3d2c"/><circle r="41" fill="#a9c262"/>'
                        '<path d="M-27 -10 C-14 -32 16 -32 27 -10 C33 6 18 28 0 26" fill="none" stroke="#dbe8ae" stroke-width="4" stroke-linecap="round"/>'
                        '<circle r="25" fill="#f6f8ea"/>'), "#4f6b1f"),
    "lavender": (seal_svg(spin(14, '<ellipse cx="0" cy="-43" rx="4.5" ry="8.5" fill="#8f73dc"/>') + spin(14, '<ellipse cx="0" cy="-33" rx="3" ry="6" fill="#6fa77c"/>', 12.9)
                          + '<circle r="26" fill="#faf8ff" stroke="#5f45ad" stroke-width="2"/>'), "#5f45ad"),
    "imperial": (seal_svg('<circle r="47" fill="#7a1b1f" stroke="#e4b44f" stroke-width="3"/><circle r="38" fill="none" stroke="#e4b44f" stroke-width="1.5"/>'
                          + spin(24, '<circle cx="0" cy="-42.5" r="1.6" fill="#e4b44f"/>')), "#f0c96a"),
    "indigo": (seal_svg('<circle r="47" fill="#1c2b4b" stroke="#79a8ff" stroke-width="2.5"/><g fill="none" stroke="#79a8ff" stroke-opacity="0.4" stroke-width="2">'
                        '<path d="M-39 22 a13 13 0 0 1 26 0 a13 13 0 0 1 26 0 a13 13 0 0 1 26 0"/><path d="M-26 34 a13 13 0 0 1 26 0 a13 13 0 0 1 26 0"/></g>'), "#d7e5ff"),
    "lantern": (seal_svg('<path d="M0 -55 V-44" stroke="#f2c14e" stroke-width="2.5"/><rect x="-16" y="-46" width="32" height="8" rx="2" fill="#f2c14e"/>'
                         '<ellipse cx="0" cy="-4" rx="45" ry="36" fill="#e2521f"/><g fill="none" stroke="#b63a12" stroke-width="1.5"><ellipse cx="0" cy="-4" rx="15" ry="36"/><ellipse cx="0" cy="-4" rx="31" ry="36"/></g>'
                         '<rect x="-16" y="30" width="32" height="8" rx="2" fill="#f2c14e"/><path d="M0 38 V55 M-7 38 V50 M7 38 V50" stroke="#f2c14e" stroke-width="2.5" stroke-linecap="round"/>'), "#fff1cf"),
    "maple": (seal_svg(f'<g stroke="#ad431b" stroke-width="2.5" stroke-linejoin="round">{maple_leaf(0, 0, 52, 0, "#fbd9c4", 1)}</g><circle r="23" fill="#fffdf9"/>'), "#ad431b"),
    "plum": (seal_svg('<g fill="#fbe4ea" stroke="#981d43" stroke-width="2.5">' + spin(5, '<circle cx="0" cy="-27" r="20"/>') + '</g><circle r="27" fill="#ffffff"/>'), "#981d43"),
    "dunhuang": (seal_svg('<g stroke="#fdf8ee" stroke-width="1.5">' + spin(8, '<ellipse cx="0" cy="-30" rx="12" ry="22" fill="#1d578b"/>')
                          + spin(8, '<ellipse cx="0" cy="-27" rx="8" ry="17" fill="#c9882a"/>', 22.5) + '</g><circle r="25" fill="#fdf8ee" stroke="#1d578b" stroke-width="2"/>'), "#1d578b"),
    "moon": (seal_svg('<circle r="55" fill="#c6adff" fill-opacity="0.14"/><circle r="47" fill="#c6adff" fill-opacity="0.2"/><circle r="39" fill="#efeaff"/>'
                      '<circle cx="-16" cy="-18" r="6" fill="#dcd3f6"/><circle cx="19" cy="15" r="8" fill="#dcd3f6"/><circle cx="14" cy="-22" r="3.5" fill="#dcd3f6"/>'), "#2a2148"),
    "pine": (seal_svg('<circle r="37" fill="#1d3a2a" stroke="#82d6a3" stroke-width="2"/>' + spin(30, '<path d="M0 -41 V-53" stroke="#82d6a3" stroke-width="2" stroke-linecap="round"/>')), "#a8e6c0"),
    "slate": (seal_svg('<circle r="48" fill="#ffffff" stroke="#1f5fae" stroke-width="2.5"/>' + spin(60, '<path d="M0 -47 V-43" stroke="#1f5fae" stroke-width="1"/>')
                       + spin(12, '<path d="M0 -47 V-38" stroke="#1f5fae" stroke-width="2"/>')
                       + '<circle r="29" fill="none" stroke="#1f5fae" stroke-opacity="0.35" stroke-width="1.5" stroke-dasharray="3 4"/>'), "#1f5fae"),
    "rouge": (seal_svg('<g fill="#7d1f42" stroke="#e8b96a" stroke-width="1.5">' + spin(6, '<circle cx="0" cy="-26" r="22"/>')
                       + spin(6, '<circle cx="0" cy="-17" r="16" fill="#a22b57"/>', 30) + '</g><circle r="25" fill="#2a0f1a" stroke="#e8b96a" stroke-width="1.5"/>'), "#f3cf8f"),
}

# ----- what drifts over the finished-session box: (tile, how it moves, how strong) -----
DRIFT = {
    "plum": (tile(dots(1, 10, ["#c2cddd", "#d6dfeb"], 2.5, 5)), "fall", 0.8),
    "pine": (tile(dots(2, 10, ["#e9f2eb"], 1.8, 4.2)), "fall", 0.5),
    "maple": (tile(maple_leaf(46, 50, 13, 30, "#d9733a", 0.8) + maple_leaf(190, 110, 16, -40, "#c9562a", 0.7) + maple_leaf(104, 190, 11, 150, "#e0913f", 0.8)), "fall", 0.4),
    "lavender": (tile(dots(4, 12, ["#9a7fe0", "#b9a3f0", "#8f73dc"], 2, 4, op=(0.5, 0.85))), "fall", 0.55),
    "rouge": (tile(loose_petal(40, 46, 13, 30, 0.8, fill="#e0608c") + loose_petal(196, 96, 15, -40, 0.7, fill="#f08fb0")
                   + loose_petal(104, 170, 11, 200, 0.7, fill="#e0608c") + loose_petal(200, 224, 12, -110, 0.65, fill="#f08fb0")), "fall", 0.45),
    "lantern": (tile(dots(3, 9, ["#ff8f45", "#f2c14e", "#ffb37a"], 1.4, 3.2)), "rise", 0.7),
    "moon": (tile(star4(40, 50, 6, "#efeaff", 0.9) + star4(170, 30, 4, "#efeaff", 0.7) + star4(210, 150, 7, "#efeaff", 0.9)
                  + star4(90, 190, 4.5, "#efeaff", 0.8) + star4(140, 110, 3, "#efeaff", 0.6)), "twinkle", 0.7),
    "imperial": (tile(star4(50, 40, 5, "#e4b44f", 0.9) + star4(190, 70, 3.5, "#e4b44f", 0.7) + star4(120, 150, 6, "#e4b44f", 0.9)
                      + star4(30, 200, 3.5, "#e4b44f", 0.7) + star4(210, 210, 4.5, "#e4b44f", 0.8)), "twinkle", 0.6),
}

def rule(selector, props):
    return selector + " {\n" + "".join(f"  {k}: {v};\n" for k, v in props.items()) + "}\n"

def kit(key, title=None, button=None, logo=None, flash=None, corner=None, bar=None, nav=None):
    """The CSS for one theme's details. Everything is optional:
    title  ("highlight" | "rule" | "double", colour)   the mark under page titles
    button "pill" | "round" | "sharp"                  the primary button's shape
    logo   {css properties}                            the app's mark in the side bar
    flash  ("tape", c1, c2) | ("corners", colour) | ("stitch", colour) | ("ring", colour)
    corner (image, width, height, "top right" …)       a drawing on a corner of the flashcard
    bar    (from, to)                                  the progress bar's colours
    nav    colour                                      a line beside the page that is open"""
    S = f':root[data-theme="{key}"]'
    out = [f"\n/* {key}: its own details (scripts/gen_themes.py) */\n"]
    if title:
        kind, color = title
        marks = {
            "highlight": {"text-decoration-thickness": "0.4em", "text-underline-offset": "-0.16em"},
            "rule": {"text-decoration-thickness": "0.09em", "text-underline-offset": "0.3em"},
            "double": {"text-decoration-style": "double", "text-decoration-thickness": "0.05em", "text-underline-offset": "0.3em"},
        }[kind]
        out.append(rule(f"{S} h1", {"text-decoration": "underline", "text-decoration-color": color, "text-decoration-skip-ink": "none", **marks}))
    if button == "pill":
        out.append(rule(f"{S} .btn.primary", {"border-radius": "999px", "box-shadow": "0 2px 0 var(--accent-hover)",
                                              "transition": "background 0.12s, transform 0.06s, box-shadow 0.06s"}))
        out.append(rule(f"{S} .btn.primary:active:not(:disabled)", {"transform": "translateY(2px)", "box-shadow": "none"}))
    elif button == "round":
        out.append(rule(f"{S} .btn.primary", {"border-radius": "999px"}))
    elif button == "sharp":
        out.append(rule(f"{S} .btn", {"border-radius": "4px"}))
        out.append(rule(f"{S} .btn.primary", {"letter-spacing": "0.02em"}))
    if logo:
        out.append(rule(f"{S} .logo", logo))
    if nav:
        out.append(rule(f"{S} .navlink.on", {"box-shadow": f"inset 3px 0 0 {nav}"}))
        out.append(rule(f"{S} .compact .navlink.on", {"box-shadow": "none"}))
    if flash:
        kind = flash[0]
        base = {"content": '""', "position": "absolute", "pointer-events": "none"}
        if kind == "tape":
            out.append(rule(f"{S} .flash::before", {
                **base, "top": "-12px", "left": "50%", "width": "116px", "height": "26px", "transform": "translateX(-50%) rotate(-1.6deg)",
                "background": f"repeating-linear-gradient(135deg, {flash[1]} 0 9px, {flash[2]} 9px 18px)",
                "clip-path": "polygon(0 0, 3% 25%, 0 50%, 3% 75%, 0 100%, 100% 100%, 97% 75%, 100% 50%, 97% 25%, 100% 0)",
                "filter": "drop-shadow(0 1px 1px rgb(0 0 0 / 0.15))"}))
        elif kind == "corners":
            c = flash[1]
            arms = ", ".join(f"linear-gradient({c}, {c}) {pos} / {size} no-repeat"
                             for pos in ("left top", "right top", "left bottom", "right bottom") for size in ("24px 2px", "2px 24px"))
            out.append(rule(f"{S} .flash::before", {**base, "inset": "8px", "background": arms}))
        elif kind == "stitch":
            out.append(rule(f"{S} .flash::before", {**base, "inset": "9px", "border": f"1.5px dashed {flash[1]}", "border-radius": "calc(var(--radius-lg) - 7px)"}))
        elif kind == "ring":
            out.append(rule(f"{S} .flash::before", {**base, "inset": "7px", "border": f"1px solid {flash[1]}", "border-radius": "calc(var(--radius-lg) - 6px)"}))
    if corner:
        image, w, h, where = corner
        v, hz = where.split()
        # (under the card's words, should a long answer reach the corner)
        out.append(rule(f"{S} .flash", {"isolation": "isolate"}))
        out.append(rule(f"{S} .flash::after", {"content": '""', "position": "absolute", "z-index": "-1", "pointer-events": "none", v: "12px", hz: "14px",
                                               "width": f"{w}px", "height": f"{h}px", "background": f"{image} center / contain no-repeat"}))
    if bar:
        out.append(rule(f"{S} .topbar .bar", {"height": "8px", "border-radius": "8px"}))
        out.append(rule(f"{S} .topbar .bar div", {"border-radius": "8px", "background": f"linear-gradient(90deg, {bar[0]}, {bar[1]})"}))
    if key in SEALS:
        image, ink = SEALS[key]
        out.append(rule(f"{S} .seal", {"width": "108px", "height": "108px", "border-radius": "0", "background": f"{image} center / contain no-repeat",
                                       "color": ink, "font-family": "var(--font-han-serif)", "font-size": "40px", "font-weight": "700",
                                       **({"padding-bottom": "8px"} if key == "lantern" else {})}))
    if key in DRIFT:
        image, move, strength = DRIFT[key]
        out.append(rule(f"{S} .donebox", {"position": "relative", "overflow": "hidden"}))
        out.append(rule(f"{S} .donebox > *", {"position": "relative", "z-index": "1"}))
        both = f"{S} .donebox::before,\n{S} .donebox::after"
        layer = {"content": '""', "position": "absolute", "inset": "0", "z-index": "0", "pointer-events": "none",
                 "--drift": "240px", "background": f"{image} 0 0 / 240px 240px repeat", "opacity": f"{strength}"}
        if move == "twinkle":
            out.append(rule(both, {**layer, "--dim": f"{strength * 0.25:.2f}", "--bright": f"{strength}", "animation": "theme-twinkle 3.4s ease-in-out infinite"}))
            out.append(rule(f"{S} .donebox::after", {"background-position": "110px 70px", "animation-duration": "4.9s", "animation-delay": "-1.6s"}))
        else:
            out.append(rule(both, {**layer, "animation": f"theme-{move} 14s linear infinite, theme-sway 5s ease-in-out infinite alternate"}))
            out.append(rule(f"{S} .donebox::after", {"--drift": "204px", "background-size": "204px 204px", "opacity": f"{strength * 0.6:.2f}",
                                                     "animation-duration": "19s, 7s"}))
    return "".join(out)

# Movement the details share (one copy, whatever the theme).
COMMON_CSS = """
/* movement shared by the themes' details */
@keyframes theme-fall {
  to {
    background-position: 0 var(--drift);
  }
}
@keyframes theme-rise {
  to {
    background-position: 0 calc(-1 * var(--drift));
  }
}
@keyframes theme-sway {
  from {
    transform: translateX(-10px);
  }
  to {
    transform: translateX(10px);
  }
}
@keyframes theme-twinkle {
  0%,
  100% {
    opacity: var(--dim);
  }
  50% {
    opacity: var(--bright);
  }
}
@media (prefers-reduced-motion: reduce) {
  :root[data-theme] .donebox::before,
  :root[data-theme] .donebox::after {
    animation: none;
  }
}
"""

def mix(a, b, t):
    """Colour `a` moved a share `t` of the way to `b`."""
    pa, pb = (tuple(int(c.lstrip("#")[i:i + 2], 16) for i in (0, 2, 4)) for c in (a, b))
    return "#" + "".join(f"{round(x + (y - x) * t):02x}" for x, y in zip(pa, pb))

def charts(palette, series):
    """Chart colours of the theme's own: four series, and the heatmap as steps of its accent."""
    acc, dark_theme = palette["accent"], palette["scheme"] == "dark"
    if dark_theme:
        steps = [mix(acc, palette["bg"], 0.72), mix(acc, palette["bg"], 0.48), mix(acc, palette["bg"], 0.2), mix(acc, "#ffffff", 0.2)]
    else:
        steps = [mix(acc, "#ffffff", 0.78), mix(acc, "#ffffff", 0.52), mix(acc, "#ffffff", 0.22), mix(acc, "#000000", 0.18)]
    return {**{f"series-{i + 1}": c for i, c in enumerate(series)}, **{f"seq-{i + 1}": c for i, c in enumerate(steps)}}

# ----- two more themes, so that the light and the dark side each have a bold and a soft one -----
THEMES["slate"] = ("Slate", "青石", "石", light(
    # a drawing office: cool squared paper, blue-black ink, a draughtsman's blue
    bg="#eef1f5", surface="#ffffff", **{"surface-2": "#e3e8ef", "surface-3": "#d3dbe6"}, sidebar="#f6f8fb",
    border="#cdd6e2", text="#111a26", **{"text-2": "#3f4c5e", "text-3": "#657285"},
    accent="#1f5fae", **{"accent-hover": "#184d8f", "accent-soft": "#dde9f8", "accent-text": "#ffffff"},
    art=SLATE_ART, artsize="100px 100px", artpos="0 0", artrepeat="repeat",
    shadow="0 1px 2px rgb(20 40 80 / 0.07), 0 4px 16px rgb(20 40 80 / 0.06)",
    **{"seq-0": "#e2e8f0", "grid": "#dbe2ec"}))
THEMES["rouge"] = ("Rouge", "胭脂", "胭", dark(
    # evening: wine-dark, rose and a little gold
    bg="#1a0d13", surface="#25131b", **{"surface-2": "#311a24", "surface-3": "#40222f"}, sidebar="#13090e",
    border="#4a2838", text="#f8eaf0", **{"text-2": "#d9bdc8", "text-3": "#a98895"},
    accent="#ff8fb3", **{"accent-hover": "#ffa9c4", "accent-soft": "#45182a", "accent-text": "#2a0b17"},
    art=ROUGE_ART, artsize="100% 100%, 440px 440px", artpos="0 0, 0 0", artrepeat="no-repeat, repeat",
    shadow="inset 0 1px 0 rgb(255 200 220 / 0.04), 0 1px 2px rgb(0 0 0 / 0.45), 0 6px 18px rgb(0 0 0 / 0.25)",
    **{"seq-0": "#2c1720", "grid": "#351c28"}))

def dress(key, series, art=None, extra=None, **details):
    """Give a theme its chart colours, page art (when it had none) and details."""
    p = THEMES[key][3]
    p.update(charts(p, series))
    if art:
        p.update(art=art[0], artsize=art[1], artpos=art[2], artrepeat=art[3])
    if extra:
        p["extra"] = extra
    p["css"] = kit(key, **details)

SERIF = {"font-family": "var(--font-han-serif)"}
dress("ink", ["#a82127", "#3d3a34", "#2f5f8a", "#a87a1c"], extra={"radius": "6px", "radius-lg": "10px"},
      title=("rule", "#1b1a17"), button="sharp", logo={**SERIF, "border-radius": "6px", "background": "#a82127"},
      corner=(INK_SEAL, 40, 40, "bottom right"), bar=("#77726a", "#1b1a17"), nav="#a82127")
dress("jade", ["#24705f", "#c26a4a", "#3f7fb5", "#b98a1e"], art=(JADE_ART, "520px 520px", "right bottom", "no-repeat"),
      extra={"radius": "14px", "radius-lg": "22px"},
      title=("highlight", "rgb(120 200 175 / 0.45)"), button="round", logo={"border-radius": "50%", "background": "radial-gradient(circle at 35% 30%, #4a9a86, #1b5a4c)"},
      flash=("ring", "#cddfd7"), bar=("#a9d3c4", "#24705f"))
dress("matcha", ["#5d7d25", "#b5651d", "#3e8a8a", "#b08a18"], art=(MATCHA_ART, "440px 440px", "0 0", "repeat"),
      extra={"radius": "12px", "radius-lg": "18px"},
      title=("highlight", "rgb(186 208 88 / 0.5)"), button="round", logo={"border-radius": "50% 50% 50% 12px", "background": "linear-gradient(145deg, #6f8f2c, #405818)"},
      corner=(TEA_CORNER, 70, 57, "bottom left"), bar=("#c4d88a", "#4f6b1f"))
dress("lavender", ["#7256c4", "#5f9a6b", "#d0679a", "#b9871f"], art=(LAVENDER_ART, "max(1600px, 100%) auto", "bottom center", "no-repeat"),
      extra={"radius": "14px", "radius-lg": "20px"},
      title=("highlight", "rgb(190 165 245 / 0.5)"), button="pill", logo={"border-radius": "14px", "background": "linear-gradient(145deg, #7e63cf, #4f3896)"},
      flash=("tape", "rgb(170 145 235 / 0.55)", "rgb(206 190 246 / 0.55)"), corner=(LAVENDER_CORNER, 60, 68, "bottom right"), bar=("#cbbdf3", "#5f45ad"), nav="#5f45ad")
dress("imperial", ["#e4b44f", "#e0604a", "#6fb7a8", "#b99bd6"], art=(IMPERIAL_ART, "72px 72px", "0 0", "repeat"),
      extra={"radius": "8px", "radius-lg": "12px"},
      title=("rule", "#e4b44f"), button="sharp", logo={**SERIF, "border-radius": "8px", "background": "linear-gradient(145deg, #a4262b, #6e1519)", "color": "#f0c96a",
                                                      "box-shadow": "inset 0 0 0 1.5px #e4b44f"},
      flash=("corners", "#e4b44f"), bar=("#8a6a25", "#e4b44f"), nav="#e4b44f")
dress("indigo", ["#79a8ff", "#f0a35e", "#6fd0b0", "#e58fb0"], extra={"radius": "8px", "radius-lg": "14px"},
      title=("rule", "#79a8ff"), button="sharp", logo={"border-radius": "8px", "background": "linear-gradient(145deg, #3b6fd6, #1c3f8f)", "color": "#ffffff"},
      flash=("stitch", "rgb(121 168 255 / 0.4)"), bar=("#2f4f8f", "#79a8ff"), nav="#79a8ff")
dress("lantern", ["#ff8f45", "#f2c14e", "#6fc1a4", "#e2729a"], art=(LANTERN_ART, "100% 100%, 380px 270px", "0 0, right top", "no-repeat, no-repeat"),
      title=("highlight", "rgb(255 143 69 / 0.3)"), logo={"border-radius": "50%", "background": "radial-gradient(circle at 50% 40%, #ffb066, #e2521f)", "color": "#2a1206"},
      corner=(LANTERN_MINI, 40, 66, "top right"), bar=("#8a3d16", "#ff8f45"))
dress("maple", ["#c9511f", "#7a8f2a", "#3f7fb5", "#b98414"],
      title=("highlight", "rgb(255 170 90 / 0.5)"), logo={"border-radius": "12px", "background": "linear-gradient(145deg, #d2622a, #923716)"},
      flash=("tape", "rgb(222 150 84 / 0.6)", "rgb(238 190 140 / 0.6)"), bar=("#f0b78a", "#ad431b"), nav="#ad431b")
dress("plum", ["#a3224b", "#4f7fa8", "#5c9a6b", "#b98a26"],
      title=("rule", "#981d43"), logo={**SERIF, "border-radius": "12px", "background": "linear-gradient(145deg, #b32a55, #771332)"},
      corner=(PLUM_CORNER, 124, 74, "top right"), bar=("#e3a9bb", "#981d43"), nav="#981d43")
dress("dunhuang", ["#1d578b", "#c0622d", "#3f8f7a", "#b98722"],
      title=("highlight", "rgb(226 172 80 / 0.5)"), logo={**SERIF, "border-radius": "10px", "background": "linear-gradient(145deg, #2a6aa3, #174773)",
                                                         "box-shadow": "inset 0 0 0 1.5px #d9a441"},
      corner=(CLOUD_CORNER, 78, 52, "top right"), bar=("#d9a441", "#1d578b"), nav="#c0622d")
dress("moon", ["#c6adff", "#7fb6ff", "#7fd6b5", "#f2b56b"], extra={"radius": "14px", "radius-lg": "20px"},
      title=("highlight", "rgb(198 173 255 / 0.28)"), button="round", logo={"border-radius": "50%", "background": "radial-gradient(circle at 35% 30%, #efeaff, #b9a2f0)", "color": "#2a2148"},
      corner=(CRESCENT, 48, 48, "top right"), bar=("#4a3a86", "#c6adff"))
dress("pine", ["#82d6a3", "#e8b25c", "#7fb6e8", "#e58f8f"],
      title=("rule", "#82d6a3"), button="sharp", logo={"border-radius": "10px", "background": "linear-gradient(145deg, #3f9a68, #1d5a3a)", "color": "#ffffff"},
      flash=("ring", "#2f4a3b"), bar=("#2f6a48", "#82d6a3"), nav="#82d6a3")
dress("slate", ["#1f5fae", "#c9651f", "#2f9a7a", "#b98a1a"], extra={"radius": "6px", "radius-lg": "10px"},
      title=("double", "#1f5fae"), button="sharp", logo={"border-radius": "6px", "background": "#1f5fae", "box-shadow": "inset 0 0 0 2px #ffffff, 0 0 0 1.5px #1f5fae"},
      flash=("corners", "#1f5fae"), bar=("#9dbde6", "#1f5fae"), nav="#1f5fae")
dress("rouge", ["#ff8fb3", "#e8b96a", "#8fc7b0", "#b9a2f0"], extra={"radius": "12px", "radius-lg": "18px"},
      title=("rule", "#e8b96a"), button="pill", logo={**SERIF, "border-radius": "14px", "background": "linear-gradient(145deg, #c23a6c, #7d1f42)", "color": "#ffe9c4",
                                                     "box-shadow": "inset 0 0 0 1.5px #e8b96a"},
      flash=("ring", "rgb(232 185 106 / 0.42)"), corner=(ROUGE_CORNER, 62, 56, "bottom right"), bar=("#7d1f42", "#ff8fb3"), nav="#ff8fb3")

# How the picker lays the themes out: by feel, with light and dark ones in each row.
GROUPS = [
    ("soft", ["sakura", "lavender", "plum", "rouge"]),
    ("bold", ["slate", "indigo", "pine", "imperial", "lantern"]),
    ("calm", ["ink", "jade", "matcha", "maple", "dunhuang", "moon", "contrast"]),
]
assert sorted(k for _, keys in GROUPS for k in keys) == sorted(THEMES), "every theme is in one group"
GROUP_OF = {k: g for g, keys in GROUPS for k in keys}
THEMES = {k: THEMES[k] for _, keys in GROUPS for k in keys}

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
    # chart colours are also used for figures and marks on the page
    ("series-1", "surface", 3), ("series-2", "surface", 3), ("series-3", "surface", 3), ("series-4", "surface", 3),
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
# blue, orange, green: told apart with every common kind of colour blindness, on light and dark
CAT_LIGHT = ["#2a78d6", "#eb6834", "#1baf7a"]
CAT_DARK = ["#3987e5", "#d95926", "#199e70"]
css = ["/* THEMES:BEGIN (generated by scripts/gen_themes.py; contrast-checked) */"]
for key, (_, _, _, p) in THEMES.items():
    lines = [f'  color-scheme: {p["scheme"]};'] + [f"  --{v}: {p[v]};" for v in VARS]
    lines += [f"  --page-art: {p['art']};", f"  --page-art-size: {p['artsize']};",
              f"  --page-art-pos: {p['artpos']};", f"  --page-art-repeat: {p['artrepeat']};"]
    # charts that tell series apart by colour use one checked set per scheme, whatever the theme
    # (a theme's own chart colours are for single-series charts: see the dataviz check in StatsView)
    lines += [f"  --cat-{i + 1}: {c};" for i, c in enumerate(CAT_DARK if p["scheme"] == "dark" else CAT_LIGHT)]
    # a theme may set other variables (corner radii …) and bring rules of its own
    lines += [f"  --{name}: {value};" for name, value in p.get("extra", {}).items()]
    css.append(f':root[data-theme="{key}"] {{\n' + "\n".join(lines) + "\n}" + p.get("css", "").rstrip())
css.append(COMMON_CSS.strip())
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
      "  /** Which row of the picker it sits in. */",
      "  group: ThemeGroup;",
      "}",
      "",
      "/** The picker's rows, in order. */",
      "export const THEME_GROUPS = [" + ", ".join(f'"{g}"' for g, _ in GROUPS) + "] as const;",
      "export type ThemeGroup = (typeof THEME_GROUPS)[number];",
      "",
      "export const CUSTOM_THEMES: ThemeInfo[] = ["]
for key, (name, zh, emblem, p) in THEMES.items():
    ts.append(f'  {{ key: "{key}", name: "{name}", zh: "{zh}", emblem: "{emblem}", dark: {"true" if p["scheme"] == "dark" else "false"}, '
              f'bg: "{p["bg"]}", surface: "{p["surface"]}", accent: "{p["accent"]}", accentSoft: "{p["accent-soft"]}", group: "{GROUP_OF[key]}" }},')
ts += ["];", "", "export const isCustomTheme = (t: string) => CUSTOM_THEMES.some((x) => x.key === t);", ""]
open(os.path.join(ROOT, "src", "lib", "themes.ts"), "w", encoding="utf-8", newline="").write("\n".join(ts))
print("wrote app.css themes and src/lib/themes.ts")
