# Fonts

Three faces, three jobs, all OFL-licensed so they can ship inside a paid binary
(spec §5.3):

| File | Face | Job |
|---|---|---|
| `SpaceGrotesk-Medium.woff2`, `SpaceGrotesk-Bold.woff2` | Space Grotesk 500/700 | Focus clock, view titles, large numerals |
| `InstrumentSans-Regular/Medium/SemiBold.woff2` | Instrument Sans 400/500/600 | All interface text |
| `CommitMono-Regular/Medium.woff2` | Commit Mono 400/500 | Durations, clock times, parser tokens, the OFFLINE badge |

**These files are deliberately not committed** — they are third-party binaries
with their own licence texts, and vendoring them is a release decision, not a
build-time one. Drop the woff2 files here (and their `OFL.txt`) before cutting a
release.

Two rules, both from the spec:

1. **Never reference a font CDN.** An offline-first app that links a CDN
   silently falls back to system faces on a machine with no network — the exact
   machine this app is built for. I7 is verified by launching with networking
   disabled and confirming zero requests.
2. **Bundle as woff2 and self-host via `@font-face`.** The rules already live in
   `src/styles/tokens.css`; until the files are present the stacks degrade to
   system faces, which is a visual regression but never a network call.
