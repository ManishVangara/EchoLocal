# Running EchoLocal as a standalone app

`bun run tauri dev` is for development. For everyday use, build a real
`EchoLocal.app`. There are four ways, from simplest to most polished.

## 1. Install on your own Mac (recommended now)

```bash
./scripts/install-mac.sh
```

This builds a release version, copies `EchoLocal.app` into `/Applications`,
removes the download-quarantine flag and launches it. EchoLocal then lives in
the menu bar; turn on **Launch at login** (General) so it's always there.

- The app is **ad-hoc signed** (`"signingIdentity": "-"` in
  `src-tauri/tauri.conf.json`), which Apple Silicon requires and which needs no
  Apple account.
- macOS remembers Accessibility permission per signature. After installing a
  **new** build, if dictation stops typing, open System Settings → Privacy &
  Security → Accessibility, remove EchoLocal with **–**, and allow it again.
  Microphone permission usually survives.
- Update later with `git pull && ./scripts/install-mac.sh`.

## 2. Download a DMG from GitHub Releases

Push a version tag and GitHub Actions builds the app and a DMG on an Apple
Silicon runner (`.github/workflows/release.yml`), attaching them to a draft
release:

```bash
git tag v0.2.0 && git push origin v0.2.0
```

Without signing secrets the DMG is ad-hoc signed: it works, but on another
Mac Gatekeeper says it "can't be checked for malware" the first time. Right-click
the app → **Open** (or System Settings → Privacy & Security → **Open Anyway**).

## 3. Signed and notarized (for sharing with others)

To let anyone open EchoLocal normally, sign it with a **Developer ID
Application** certificate and have Apple notarize it. This needs an Apple
Developer Program membership ($99/year). Add these repository secrets and the
release workflow switches to the signed build automatically:

| Secret | Value |
| --- | --- |
| `APPLE_CERTIFICATE` | Developer ID Application certificate exported as `.p12`, base64-encoded |
| `APPLE_CERTIFICATE_PASSWORD` | Password of that `.p12` |
| `APPLE_SIGNING_IDENTITY` | e.g. `Developer ID Application: Your Name (TEAMID)` |
| `APPLE_ID` | Your Apple ID email |
| `APPLE_PASSWORD` | An app-specific password for that Apple ID |
| `APPLE_TEAM_ID` | Your 10-character team ID |

A stable signature also means macOS keeps Accessibility permission across
updates.

## 4. Later: automatic updates and Homebrew

- **Auto-update:** Tauri's updater plugin can check GitHub Releases and update
  in place; it needs its own signing key pair and a published `latest.json`.
  Worth adding once releases are regular.
- **Homebrew:** a cask (`brew install --cask echolocal`) can point at the
  release DMG, ideally once builds are notarized.

## What the app needs at runtime

| Permission | Why | Asked |
| --- | --- | --- |
| Microphone | Recording while the shortcut is held | During onboarding |
| Accessibility | Detecting the shortcut, typing text into other apps | During onboarding |

Models are downloaded into `~/Library/Application Support/app.echolocal.desktop/models`,
settings live in `~/Library/Application Support/app.echolocal.desktop/settings.json`,
and logs in `~/Library/Logs/app.echolocal.desktop/`.
