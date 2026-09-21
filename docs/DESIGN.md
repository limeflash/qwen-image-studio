# Qwen Image Studio — design spec

Designed by Fable 5.1. Visual reference: qwen.ai (real computed tokens), pushed further for a
tool that gets opened forty times a day instead of read once.

## 1. Design direction

**Feel:** the front panel of a piece of equipment — an amplifier, a modem — not a web page. A
panel has a fixed set of readouts and one or two controls, you read it in a glance, nothing on
it is decorative, and when something goes wrong exactly one light changes. qwen.ai puts one
input in a void because a marketing page has one thing to say. This app has eight things to say
forty times a day, so the black is earned differently: **every lit pixel is a reading or a
control.** Black is the panel. Light is information. Blue is the one thing to press next.

Three decisions shaped the layout:

**1. Blue is assigned, not placed.** qwen.ai uses `#0A28F0` on one CTA. Keep "exactly one" but
make it dynamic: at any moment the single blue element is the *next action the app needs from
you* — Download → Start → Copy the new tunnel URL → Retry → Restart with Q6_K. Priority when
several apply: setup action > error recovery > explicit intent (Restart with tier) > Start when
off > Copy when the tunnel URL is new and uncopied. When nothing is blue, the app needs nothing
and is quiet. This is what makes the tunnel chore bearable: the blue walks you through open →
Start → Copy and then goes away.

**2. Nothing moves; things light up.** Every element has a fixed slot. Conditional lines (the
notice under the URLs) reserve their height even when empty. State changes are colour and
opacity only. The top-left slot always says *what state am I in* (Install / Installing / Off /
Loading / Ready / Error) and the top-right slot always holds *the control for that state*
(Download / Pause / Start / Stop). Both screens share these slots so setup → main does not jump.

**3. Two bars, one language.** Setup progress and the VRAM gauge are the same object — a
segmented bar, 8px tall, full width, dark track, white fill, 2px black gaps between segments.
Six downloads become six proportional segments of one bar (a stalled one visibly stops filling;
a verified one turns full white). The VRAM gauge is "other / engine / free" on the same bar. The
download list below is text rows, not bars.

**Departures from qwen.ai, and why:**

- Left-aligned 32px grid instead of centred composition. A panel is read top-left to
  bottom-right; centring works for one element, not eight.
- Two hairline section dividers on the main screen. qwen.ai barely needs them; density needs
  structure.
- Added a monospace for URLs, an amber/red alarm pair, two secondary greys. No green: white
  means "alive" (ready dot, gauge fill, toggle-on). Green would compete with the only real
  colour on screen — the gallery images — and with blue.
- Kept verbatim: black, `#F7F8FC`, the hairline, blue as fills-only single CTA, pill buttons,
  12px cards, the raised `#2B2B2B` input card (quoted directly as the prompt box), tight
  tracking, the system stack.
- No in-app title bar or logo. The native Windows 11 title bar (dark) carries "Qwen Image
  Studio" and the current state, so the taskbar reads the state before you switch to the window.

## 2. Colour tokens

```css
:root {
  color-scheme: dark;

  /* Qwen, verbatim */
  --bg:            #000000;                 /* window. nothing else is this dark */
  --fg:            #F7F8FC;                 /* primary text; also "lit": ready dot, gauge fill, toggle-on */
  --hairline:      rgba(53, 53, 61, 0.6);   /* 1px dividers, outlined tier cells */
  --accent:        #0A28F0;                 /* the single next action. fills only — 2.6:1 on black, never text */
  --surface-2:     #2B2B2B;                 /* raised: prompt card, ghost buttons, selected tier cell */

  /* derived from Qwen values */
  --accent-hover:  #2340F5;
  --accent-active: #0820C0;
  --surface-1:     #151517;                 /* recessed: gauge and bar tracks, hover on outlined cells */
  --surface-3:     #363639;                 /* hover on raised; ghost button sitting on a raised card */
  --fg-2:          #9A9BA3;                 /* secondary: labels, sublines, legends, notices (7.5:1) */
  --fg-3:          #5C5D66;                 /* tertiary: off-ring, disabled, footnotes (3.2:1, never body copy) */
  --focus:         rgba(10, 40, 240, 0.45); /* 2px focus ring, offset 2px */

  /* added: the alarm pair — the only hues on screen besides blue */
  --warn:          #F0A830;                 /* VRAM warning, stalled download (10:1 on black) */
  --danger:        #F03B2E;                 /* Error status, VRAM critical, failed download, OOM tick (5.4:1) */

  /* state map */
  --status-off:      var(--fg-3);           /* hollow ring */
  --status-loading:  var(--fg);             /* solid, breathing */
  --status-ready:    var(--fg);             /* solid */
  --status-error:    var(--danger);         /* solid */
  --vram-other:      var(--fg-3);           /* Windows + other processes */
  --vram-loading:    var(--fg-2);           /* engine segment while model is read from disk */
  --vram-safe:       var(--fg);             /* free > 1.5 GB */
  --vram-warn:       var(--warn);           /* 0.5–1.5 GB free */
  --vram-critical:   var(--danger);         /* < 0.5 GB free, or OOM this session */
  --dl-active:       var(--fg-2);           /* bytes landed, not yet verified/unpacked */
  --dl-done:         var(--fg);             /* verified (models) or unpacked (zips) */
}
```

`--warn`/`--danger` exist because the app has real failure states qwen.ai does not (OOM, stalled
downloads, dead tunnel), and a panel needs "watch" and "broken" distinguishable at a glance. Both
are picked to match the hardness of Qwen's blue rather than pastel. The greys are opacity steps
of `--fg` over black, rendered as solids so hairline-adjacent text stays crisp. No other hue: no
green, no purple, no gradient anywhere.

Ghost-button rule: on `--bg` a ghost fills `--surface-2`; on a `--surface-2` card it fills
`--surface-3`.

## 3. Type scale

Font: `system-ui, "Segoe UI Variable", -apple-system, sans-serif`.
Mono: `"Cascadia Mono", Consolas, ui-monospace, monospace`.
`font-variant-numeric: tabular-nums` on everything that updates (timers, GB, percent).

| Token | Size / line | Weight | Tracking | Used for |
|---|---|---|---|---|
| heading | 24 / 32 | 600 | −0.32px | status word; setup heading. One per screen. |
| body | 14 / 20 | 400 | −0.28px | sublines, notice line, download row state, disabled placeholder |
| body-strong | 14 / 20 | 500 | −0.28px | 36px buttons, tier names, download row names |
| label | 13 / 16 | 500 | −0.20px | section labels, row labels (VRAM, Local, Tunnel), 28px buttons, links |
| small | 12 / 16 | 400 | −0.12px | gauge legend, tier descriptions, setup "Also:" line, footnotes |
| input | 16 / 24 | 400 | −0.32px | prompt box text and placeholder |
| mono | 13 / 20 | 400 | 0 | URLs, install path. Mono because 0/O and l/1 matter in a secret. |

Colour: heading and body-strong in `--fg`; body in `--fg-2` unless it carries the primary reading
(the "38%" token, the URL); label and small in `--fg-2`; footnotes in `--fg-3`.

## 4. Layout

Window: 900×640 content area, `resizable: false`, `maximizable: false`, native title bar, Tauri
theme Dark. Outer padding 32 all sides → content box x 32–868 (836 wide), y 32–608. Spacing base
4; rhythm 8 / 16 / 20 / 24 / 32. Sections separated by 20 + hairline + 20. All labels left-align
at x=32; all row content after a label column aligns at x=104 (72px label column). All right-slot
controls right-align at x=868.

### Screen 1A — Install (nothing on disk)

```
x=32                                                                      x=868
┌──────────────────────────────────────────────────────────────────────────────┐
│ Install                                                                      │ y 32–64   heading
│ Downloads 15.0 GB. Pick a model tier — the rest is required.                 │ y 66–86   body fg-2
│                                                                              │
│ ┌────────────────────────┐ ┌────────────────────────┐ ┌────────────────────┐ │ y 110–162 tier selector
│ │ Q8_0           7.69 GB │ │ Q6_K           6.00 GB │ │ Q4_K_M     4.20 GB │ │   3 cells, gap 8
│ │ Best quality. For 16 GB│ │ If large images run out│ │ Smallest. Visibly  │ │
│ └────────────────────────┘ └────────────────────────┘ └────────────────────┘ │
│ Also: engine 0.33 · CUDA runtime 0.56 · text encoder 4.68 · vision           │ y 178–194 small fg-2
│ projector 1.08 · VAE 0.63 GB                                                 │
│                                                                              │
│ ( Download 15.0 GB )   to G:\…\models · 212 GB free · Change                 │ y 218–254 primary pill
│                                                                              │
└──────────────────────────────────────────────────────────────────────────────┘
```

The button's size follows the tier: 15.0 / 13.3 / 11.5 GB. Q8_0 preselected. Disk check runs on
the install folder: if free < required, the path line turns `--danger` ("Not enough space on G: —
15.0 GB needed, 9.1 GB free") and Download is disabled. "Change" opens the native folder picker —
the only setting in the app, and cheaper than a support conversation about a full C:.

### Screen 1B — Installing

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ Installing                                                       ( Pause )   │ y 32–64  heading + ghost pill
│ 38% · about 24 min left · 5.7 of 15.0 GB · 52 MB/s                           │ y 66–86  "38%" fg, rest fg-2
│                                                                              │
│ ▕████████░░░░░░░░░░░░░▏▕█████░░░░░░░▏▕██░▏▕█▏▕▏▕█▏                            │ y 110–118 segmented bar
│                                                                              │
│ ●  Model Q8_0           3.2 / 7.69 GB                                        │ y 134–166 rows, 32h each
│ ●  Text encoder         1.9 / 4.68 GB                                        │
│ ●  Vision projector     0.4 / 1.08 GB   stalled · retrying in 12 s           │   warn dot
│ ●  VAE                  0.1 / 0.63 GB                                        │
│ ✓  Engine                     0.33 GB                                        │   done
│ ●  CUDA runtime               0.56 GB   unpacking                            │
│                                                                              │ y 326
│ Closing the window pauses the download. It picks up where it left off        │ y 350–366 small fg-3
│ next time.                                                                   │
└──────────────────────────────────────────────────────────────────────────────┘
```

Segment and row order are identical (Model, Text encoder, Vision projector, VAE, Engine, CUDA
runtime) so the bar and the list read as one thing; the model segment is ~half the bar, the engine
~19px — every file stays visible. Retries are automatic (no bytes for 30 s = stalled; 3 attempts
with backoff); only after that does a row become *failed* and grow a Retry button.

**What the window says when the user comes back:**

- **Finished:** they never see this screen again. The app has advanced to the main panel, engine
  Off, subline "Installed · Q8_0 selected · start to load it", title bar "Off — Qwen Image Studio".
  Nothing autostarts: the engine takes 14 GB of a gaming card, so it waits for the blue Start.
- **Half-finished, window left open:** exactly the live screen above.
- **Half-finished, window was closed:** downloads resume on launch without a click; screen 1B
  appears mid-progress. No "resumed" banner — the moving bar says it.
- **Paused:** subline "Paused at 38% · 9.3 GB left", right slot becomes blue "Resume", all active
  rows read "paused".
- **Failed:** subline "Stopped at 71% · 1 file failed" with the last token in `--danger`; the
  failed row reads its reason and carries a Retry (blue on the first failed row, ghost on any
  others — the blue walks down as you clear them). Title bar "Install stopped — Qwen Image
  Studio", so the taskbar tells them before they switch.
- **Corrupt or missing files later** (checked on every launch): screen 1B with only the affected
  rows. No separate "reinstall" flow.

### Screen 2 — Control panel

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ ● Ready                                                          ( Stop )    │ y 32–64  dot 10px @x32, word @x50
│   Q8_0 · loaded in 41 s                                                      │ y 66–86  subline @x50
│                                                                              │
│ VRAM   other 1.7 · engine 12.4 · free 1.9                       14.1 / 16 GB │ y 110–130 label@32 legend@104
│ ▕▒▒▒▒▒▏▕████████████████████████████████████████▏     ╷            ▏         │ y 134–142 gauge; ╷ = peak tick
│                                                                              │
│ ┌────────────────────────┐ ┌────────────────────────┐ ┌────────────────────┐ │ y 166–218 tier selector
│ │ ● Q8_0         7.69 GB │ │ Q6_K         ↓ 6.00 GB │ │ Q4_K_M   ↓ 4.20 GB │ │   dot=loaded; fill=selected
│ │ Best quality. For 16 GB│ │ If large images run out│ │ Smallest. Visibly  │ │
│ └────────────────────────┘ └────────────────────────┘ └────────────────────┘ │
│ ──────────────────────────────────────────────────────────────────────────── │ y 238  hairline
│ Connect                                                                      │ y 258–274 label
│ Local    http://localhost:8765/a1b2…7b8f/mcp                      ( Copy )   │ y 282–318 36h row
│ Tunnel   https://calm-otter-bright-sky.trycloudflare.com/…/mcp ( Copy )  [●] │ y 322–358 toggle @x836
│          New since last launch. claude.ai still has the old one — copy this  │ y 362–382 notice @x104
│ ──────────────────────────────────────────────────────────────────────────── │ y 402  hairline
│ ┌ Try a prompt                                                 ( Generate ) ┐│ y 422–462 raised card r12
│ Recent                                                          Open folder  │ y 478–494 label + link
│ ▣▣▣▣▣▣▣▣  8 × 96px, gap 8, newest left                                       │ y 502–598 strip
└──────────────────────────────────────────────────────────────────────────────┘ y 608
```

Why this order: the panel is read top to bottom in the order of a launch. State (is it on?),
health (will it run out?), configuration (which model?), the reason you opened the window (the
URLs), and then what came out of it. The gallery is a tray along the bottom edge because images
arrive from elsewhere; the prompt card sits directly above it because what you type there lands in
the strip below. The URL rows are full width because the tunnel string is up to 80 mono characters
and must never be end-ellipsized (the `/mcp` matters).

URL typography inside a row: scheme in `--fg-3`, hostname in `--fg`, the secret abbreviated to
first 4 + last 4 characters in `--fg-3`, `/mcp` in `--fg-2`. The random words are the part that
changes, so they are the brightest. The clipboard always gets the full string.

Assumptions stated once: the local secret is generated at install and persists (otherwise the
local row would have the same chore); the MCP server is up whenever the app is open, independent
of the engine; cloudflared forwards the original `Host` header, so the Rust server can tell a
tunnel request from a local one and the app can *confirm* that claude.ai connected through the new
URL.

## 5. Component specs

### 5.1 Engine status indicator

10px circle at x=32, vertically centred on the heading; word in `heading` at x=50; subline in
`body` at x=50; `aria-live="polite"` on the subline. Title bar mirrors the word.

| State | Dot | Word | Subline |
|---|---|---|---|
| Off | ring, 1.5px `--fg-3`, no fill | Off | `Q8_0 selected · loads in about 40 s` (last measured; first time "about a minute") |
| Loading | solid `--fg`, breathing | Loading | `Reading Q8_0 from disk · 23 s` (counts up) |
| Ready | solid `--fg` | Ready | `Q8_0 · loaded in 41 s` |
| Ready, generating | solid `--fg` | Ready | `Generating 1024×1024 · 8 s` |
| Ready, last image OOM'd | solid `--fg` | Ready | `Out of memory on the last image (1536×1536). Try a smaller size or Q6_K.` — "Q6_K" is a link that selects the tier |
| Ready, tier mismatch | solid `--fg` | Ready | `Running Q8_0 · Q6_K selected — restart to switch` |
| Stopping (≤2 s) | ring `--fg-3` | Off | `Stopping…` |
| Error | solid `--danger` | Error | reason + fix, see §7; `Show log` opens the log in Notepad |

No spinner: the counting timer and the VRAM gauge filling are real progress; a spinner is fake
progress.

### 5.2 Start / Stop control

Pill, height 36, padding 0 20, `body-strong`, right slot of the heading row. Never scales or moves.

| Engine state | Label | Style |
|---|---|---|
| Off | Start | primary |
| Loading | Stop | ghost |
| Ready | Stop | ghost |
| Ready, tier mismatch | Restart with Q6_K | primary (explicit intent outranks everything but setup) |
| Error | Start | primary |
| Stopping | Stopping… | ghost, disabled |

Primary: fill `--accent`, text `#FFF`; hover `--accent-hover`; active `--accent-active`; disabled
fill `--surface-2` text `--fg-3`. Ghost: fill `--surface-2`, text `--fg`; hover `--surface-3`;
active `--surface-1`; disabled text `--fg-3`. Focus-visible: 2px `--focus` ring, offset 2. The same
two styles at height 28 / padding 0 12 / `label` are the small buttons (Copy, Retry, Generate,
Pause).

### 5.3 VRAM gauge

Track 836×8, radius 4, `--surface-1`. Segments left to right with 2px black gaps: **other**
(`--vram-other`), **engine** (state colour), remaining track = free. 1 GB = 52px. Polled every 1 s.
Legend row above (label `VRAM` at x=32, legend at x=104 in `small`, readout `14.1 / 16 GB`
right-aligned in `body` `--fg`). Peak tick: 1×12px, centred vertically on the bar, extends 2px past
top and bottom, `--fg-2`, at the session's highest reading; resets on engine start.

| State | Engine segment | Legend tail | Readout |
|---|---|---|---|
| Engine off | absent | `other 1.7 · free 14.3` | `1.7 / 16 GB`, tick hidden |
| Loading | `--vram-loading`, growing live | `engine 6.2 · free 8.1` | live |
| Ready, safe (free > 1.5) | `--vram-safe` | `free 1.9` | `--fg` |
| Warning (0.5–1.5) | `--vram-warn` | `1.1 GB free — tight for large images` in `--warn` | `--warn` |
| Critical (< 0.5) | `--vram-critical` | `0.3 GB free — generation will fail` in `--danger` | `--danger` |
| OOM occurred | as measured | as measured | tick pinned at 16 GB in `--danger` until next start |

The whole engine segment recolours, not the sliver of free space, because a red sliver 15px wide is
invisible and running out is the recurring failure. During a generation the fill visibly rises and
may cross into amber and back — that is the tachometer behaviour. Thresholds are a first guess;
expose them as three constants and tune against the actual card.

### 5.4 URL-copy row

Height 36. Label (`label`, `--fg-2`) at x=32; URL (`mono`) at x=104, `cursor: copy`, clicking the
text copies too; small Copy button right-aligned (tunnel row: Copy sits 12px left of the toggle).
One notice line under the two rows, height always reserved.

| State | URL area | Button | Notice line |
|---|---|---|---|
| Rest | URL, hierarchy as in §4 | Copy, ghost | — |
| Next action | same | Copy, **primary** | `New since last launch. claude.ai still has the old one — copy this into Settings → Connectors.` |
| URL hover | `--surface-1` behind the text, padding 0 4, radius 6 | — | — |
| Copied (1500 ms) | — | label `Copied` with check glyph, ghost regardless of prior style | `Copied. Waiting for claude.ai to connect through it…` |
| Connected | — | ghost | `claude.ai connected through the tunnel at 09:14.` |
| Tunnel off | `Off` in `--fg-3` | hidden | `Turn on the tunnel to reach this PC from your laptop or phone.` |
| Tunnel connecting (2–5 s) | `Connecting to Cloudflare…` `--fg-2` | hidden | — |
| Tunnel failed | 8px `--danger` dot + `Couldn't reach Cloudflare` | Retry (primary by the blue rule) | `Check the internet connection, then retry.` |

Notice priority: failed > new > copied-waiting > connected > off. The "new" state is tracked per
URL: cleared by a copy, and the "connected" state is cleared by the next app restart. No "new"
badge exists — the blue Copy button *is* the badge; a chip next to it would say the same thing
twice.

**Tunnel toggle:** 32×18 pill, knob 14. Off: track `--surface-2`, knob `--fg-2`. On: track `--fg`,
knob `#000`. Connecting: on-position, opacity .6, pointer-events none. Persisted across launches,
and the tunnel reconnects on launch if it was on. Engine restarts (tier switch) never restart the
tunnel, so the URL only changes on app restart.

### 5.5 Download row

Height 32. 8px glyph centred in a 16px box at x=32; name (`body-strong`) at x=56; progress
`3.2 / 7.69 GB` (`body`, tabular, `--fg-2`) right-aligned in a column ending at x=352; state text
(`body`, `--fg-2`) at x=368; Retry small pill right-aligned, only when failed.

| State | Glyph | Progress column | State text |
|---|---|---|---|
| connecting (before first byte) | ring `--fg-3` | `— / 7.69 GB` | `connecting` |
| downloading | solid `--fg` | `3.2 / 7.69 GB` | — |
| stalled (auto-retrying) | solid `--warn` | frozen value | `stalled · retrying in 12 s` |
| verifying (checksum) | solid `--fg` | `7.69 GB` | `verifying` |
| unpacking (zips) | solid `--fg` | `0.56 GB` | `unpacking` |
| done | check glyph `--fg` | `7.69 GB` | — |
| paused | ring `--fg-3` | frozen | `paused` |
| failed | solid `--danger` | frozen | `failed · checksum didn't match` / `failed · connection lost` / `failed · server error 503` / `failed · stalled 3 times` / `failed · not enough disk space` |

Bar segment colours follow the row: `--dl-active` while bytes land, `--dl-done` after
verify/unpack, `--warn` frozen while stalled, `--danger` frozen when failed. Checksum failure
deletes the partial and restarts that file from zero on Retry; everything else resumes by byte
range.

### 5.6 Model-tier selector

Three cells in a grid (`repeat(3, 1fr)`, gap 8), height 52, radius 12, padding 10 14. Line 1: name
(`body-strong`) left, size (`label`, tabular, `--fg-2`) right. Line 2: description (`small`,
`--fg-2`). Implemented as visually-hidden radio inputs so arrow keys and focus rings come free.

| State | Cell |
|---|---|
| rest, on disk | transparent, 1px `--hairline` |
| hover | fill `--surface-1` |
| selected | fill `--surface-2`, border transparent |
| loaded in engine | 8px solid `--fg` dot before the name (independent of selection: dot = loaded, fill = chosen) |
| not on disk | size prefixed with a 10px down-arrow glyph: `↓ 6.00 GB` |
| downloading | background is a left-to-right `--surface-1` progress fill (`linear-gradient` with a `--p` stop), line 2 becomes `2.1 / 6.00 GB · 41 MB/s`; selection stays on the previous tier until done |
| disabled (setup download running, engine loading) | opacity .5, no pointer |
| focus-visible | 2px `--focus` ring |

Selecting a different on-disk tier while the engine runs does not open a dialog: it flips the Stop
button to a blue "Restart with Q6_K" and the subline to "Running Q8_0 · Q6_K selected — restart to
switch". Clicking back on Q8_0 undoes it. Selection is intent; the panel shows the mismatch. When
the engine is off, Start simply loads the selected tier.

### 5.7 Small parts

- **Hairline:** 1px `--hairline`, full content width, 20px clearance above and below.
- **Section label:** `label`, `--fg-2`, sentence case (no uppercase tracking — that is decoration).
- **Prompt card:** 836×40, radius 12, `--surface-2`, text `input` with 16px left padding,
  placeholder `--fg-2`; Generate as a small ghost (`--surface-3`) pill inside at right with 6px
  margin; Enter submits. Disabled when engine not Ready: placeholder "Start the engine to try a
  prompt", button hidden. While generating: button label `Generating…`, disabled. Errors surface in
  the status subline, which is the engine's voice.
- **Gallery thumbnail:** 96×96, radius 8, `object-fit: cover`, `title="{prompt} · {W×H} · {HH:MM}"`
  (native tooltip), hover `outline: 1px solid var(--fg-3)`, click opens the file in the OS viewer.
  Shows the latest 8; "Open folder" covers the rest. In-progress placeholder: `--surface-1` square
  with a centred 10px breathing dot, replaced by the image on arrival. Empty: no outlines drawn,
  the label reads `Recent — nothing yet`.
- **Icons:** two inline SVGs, both `viewBox="0 0 16 16"`, `stroke="currentColor"`,
  `stroke-width="1.5"`, round caps and joins, no fill.
  Check: `<path d="M3.5 8.5l3 3 6-7"/>`. Down arrow: `<path d="M8 3v10M4 9l4 4 4-4"/>`.
  Dots and the toggle are CSS. No copy icon, no external-link icon: words are enough.

## 6. Motion

| What | Property | Duration | Easing |
|---|---|---|---|
| Status dot while Loading; gallery placeholder dot | opacity 1 → .35 → 1 | 1600 ms loop | ease-in-out |
| VRAM fill, download segments | width, background-color | 400 ms | linear |
| Toggle knob | transform (16px travel) | 120 ms | ease-out |
| Button / cell hover | background-color | 80 ms | linear |
| New thumbnail | opacity 0 → 1 | 200 ms | ease-out |

Gauge and bar transitions are linear because they track a value sampled every second; easing would
fake acceleration. Breathing is the only loop and it is the LED convention for "working"; it stops
the instant the state changes.

**Never animates:** position or size of anything (no slide-ins, no `height: auto` reveals —
conditional lines reserve their space); the heading word (instant swap); screen 1 → screen 2
(instant, and the heading slot does not move); button label swaps; the blue moving from one button
to another (it is a state, not a journey — tweening it would look like a bug); gallery reorder
(existing thumbs jump right, only the new one fades). Reason: a fixed panel that shifts under the
cursor feels unstable, and an animation that charms once is a tax by the tenth open of the day.
`prefers-reduced-motion`: breathing becomes a static 60% dot; everything else is already ≤ 400 ms
and non-positional.

## 7. Microcopy

**Title bar:** `Install — Qwen Image Studio` / `Installing 38% — …` / `Install stopped — …` /
`Off — …` / `Loading — …` / `Ready — …` / `Error — …`

**Install (1A):** heading `Install` · intro `Downloads 15.0 GB. Pick a model tier — the rest is
required.` · tiers `Q8_0 · 7.69 GB / Best quality. For 16 GB.` `Q6_K · 6.00 GB / If large images
run out of memory.` `Q4_K_M · 4.20 GB / Smallest. Visibly lower quality.` · `Also: engine 0.33 ·
CUDA runtime 0.56 · text encoder 4.68 · vision projector 1.08 · VAE 0.63 GB` · button
`Download 15.0 GB` · path `to G:\qwen-image\models · 212 GB free · Change` · no space
`Not enough space on G: — 15.0 GB needed, 9.1 GB free.`

**Installing (1B):** heading `Installing` · `38% · about 24 min left · 5.7 of 15.0 GB · 52 MB/s` ·
under a minute `98% · less than a minute left` · partially failed `71% · about 9 min left · 1 file
failed` · all stopped `Stopped at 71% · 1 file failed` · paused `Paused at 38% · 9.3 GB left` ·
buttons `Pause` `Resume` `Retry` · footnote `Closing the window pauses the download. It picks up
where it left off next time.`

**Status:** words `Off` `Loading` `Ready` `Error`. Error sublines:

- `Crashed: out of memory. Close other GPU apps or switch to Q6_K.`
- `Port 8765 is in use by another program.`
- `Engine exited unexpectedly (code 1). Show log`
- `Couldn't start: CUDA runtime not found. Show log`

**Buttons:** `Start` `Stop` `Stopping…` `Restart with Q6_K` `Copy` `Copied` `Generate`
`Generating…` `Retry` `Open folder` `Change` `Show log`

**VRAM:** `VRAM` · `other 1.7 · engine 12.4 · free 1.9` · `1.1 GB free — tight for large images` ·
`0.3 GB free — generation will fail` · readout `14.1 / 16 GB`

**Connect:** labels `Connect` `Local` `Tunnel` · tunnel off `Turn on the tunnel to reach this PC
from your laptop or phone.` · connecting `Connecting to Cloudflare…` · failed `Couldn't reach
Cloudflare` + `Check the internet connection, then retry.`

**The moment the URL changed:**

> `New since last launch. claude.ai still has the old one — copy this into Settings → Connectors.`

Three flat clauses: the fact, the consequence, the action. No "again", no apology, no exclamation
mark — the situation is annoying, the app should not be. The line is the only place that says it
(no toast, no modal, no badge), the Copy button beside it is blue, and it changes the instant you
act so it never nags once you have done your part:

> `Copied. Waiting for claude.ai to connect through it…`
> `claude.ai connected through the tunnel at 09:14.`

That last line is the real answer to the chore: the app verifies the paste worked by seeing
claude.ai arrive on the new hostname, instead of just complaining that the hostname changed.

**Prompt / gallery:** placeholder `Try a prompt` · disabled `Start the engine to try a prompt` ·
`Recent` · empty `Recent — nothing yet` · generation failure in subline `Generation failed: out of
memory (1536×1536). Try a smaller size or Q6_K.`

## 8. Cuts

- No in-app title bar or logo — the native title bar carries name and state.
- No toasts — confirmation happens in the button you are looking at.
- No confirm dialog for tier switch — selection is intent, the Stop button becomes the restart.
- No spinner — the timer and the gauge filling are the progress.
- No green — white means alive.
- No per-file progress bars — one segmented bar and text rows.
- No "new" badge — the blue Copy button is the badge.
- No QR code or device sync for the phone — Windows cloud clipboard already moves the clipboard.
- No lightbox — the OS image viewer is better, "Open folder" covers the archive.
- No empty-state art — one line of text.
- No settings screen — the install folder link and the persisted tunnel toggle are the only settings.
- No log viewer — "Show log" opens the file in Notepad.
- No tray icon — but if added later, the tunnel process survives window close and the URL changes
  far less often; the UI needs no change for it. The full fix is a named Cloudflare tunnel with a
  stable hostname, out of scope here.
