/**
 * The UI half of §8 that a headless browser can actually check.
 *
 *   I1  every colour resolves to a §5.2 token; no literal hex in components
 *   I4  every changing numeral uses tabular figures
 *   I5  the layout holds at 960×640, at each §5.8 breakpoint, and at 125%
 *       OS text scaling, with no clipping
 *   I6  prefers-reduced-motion disables the rail settle and all transitions
 *   I7  fonts load from the bundle with no network request
 *   U10 keyboard focus is visible on every interactive element
 *
 *   node scripts/check-ui.mjs            # against `npm run preview`
 *
 * The rest of §8 lives in `cargo test` (F/D/U criteria) or needs a human
 * (contrast over the Focus gradients, tray legibility) — see docs/ACCEPTANCE.md.
 */

import { chromium } from "playwright";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

const URL = process.env.FRUIT_URL ?? "http://localhost:4173/";
const EXECUTABLE = process.env.CHROMIUM_PATH ?? undefined;

let failures = 0;
const check = (name, ok, detail = "") => {
  console.log(`${ok ? "  ok  " : "FAIL  "}${name}${detail ? ` — ${detail}` : ""}`);
  if (!ok) failures++;
};

/* ─── I1: no literal colours in components ─────────────────────────────── */

function walk(dir) {
  return readdirSync(dir).flatMap((entry) => {
    const path = join(dir, entry);
    return statSync(path).isDirectory() ? walk(path) : [path];
  });
}

const componentFiles = walk("src").filter(
  (f) => /\.(tsx|ts)$/.test(f) && !f.includes("/dev/") && !f.endsWith("types.ts"),
);
const literalColours = [];
for (const file of componentFiles) {
  const text = readFileSync(file, "utf8");
  text.split("\n").forEach((line, i) => {
    // `#RGB`, `#RRGGBB`, `rgb(...)`, `hsl(...)` — anything that isn't a token.
    const hit = line.match(/#[0-9a-fA-F]{3,8}\b|rgba?\(|hsla?\(/);
    if (hit && !line.includes("var(--")) {
      literalColours.push(`${file}:${i + 1} ${hit[0]}`);
    }
  });
}
check("I1 no literal colours in components", literalColours.length === 0, literalColours.join(", "));

/* ─── browser checks ───────────────────────────────────────────────────── */

const browser = await chromium.launch(EXECUTABLE ? { executablePath: EXECUTABLE } : {});

// I7: an offline-first app must make no network request at all.
{
  const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  const page = await ctx.newPage();
  const external = [];
  page.on("request", (r) => {
    if (!r.url().startsWith(URL) && !r.url().startsWith("data:")) external.push(r.url());
  });
  await page.goto(URL, { waitUntil: "networkidle" });
  await page.waitForTimeout(500);
  check("I7 no external requests (fonts included)", external.length === 0, external.join(", "));

  // These three are per-view: a control that only exists in Settings — the
  // Activity switches, say — is never seen by a check that stays on the Planner.
  const nonTabular = [];
  const focusRing = [];
  let unlabelled = 0;
  /* Navigate with `G then <key>`, never a click: `:focus-visible` stops
     matching programmatic focus once the page has seen a mouse interaction, so
     clicking through the views would make U10 fail on every element for a
     reason that has nothing to do with the CSS. */
  for (const [view, key] of [
    ["Day", "d"],
    ["Planner", "p"],
    ["Projects", "t"],
    ["Activity", "a"],
    ["Reports", "r"],
    ["Settings", "s"],
  ]) {
    await page.keyboard.press("g");
    await page.keyboard.press(key);
    await page.waitForTimeout(250);
    if (!(await page.locator(`button[aria-label^="${view}"][aria-current="page"]`).count())) {
      check(`U10 could reach ${view} by keyboard`, false);
      continue;
    }

    // I4: every numeral that changes in place is tabular.
    nonTabular.push(
      ...(await page.evaluate(() => {
        const bad = [];
        for (const el of document.querySelectorAll(".data, .micro, .focus-clock")) {
          const v = getComputedStyle(el).fontVariantNumeric;
          if (!v.includes("tabular-nums")) bad.push(el.className);
        }
        return bad;
      })),
    );

    // U10: a visible focus ring on every interactive element, never hover only.
    focusRing.push(
      ...(await page.evaluate(() => {
        const targets = [
          ...document.querySelectorAll("button, input, select, textarea, [tabindex]"),
          // A disabled control cannot take focus at all, so a focus ring is not
          // the thing to assert about it. Prefer an enabled control that
          // explains itself; where one is genuinely disabled, skip it here.
        ].filter((el) => el.offsetParent !== null && !el.disabled);
        const missing = [];
        for (const el of targets) {
          el.focus();
          const s = getComputedStyle(el);
          const ring =
            (s.outlineStyle !== "none" && parseFloat(s.outlineWidth) >= 2) ||
            s.boxShadow !== "none";
          if (!ring) missing.push(el.getAttribute("aria-label") || el.className || el.tagName);
        }
        return missing;
      })),
    );

    // I3: no state distinguishable by colour alone — every drift rail and every
    // Activity bar carries an accessible name saying what the colour says.
    unlabelled += await page.evaluate(
      () =>
        [...document.querySelectorAll(".rail-compact, .rail-bar, .app-bar")].filter(
          (el) =>
            !el.getAttribute("aria-label") &&
            !el.getAttribute("title") &&
            el.getAttribute("aria-hidden") !== "true",
        ).length,
    );
  }
  check("I4 changing numerals use tabular figures", nonTabular.length === 0, nonTabular.join(", "));
  check("U10 focus is visible on every interactive element", focusRing.length === 0, focusRing.join(", "));
  check("I3 drift encodings carry a text alternative", unlabelled === 0, `${unlabelled} unlabelled`);

  await ctx.close();
}

// I5: the layout holds at each breakpoint, and at 125% text scaling.
//
// Every view, not just the one that happens to open first: Activity's timeline
// and Settings' widest hint lines are the two most likely places for a stray
// pixel of horizontal scroll, and neither is the default screen.
const VIEWS = ["Day", "Planner", "Projects", "Activity", "Reports", "Settings"];

// The widths are the canonical breakpoint map's edges, plus one either side of
// the 1200px tier: the detail panel and the backlog appear exactly there, so
// 1200 is the first width at which the widest layout has to fit and 1199 is the
// last at which it must not be attempted.
for (const [width, height, scale] of [
  [960, 640, 1],
  [1130, 720, 1],
  [1200, 800, 1],
  [1280, 800, 1],
  [1490, 900, 1],
  [1440, 900, 1.25],
]) {
  const ctx = await browser.newContext({ viewport: { width, height } });
  const page = await ctx.newPage();
  await page.goto(URL, { waitUntil: "networkidle" });
  if (scale !== 1) {
    await page.addStyleTag({ content: `html { font-size: ${16 * scale}px }` });
  }
  await page.waitForTimeout(400);
  const worst = { doc: 0, body: 0, view: "Planner" };
  const measure = async (view) => {
    await page.waitForTimeout(250);
    const overflow = await page.evaluate(() => ({
      doc: document.documentElement.scrollWidth - document.documentElement.clientWidth,
      body: document.body.scrollWidth - document.body.clientWidth,
    }));
    if (overflow.doc > worst.doc || overflow.body > worst.body) {
      Object.assign(worst, overflow, { view });
    }
  };
  for (const view of VIEWS) {
    await page.click(`button[aria-label^="${view}"]`);
    await measure(view);
  }
  // Import and Export are reached from Reports rather than the rail, and both
  // are wide tables — the exact shape that overflows first.
  for (const label of ["Import a workbook", "Export month to Excel"]) {
    await page.click(`button[aria-label^="Reports"]`);
    await page.waitForTimeout(200);
    await page.click(`button:text-is("${label}")`);
    if (label === "Import a workbook") {
      // Land on the mapping and variance tables rather than the empty state:
      // the empty state has never overflowed anything, and the tables are the
      // reason this view is in the list.
      await page.fill("#import-path", "fixture.xlsx");
      await page.click('button:text-is("Read it")');
      await page.waitForTimeout(500);
    }
    await measure(label);
  }
  check(
    `I5 no horizontal overflow at ${width}×${height}${scale === 1 ? "" : " @125% text"}`,
    worst.doc <= 0 && worst.body <= 0,
    JSON.stringify(worst),
  );
  await ctx.close();
}

// I6: prefers-reduced-motion removes the settle and every transition.
{
  const ctx = await browser.newContext({
    viewport: { width: 1440, height: 900 },
    reducedMotion: "reduce",
  });
  const page = await ctx.newPage();
  await page.goto(URL, { waitUntil: "networkidle" });
  await page.waitForTimeout(300);
  const moving = await page.evaluate(() => {
    const bad = [];
    for (const el of document.querySelectorAll("*")) {
      const s = getComputedStyle(el);
      const dur = parseFloat(s.animationDuration) + parseFloat(s.transitionDuration);
      if (dur > 0.05) bad.push(el.className || el.tagName);
    }
    return bad.slice(0, 5);
  });
  check("I6 reduced motion disables settle and transitions", moving.length === 0, moving.join(", "));
  await ctx.close();
}

await browser.close();

console.log(failures === 0 ? "\nAll UI checks passed." : `\n${failures} check(s) failed.`);
process.exit(failures === 0 ? 0 : 1);
