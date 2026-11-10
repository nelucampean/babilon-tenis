# Babilon Tennis — Rust port (desktop, mobile-ready)

Separate workspace from the web version (`../Babilon-Tennis/`).
Core rules are a faithful port of `index.html` v0.5 JS.

## Layout

- `babilon-core/` — pure rules, no UI deps, no user-facing strings.
  Reused by desktop now, mobile later.
  - scoring: 1 set to 6 (win by 2), 6–6 → tie-break to 7 (win by 2)
  - state machine: Ready → Serve → Incoming → Hit → Return → Ready/Finished
  - serve ace chance (easy .13 / normal .08 / hard .035)
  - return math: quality windows (330/750/1100 ms), out/winner/counter chances
  - structured `PointResult` flags + EN umpire calls + `sunete/*.mp3` clip keys
  - `cargo test -p babilon-core` → 20 tests, all passing
- `babilon-desktop/` — Bevy 2D accessible desktop game (library + thin bin).
  - same keyboard: `S` serve, `Space` hit, `←/→` direction, `1/2/3` shot,
    `R` repeat, `P` pause, `T` audio test
  - court + paddles + ball drawn in 2D (mirrors `court()` in JS)
  - Romanian labels, score lines and announcements live in `src/text.rs`
  - umpire MP3s from `babilon-desktop/assets/sunete/` (37 files, copied)
  - screen readers: Bevy UI text nodes (AccessKit → NVDA/Narrator/VoiceOver/Orca)
    + stdout `[ANUNȚ]` lines as fallback
  - `cargo test -p babilon-desktop` → 39 headless tests, all passing
- `assets/sunete/` — copied verbatim from the web version.

## Architecture

The desktop crate is split so a new screen (menu, settings, results) is a
small change instead of a rewrite:

| Module | Owns |
| --- | --- |
| `app.rs` | `run()` (plugins, Startup scene) and `build_app(app, settings)` — the shared state/resources/`Update` chain used by both the real app and the headless tests |
| `state.rs` | `AppState { Playing, Paused }`; gameplay systems run only in `Playing` |
| `settings.rs` | one `Settings` resource (difficulty, audio mode, volumes, stereo swap) + `autosave` |
| `announce.rs` | `Announce` event → `Announcer` status line + stdout screen-reader line |
| `sound.rs` | `Sound` events; gameplay requests audio without knowing rodio |
| `audio.rs` | rodio engine + `audio_service`, the only system that resolves `Sound` events against `Settings` |
| `text.rs` | every Romanian label, score line, point message and TV column |
| `layout.rs` / `theme.rs` | window/court geometry + the full palette as constants |
| `hud.rs` | one `InfoLine` enum + `HudWorld` SystemParam; data-driven panel rows |
| `gameplay.rs` / `ball.rs` / `input.rs` | rules, flight, keyboard (events in, events out) |

`babilon-core` knows nothing about Bevy, audio, Romanian text or persistence:
it exposes points/games/sets, umpire clip keys and structured `PointResult`
flags. All presentation is a desktop concern, so a future mobile crate can
reuse the rules with different text and input.

## Run

```sh
cargo test --workspace                       # 20 core + 39 desktop, headless
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
cargo run -p babilon-desktop
```

`.github/workflows/ci.yml` runs exactly those three checks on every push
and PR (Linux); `windows.yml` / `release.yml` build the Windows exe.

Needs a display + audio (desktop). First keypress unlocks audio, same as web.

> Linux note: full Bevy default features need `libudev-dev` (gamepad) which
> may not be installed. This workspace uses minimal Bevy features (no
> `bevy_gilrs`) + `mp3` support, so it builds without sudo. If you install
> `libudev-dev`, you can switch back to `bevy = "0.16"` with defaults.

## How a rally plays

- Serve with `S` from wherever you stand; the ball starts at your paddle.
- On the opponent's serve you first get a quiet beat after the score, then
  his stance call and a 3-bip, 2-second countdown before the ball — enough
  time to hear the score and get ready.
- The bot announces its lane ("Mingea vine spre …") and answers the side
  you're on — camp in one lane and it wrong-foots you to the far corner.
- Answer only after the ball crosses the net ("Prea devreme" otherwise),
  and **be where it lands**: contact quality comes from paddle-to-ball
  distance, capped by reaction time after the "Acum lovește!" cue.
  Swinging from over 120px away whiffs outright ("Erai prea departe").
- `O` tells you where the opponent is at any moment (lower octave blip,
  never confused with your own lane blip).

## UI layout

- 1160×800 window: the court stays centered-left, and all text lives in a
  dark side panel docked right (268px) — nothing is drawn over the game.
- Panel (top→bottom): title, scoreboard, status/announcement, shot,
  phase, mode/difficulty/volumes line, key help. The only world-space
  text is the BABILON OPEN banner — no labels over the court, athletes
  stay readable by color (galben = tu, roșu = adversar) and sound.
- TV score bug, top-left over the free background: serve square + TU/CPU
  rows with SET / GAME / PCT columns (gold square = server, AD/40/tie-break
  numbers like the broadcasts). Visual only for screen readers — the same
  score lives in the right panel's browsable label, so no extra tab stop.
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

## Release

Tag-driven, version-checked: bump `babilon-desktop/Cargo.toml`, then

```sh
git tag v0.5.0
git push origin v0.5.0
```

The `release` workflow (`.github/workflows/release.yml`) fails if the tag
doesn't match the Cargo version, builds the Windows exe, and publishes
`BabilonTennis-v0.5.0-windows-x86_64.zip` (exe + assets + README) on the
tag's GitHub Release, with auto-generated notes. Plain pushes still only
produce the expiring Actions artifact (`windows-build` skips `v*` tags).

## Mobile later — what stays, what changes

Keeps `babilon-core` untouched. New `babilon-mobile` crate (Bevy, same engine):
- touch: swipe = direction, tap = hit, buttons for serve/shot type
- bigger type, haptics on the HIT cue, first-tap audio unlock
- reuse `app::build_app` for the same schedule; swap `text.rs` for another
  language and provide a mobile `audio_service` (or none at all)
- `cargo apk` (Android) / Xcode (iOS) builds from the same Bevy app

## Sound (`sound.rs` events → `audio.rs` service)

Gameplay emits `Sound` events (synth / cue / clip / umpire / guide /
schedule) through `commands`; `audio_service` is the only system that
touches rodio, applying group volumes, Access/Realist behaviour, stereo
swap, ambience and the delayed-cue queue. Headless tests capture those
events without any audio device.

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
  Startup opens the OS default output directly: enumerating every ALSA
  device can block on the JACK plugin when no JACK server is running
  ("Cannot connect to server request channel"), so that probe is opt-in
  via `BABILON_AUDIO_DEBUG=1`. A missing ambience sink only disables the
  crowd loop, never the one-shot SFX/umpire clips.
* Keys: `G` difficulty (Ușor/Normal/Greu — harder bots read serves
  better, eat more of your timing, counter weak balls, and hunt the far
  corner from where you stand), `M` audio mode
  (accesibil/realist — Realist drops the guide beep, keeps a soft cue),
  `C` volume group (minge/arbitru/public), `Z`/`X` ∓10%, `T` stadium test,
  `N` stereo swap.

## Bluetooth / output devices

`rodio` talks to raw ALSA and opens the OS *default* device — on Linux a
Bluetooth headset routed through PipeWire/PulseAudio is often *not* the
ALSA default, so the game plays through the speakers instead. In-game
device switching was tried and removed: cycling raw `hw:` outputs is
exclusive-access with no format conversion and segfaulted an HDMI driver
here (exit 139). If you hear nothing or the wrong speakers, set the right
output as the OS default before starting (on Linux: `pactl list sinks`,
`pactl set-default-sink …`, or the desktop sound settings; installing the
`pipewire-alsa` bridge helps), then restart the game.

* Tip: keep the headset in music mode (A2DP). If its microphone is active
  the headset drops to 8 kHz mono (HSP/HFP) and everything sounds dull —
  that is a Bluetooth limitation, not a game bug.

## Running in a VM (VMware/VirtualBox black screen or crash)

Virtual GPUs (e.g. VMware SVGA3D through old Mesa) often top out at
OpenGL 4.1, but Bevy builds a compute-shader pipeline at startup
(compute needs GL 4.3+), so the game dies during init with
`ComputePipeline ... doesn't support Features(COMPUTE_SHADER ...)`.
Workarounds, first to last:

```powershell
# 1. Stay on GL but take Bevy's no-compute path (2D game renders fine):
$env:WGPU_SETTINGS_PRIO = 'webgl2'
cargo run -p babilon-desktop
```

```powershell
# 2. Try native DirectX 12 instead of GL (needs a DX12-capable guest):
$env:WGPU_BACKEND = 'dx12'
cargo run -p babilon-desktop
```

3. Upgrade the guest: latest VMware Tools / Guest Additions, virtual
   hardware to the newest version, 3D acceleration ON, graphics memory
   up (128MB+). Newer SVGA3D exposes DX11/DX12 and newer GL.
4. If it still fails, paste the last 20 log lines — the line *after* the
   wgpu error names the real failure.

## Gaps vs web 0.5 (next steps)

* Real rally-hit MP3 (web currently synthesizes it too — ours is a faithful
  synth crack + thump).
* TTS fallback for tie-break scores (currently a console line + English MP3s
  where they exist).
* High-contrast / font-size options.
* Menu / settings / results screens: `AppState` and `build_app` already
  gate gameplay, so these are new states + UI layers, not a rewrite.
