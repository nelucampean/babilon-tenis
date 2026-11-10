//! Stadium sound for Babilon Tennis: stereo-positioned synth SFX
//! (guide / cue / racket / bounce / win-lose jingles, mirroring the
//! WebAudio synth in `index.html`) plus the 37 umpire/crowd MP3s and a
//! looping crowd ambience — all through `rodio`.
//!
//! Volume groups (minge / arbitru / public) and audio mode (accesibil /
//! realist) live in `Settings`; gameplay emits `Sound` events and
//! `audio_service` resolves them against those settings and the engine.

use crate::settings::Settings;
use crate::sound::Sound;

use bevy::ecs::change_detection::DetectChanges;
use bevy::prelude::{EventReader, Res, ResMut, Resource, Time};
use rodio::buffer::SamplesBuffer;
use rodio::cpal::traits::{DeviceTrait, HostTrait};
use rodio::source::Source;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sample, Sink};
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::time::Duration;

// ---- synth sources (mono 44.1k, WebAudio params from index.html) -----------

const RATE: u32 = 44100;

/// Raised-edge length for every synth blip (rodio's own `fade_out` ramps
/// the FIRST milliseconds and then clamps to silence, which turns every
/// sound into a 5ms pop — so fades are built into the sources instead).
const FADE_SECS: f32 = 0.005;

fn fade_n() -> u64 {
    (FADE_SECS * RATE as f32) as u64
}

/// Sine with a linear frequency slide. `from`->`to` over `slide_over` secs.
/// `total` bounds the sound (it fades out over the last `fade_n` samples);
/// 0 means infinite (no fade-out — the caller must bound it).
pub struct Tone {
    pos: u64,
    phase: f32,
    from: f32,
    to: f32,
    slide_over: f32,
    total: u64,
    fade_n: u64,
}

impl Tone {
    fn new(from: f32, to: f32) -> Self {
        Self {
            pos: 0,
            phase: 0.0,
            from,
            to,
            slide_over: 0.25,
            total: 0,
            fade_n: fade_n(),
        }
    }

    fn edge_gain(&self) -> f32 {
        let mut g = 1.0;
        if self.pos < self.fade_n {
            g = self.pos as f32 / self.fade_n as f32;
        }
        if self.total > 0 {
            let remaining = self.total.saturating_sub(self.pos);
            if remaining < self.fade_n {
                g = g.min(remaining as f32 / self.fade_n as f32);
            }
        }
        g
    }
}

impl Iterator for Tone {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        let t = self.pos as f32 / RATE as f32;
        let k = (t / self.slide_over).clamp(0.0, 1.0);
        let f = self.from + (self.to - self.from) * k;
        self.phase += 2.0 * std::f32::consts::PI * f / RATE as f32;
        let g = self.edge_gain();
        self.pos += 1;
        // Full-scale oscillator like the web's; recipe amps set the mix.
        Some(self.phase.sin() * g)
    }
}

impl Source for Tone {
    fn channels(&self) -> u16 {
        1
    }
    fn sample_rate(&self) -> u32 {
        RATE
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
    fn current_frame_len(&self) -> Option<usize> {
        None
    }
}

/// Band-passed noise burst (one-pole lowpass pair). Mirrors the web
/// `noiseBuffer` + biquad bandpass used for hit/bounce.
pub struct Burst {
    rng: u32,
    lp_hi: f32,
    lp_lo: f32,
    cutoff_hi: f32,
    cutoff_lo: f32,
    pos: u64,
    total: u64,
    fade_n: u64,
}

impl Burst {
    fn new(cutoff_hi: f32, cutoff_lo: f32) -> Self {
        Self {
            rng: 0x12345678,
            lp_hi: 0.0,
            lp_lo: 0.0,
            cutoff_hi,
            cutoff_lo,
            pos: 0,
            total: 0,
            fade_n: fade_n(),
        }
    }
    fn step(&mut self) -> f32 {
        // xorshift white noise in [-1, 1]
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        let w = (self.rng as f32 / u32::MAX as f32) * 2.0 - 1.0;
        let dt = 1.0 / RATE as f32;
        let ah = (2.0 * std::f32::consts::PI * self.cutoff_hi * dt).min(1.0);
        let al = (2.0 * std::f32::consts::PI * self.cutoff_lo * dt).min(1.0);
        self.lp_hi += ah * (w - self.lp_hi);
        self.lp_lo += al * (w - self.lp_lo);
        // Band-passed noise roughly in [-1, 1] like the web's biquad —
        // no extra boost (it clipped harshly before).
        self.lp_hi - self.lp_lo
    }
    fn edge_gain(&self) -> f32 {
        let mut g = 1.0;
        if self.pos < self.fade_n {
            g = self.pos as f32 / self.fade_n as f32;
        }
        if self.total > 0 {
            let remaining = self.total.saturating_sub(self.pos);
            if remaining < self.fade_n {
                g = g.min(remaining as f32 / self.fade_n as f32);
            }
        }
        g
    }
}

impl Iterator for Burst {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        let g = self.edge_gain();
        self.pos += 1;
        Some(self.step() * g)
    }
}

impl Source for Burst {
    fn channels(&self) -> u16 {
        1
    }
    fn sample_rate(&self) -> u32 {
        RATE
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
    fn current_frame_len(&self) -> Option<usize> {
        None
    }
}

// ---- sound recipes (WebAudio params, index.html `sound()`) -----------------

fn tone(from: f32, to: f32, secs: f32, amp: f32) -> impl Source<Item = f32> + Send + 'static {
    // 5ms raised edges: synth cut mid-waveform otherwise, which the ear
    // hears as a pop/click on every blip.
    let mut t = Tone::new(from, to);
    t.total = (secs * RATE as f32) as u64;
    t.amplify(amp).take_duration(Duration::from_secs_f32(secs))
}

fn burst(hi: f32, lo: f32, secs: f32, amp: f32) -> impl Source<Item = f32> + Send + 'static {
    let mut b = Burst::new(hi, lo);
    b.total = (secs * RATE as f32) as u64;
    b.amplify(amp).take_duration(Duration::from_secs_f32(secs))
}

/// Racket hit: noise crack + low thump, like the web `hit`.
fn hit_src() -> impl Source<Item = f32> + Send + 'static {
    burst(1450.0, 500.0, 0.09, 0.32).mix(tone(185.0, 127.0, 0.09, 0.18))
}

fn bounce_src() -> impl Source<Item = f32> + Send + 'static {
    burst(750.0, 250.0, 0.11, 0.24)
}

/// Which synth to play.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Synth {
    Guide,
    Cue,
    Hit,
    Bounce,
    Good,
    Bad,
    /// Position blip when YOU move: pitch + pan both encode the lane, so it
    /// reads even on one working ear / one speaker.
    Lane(i8),
    /// Opponent locator (O key): same lane coding one octave lower plus a
    /// longer tail, so you can never confuse it with your own blip.
    Foe(i8),
}

/// Lane pitch, ascending left to right (Eb4 F4 G4 A4 B4): your ears get
/// a second, speaker-proof position cue next to the stereo pan.
pub fn lane_freq(dir: i8) -> f32 {
    match dir {
        d if d <= -2 => 311.13,
        -1 => 349.23,
        0 => 392.0,
        1 => 440.0,
        _ => 493.88,
    }
}

// ---- volume groups + audio mode --------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum VolGroup {
    Ball,
    Umpire,
    Crowd,
}

impl VolGroup {
    pub fn label_ro(self) -> &'static str {
        match self {
            VolGroup::Ball => "minge",
            VolGroup::Umpire => "arbitru",
            VolGroup::Crowd => "public",
        }
    }
    pub fn next(self) -> Self {
        match self {
            VolGroup::Ball => VolGroup::Umpire,
            VolGroup::Umpire => VolGroup::Crowd,
            VolGroup::Crowd => VolGroup::Ball,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Resource)]
pub struct Volumes {
    pub ball: f32,
    pub umpire: f32,
    pub crowd: f32,
    pub group: VolGroup,
}

impl Default for Volumes {
    fn default() -> Self {
        // Same defaults as the web soundboard: 100 / 100 / 65.
        Self {
            ball: 1.0,
            umpire: 1.0,
            crowd: 0.65,
            group: VolGroup::Ball,
        }
    }
}

impl Volumes {
    pub fn adjust(&mut self, delta: f32) -> (VolGroup, f32) {
        let v = match self.group {
            VolGroup::Ball => &mut self.ball,
            VolGroup::Umpire => &mut self.umpire,
            VolGroup::Crowd => &mut self.crowd,
        };
        *v = (*v + delta).clamp(0.0, 2.0);
        (self.group, *v)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Resource)]
pub enum AudioMode {
    Access,
    Real,
}

impl AudioMode {
    pub fn label_ro(self) -> &'static str {
        match self {
            AudioMode::Access => "accesibil",
            AudioMode::Real => "realist",
        }
    }
}

// ---- engine ----------------------------------------------------------------
// `rodio::OutputStream` is `!Send`, so all audio lives on a dedicated
// thread. Game systems only hold an `mpsc::Sender` (cheap, `Send + Sync`),
// and every call below is skip-safe when the engine is absent (headless).

/// Replace ALSA's chatty stderr reporter with silence. Card probing
/// (`/dev/dsp` missing, dmix/dsnoop capability notes, channel-map
/// mismatches) is normal on every enumerate/open — without this it reads
/// like a crash. Called once, on the audio thread, before first use.
#[cfg(target_os = "linux")]
fn silence_alsa_chatter() {
    unsafe extern "C" fn noop(
        _file: *const std::os::raw::c_char,
        _line: std::os::raw::c_int,
        _func: *const std::os::raw::c_char,
        _err: std::os::raw::c_int,
        _fmt: *const std::os::raw::c_char,
        mut _args: ...
    ) {
    }
    unsafe {
        alsa_sys::snd_lib_error_set_handler(Some(noop));
    }
}

/// Crowd ambience gain, mirroring `updateAmbient()` in index.html.
pub fn ambient_gain(crowd: f32) -> f32 {
    (crowd * 0.28).min(0.42)
}

#[derive(Debug)]
enum Cmd {
    Synth { kind: Synth, pan: f32, vol: f32 },
    Clip { key: String, pan: f32, vol: f32 },
    AmbientVol(f32),
}

#[derive(Debug, Resource)]
pub struct Sfx {
    tx: std::sync::mpsc::Sender<Cmd>,
    swap: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

fn asset_dirs() -> Vec<std::path::PathBuf> {
    // `cargo run -p babilon-desktop` from the workspace root vs. a packaged
    // `exe` next to `assets/`.
    ["babilon-desktop/assets/sunete", "assets/sunete"]
        .iter()
        .map(std::path::PathBuf::from)
        .collect()
}

fn load_mono(path: &std::path::Path) -> Option<SamplesBuffer<i16>> {
    let file = File::open(path).ok()?;
    let dec = Decoder::new(BufReader::new(file)).ok()?;
    let (ch, rate) = (dec.channels(), dec.sample_rate());
    let raw: Vec<i16> = dec.convert_samples().collect();
    let mono: Vec<i16> = if ch >= 2 {
        raw.chunks_exact(ch as usize)
            .map(|f| (f.iter().map(|&s| s as i32).sum::<i32>() / ch as i32) as i16)
            .collect()
    } else {
        raw
    };
    if mono.is_empty() {
        return None;
    }
    Some(SamplesBuffer::new(1, rate, mono))
}

/// One-shot MP3 with raised edges baked in (same 5ms rule as the synths:
/// a hard cut mid-waveform is heard as a pop). The looping ambience keeps
/// its raw buffer so the loop point stays untouched.
fn faded_clip(buf: &SamplesBuffer<i16>, fade_n: u64) -> SamplesBuffer<i16> {
    let rate = buf.sample_rate();
    let mut raw: Vec<i16> = buf.clone().collect();
    let n = (fade_n as usize).min(raw.len() / 2);
    for (i, s) in raw.iter_mut().enumerate().take(n) {
        let g = i as f32 / n as f32;
        *s = (*s as f32 * g) as i16;
    }
    for (i, s) in raw.iter_mut().rev().enumerate().take(n) {
        let g = i as f32 / n as f32;
        *s = (*s as f32 * g) as i16;
    }
    SamplesBuffer::new(1, rate, raw)
}

fn sink_for(handle: &OutputStreamHandle) -> Option<Sink> {
    // Volume lives in the sample stream (see `with_vol`), not the sink —
    // so it is unit-testable and independent of backend quirks. Sinks stay
    // at their default gain of 1.0.
    Sink::try_new(handle).ok()
}

/// Apply the group volume inside the DSP chain (sample-exact, testable).
fn with_vol<S>(src: S, vol: f32) -> rodio::source::Amplify<S>
where
    S: Source,
    S::Item: Sample,
{
    src.amplify(vol.clamp(0.0, 2.0))
}

/// Constant-power stereo gains. pan < 0 is LEFT, pan > 0 is RIGHT —
/// unlike the old distance model this cannot flip: full left is
/// hard-left, center is equal power, full right is hard-right.
pub fn pan_gains(pan: f32) -> (f32, f32) {
    let a = (pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
    (a.cos(), a.sin())
}

/// Mono → stereo adapter with an exact pan position.
/// Every mono sample is emitted TWICE (left scaled, then right scaled) —
/// emitting it once per channel would halve the duration (2x speed).
pub struct Pan<S> {
    input: S,
    left: f32,
    right: f32,
    pending_right: Option<f32>,
}

impl<S> Pan<S> {
    fn new(input: S, pan: f32) -> Self {
        let (left, right) = pan_gains(pan);
        Self {
            input,
            left,
            right,
            pending_right: None,
        }
    }
}

impl<S> Iterator for Pan<S>
where
    S: Source<Item = f32>,
{
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if let Some(r) = self.pending_right.take() {
            return Some(r);
        }
        let s = self.input.next()?;
        self.pending_right = Some(s * self.right);
        Some(s * self.left)
    }
}

impl<S> Source for Pan<S>
where
    S: Source<Item = f32>,
{
    fn channels(&self) -> u16 {
        2
    }
    fn sample_rate(&self) -> u32 {
        self.input.sample_rate()
    }
    fn total_duration(&self) -> Option<Duration> {
        self.input.total_duration()
    }
    fn current_frame_len(&self) -> Option<usize> {
        if self.pending_right.is_some() {
            Some(1)
        } else {
            self.input.current_frame_len().map(|n| n * 2)
        }
    }
}

fn play_synth(handle: &OutputStreamHandle, kind: Synth, pan: f32, vol: f32) {
    let Some(sink) = sink_for(handle) else { return };
    let src: Box<dyn Source<Item = f32> + Send> = match kind {
        Synth::Guide => Box::new(Pan::new(with_vol(tone(475.0, 580.0, 0.16, 0.16), vol), pan)),
        Synth::Cue => Box::new(Pan::new(with_vol(tone(900.0, 740.0, 0.21, 0.26), vol), pan)),
        Synth::Hit => Box::new(Pan::new(with_vol(hit_src(), vol), pan)),
        Synth::Bounce => Box::new(Pan::new(with_vol(bounce_src(), vol), pan)),
        Synth::Good => Box::new(Pan::new(with_vol(tone(760.0, 900.0, 0.23, 0.17), vol), pan)),
        Synth::Bad => Box::new(Pan::new(with_vol(tone(230.0, 150.0, 0.22, 0.15), vol), pan)),
        Synth::Lane(d) => {
            let f = lane_freq(d);
            Box::new(Pan::new(with_vol(tone(f, f, 0.09, 0.14), vol), pan))
        }
        Synth::Foe(d) => {
            let f = lane_freq(d) / 2.0;
            Box::new(Pan::new(with_vol(tone(f, f, 0.12, 0.14), vol), pan))
        }
    };
    sink.append(src);
    sink.detach();
}

fn open_output(
    crowd: f32,
    clips: &HashMap<String, SamplesBuffer<i16>>,
) -> Option<(OutputStream, OutputStreamHandle, Sink)> {
    // Always the OS default device (no in-game switching).
    eprintln!("[AUDIO] opening default output...");
    let (stream, handle) = match OutputStream::try_default() {
        Ok(x) => x,
        Err(e) => {
            eprintln!("[AUDIO] no output device ({e}). Game stays silent; on Linux check pipewire-alsa/PulseAudio.");
            return None;
        }
    };
    eprintln!("[AUDIO] stream open, creating ambient sink...");
    let ambient = Sink::try_new(&handle).ok()?;
    ambient.set_volume(ambient_gain(crowd));
    if let Some(buf) = clips.get("ambient") {
        ambient.append(buf.clone().repeat_infinite());
    }
    eprintln!("[AUDIO] output: default (ambient looping)");
    Some((stream, handle, ambient))
}

fn audio_thread(rx: std::sync::mpsc::Receiver<Cmd>, crowd: f32) {
    eprintln!("[AUDIO] audio thread started");
    let dir = asset_dirs().into_iter().find(|d| d.is_dir());
    let mut clips = HashMap::new();
    if let Some(dir) = dir {
        for key in babilon_core::ALL_CLIP_KEYS {
            if let Some(file) = babilon_core::clip_filename(key) {
                if let Some(buf) = load_mono(&dir.join(file)) {
                    clips.insert(key.to_string(), buf);
                }
            }
        }
    } else {
        eprintln!("[AUDIO] sound folder not found (looked for babilon-desktop/assets/sunete and assets/sunete)");
    }
    eprintln!("[AUDIO] decoded {} clips", clips.len());
    eprintln!("[AUDIO] devices: {:?}", list_output_devices());
    let audio = open_output(crowd, &clips);
    eprintln!("[AUDIO] entering command loop");
    for cmd in rx {
        match cmd {
            Cmd::Synth { kind, pan, vol } => {
                if let Some((_, handle, _)) = &audio {
                    play_synth(handle, kind, pan, vol);
                }
            }
            Cmd::Clip { key, pan, vol } => {
                if let (Some(buf), Some((_, handle, _))) = (clips.get(&key), &audio) {
                    if let Some(sink) = sink_for(handle) {
                        sink.append(Box::new(Pan::new(
                            with_vol(faded_clip(buf, fade_n()).convert_samples::<f32>(), vol),
                            pan,
                        ))
                            as Box<dyn Source<Item = f32> + Send>);
                        sink.detach();
                    }
                }
            }
            Cmd::AmbientVol(v) => {
                if let Some((_, _, ambient)) = &audio {
                    ambient.set_volume(ambient_gain(v));
                }
            }
        }
    }
}

impl Sfx {
    pub fn start(crowd: f32) -> Self {
        #[cfg(target_os = "linux")]
        silence_alsa_chatter();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || audio_thread(rx, crowd));
        let sfx = Self {
            tx,
            swap: Default::default(),
        };
        sfx.refresh_ambient(crowd);
        sfx
    }

    fn send(&self, cmd: Cmd) {
        let _ = self.tx.send(cmd);
    }

    /// Swap left/right channels (swapped headset, OS remap, single-sided
    /// setups). Applies to every positioned sound from here on.
    pub fn set_swapped(&self, swapped: bool) {
        self.swap
            .store(swapped, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn swapped(&self) -> bool {
        self.swap.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn placed(&self, pan: f32) -> f32 {
        if self.swapped() {
            -pan
        } else {
            pan
        }
    }

    pub fn refresh_ambient(&self, crowd: f32) {
        self.send(Cmd::AmbientVol(crowd));
    }

    /// Positioned synth SFX. `sfx` may be `None` headless — silently skipped.
    pub fn synth(sfx: Option<&Self>, kind: Synth, pan: f32, vol: f32) {
        if let Some(s) = sfx {
            let pan = s.placed(pan);
            s.send(Cmd::Synth { kind, pan, vol });
        }
    }

    /// Positioned MP3 clip (`sunete/*.mp3`), mono-ized at load so panning is exact.
    pub fn clip(sfx: Option<&Self>, key: &str, pan: f32, vol: f32) {
        if let Some(s) = sfx {
            let pan = s.placed(pan);
            s.send(Cmd::Clip {
                key: key.to_string(),
                pan,
                vol,
            });
        }
    }

    /// Umpire voice, always centered. Tie-break scores have no MP3 (same as
    /// the web version) — the caller falls back to TTS/`println!`.
    pub fn umpire(sfx: Option<&Self>, call: &babilon_core::UmpireCall, vol: f32) {
        if let Some(key) = &call.clip_key {
            Self::clip(sfx, key, 0.0, vol);
        }
    }
}

// ---- output device diagnostics --------------------------------------------
// The game always uses the OS default device (no in-game switching).
// Kept as diagnostics: logged once at startup on the audio thread.

/// All OS output devices by name. Empty when the backend can't enumerate
/// (some Linux setups without pipewire-alsa/PulseAudio routing).
pub fn list_output_devices() -> Vec<String> {
    match rodio::cpal::default_host().output_devices() {
        Ok(devs) => devs.filter_map(|d| d.name().ok()).collect(),
        Err(e) => {
            eprintln!("[AUDIO] cannot list output devices ({e})");
            Vec::new()
        }
    }
}

/// Raw `hw:` outputs were removed from the old device cycler: exclusive
/// access with no format conversion segfaulted an HDMI driver here (exit
/// 139). Kept as documentation + unit test of the rule.
#[allow(dead_code)]
fn is_unsafe_output(name: &str) -> bool {
    name.starts_with("hw:")
        || name.starts_with("dsnoop")
        || name.starts_with("oss")
        || name == "null"
}

// ---- delayed cues (the web `setTimeout(..., 490)` guide beep) --------------

pub struct Scheduled {
    pub at: f32,
    pub kind: Synth,
    pub pan: f32,
    pub vol: f32,
}

#[derive(Resource, Default)]
pub struct Pending(pub Vec<Scheduled>);

/// Which volume group a cue belongs to (`C` cycles these in the UI).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bus {
    Ball,
    Umpire,
    Crowd,
}

/// Resolve a sound request against the current settings and engine.
///
/// This is the *only* system that knows about rodio: gameplay sends
/// [`Sound`] events, the service applies group volumes, Access/Realist
/// mode, stereo swap, ambience and the delayed-cue queue. Without an
/// `Sfx` resource (headless tests) every request is safely dropped, while
/// the queue and events still behave exactly as in the real app.
pub(crate) fn audio_service(
    mut sounds: EventReader<Sound>,
    time: Res<Time>,
    settings: Res<Settings>,
    sfx: Option<Res<Sfx>>,
    mut pending: ResMut<Pending>,
) {
    // Settings are the single source of truth for stereo swap and crowd
    // ambience; mirror any change onto the engine once per changed frame.
    if settings.is_changed() {
        if let Some(s) = sfx.as_deref() {
            s.set_swapped(settings.swap_stereo);
            s.refresh_ambient(settings.vols.crowd);
        }
    }
    let vol = |bus: Bus| match bus {
        Bus::Ball => settings.vols.ball,
        Bus::Umpire => settings.vols.umpire,
        Bus::Crowd => settings.vols.crowd,
    };
    let sfx = sfx.as_deref();

    // Fire due scheduled beeps (the web `setTimeout` cues).
    let now = time.elapsed_secs();
    pending.0.retain(|p| {
        if now >= p.at {
            Sfx::synth(sfx, p.kind, p.pan, p.vol);
            false
        } else {
            true
        }
    });

    for ev in sounds.read() {
        match ev {
            Sound::Synth { kind, pan, bus } => Sfx::synth(sfx, *kind, *pan, vol(*bus)),
            Sound::Cue { pan } => {
                let amp = vol(Bus::Ball)
                    * if settings.mode == AudioMode::Access {
                        1.0
                    } else {
                        0.4
                    };
                Sfx::synth(sfx, Synth::Cue, *pan, amp);
            }
            Sound::Clip { key, pan, bus } => Sfx::clip(sfx, key, *pan, vol(*bus)),
            Sound::Umpire(call) => {
                if call.clip_key.is_some() {
                    Sfx::umpire(sfx, call, vol(Bus::Umpire));
                } else {
                    // Tie-break scores have no MP3: console TTS fallback.
                    println!("[ARBITRU TTS] {}", call.text_en);
                }
            }
            Sound::Guide { at, pan } => {
                if settings.mode == AudioMode::Access {
                    pending.0.push(Scheduled {
                        at: *at,
                        kind: Synth::Guide,
                        pan: *pan,
                        vol: vol(Bus::Ball),
                    });
                }
            }
            Sound::Schedule { at, kind, pan, bus } => pending.0.push(Scheduled {
                at: *at,
                kind: *kind,
                pan: *pan,
                vol: vol(*bus),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gain_groups_and_adjust() {
        assert!((ambient_gain(1.0) - 0.28).abs() < 1e-6);
        assert!((ambient_gain(2.0) - 0.42).abs() < 1e-6);
        assert_eq!(VolGroup::Ball.next(), VolGroup::Umpire);
        assert_eq!(VolGroup::Umpire.next(), VolGroup::Crowd);
        assert_eq!(VolGroup::Crowd.next(), VolGroup::Ball);
        let mut v = Volumes::default();
        assert!((v.ball - 1.0).abs() < 1e-6);
        let (g, x) = v.adjust(0.1);
        assert_eq!(g, VolGroup::Ball);
        assert!((x - 1.1).abs() < 1e-6);
        let (_, x) = v.adjust(-5.0);
        assert!((x - 0.0).abs() < 1e-6);
    }

    #[test]
    fn raw_hw_and_capture_devices_are_filtered() {
        for bad in [
            "hw:CARD=HDMI,DEV=8",
            "hw:CARD=PCH,DEV=0",
            "dsnoop:CARD=PCH,DEV=0",
            "oss",
            "null",
        ] {
            assert!(is_unsafe_output(bad), "{bad}");
        }
        for good in [
            "default",
            "pulse",
            "pipewire",
            "plughw:CARD=HDMI,DEV=3",
            "hdmi:CARD=HDMI,DEV=1",
            "dmix:CARD=PCH,DEV=0",
            "front:CARD=PCH,DEV=0",
        ] {
            assert!(!is_unsafe_output(good), "{good}");
        }
    }

    #[test]
    fn pan_is_exact_and_full_range() {
        let (l, r) = pan_gains(-1.0);
        assert!((l - 1.0).abs() < 1e-6 && r.abs() < 1e-6);
        let (l, r) = pan_gains(1.0);
        assert!(l.abs() < 1e-6 && (r - 1.0).abs() < 1e-6);
        let (l, r) = pan_gains(0.0);
        assert!((l - r).abs() < 1e-6 && (l - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-3);
        // constant power: total energy is flat across the field
        for p in [-1.0, -0.5, 0.0, 0.5, 1.0] {
            let (l, r) = pan_gains(p);
            assert!(((l * l + r * r) - 1.0).abs() < 1e-6, "{p}");
        }
    }

    #[test]
    fn lane_pitch_rises_left_to_right() {
        let fs: Vec<f32> = [-2, -1, 0, 1, 2].iter().map(|&d| lane_freq(d)).collect();
        for w in fs.windows(2) {
            assert!(w[0] < w[1], "{fs:?}");
        }
        assert_eq!(lane_freq(-9), lane_freq(-2));
        assert_eq!(lane_freq(9), lane_freq(2));
    }

    #[test]
    fn volume_scales_output_amplitude() {
        // The Z/X keys must change loudness, not just a number on screen.
        let peak = |vol: f32| {
            with_vol(tone(440.0, 440.0, 0.2, 0.5), vol)
                .take(2000)
                .map(|s: f32| s.abs())
                .fold(0.0f32, f32::max)
        };
        let full = peak(1.0);
        assert!(full > 0.2, "inaudible at full volume: {full}");
        let half = peak(0.5);
        assert!((half / full - 0.5).abs() < 0.05, "{half} vs {full}");
        assert_eq!(peak(0.0), 0.0);
    }

    #[test]
    fn pan_channel_order_is_exact() {
        // Hard left: sound ONLY on even (left) slots; hard right: mirrored.
        // Proves the previous "reversed stereo" report can't come from here.
        let left: Vec<f32> = Pan::new(tone(440.0, 440.0, 0.02, 1.0), -1.0).collect();
        assert!(!left.is_empty() && left.len().is_multiple_of(2));
        for (i, s) in left.iter().enumerate() {
            if i % 2 == 1 {
                assert!(s.abs() < 1e-6, "right leaks on hard-left pan: {s}");
            }
        }
        assert!(left.iter().step_by(2).any(|s| s.abs() > 0.1));
        let right: Vec<f32> = Pan::new(tone(440.0, 440.0, 0.02, 1.0), 1.0).collect();
        for (i, s) in right.iter().enumerate() {
            if i % 2 == 0 {
                assert!(s.abs() < 1e-4, "left leaks on hard-right pan: {s}");
            }
        }
        assert!(right.iter().skip(1).step_by(2).any(|s| s.abs() > 0.1));
    }

    #[test]
    fn synth_edges_start_and_end_at_silence() {
        // Abrupt edges are heard as pop/click on every blip. 5ms raised
        // edges in, silence out (first sample is exactly 0).
        for (name, mut src) in [
            (
                "tone",
                Box::new(tone(440.0, 880.0, 0.16, 1.0)) as Box<dyn Source<Item = f32>>,
            ),
            (
                "bounce",
                Box::new(bounce_src()) as Box<dyn Source<Item = f32>>,
            ),
            ("hit", Box::new(hit_src()) as Box<dyn Source<Item = f32>>),
        ] {
            let first = src.next().unwrap_or(1.0);
            assert!(first.abs() < 1e-6, "{name} click at start: {first}");
            let mut last = 0.0;
            for s in src.by_ref() {
                last = s;
            }
            assert!(last.abs() < 0.05, "{name} click at end: {last}");
        }
    }

    #[test]
    fn sounds_survive_past_the_first_5ms() {
        // Regression: rodio's own `fade_out` ramps the FIRST milliseconds
        // and clamps the rest to silence, turning every sound into a pop.
        // Fades here are raised edges only — the body must stay loud.
        let all: Vec<f32> = tone(440.0, 440.0, 0.2, 1.0).collect();
        let mid = &all[all.len() / 3..2 * all.len() / 3];
        assert!(mid.iter().any(|s| s.abs() > 0.1), "body muted");
    }

    #[test]
    fn clip_fade_pins_both_ends_to_silence() {
        let buf = SamplesBuffer::new(1, 44100, vec![30000i16; 4410]);
        let faded = faded_clip(&buf, 220);
        let v: Vec<i16> = faded.into_iter().collect();
        assert_eq!(v.len(), 4410);
        assert_eq!(v[0], 0);
        assert_eq!(v[v.len() - 1], 0);
        assert!(v[v.len() / 2].abs() > 20000, "fade must not touch the body");
    }

    #[test]
    fn synth_lengths_match_their_durations() {
        // 0.16s @44100 mono -> 7056 samples -> 14112 stereo samples.
        // Any off-by-N here would mean wrong playback speed.
        let n = Pan::new(tone(440.0, 440.0, 0.16, 1.0), 0.0).count();
        assert_eq!(n, 7056 * 2, "{n}");
        let n = Pan::new(hit_src(), 0.0).count();
        assert_eq!(n, 3969 * 2, "{n}");
        let n = Pan::new(bounce_src(), 0.0).count();
        assert_eq!(n, 4851 * 2, "{n}");
    }

    #[test]
    fn swap_toggle_roundtrips_without_audio_device() {
        // No audio hardware needed: the flag lives on the handle side.
        let sfx = Sfx::start(0.65);
        assert!(!sfx.swapped());
        sfx.set_swapped(true);
        assert!(sfx.swapped());
        sfx.set_swapped(false);
        assert!(!sfx.swapped());
    }

    #[test]
    fn pan_duplicates_each_sample_to_both_channels() {
        // Center pan: every L/R pair must carry the SAME mono sample
        // (splitting samples across channels instead halves durations).
        let out: Vec<f32> = Pan::new(tone(440.0, 440.0, 0.02, 1.0), 0.0).collect();
        assert_eq!(out.len() % 2, 0);
        for pair in out.chunks_exact(2) {
            assert!((pair[0] - pair[1]).abs() < 1e-6, "{pair:?}");
        }
    }
}
