# Fruit — Design System

**Source:** *Design System Reference — ERPNext / Frappe Desk* (supplied
`Design_system.docx`, inspection date 10 August 2026)
**Status:** The contract this product's look is built from.

This is the normative extract. The supplied document carries the full
provenance discussion, the login-route observations, and the Frappe-specific
guidance; what follows is the part this codebase is held to, so a reviewer can
check a value without opening Word.

The document labels its own confidence, and that survives here:

| Label | Meaning |
|---|---|
| **SITE ASSET** | Exact token or component rule served by the inspected deployment. High confidence. |
| **OBSERVED** | Visible in a rendered capture of the public login route. |
| **INFERRED** | Expected Frappe/ERPNext behaviour from official documentation. Validate before pixel-matching. |

Everything below is SITE ASSET unless marked otherwise. `src/styles/tokens.css`
is the implementation and cites this document throughout;
`scripts/check-ui.mjs` I8 measures the dimensions on the rendered page.

---

## 1. Colour

Neutral-first. Grays carry hierarchy, borders, control fills and primary
actions; chromatic families are reserved for status, links and charts.

| Token | Hex | Role |
|---|---|---|
| gray-50 | `#F8F8F8` | Page/list hover, subtle background |
| gray-100 | `#F3F3F3` | Default control and secondary-button fill |
| gray-200 | `#EDEDED` | Dividers and table borders |
| gray-300 | `#E2E2E2` | Input borders and hover surfaces |
| gray-400 | `#C7C7C7` | Stronger borders, scrollbars |
| gray-500 | `#999999` | Placeholders and tertiary iconography |
| gray-600 | `#7C7C7C` | Muted labels — see the contrast note below |
| gray-700 | `#525252` | Secondary text |
| gray-800 | `#383838` | Primary body text / hover action |
| gray-900 | `#171717` | Headings and primary actions |

### Semantic status pairs

| Role | Surface | Text/dot | Contrast |
|---|---|---|---|
| Blue / info | `#EDF6FD` | `#0070CC` | 4.58:1 |
| Green / success | `#E4F5E9` | `#16794C` | 4.79:1 |
| Yellow / caution | `#FFF7D3` | `#AB6E05` | 3.93:1 |
| Orange / attention | `#FFF1E7` | `#BD3E0C` | 4.92:1 |
| Red / danger | `#FFF0F0` | `#B52A2A` | 5.70:1 |
| Purple | `#F9F0FF` | `#6E399D` | 6.92:1 |
| Pink | `#FFF7FC` | `#9C2671` | 6.81:1 |
| Cyan | `#F5FBFC` | `#267A94` | 4.68:1 |

**Two of these fail the document's own accessibility bar, and it says so.**
Both remediations are its own words and both are taken in `tokens.css`:

- gray-600 `#7C7C7C` is 4.17:1 on white — below AA for normal text. Use
  **`#6B6B6B` or darker** for 14px labels and links. (`--faint`.)
- The yellow pair is 3.93:1 for text. **Darken amber text to approximately
  `#8C5600`.** Graphical use — a 3px trace, a progress fill — needs only 3:1,
  so fills keep `#AB6E05` (`--track-graphic`) and text uses `--track`.

### Dark theme

| Semantic | Light | Dark |
|---|---|---|
| surface-white | `#FFFFFF` | `#0F0F0F` |
| surface-gray-1 | `#F8F8F8` | `#232323` |
| surface-gray-2 | `#F3F3F3` | `#2B2B2B` |
| surface-modal | `#FFFFFF` | `#232323` |
| ink-gray-6 | `#525252` | `#999999` |
| ink-gray-8 | `#383838` | `#D4D4D4` |
| ink-gray-9 | `#171717` | `#F8F8F8` |

The document publishes no dark values for the chromatic families — it states a
dark theme exists in the deployment CSS but was never rendered. The hues in
`tokens.css` are therefore a marked extension, lifted until they clear 4.5:1 on
`#0F0F0F` and `#232323`, to be replaced when a logged-in dark capture exists.

---

## 2. Typography

InterVariable first, then Inter, then a native stack.

**Preserve the regular weight of 420.** It is a deliberate midpoint that reads
slightly sturdier than 400 at compact enterprise sizes.

| Role | Size | Weight | Use |
|---|---|---|---|
| text-tiny | 11px | 420 | Metadata, dense supporting detail |
| text-xs | 12px | 420–600 | Table headers, timestamps, compact UI |
| text-sm | 13px | 420–500 | Sidebar and compact labels |
| text-base | 14px | 420 | Default controls and UI copy |
| text-lg | 16px | 420–600 | Body lead, small headings |
| text-2xl | 20px | 700 | Page heading |

Typical tracking is `.02em`.

---

## 3. Spacing, shape, elevation

| System | Tokens | Guidance |
|---|---|---|
| Spacing | 5, 7, 15, 20, 30, 40px | 5/7 compact controls; 15/20 component padding; 30/40 page separation |
| Radius | 4, 8, 10, 12, 16, 20, 999px | 8px default control; 10–12px cards/modals; 999px pills |
| Control | 28px buttons and inputs | Dense desktop rhythm; ≥44px target on coarse pointers |
| Elevation | xs → 2xl | Prefer borders/gray shifts; reserve shadow for menus and overlays |
| Focus | 2px rings | Ring colour by semantic context; must stay visible on the local surface |

> **Implementation rule** Use semantic aliases (`--surface`, `--text-primary`,
> `--border`, `--control-bg`, `--focus-ring`). Keep raw ramps private to the
> theme layer so dark mode remaps without component rewrites.

---

## 4. Layout and navigation

| Token | Value |
|---|---|
| Navbar height | 48px |
| Page head height | 48px, sticky, 1px bottom border |
| Primary sidebar | 220px |
| Form sidebar | 277px |
| Page max width | 900px |
| Page bottom margin | 60px |
| Breakpoints | 576 / 768 / 992 / 1200 / 1440px |

Anchor global chrome to 48px vertical units. Let data grids scroll
horizontally rather than crushing columns; keep a sticky action region on the
trailing edge of wide lists.

> **Responsive caution** The source codebase carries both tokenised breakpoints
> and compiled media queries at adjacent values (567/576, 991/992). Adopt one
> canonical map. This repo uses 576/768/992/1200/1440.

---

## 5. Components

### List row

30px minimum · 7px inline padding · 1px gray-200 bottom border · 8px radius ·
14px/420/.02em · hover **and** `:focus-within` fill gray-50 · main region
`flex: 4`, min 80% · actions region sticky right, 9×10px padding.

Header row: 30px, gray-100 fill, 8px radius, 7px inline padding — a quiet
column-label band, not a heavy table header.

### Buttons

| Variant | Rest | Hover / active | Focus |
|---|---|---|---|
| Primary | `#171717` / white | `#040404` / `#000000` | 2px dark ring |
| Default | `#F3F3F3` / `#171717` | `#E0E0E0` / `#DADADA` | 2px `#D2D2D280` |
| Secondary | `#7C7C7C` / white | `#696969` / `#636363` | 2px `#90909080` |
| Danger | `#E03636` / white | `#D02020` / `#C51E1E` | 2px `#E5545480` |
| Icon | 28×28px | Surface shift | Same semantic ring |

> **Hierarchy rule** One primary action per page. Neutral-first emphasis;
> reserve chromatic fills for destructive or explicit semantic actions.

### Status pill

Inline-flex · 20px high · 4.5×8px padding · full radius · 13px/420 · 6px
semantic dot with a 6px gap. Round indicators are 24×24px.

### Form controls

28px high · 14px · 8px radius · 6×8px padding · 1px `#E2E2E2` border. Focus
takes a stronger border plus a 2px ring. Checkbox 14px desktop, 18px below
992px. Errors: red outline **plus** inline text, placed immediately after the
control and programmatically associated.

### Modals

Max width 575px · 12px radius · 1px border · soft `0 5px 10px / 10%` shadow ·
header 16px padding with bottom border · body 16px · footer 12px, top border,
right-aligned actions ordered secondary → primary. Motion respects
`prefers-reduced-motion`.

### Cards

Generic 12px radius with a light border and 20px body padding; a Desk main
section flattens to 10px radius with no border or shadow.

---

## 6. Accessibility

The document's own findings, and what they require:

1. Keyboard reaches every control in a logical order; overlays trap and restore
   focus.
2. Focus indicators stay visible in both themes and against selected/hover
   surfaces, at ≥3:1.
3. Normal text ≥4.5:1; large text and meaningful UI boundaries ≥3:1.
4. Touch targets ≥44×44px, or equivalent wrapper behaviour — 28px controls are
   below the recommendation and need a coarse-pointer mode.
5. Tables expose header associations; status changes are announced.
6. Reduced-motion users get equivalent feedback without movement.
7. Errors are specific, adjacent, associated, and summarised on failure.
8. Icon-only controls carry an accessible name; decorative SVG is hidden.
9. Drag-and-drop always has a keyboard equivalent that announces the result.

**No status may depend on colour alone.**

---

## 7. What this repo does with it

- `src/styles/tokens.css` — the whole token layer, citing this document inline.
  43/43 of the numbers above are implemented literally.
- `scripts/check-ui.mjs` — I1 forbids literal colour outside the token layer;
  I3 requires a text alternative for every encoding; I5 holds the layout at all
  five breakpoints; I6 honours reduced motion; **I8 measures the 28px control,
  8px radius, 48px chrome, 30px list band and Inter stack on the rendered
  page**, so a component that opts out of the system fails the build rather
  than drifting quietly.
- Deviations, with reasons, are in `SPEC-DEVIATIONS.md`.
