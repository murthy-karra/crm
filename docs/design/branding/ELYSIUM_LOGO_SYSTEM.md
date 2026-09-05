# Elysium CRM logo system

Status: APPROVED — v1.0 production masters (D-044, 2026-09-03).

The PNG concept boards in this directory are presentation artifacts. The
flattened, self-contained files in `svg/` are the authoritative masters;
everything in `exports/` is derived from them.

## Core idea: the Threshold E

The mark is one continuous geometric ribbon forming an abstract `E`. Its open
edge and central negative space also read as a doorway or threshold. The form
connects three ideas without using literal real-estate imagery:

- a relationship carried forward over time;
- organized paths through a complex workflow; and
- a trusted threshold into a place or opportunity.

The silhouette deliberately avoids roofs, keys, map pins, handshakes, chat
bubbles, brains, circuits, sparkles, and infinity marks.

## System variants

| Variant | Intended use |
|---|---|
| Horizontal lockup | Website header, sales material, signage |
| Name-only horizontal lockup | Compact product chrome and narrow headers |
| Mark only | Favicon, compact navigation, avatar, social profile |
| Stacked lockup | Square placements, event signage, merchandise |
| Name-only stacked lockup | Compact square placements and sign-in surfaces |
| App tile | iOS, Android, PWA, marketplace listings |
| Android adaptive layers | Native foreground/background/themed-icon generation |
| Micro mark | 16–32 px browser and dense-interface contexts |
| Social card | 1200 × 630 px link-preview artwork |
| One-color positive | Print, stamp, engraving, embroidery |
| One-color reversed | Dark surfaces and photography with sufficient contrast |

`CRM` is part of the full formal lockups. Use the supplied name-only lockups
when the surrounding product context already establishes the category or the
available size would make the descriptor too small. Never remove `ELYSIUM`
from a wordmark lockup or manually hide `CRM` in a full lockup.

## Color

| Role | Value | Notes |
|---|---|---|
| Elysium indigo | `#1E1B4B` | Primary mark; matches the accepted web accent |
| White | `#FFFFFF` | Reversed mark and app-tile glyph |
| Warm white | `#F9FAFB` | Presentation ground; matches the web page surface |
| Black | `#000000` | One-color production fallback only |

The production logo is flat color. Do not add gradients, glows, bevels,
textures, outlines, or decorative shadows.

The values above are digital/sRGB masters. Define a process-CMYK build and a
spot-color match, then approve a physical proof on the actual substrate before
producing indigo merchandise or signage.

## Typography

The wordmark uses Inter Bold 700 with custom tracking: `0.06em` for `ELYSIUM`
and `0.75em` for `CRM`. Every SVG contains flattened glyph outlines, not
`<text>`, so it has no runtime font dependency. Product UI surrounding the logo
continues to use Inter per `docs/design/UI_STYLE.md`.

## Scale and clear space

- Use the micro mark from 16–31 px. Use the standard mark at 32 px and above.
- Keep the full horizontal lockup at least 240 px wide on screen or 63 mm in
  print. The name-only horizontal lockup may be used down to 144 px or 38 mm.
- Keep the full stacked lockup at least 192 px wide on screen or 51 mm in
  print. The name-only stacked lockup may be used down to 96 px or 25 mm.
- Use clear space on every side equal to the width of the mark's central
  vertical doorway stem.
- App tiles keep at least 18% padding from the mark to the tile edge. The
  operating system, not the artwork, supplies platform-specific masks.
- Android adaptive layers use a 108 × 108 artboard and a platform-adjusted
  48.1 × 51.6 dp symbol that stays inside the guaranteed 66 dp circular safe
  zone. The PWA maskable tile uses the same safe symbol geometry.
- For embroidery, start with the one-color mark at no less than 30 mm wide and
  confirm a minimum bridge/stroke thickness of 1.2 mm in a physical stitch-out.
  Enlarge it for coarse material or heavy thread, and omit `CRM` when it cannot
  remain cleanly legible.

## Misuse

Do not stretch, rotate, outline, recolor individual ribbon segments, place the
mark on low-contrast imagery, alter the doorway negative space, or substitute a
plain typed `E` for the symbol.

## Authoritative SVG masters

The `svg/` directory contains:

- standard mark: indigo, white, and black;
- optically simplified micro mark: indigo and white;
- full and name-only horizontal lockups: indigo, white, and black;
- full and name-only stacked lockups: indigo, white, and black;
- full-bleed, unmasked app-tile master; and
- separate Android adaptive foreground, background, and monochrome masters;
- a safe-zone-adjusted PWA maskable-tile master;
- rounded browser-favicon artwork; and
- a 1200 × 630 social-card master.

Every file is standalone and uses vector geometry only: no text, embedded
raster, script, external reference, gradient, filter, or stroke. Color variants
share identical geometry. The standard and micro symbols are each a single
boolean-unioned closed contour so fabrication tools do not encounter coincident
or zero-width joins. The adaptive symbol makes a small optical cap adjustment
solely to meet circular platform masks; do not substitute it for the standard
mark.

## Derived production exports

The checked-in `exports/` package contains:

- favicon: 16/32/48 px PNG fallbacks and a three-size ICO;
- web/PWA: opaque 192 px and 512 px standard tiles plus dedicated maskable
  variants;
- Apple touch icon: 180 px;
- native staging assets: opaque 1024 px iOS artwork and separate 432 px Android
  adaptive foreground, background, and monochrome layers;
- transparent 512 px marks and 2× horizontal/stacked lockups; and
- an opaque 1200 × 630 social-sharing image.

The web client consumes the approved favicon, compact lockups, Apple touch icon,
PWA tiles, and manifest from `web/public/`. Native projects do not exist yet;
their staged exports must be imported into the platform asset catalogs when
those projects are created.
