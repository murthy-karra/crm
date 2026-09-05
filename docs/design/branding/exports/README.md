# Elysium CRM derived exports

These files are production derivatives of the authoritative SVG masters in
`../svg/`. Do not edit a PNG or ICO directly; change the matching SVG master,
re-export it, and repeat the visual and structural checks.

## Export matrix

| Directory | Files | Intended use |
|---|---|---|
| `web/` | `favicon-16x16.png`, `favicon-32x32.png`, `favicon-48x48.png`, `favicon.ico` | Browser fallbacks; the ICO contains all three sizes |
| `web/` | `apple-touch-icon.png` | 180 × 180 Apple touch icon |
| `web/` | `elysium-192.png`, `elysium-512.png` | Standard PWA icons (`purpose: any`) |
| `web/` | `elysium-maskable-192.png`, `elysium-maskable-512.png` | Safe-zone-adjusted PWA maskable icons |
| `web/` | `og-image.png` | 1200 × 630 social/link-preview image |
| `native/` | `ios-app-icon-1024.png` | Opaque 1024 × 1024 iOS App Store master |
| `native/` | `android-adaptive-foreground-432.png` | Transparent Android adaptive foreground |
| `native/` | `android-adaptive-background-432.png` | Opaque Android adaptive background |
| `native/` | `android-adaptive-monochrome-432.png` | Transparent one-color layer for Android themed icons |
| `raster/` | 512 px marks and 2× lockups | Transparent presentation and bitmap fallbacks |

The app tiles, social card, and Android background are RGB with no alpha
channel. Favicon, lockup, mark, and Android foreground PNGs retain transparency.
The native files are staged because `ios/` and `android/` asset catalogs have
not been created yet.

The Android layers follow the current
[adaptive-icon guidance](https://developer.android.com/develop/ui/compose/system/icon_design_adaptive):
a 108 × 108 source, a symbol at least 48 × 48 dp, all visible pixels inside the
66 dp guaranteed safe circle, separate color layers, and a monochrome layer for
themed icons.
