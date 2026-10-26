# FM-1 / SMK-37 desktop app: design notes

**State (23 Oct 2026):** first version built: `crates/fm1-desk` (FM-1 · SMK-37 Desk): play mode,
picker over the panel (tabs, search, statuses, downloads, update all, own .fwsc), lists fetched
from upstream with the built-in ones as fallback, flash saved per firmware, window fitted to
each board's panel. 0.2 (24 Oct, a friend's requests): the computer keyboard in Ableton or
FL / tracker layout with its letters on the keys; the Library strip (F2), Kontakt-like: both
boards as pictures, the firmwares here as tiles of their screens (the playing one live), the
last played first. 0.3 (24 Oct): colour themes from the real units' colourways (Classic =
upstream's own drawing; Black, Graphite, Orange, Mint, Cream), per board, in `skin.rs`, which
repaints upstream's panels in a theme's colours with the same geometry; the FM-1's keybed swap,
pills or a piano keyboard (F3..G5) in the same groove. 0.4 (26 Oct): the Sampler (`sampler.rs`), a host-side add-on, SK-1 / VSS spirit, both
boards: records the input or resamples the synth (tapped in the audio output), loads / saves
WAV, plays from the note keys and MIDI as routed (Synth / Sampler / Both); SK-1 grit = 8-bit at
9.38 kHz, no anti-alias, zero-order hold; memory 1.4 s (SK-1) or longer (the "cheat"). It is
kept apart from the emulation: the firmware never sees it. Still to do: the app's own update check, and the "big screen" layout.

The goal: one Windows `.exe` for both boards, with no Node.js and no console. The emulator is already one
program for both: a package says which board it runs on, and the panel follows it.

## Two modes, one window

- **Play mode.** The synth panel alone, nothing else competing for space.
- **Picker mode.** Opens over the panel, full window, with the panel dimmed behind (a button, or F1). Browsing and playing are separate moments, so the picker gets room for readable descriptions.
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
- The real-panel ("legacy") layout stays, more or less as it is, and is the default at first; "big
  screen" is the option to switch to.

## Rules

- Both boards, always: every feature is built and checked on the FM-1 and the SMK-37.
- Lives as its own crate in this fork (as upstream keeps the plugin), so upstream merges stay
  conflict-free. Likely egui, reusing the panel drawing where it fits.
