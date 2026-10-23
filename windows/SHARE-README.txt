FM-1 SMK-37 Desk
================

Two little M-VAVE synths, emulated on your PC in one window:

  - the M-VAVE FM-1 (a pocket FM synth)
  - the M-VAVE SMK-37 (a 37-key keyboard with pads; also SMK-37 Pro / Elite, MKE-P37)

A community of people writes their own firmwares for these synths: grooveboxes, drum
machines, ambient generators, a chord harp, a tracker, a C64 SID synth... This app runs
those firmwares unchanged, as the real hardware would, and fetches them for you.


START
-----

1. Unzip this folder somewhere you can write to (Desktop, Documents, D:\Synths...).
2. Double-click "FM-1 SMK-37 Desk.exe".
   Windows may say "Windows protected your PC" (the app is not code-signed):
   click "More info", then "Run anyway".
3. The firmware list opens. Pick the FM-1 or SMK-37 tab, then a firmware:
   "Download & play" fetches it once from its author; after that it is "Play".

Next time, the app starts with the firmware you played last.

Needs: Windows 10 or 11, 64-bit; an internet connection for the firmware downloads.
A recent CPU plays every firmware in real time. If the sound crackles, lower the clock
in the bar at the bottom (try 160 MHz, then 120).


PLAYING
-------

While a synth plays, its panel fills the window. Mouse: click or drag across the keys
(lower on a key = harder), click buttons and pads, drag or scroll the knobs, drag faders
and wheels. Any MIDI keyboard plugged in plays it too (choose the input at the bottom).

  Computer keys        FM-1                 SMK-37
  A ... '              keys C4 .. F5        keys C4 .. F5
  W E T Y U O P        the black keys       the black keys
  Z / X                octave - / +         octave - / +
  Space / R            PLAY / REC           SEQ PLAY / SEQ REC
  1 ... 0 / 1 ... 8    buttons              the top row of pads
  arrows, comma/dot    encoders             K1, K2, K3

  F1                   the firmware list (over the panel)
  Esc                  back to playing

What each button does is up to the firmware: its "source" link in the list leads to
its page, which usually has a manual.

In the list: type to search; "updates available" appears when firmwares have new
versions; "Open own .fwsc..." plays a package from your PC, e.g. M-VAVE's own firmware
(FM-1.fwsc / SMK-37 Pro_016.fwsc, which M-VAVE's site has and this app cannot fetch).


WHERE THINGS GO
---------------

Next to the .exe:
  firmwares\   the downloaded firmware packages
  saves\       what each firmware saved to its flash (patches, settings, songs)
  catalogs\    the firmware lists, as last fetched
  fm1-desk.json  the app's settings
(If the folder is not writable, all of this goes to %LOCALAPPDATA%\FM-1 SMK-37 Desk.)

To start over, delete those. To move the app, move the whole folder.


CREDITS
-------

The emulator is fm1-emu by Joakim Eriksson (MIT licence, LICENSE.txt):
  https://github.com/joakimeriksson/fm1-emu   (it also runs in the browser)
This Windows app is built on it:
  https://github.com/Juris3D/fm1-emu   (branch "windows", crates/fm1-desk)

The firmwares are their authors' work, under their own licences (shown in the list),
downloaded from the authors' and the FM-1 Firmware Hub's sites. The SMK-37 builds come
from genkigenki's fm1-to-smk37 ports.

Not made by or affiliated with M-VAVE.
