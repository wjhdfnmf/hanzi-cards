<script lang="ts" module>
  // A book cover for a story: a colour and a small scene chosen from what the story is about
  // (words in its title, then in its opening lines), so the same story always gets the same
  // cover and stories about the sea, the moon or dinner look the part. Covers keep their own
  // colours in every theme, as printed books do.

  type Scene = "mountain" | "night" | "sea" | "festival" | "bamboo" | "tea" | "home" | "city" | "rain" | "blossom" | "food" | "school" | "road" | "pet";

  const WORDS: [Scene, RegExp][] = [
    ["night", /月|星|夜|晚上|梦|睡|moon|star|night|dream|sleep/i],
    ["sea", /海|河|湖|船|鱼|游泳|水|sea|ocean|river|lake|boat|fish|swim/i],
    ["festival", /春节|新年|过年|节|灯|龙|红包|生日|礼物|festival|new year|lantern|dragon|birthday|gift/i],
    ["food", /吃|饭|菜|面|饺子|餐|厨|饿|food|eat|dinner|lunch|noodle|dumpling|restaurant|cook/i],
    ["tea", /茶|咖啡|喝|tea|coffee|drink/i],
    ["pet", /猫|狗|兔|鸟|熊|动物|宠物|cat|dog|rabbit|bird|panda|animal|pet/i],
    ["school", /学|书|老师|课|考试|字|school|class|teacher|book|study|exam|lesson/i],
    ["rain", /雨|雪|风|天气|冬|云|rain|snow|wind|weather|winter|cloud/i],
    ["blossom", /花|春|树|公园|草|flower|spring|tree|garden|park/i],
    ["mountain", /山|爬|太阳|日出|长城|mountain|hill|climb|sun|great wall/i],
    ["road", /路|旅|车|飞机|去|走|旅行|火车|trip|travel|road|train|journey|bus|plane/i],
    ["city", /城|北京|上海|商店|买|工作|公司|街|city|shop|town|work|office|street|market/i],
    ["home", /家|妈|爸|朋友|房|爷|奶|home|family|mother|father|friend|house/i],
    ["bamboo", /竹|故事|传说|古|bamboo|story|legend|tale/i],
  ];
  const ALL: Scene[] = ["mountain", "night", "sea", "festival", "bamboo", "tea", "home", "city", "rain", "blossom", "food", "school", "road", "pet"];

  /** Cover colours: the board (two shades), and the colour of the picture's highlights. */
  const BOARDS: Record<Scene, [string, string, string][]> = {
    night: [["#27346b", "#161d45", "#f6e3a1"], ["#3b2a66", "#1f1640", "#f3d9a4"]],
    sea: [["#1f7a8c", "#0f4c5c", "#ffe8b0"], ["#2a6f97", "#143d5c", "#ffe1a8"]],
    festival: [["#b3262e", "#7c1319", "#ffd166"], ["#c0392b", "#7e1e14", "#ffdf8a"]],
    food: [["#c8553d", "#8a3324", "#ffe5b4"], ["#b5651d", "#7a4011", "#fff0c9"]],
    tea: [["#6b8f71", "#3f5f48", "#fdf0d5"], ["#8d6e52", "#5b4331", "#fdf0d5"]],
    pet: [["#d98c5f", "#a85c36", "#fff3dc"], ["#7b8cde", "#4a57a3", "#fff3dc"]],
    school: [["#3d5a80", "#243b57", "#ffdd99"], ["#5c6f4f", "#394731", "#ffe6a7"]],
    rain: [["#5c7c99", "#3a546b", "#eaf4ff"], ["#6c7a89", "#434f5b", "#f1f6fb"]],
    blossom: [["#d9728f", "#a8455f", "#fff0f3"], ["#c77dab", "#8f4a78", "#fff0f6"]],
    mountain: [["#4f772d", "#31572c", "#ffe08a"], ["#577590", "#33475b", "#ffd98a"]],
    road: [["#e09f3e", "#a86f1c", "#fff6dd"], ["#4d908e", "#2c5f5d", "#fff1cc"]],
    city: [["#52489c", "#322b66", "#ffd6a5"], ["#3a5a78", "#22384d", "#ffd6a5"]],
    home: [["#bc6c25", "#84480f", "#fefae0"], ["#a44a3f", "#6f2c24", "#fbe9d0"]],
    bamboo: [["#2d6a4f", "#1b4332", "#f1e3b5"], ["#588157", "#344e41", "#f5ecc8"]],
  };

  function hash(s: string) {
    let h = 2166136261;
    for (let i = 0; i < s.length; i++) h = Math.imul(h ^ s.charCodeAt(i), 16777619);
    return h >>> 0;
  }

  /** The kinds of cover (Settings › Reading):
   * - "books": a small scene, as above
   * - "cloth": a cloth binding with a gilt frame and the title's first character in a roundel
   * - "thread": a thread-bound book (线装), sewn on the right, with a seal
   * - "poster": flat colour and bold shapes */
  export const LOOKS = ["books", "cloth", "thread", "poster"] as const;
  export type Look = (typeof LOOKS)[number];

  /** Cloth: the board's two shades. The gilt is the same on all. */
  const CLOTH: [string, string][] = [
    ["#6b1f2a", "#48131b"],
    ["#1f2f52", "#131d36"],
    ["#22483a", "#152e25"],
    ["#4a2a55", "#301a38"],
    ["#3a3a3e", "#222225"],
    ["#1d4e5a", "#11333b"],
    ["#7a4a1e", "#523110"],
  ];
  const GILT = "#e9cf8b";
  /** Thread-bound: the cover's two shades, the thread, and ink that reads on the cover. */
  const THREAD: [string, string, string, string][] = [
    ["#2f4f76", "#223c5b", "#efe6d0", "#f4eede"],
    ["#c2a777", "#ad9160", "#4a3521", "#2a1d12"],
    ["#5e7466", "#485c50", "#efe6d0", "#f4eede"],
    ["#7b3f35", "#613027", "#efe6d0", "#f4eede"],
    ["#3c3c44", "#2a2a30", "#e6dcc4", "#f4eede"],
  ];
  /** Poster: the ground, two shape colours, and the title's colour. */
  const POSTER: [string, string, string, string][] = [
    ["#f4c430", "#d7263d", "#1b1b1e", "#1b1b1e"],
    ["#1e3a8a", "#f97316", "#fde8c8", "#ffffff"],
    ["#0f766e", "#fbbf24", "#f1f5f9", "#ffffff"],
    ["#f1ece1", "#e4572e", "#17bebb", "#1b1b1e"],
    ["#ef476f", "#ffd166", "#073b4c", "#ffffff"],
    ["#2b2d42", "#ef233c", "#edf2f4", "#ffffff"],
  ];

  /** A story's cover: the same for the same title every time. `hi` is the colour of the
   * picture's highlights (the gilt, the thread), `ink` one that reads on the bare cover. */
  export function coverOf(title: string, text = "", look: Look = "books") {
    const h = hash(title);
    const scene = WORDS.find(([, re]) => re.test(title))?.[0] ?? WORDS.find(([, re]) => re.test(text))?.[0] ?? ALL[h % ALL.length];
    const pick = <T,>(list: T[]) => list[(h >>> 8) % list.length];
    const first = Array.from(title.trim())[0] ?? "";
    const base = { scene, first, variant: (h >>> 16) % 4, s2: "" };
    if (look === "cloth") {
      const [a, b] = pick(CLOTH);
      return { ...base, a, b, hi: GILT, ink: GILT };
    }
    if (look === "thread") {
      const [a, b, hi, ink] = pick(THREAD);
      return { ...base, a, b, hi, ink };
    }
    if (look === "poster") {
      const [a, hi, s2, ink] = pick(POSTER);
      return { ...base, a, b: a, hi, s2, ink };
    }
    const [a, b, hi] = pick(BOARDS[scene]);
    return { ...base, a, b, hi, ink: "#ffffff" };
  }
</script>

<script lang="ts">
  let {
    title,
    text = "",
    band = false,
    look = "books",
  }: {
    title: string;
    /** The story's opening lines: only read for what the story is about, never shown. */
    text?: string;
    /** A wide strip (the head of an open story) instead of an upright cover. */
    band?: boolean;
    look?: Look;
  } = $props();
  const c = $derived(coverOf(title, text, look));
</script>

<svg
  class="art"
  class:band
  viewBox={band ? "-420 62 540 98" : "0 0 120 160"}
  preserveAspectRatio={band ? "xMaxYMid slice" : "xMidYMax slice"}
  style="--a:{c.a}; --b:{c.b}; --hi:{c.hi}; --s2:{c.s2}"
  aria-hidden="true"
>
  <!-- as a strip, the scene sits at the right end and fades out towards the title -->
  <defs>
    <linearGradient id="cover-fade-g" gradientUnits="userSpaceOnUse" x1="0" x2="34" y1="0" y2="0">
      <stop offset="0" stop-color="#000" /><stop offset="1" stop-color="#fff" />
    </linearGradient>
    <mask id="cover-fade" maskUnits="userSpaceOnUse" x="0" y="0" width="120" height="160">
      <rect width="120" height="160" fill="url(#cover-fade-g)" />
    </mask>
  </defs>
  <g mask={band ? "url(#cover-fade)" : undefined}>
  {#if look === "cloth"}
    <!-- the weave of the cloth, a gilt frame (upright covers), and the title's first character -->
    <defs>
      <pattern id="cover-weave" width="3" height="3" patternUnits="userSpaceOnUse">
        <path d="M0 .5h3 M.5 0v3" stroke="#000" stroke-opacity=".16" stroke-width=".5" />
      </pattern>
    </defs>
    <rect x="-600" width="720" height="160" fill="url(#cover-weave)" />
    {#if !band}
      <rect x="9" y="9" width="102" height="142" rx="2" class="rule" />
      <rect x="12.5" y="12.5" width="95" height="135" rx="1" class="rule thin" />
      {#each [[9, 9], [111, 9], [9, 151], [111, 151]] as [x, y], i (i)}
        <path d="M{x} {y - 4} l4 4 -4 4 -4 -4Z" class="hi" />
      {/each}
    {/if}
    <path d="M26 114 h16 M78 114 h16" class="rule" />
    <path d="M38 114 l3 -3 3 3 -3 3Z M76 114 l3 -3 3 3 -3 3Z" class="hi" />
    <circle cx="60" cy="114" r="15" class="rule" />
    <circle cx="60" cy="114" r="12.2" class="rule thin" />
    <text x="60" y="120" text-anchor="middle" class="seal-char gilt">{c.first}</text>
  {:else if look === "thread"}
    <!-- sewn through five holes on the right; a red seal in the corner -->
    <rect x="103" width="17" height="160" class="dk" opacity=".35" />
    <path d="M108 0 V160" class="thread" />
    {#each [14, 47, 80, 113, 146] as y (y)}
      <path d="M108 {y} H120" class="thread" />
      <circle cx="108" cy={y} r="1.5" class="dk2" />
    {/each}
    <rect x="13" y="128" width="19" height="19" rx="2.5" fill="#b3261e" />
    <rect x="15" y="130" width="15" height="15" rx="1.5" fill="none" stroke="#f7e9d7" stroke-width=".8" />
    <text x="22.5" y="142.4" text-anchor="middle" class="seal-char paper">{c.first}</text>
  {:else if look === "poster"}
    {#if c.variant === 0}
      <circle cx="82" cy="108" r="32" class="hi" />
      <path d="M-10 150 L50 90 L58 98 L-2 158Z M-10 170 L70 90 L78 98 L-2 178Z M10 170 L90 90 L98 98 L18 178Z" class="s2" />
    {:else if c.variant === 1}
      <circle cx="120" cy="160" r="84" class="hi" />
      <circle cx="120" cy="160" r="60" class="bg" />
      <circle cx="120" cy="160" r="38" class="s2" />
      <circle cx="22" cy="140" r="7" class="s2" />
    {:else if c.variant === 2}
      <path d="M0 160 V92 L68 160Z" class="hi" />
      <path d="M120 160 V78 L38 160Z" class="s2" opacity=".92" />
      <circle cx="60" cy="92" r="9" class="hi" />
    {:else}
      <path d="M12 160 a48 48 0 0 1 96 0Z" class="hi" />
      {#each [0, 1, 2, 3, 4, 5] as i (i)}
        {#each [0, 1] as j (j)}
          <circle cx={14 + i * 18.4} cy={82 + j * 14} r="3" class="s2" />
        {/each}
      {/each}
    {/if}
  {:else if c.scene === "mountain"}
    <circle cx="84" cy="84" r="13" class="hi" />
    <path d="M0 160 L0 126 L30 92 L52 120 L70 100 L120 146 L120 160Z" class="dk" />
    <path d="M0 160 L0 144 L40 112 L64 136 L88 118 L120 140 L120 160Z" class="dk2" />
    <path d="M30 92 L38 101 L31 100 L26 104 L22 100Z" class="lt" />
  {:else if c.scene === "night"}
    <circle cx="82" cy="86" r="15" class="hi" />
    <circle cx="76" cy="81" r="13" class="bg" />
    <g class="hi"><circle cx="24" cy="78" r="1.6" /><circle cx="44" cy="96" r="1.2" /><circle cx="102" cy="112" r="1.4" /><circle cx="18" cy="110" r="1.1" /><circle cx="58" cy="74" r="1.2" /></g>
    <path d="M0 160 L0 138 Q30 122 60 136 T120 130 L120 160Z" class="dk" />
    <path d="M20 160 L20 134 L28 126 L36 134 L36 160Z M27 142 h3 v5 h-3Z" class="dk2" />
  {:else if c.scene === "sea"}
    <circle cx="86" cy="96" r="12" class="hi" />
    <path d="M0 118 Q10 110 20 118 T40 118 T60 118 T80 118 T100 118 T120 118 L120 160 L0 160Z" class="dk" />
    <path d="M0 134 Q10 126 20 134 T40 134 T60 134 T80 134 T100 134 T120 134 L120 160 L0 160Z" class="dk2" />
    <path d="M26 114 L52 114 L46 122 L32 122Z M38 92 L38 113 L24 113Z M40 96 L40 113 L50 113Z" class="lt" />
  {:else if c.scene === "festival"}
    <path d="M0 70 Q60 86 120 70" class="line" />
    <g class="hi">
      <ellipse cx="28" cy="98" rx="11" ry="13" /><ellipse cx="62" cy="106" rx="12" ry="14" /><ellipse cx="96" cy="98" rx="11" ry="13" />
    </g>
    <g class="dk2"><rect x="23" y="83" width="10" height="3" /><rect x="57" y="90" width="10" height="3" /><rect x="91" y="83" width="10" height="3" />
      <rect x="27" y="111" width="2" height="12" /><rect x="61" y="120" width="2" height="13" /><rect x="95" y="111" width="2" height="12" /></g>
    <path d="M28 74 V83 M62 79 V90 M96 74 V83" class="line" />
  {:else if c.scene === "bamboo"}
    <g class="dk"><rect x="22" y="70" width="7" height="90" rx="2" /><rect x="46" y="88" width="6" height="72" rx="2" /><rect x="88" y="62" width="8" height="98" rx="2" /></g>
    <g class="bg"><rect x="22" y="96" width="7" height="2" /><rect x="22" y="124" width="7" height="2" /><rect x="46" y="112" width="6" height="2" /><rect x="88" y="90" width="8" height="2" /><rect x="88" y="122" width="8" height="2" /></g>
    <g class="hi"><path d="M29 84 q14 -8 22 2 q-12 2 -22 -2Z" /><path d="M96 78 q13 -9 21 0 q-11 3 -21 0Z" /><path d="M88 104 q-14 -6 -20 4 q11 1 20 -4Z" /><path d="M52 100 q10 -7 17 1 q-9 2 -17 -1Z" /></g>
  {:else if c.scene === "tea"}
    <path d="M38 112 h44 v10 q0 18 -22 18 q-22 0 -22 -18Z" class="hi" />
    <path d="M82 116 q14 0 12 10 q-2 8 -14 6" class="line thick" />
    <rect x="30" y="142" width="60" height="4" rx="2" class="dk" />
    <path d="M50 102 q-6 -8 0 -14 q6 -6 0 -14 M66 102 q-6 -8 0 -14 q6 -6 0 -14" class="line" />
  {:else if c.scene === "home"}
    <circle cx="94" cy="80" r="10" class="hi" />
    <path d="M0 160 L0 140 Q60 126 120 140 L120 160Z" class="dk" />
    <path d="M34 142 V112 L60 90 L86 112 V142Z" class="lt" />
    <path d="M28 114 L60 86 L92 114 L86 118 L60 96 L34 118Z" class="dk2" />
    <rect x="54" y="122" width="12" height="20" class="dk2" /><rect x="40" y="118" width="9" height="9" class="hi" /><rect x="71" y="118" width="9" height="9" class="hi" />
  {:else if c.scene === "city"}
    <circle cx="26" cy="82" r="11" class="hi" />
    <path d="M0 160 V120 h16 V98 h18 v30 h10 V84 h20 v40 h12 V104 h16 v22 h12 V92 h16 V160Z" class="dk" />
    <g class="hi"><rect x="20" y="104" width="3" height="4" /><rect x="27" y="104" width="3" height="4" /><rect x="49" y="92" width="3" height="4" /><rect x="57" y="92" width="3" height="4" /><rect x="49" y="104" width="3" height="4" /><rect x="81" y="112" width="3" height="4" /><rect x="109" y="100" width="3" height="4" /><rect x="109" y="112" width="3" height="4" /></g>
  {:else if c.scene === "rain"}
    <path d="M30 96 a14 14 0 0 1 26 -6 a11 11 0 0 1 20 4 a10 10 0 0 1 2 20 H32 a9 9 0 0 1 -2 -18Z" class="lt" />
    <g class="hi"><rect x="36" y="122" width="2.4" height="9" rx="1.2" /><rect x="50" y="128" width="2.4" height="9" rx="1.2" /><rect x="64" y="122" width="2.4" height="9" rx="1.2" /><rect x="78" y="128" width="2.4" height="9" rx="1.2" /><rect x="43" y="140" width="2.4" height="9" rx="1.2" /><rect x="71" y="140" width="2.4" height="9" rx="1.2" /></g>
  {:else if c.scene === "blossom"}
    <path d="M-4 150 Q40 132 62 108 T124 78" class="line thick dkl" />
    <path d="M62 108 Q70 90 92 86" class="line dkl" />
    {#each [[30, 134], [62, 106], [92, 86], [104, 100], [78, 118]] as [x, y], i (i)}
      <g transform="translate({x} {y})" class="hi">
        {#each [0, 72, 144, 216, 288] as r (r)}<ellipse cx="0" cy="-5" rx="3.2" ry="5" transform="rotate({r})" />{/each}
        <circle r="1.8" class="dk2" />
      </g>
    {/each}
  {:else if c.scene === "food"}
    <path d="M30 118 h60 q0 26 -30 26 q-30 0 -30 -26Z" class="hi" />
    <rect x="26" y="114" width="68" height="5" rx="2.5" class="lt" />
    <path d="M64 112 L96 76 M72 112 L104 82" class="line thick" />
    <path d="M44 106 q-5 -7 0 -12 q5 -5 0 -12 M56 106 q-5 -7 0 -12 q5 -5 0 -12" class="line" />
    <rect x="46" y="144" width="28" height="4" rx="2" class="dk" />
  {:else if c.scene === "school"}
    <path d="M22 108 q19 -10 38 0 v36 q-19 -10 -38 0Z" class="hi" />
    <path d="M98 108 q-19 -10 -38 0 v36 q19 -10 38 0Z" class="lt" />
    <path d="M30 116 q11 -5 22 -1 M30 124 q11 -5 22 -1 M30 132 q11 -5 22 -1" class="line dkl" />
    <path d="M90 78 l8 8 l-22 22 l-10 2 l2 -10Z" class="dk2" />
  {:else if c.scene === "road"}
    <circle cx="88" cy="82" r="12" class="hi" />
    <path d="M0 160 V118 Q40 100 76 112 T120 108 V160Z" class="dk" />
    <path d="M46 160 Q60 132 70 114 L78 114 Q76 134 84 160Z" class="lt" />
    <path d="M63 156 L65 146 M67 138 L69 130 M71 124 L72 118" class="line dkl" />
  {:else}
    <circle cx="60" cy="118" r="20" class="hi" />
    <path d="M42 106 L44 88 L56 100Z M78 106 L76 88 L64 100Z" class="hi" />
    <g class="dk2"><circle cx="52" cy="116" r="2.4" /><circle cx="68" cy="116" r="2.4" /><path d="M58 123 h4 l-2 3Z" /></g>
    <path d="M60 126 q-4 5 -8 2 M60 126 q4 5 8 2 M36 118 h10 M36 124 h10 M74 118 h10 M74 124 h10" class="line dkl" />
    <path d="M0 160 V146 Q60 136 120 146 V160Z" class="dk" />
  {/if}
  </g>
</svg>

<style>
  .art {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    background: linear-gradient(160deg, var(--a), var(--b));
    display: block;
  }
  .hi {
    fill: var(--hi);
  }
  .bg {
    fill: var(--a);
  }
  .dk {
    fill: rgb(0 0 0 / 0.28);
  }
  .dk2 {
    fill: rgb(0 0 0 / 0.42);
  }
  .lt {
    fill: rgb(255 255 255 / 0.3);
  }
  .line {
    fill: none;
    stroke: var(--hi);
    stroke-width: 1.6;
    stroke-linecap: round;
    opacity: 0.85;
  }
  .line.thick {
    stroke-width: 3;
  }
  .line.dkl {
    stroke: rgb(0 0 0 / 0.4);
  }
  .s2 {
    fill: var(--s2);
  }
  .rule {
    fill: none;
    stroke: var(--hi);
    stroke-width: 1.1;
  }
  .rule.thin {
    stroke-width: 0.5;
  }
  .thread {
    fill: none;
    stroke: var(--hi);
    stroke-width: 1.3;
  }
  .seal-char {
    font-size: 16px;
    font-weight: 600;
    font-family: "Songti SC", "Noto Serif CJK SC", "Source Han Serif SC", SimSun, serif;
  }
  .seal-char.gilt {
    fill: var(--hi);
  }
  .seal-char.paper {
    fill: #f7e9d7;
    font-size: 12.5px;
  }
</style>
