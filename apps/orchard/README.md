# Orchard

**ActivityWatch, read the way Fruit reads a day.**

ActivityWatch records what was in front of you, faithfully and in enormous
detail, and then shows you a pie chart. Orchard is a desktop app that reads the
same data and answers the questions Fruit's Activity and Reports screens
answer: *when did this happen, what was it, how much of it can I account for,
and what is this total not telling me?*

It is read-only. It never writes to ActivityWatch, never leaves the machine, and
the renderer that draws your window titles has no network access at all.

```
apps/orchard/
  electron/main.js     one window, four IPC handlers, the security posture
  electron/aw.js       the ActivityWatch HTTP client — main process only
  electron/range.js    which buckets a range needs, and over what window
  electron/store.js    settings on disk, atomically, corruption-tolerant
  electron/preload.js  the bridge: four functions, no passthrough
  src/engine.js        all the arithmetic — no DOM, no fetch
  src/render.js        all the drawing — no arithmetic
  src/app.js           one read of state, every panel built from it
  src/tokens.css       Fruit's design system
  test/                37 engine tests, 13 main-process tests, 27 UI checks
```

## Running it

Orchard has its own `package.json` and its own dependencies. It is **not** part
of the Fruit build at the repository root, so `npm start` in the root directory
will not find it. Either stand in this directory:

```bash
cd apps/orchard
npm install
npm start
```

or use the passthroughs from the repository root, which work from anywhere:

```bash
npm run orchard:install
npm run orchard
npm run orchard:test
```

ActivityWatch must be running; it listens on `http://localhost:5600`. If it is
not, Orchard opens and says so — it never shows an empty chart in place of an
absent server.

`npm start` holds the terminal until you close the window. That is not a hang —
the app prints `Orchard is running. Close the window to quit.` when it starts.

### If your npm holds install scripts for approval

The `electron` package is a wrapper whose postinstall downloads the actual
binary for your platform, so npm's `allowScripts` gate applies to it. It is the
only package here with an install script.

If it was genuinely blocked, `npm start` fails with `Electron failed to install
correctly`. Any other message — including `electron.exe exited with signal
SIGINT` — means the binary is present and ran.

To approve it, run the command **inside this directory**. `npm approve-scripts`
resolves the package against the project you are standing in, so running it at
the repository root fails with `ENOMATCH`: the root project has no `electron`.

```bash
cd apps/orchard
npm approve-scripts electron
```

```bash
npm test          # engine + main process + the built page in real Chromium
```

## What it shows

Four tabs: **Day**, **Week**, **Month**, **Settings**.

**The day, on a real time axis.** Every unbroken stretch as a block at its
actual position and length, coloured by what kind of time it was. Not a bar
chart sorted by size — that is easier to read and throws away the only thing
this panel is for, which is *when*.

**What has no label yet, ranked by time.** First on the tab, because it is the
only panel that asks something of you. An empty taxonomy and a text field is a
chore; "these three things took eleven hours between them" is a question people
answer. One click labels it, and the rule applies to every day, backwards and
forwards.

**By kind of time, by application, and by hour of day.** The hour histogram
splits a stretch across the hours it actually spent time in, so it sums to the
total beside it.

**Findings** — the range read back in sentences, each naming the figure it came
from. They state facts and stop; this app does not tell you what to do about
your own week, because it does not know what your week was for.

## How a number here is arrived at

Five steps, each of which only ever narrows. Nothing after the first can invent
time the watcher did not record.

| Step | What it does | Where it comes from |
|---|---|---|
| `flood` | Closes sampler gaps up to 5s, merges equal neighbours, splits an uncertain gap at the midpoint | ported from `aw_transform/flood.py` |
| afk filter | Intersects with the not-afk periods, so a window left open while you made coffee is not two hours of work | ActivityWatch's own canonical query |
| domains | A browser window becomes the site it was showing, split where it showed several | Orchard |
| min-span floor | Drops stretches under two minutes, then re-joins what they interrupted | ported from Fruit's `apply_min_span` |
| categorise | A rule turns an app or a site into a kind of time | Fruit's `activity_rule` |

Two consequences worth knowing:

**The total is smaller than what the watcher recorded, and the app says so.**
`rawSec` is what was captured; `observedSec` is what survived. When the gap is
more than ten minutes, a finding names it: *"2h 00m of recorded time is not in
these totals."* A single number cannot say that, which is why both are carried.

**A parent bucket is never double-counted, and neither is a browser.** Only
spans whose app is a known browser can be relabelled by a browser bucket —
without that guard, a stale browser event overlapping your editor would rename
the editor, which is the bug that makes app totals stop being believable.

## Labels

Five kinds of time, carried over from Fruit unchanged: **Work**, **Study**,
**Distraction**, **Life**, **Other**. Each rolls up to `core`,
`entertainment` or `other`, so adding a category can never move a figure that
was already on screen.

Rules match an application or a domain **exactly**, first match wins, and a
domain rule covers its subdomains — one rule for `github.com` catches
`gist.github.com`. Exact matching is a deliberate limit: a regex table is more
powerful and is also the thing nobody maintains, and the one-click labelling
panel only works if a rule is a fact about one app rather than a pattern you
have to debug.

About thirty rules ship, covering the cases where the guess is safe. Not more:
a hundred shipped rules look thorough and quietly mislabel a third of somebody's
week.

## What it does with your data

This app renders window *titles* — the documents, tickets and videos you had
open, which is among the most sensitive data a desktop produces. The
architecture is built around that:

- **The renderer has no network access.** A `connect-src 'none'` CSP, set both
  as a header by the main process and as a meta tag in the page. Every request
  is made in the main process.
- **The renderer has no Node.** `contextIsolation`, `sandbox`, no
  `nodeIntegration`. Verified: `window.require` and `window.process` are
  `undefined` in the running app.
- **The bridge is four functions**, not an `invoke(channel, …)` passthrough that
  widens every time a handler is added.
- **Only a local ActivityWatch is accepted.** `aw-server` has no
  authentication, so a remote address would be reading another machine's history
  over an open connection. The address is validated, not trusted.
- **Excluded apps are dropped before anything is totalled**, so an app you
  named cannot resurface through a chart, a report or a total.
- **Everything reaches the DOM through `textContent`.** A window title is
  arbitrary text from someone else's software; `innerHTML` would make it script.

## Settings

- **Server address** — local only.
- **Hostname** — which machine's buckets to read, when the server holds more
  than one.
- **Shortest stretch to report** — two minutes by default. Set it to 0 to see
  everything the watcher recorded.
- **Applications to leave out entirely.**
- **Rules** — the full table, editable.

Stored as one JSON file in the OS application-data directory. A corrupt file is
renamed aside and the defaults used, so a bad write can never leave you with an
app that will not open.

## Testing

```bash
node --test test/engine.test.js   # 37 — the arithmetic
node --test test/main.test.js     # 13 — HTTP, address validation, disk
node test/check-ui.mjs            # 27 — the page in real Chromium
```

`engine.test.js` holds the ported transforms to the behaviour
`aw_transform/flood.py` documents in its own comments — the midpoint split, the
equal-data merge, the later-event precedence — rather than to a recording of
this code's output. A port that agrees with itself proves nothing.

`main.test.js` runs a stand-in `aw-server` on a loopback port answering the real
paths with the real shapes, and checks the things that only happen outside the
renderer: that a non-local address is refused, that the query window is widened
by a day at each end so a midnight-crossing event is not lost, that one dead
browser bucket does not take the day down with it, and that a corrupt settings
file is survivable.

`check-ui.mjs` loads the page over `file://` — the way Electron's `loadFile`
serves it, so a CSP that works over http and blocks the app's own scripts on
disk fails here rather than in a packaged build. It checks that a window title
containing `<img onerror=…>` is displayed rather than executed, that no
component outside `tokens.css` hard-codes a colour, that text on every timeline
block clears 4.5:1 in the active theme, and that an unreachable server produces
a sentence rather than an empty chart.

## What it is not

No tracking of its own — ActivityWatch does that, and a second sampler would be
a second set of numbers to reconcile. No sync, no accounts, no telemetry, no
network calls beyond `localhost:5600`. It does not write to ActivityWatch, so
nothing it does can corrupt the record it reads.
