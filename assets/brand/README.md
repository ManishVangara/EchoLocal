# EchoLocal brand assets

- `logo-original.webp` — the logo as provided.
- `logo.png` — cleaned: the dark cut-out fringe removed, trimmed and squared.
- `icon-1024.png` — the app icon: the logo on a midnight rounded tile.

## Palette (sampled from the logo)

| Name | Hex | Used for |
| --- | --- | --- |
| Pearl | `#E5DCD8` | Warm highlight, light backgrounds |
| Periwinkle | `#6B7BE5` | Accent in dark mode, gradients |
| Royal blue | `#3952BD` | Accent in light mode |
| Violet | `#8B6FE0` | Gradient end, recording |
| Indigo | `#323F7C` | Secondary text on light |
| Midnight | `#131E4E` | Icon tile, dark surfaces |
| Slate | `#707494` | Muted text |

## Regenerating

From the repository root, with Playwright available (`npm i -g playwright`):

```bash
node scripts/brand/clean-logo.mjs      # logo-original.webp → logo.png
node scripts/brand/render-icons.mjs    # logo.png → icon-1024.png, app-icon, menu-bar icons
bun tauri icon assets/brand/icon-1024.png -o /tmp/icons   # then copy the macOS files into src-tauri/icons
```
