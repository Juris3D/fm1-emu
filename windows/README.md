# FM-1 / SMK-37 emulator for Windows

A Windows 10/11 (x64) layer over [joakimeriksson/fm1-emu](https://github.com/joakimeriksson/fm1-emu):
the native emulator `fm1-live.exe`, a launcher, and a firmware downloader, always for **both**
boards, the M-VAVE **FM-1** and the M-VAVE **SMK-37** (and its siblings: SMK-37 Pro/Elite,
MKE-P37, Donner Starrykey 37 Play). The emulator is the same Rust code as the web pages; a
package says which board it is for, and the window draws that board's panel.

## Use it (the release zip)

1. Unzip anywhere, e.g. `C:\Synths\fm1-emu`.
2. Install [Node.js](https://nodejs.org) once (`winget install OpenJS.NodeJS.LTS`): the downloader
   runs the web page's own download code.
3. Run `Start.cmd`. Press **B** to pick the board, **D** to download its firmwares, then a number
   to play. **U** updates every firmware of both boards.

M-VAVE's own firmwares are not downloaded: get `FM-1.fwsc` or `SMK-37 Pro_016.fwsc` from M-VAVE
and open it with **O**. (On the SMK-37 it is a MIDI controller: screen and panel work, no sound.)

| Keys | FM-1 | SMK-37 |
|---|---|---|
| `A`…`'`, `W E T Y U O P` | keys | keys (C4–F5) |
| `Z` / `X` | octave − / + | octave − / + |
| `Space` / `R` | play / rec | SEQ PLAY / SEQ REC |
| `1`…`8` | buttons | top row of pads |
| `←→` `↓↑` `,.` | encoders | K1, K2, K3 |
| `Esc` | close | close |

The mouse works on the drawn panel. Any MIDI keyboard plays it (**M** picks the input).
If the sound crackles, the CPU can't keep up: **C** lowers the clock (try 160, then 120).

## Build from source

Install Rust from [rustup.rs](https://rustup.rs) (accept the Visual Studio C++ build tools it
offers), then in the repository:

```
powershell -ExecutionPolicy Bypass -File windows\build.ps1 -Bench            build + speed check
powershell -ExecutionPolicy Bypass -File windows\build.ps1 -Package          + dist\fm1-emu-<ver>-windows-x64.zip
windows\Start.cmd                                                             the launcher, on target\release
```

GitHub builds the same zip on every push (`.github/workflows/windows.yml`: Actions → Windows →
the run → Artifacts), tests one firmware on each board, and publishes a Release for a tag
`win-v…` (e.g. `git tag win-v0.2.0-1 && git push --tags`).

## The rule: both boards, always

Every change here is made and checked for the FM-1 **and** the SMK-37:
- the launcher and downloader treat the boards alike (`firmwares\fm1\`, `firmwares\smk37\`,
  each with its own `index.json`; ids repeat across the two catalogs, so they never share a folder);
- CI boots one firmware per board (Felucca, GHOULBOX) and checks the SMK-37 one really ran as the SMK-37;
- by hand, before a release: a firmware with sound on each board, a MIDI keyboard on each,
  M-VAVE's own package on each, and the speed line (`-Bench`) on the notebook and the PC.

## Staying in step with upstream

This layer adds files only (`windows\`, `.github\workflows\windows.yml`) and edits none of
upstream's, so upstream updates merge without conflicts:

```
git remote add upstream https://github.com/joakimeriksson/fm1-emu    (once)
git fetch upstream
git merge upstream/master
git push
```

(or GitHub's **Sync fork** button). The push runs the Windows workflow; the weekly run catches
an upstream change that breaks Windows even without our own commits. Both firmware catalogs
(`web\firmwares.json`, `web\smk37\firmwares.json`) and the download code (`web\fwsc.js`) come from
upstream, so new firmwares appear with a merge and **U** in the launcher.

In a fork, upstream's own workflows also run: `pages.yml` (fails at deploy unless Pages is
enabled; disable it under Actions if unwanted), `release.yml` (only on `v*` tags; ours are `win-v*`).

Fixes that belong in the emulator itself go upstream as pull requests, not here, so the fork
stays a thin layer.

## Known Windows differences (polish list)

- **The JIT on Windows.** Upstream compiles hot code to x86-64 since 21 Oct 2026 (`jit/x64.rs`);
  branch `x64-jit-coverage` here (offered upstream) keeps compiled code running through returns,
  if-else blocks, dual-issue pairs and critical sections: busy firmwares went from ~1.1-1.5x to
  ~1.6-2.3x real time on a 2.1 GHz Xeon, audio bit-identical to the interpreter (checked on every
  package of both catalogs). `FM1_NO_JIT=1` runs the interpreter, to compare.
- **No thread priority boost.** `high_priority()` in `crates/fm1-live/src/main.rs` only does
  something on macOS; a Windows `SetThreadPriority` / MMCSS version would help against crackles.
- **`tools/fetch_firmwares.mjs`** builds paths with `URL.pathname` (`/C:/…`), which breaks on
  Windows, and only knows the FM-1 catalog; `windows\fetch-firmwares.mjs` replaces it here.
- **No DAW plugin build** for Windows yet (`crates/fm1-plugin`: nih-plug + clap-wrapper, CLAP/VST3).
