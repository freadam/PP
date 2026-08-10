# Fonts

One family, one job. The ERPNext / Frappe Desk design system loads
**InterVariable** first, then static **Inter**, then a system stack — so Fruit
does the same. The previous three-family split (Space Grotesk for display,
Instrument Sans for UI, Commit Mono for figures) is gone: this system does not
have one, and durations get fixed-width figures from Inter's `tnum` feature
rather than from a separate monospace face.

| File | Face | Job |
|---|---|---|
| `InterVariable.woff2` | Inter Variable 100–900 | Everything. The variable axis is what makes the system's 420 regular weight reachable. |
| `Inter-Regular.woff2`, `Inter-Medium.woff2`, `Inter-SemiBold.woff2`, `Inter-Bold.woff2` | Inter 400/500/600/700 | Static fallback where the variable file is unavailable. |

Inter is OFL-licensed, so it can ship inside a paid binary.

**About weight 420.** The source document is explicit: *"Preserve the unusual
regular weight of 420: it is a deliberate midpoint that reads slightly sturdier
than 400 at compact enterprise sizes."* That weight only exists on the variable
file. With the static faces the browser rounds 420 to 400, which is a small
visual regression and not a broken layout — but it is the reason
`InterVariable.woff2` is listed first and is the one to prioritise vendoring.

**These files are deliberately not committed** — they are third-party binaries
with their own licence text, and vendoring them is a release decision, not a
build-time one. Drop the woff2 files here (and `OFL.txt`) before cutting a
release.

Two rules, both unchanged by the redesign:

1. **Never reference a font CDN.** An offline-first app that links a CDN
   silently falls back to system faces on a machine with no network — the exact
   machine this app is built for. I7 is verified by launching with networking
   disabled and confirming zero requests, and `scripts/check-ui.mjs` fails the
   build on any external request.
2. **Bundle as woff2 and self-host via `@font-face`.** The rules already live in
   `src/styles/tokens.css`; until the files are present the stack degrades to
   `system-ui` / Segoe UI, which is a visual regression but never a network
   call.
