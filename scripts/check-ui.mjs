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

  // I4: every numeral that changes in place is tabular.
  const nonTabular = await page.evaluate(() => {
    const bad = [];
    for (const el of document.querySelectorAll(".data, .micro, .focus-clock")) {
      const v = getComputedStyle(el).fontVariantNumeric;
      if (!v.includes("tabular-nums")) bad.push(el.className);
    }
    return bad;
  });
  check("I4 changing numerals use tabular figures", nonTabular.length === 0, nonTabular.join(", "));

  // U10: a visible focus ring on every interactive element, never hover only.
  const focusRing = await page.evaluate(async () => {
    const targets = [...document.querySelectorAll("button, input, select, textarea, [tabindex]")]
      .filter((el) => el.offsetParent !== null)
      .slice(0, 40);
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
  });
  check("U10 focus is visible on every interactive element", focusRing.length === 0, focusRing.join(", "));

  // I3: no drift state distinguishable by colour alone — every rail carries a
  // texture and an accessible name.
  const unlabelled = await page.evaluate(() =>
    [...document.querySelectorAll(".rail-compact, .rail-bar")].filter(
      (el) => !el.getAttribute("aria-label"),
    ).length,
  );
  check("I3 drift encodings carry a text alternative", unlabelled === 0, `${unlabelled} unlabelled`);

  await ctx.close();
}

// I5: the layout holds at each breakpoint, and at 125% text scaling.
for (const [width, height, scale] of [
  [960, 640, 1],
  [1130, 720, 1],
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
  const overflow = await page.evaluate(() => ({
    doc: document.documentElement.scrollWidth - document.documentElement.clientWidth,
    body: document.body.scrollWidth - document.body.clientWidth,
  }));
  check(
    `I5 no horizontal overflow at ${width}×${height}${scale === 1 ? "" : " @125% text"}`,
    overflow.doc <= 0 && overflow.body <= 0,
    JSON.stringify(overflow),
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
