# Babilon Tennis — Rust port (desktop, mobile-ready)

Separate workspace from the web version (`../Babilon-Tennis/`).
Core rules are a faithful port of `index.html` v0.5 JS.

## Layout

- `babilon-core/` — pure rules, no UI deps. Reused by desktop now, mobile later.
  - scoring: 1 set to 6 (win by 2), 6–6 → tie-break to 7 (win by 2)
  - state machine: Ready → Serve → Incoming → Hit → Return → Ready/Finished
  - serve ace chance (easy .13 / normal .08 / hard .035)
  - return math: quality windows (330/750/1100 ms), out/winner/counter chances
  - RO score strings + EN umpire calls + `sunete/*.mp3` clip keys
  - `cargo test -p babilon-core` → 10 tests, all passing
- `babilon-desktop/` — Bevy 2D accessible desktop game.
  - same keyboard: `S` serve, `Space` hit, `←/→` direction, `1/2/3` shot,
    `R` repeat, `P` pause, `T` audio test
  - court + paddles + ball drawn in 2D (mirrors `court()` in JS)
  - umpire MP3s from `babilon-desktop/assets/sunete/` (37 files, copied)
  - screen readers: Bevy UI text nodes (AccessKit → NVDA/Narrator/VoiceOver/Orca)
    + stdout `[ANUNȚ]` lines as fallback
- `assets/sunete/` — copied verbatim from the web version.

## Run

```sh
cargo test -p babilon-core
cargo run -p babilon-desktop
```

Needs a display + audio (desktop). First keypress unlocks audio, same as web.

> Linux note: full Bevy default features need `libudev-dev` (gamepad) which
> may not be installed. This workspace uses minimal Bevy features (no
> `bevy_gilrs`) + `mp3` support, so it builds without sudo. If you install
> `libudev-dev`, you can switch back to `bevy = "0.16"` with defaults.

## UI layout

- 1160×800 window: the court stays centered-left, and all text lives in a
  dark side panel docked right (268px) — nothing is drawn over the game.
- Panel (top→bottom): title, scoreboard, status/announcement, shot,
  phase, mode/difficulty/volumes line, key help. World-space labels
  (BABILON OPEN, TU, ADVERSAR) remain on the court.
- Typeface: bundled Inter Regular + Bold (`assets/fonts/`, OFL) with full
  ș/ț/ă/â/î coverage verified via fontconfig — identical rendering on
  Linux / Windows / macOS / mobile, no system-font roulette.

## Windows version

Same code, no forks. Verified: `cargo check -p babilon-desktop --target
x86_64-pc-windows-gnu` passes. Screen readers (NVDA/Narrator via AccessKit),
WASAPI audio, and the 37 MP3s all work through Bevy.

- On a Windows PC: `cargo run -p babilon-desktop --release`
- From this Linux box via CI: `.github/workflows/windows.yml` builds on
  `windows-latest` and uploads `BabilonTennis.exe + assets/` as an artifact.
- Ship the folder `BabilonTennis.exe` next to `assets/sunete/*.mp3` —
  the game loads umpire clips relative to `assets/`.

## Mobile later — what stays, what changes

Keeps `babilon-core` untouched. New `babilon-mobile` crate (Bevy, same engine):
- touch: swipe = direction, tap = hit, buttons for serve/shot type
- bigger type, haptics on the HIT cue, first-tap audio unlock
- `cargo apk` (Android) / Xcode (iOS) builds from the same Bevy app

## Sound (rodio, `babilon-desktop/src/audio.rs`)

* Stereo-positioned synth SFX with the web's recipes, through an exact
  constant-power panner (hard left = left, hard right = right): guide
  beep (mid-flight, Access mode), HIT cue at landing, racket crack +
  low thump, bounce noise, win/lose jingles — all at full ±1 range by
  ball position. Every `←/→` press answers with a lane blip whose pitch
  climbs left→right (Eb4 F4 G4 A4 B4) plus pan, so you always know where
  you stand — across 5 lanes: stânga / mijloc-stânga / centru /
  mijloc-dreapta / dreapta.
* `T` doubles as a stereo channel check: hard-left blip, then hard-right
  half a second later — if "stânga" comes out of your right ear, your
  headset/speakers are swapped. `N` flips stereo in-game to match
  (shown in the panel as Stereo normal/invers).
* All 37 umpire/crowd MP3s preloaded at startup, mono-ized so panning is
  exact; looping crowd ambience (`crowd-ambient.mp3`) with the web's gain
  curve; applause on your points, 40% crowd reaction on lost ones.
* Runs on a dedicated audio thread (cpal streams are `!Send`) behind a
  channel — the game never blocks on audio, and headless runs stay silent.
* Keys: `G` difficulty (Ușor/Normal/Greu), `M` audio mode
  (accesibil/realist — Realist drops the guide beep, keeps a soft cue),
  `C` volume group (minge/arbitru/public), `Z`/`X` ∓10%, `T` stadium test,
  `D` output device (Bluetooth headsets etc.).

## Bluetooth / output devices

`rodio` talks to raw ALSA and opens the OS *default* device — on Linux a
Bluetooth headset routed through PipeWire/PulseAudio is often *not* the
ALSA default, so the game plays through the speakers instead.

* Press `D` in-game to cycle output devices; each switch is announced
  (`Ieșire audio: … (2/3)`) and the console logs `[AUDIO] output: …` plus
  the full device list at startup.
* If no devices appear: on Linux install the PipeWire↔ALSA bridge
  (`pipewire-alsa`) or check `pactl list sinks`; Windows/macOS expose
  Bluetooth headsets directly.
* `D` output device (Bluetooth headsets etc.): cycles only *safe* devices.
  Raw `hw:` outputs are exclusive-access with no format conversion and
  segfaulted an HDMI driver here (exit 139) — they are hidden, as are
  capture-only `dsnoop` and dead OSS entries. The same hardware stays
  reachable through `hdmi:` / `plughw:` / `dmix:` / `pulse:` / `pipewire`.
  The old stream is released *before* the new one opens.
* Tip: keep the headset in music mode (A2DP). If its microphone is active
  the headset drops to 8 kHz mono (HSP/HFP) and everything sounds dull —
  that is a Bluetooth limitation, not a game bug.

## Gaps vs web 0.5 (next steps)

* Real rally-hit MP3 (web currently synthesizes it too — ours is a faithful
  synth crack + thump).
* TTS fallback for tie-break scores (currently a console line + English MP3s
  where they exist).
* High-contrast / font-size options.
