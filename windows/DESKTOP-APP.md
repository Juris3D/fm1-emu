# FM-1 / SMK-37 desktop app: design notes

The goal: one Windows `.exe` for both boards, with no Node.js and no console. The emulator is already one
program for both: a package says which board it runs on, and the panel follows it.

## Two modes, one window

- **Play mode.** The synth panel alone, nothing else competing for space.
- **Picker mode.** Opens over the panel, full window, with the panel dimmed behind (a button, or F1 /
  Tab). Browsing and playing are separate moments, so the picker gets room for readable descriptions.
  - Tabs: **FM-1 | SMK-37**; a search box (80+ firmwares).
  - Per firmware: name, version, author, the full description, and a status: downloaded / update
    available / not downloaded.
  - Choosing one downloads it if needed, closes the picker and boots it. Esc goes back unchanged.
  - "Open own .fwsc…" for M-VAVE's firmwares.
  - First start (nothing chosen yet): straight into the picker.

## Updates

- Both firmware lists are read from upstream's published site at start (`firmwares.json`,
  `smk37/firmwares.json`), not baked into the build: new firmwares and versions appear without a
  rebuild. Header: "N firmware updates available", one button fetches them all.
- Package download logic (`web/fwsc.js` `fetchPackage`) ported to Rust.
- The app checks this fork's GitHub releases for a newer build of itself.

## Bigger screen ("cheat with screen size")

- An option to show the synth's screen larger, at the same native resolution (240×240), scaled up
  with crisp pixels, not smoothed.
- The synth's controls (knobs, buttons, pads, faders, wheels) are reorganized onto the sides of the
  big screen instead of their real-panel positions. Both boards get their own side layout; the
  keybed stays along the bottom.
- The real-panel layout remains available: the user switches between "real" and "big screen".

## Rules

- Both boards, always: every feature is built and checked on the FM-1 and the SMK-37.
- Lives as its own crate in this fork (as upstream keeps the plugin), so upstream merges stay
  conflict-free. Likely egui, reusing the panel drawing where it fits.
