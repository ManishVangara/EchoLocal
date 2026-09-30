# EchoLocal brand

- `logo-original.webp`: the logo as provided (a glowing "E" on a navy tile).
- `icon-1024.png`: the macOS app icon, cropped from the logo into the
  standard rounded tile with a blue-to-ember rim.
- `inspiration.webp`: the design reference (glass surfaces, status orbs).

Tagline: **Speak it. Typed. Locally.**

## Design language

Dark glass over a night-blue field, lit by the logo's two lights: electric
blue from the top left and ember orange from the bottom right. Surfaces are
translucent and blurred with a hairline rim that runs blue into ember. The
wordmark is "Echo" in white and "Local" in the signature gradient. Dictation
status is a glass orb: a microphone while listening (with a ring that swells
with the voice), moving bars while transcribing, a check when inserted and an
ember orb for errors.

## Palette (sampled from the logo)

| Name | Hex | Used for |
| --- | --- | --- |
| Night | `#050818` | Window background |
| Navy | `#0B1238` | Deep surfaces, menus |
| Electric blue | `#3B7BFF` | Primary gradient, orb |
| Blue light | `#7AA4FF` | Accent text, focus, selection |
| Violet | `#7A5CFF` | Gradient middle |
| Ember | `#FF8A3D` | Live indicators, warm glow |
| Amber | `#FFC27A` | Labels on dark, warnings |
| Coral | `#FF5A3C` | Errors, ember gradient end |

Gradients: **brand** `#3B7BFF → #6A5CFF → #9A5CFF` (buttons, active items),
**signature** `#7AA4FF → #9A8CFF → #FFA062` (wordmark, progress, waveform).

## Regenerating the icons

From the repository root, with Playwright available (`npm i -g playwright`):

```bash
node scripts/brand/render-icons.mjs    # icon-1024.png, public/app-icon.png, menu-bar icons
bun tauri icon assets/brand/icon-1024.png -o /tmp/icons   # then copy the macOS files into src-tauri/icons
```

The menu-bar icon is a vector "E" drawn in the script (a template image, so
macOS tints it for light and dark menu bars).
