//! Sound requests emitted by gameplay.
//!
//! Gameplay systems never touch rodio, group volumes, audio mode, the
//! stereo-swap flag or the delayed-cue queue. They send a [`Sound`] event
//! (via the [`SoundExt`] helpers on `Commands`); [`audio::audio_service`]
//! is the single place that resolves those requests against `Settings`
//! and the `Sfx` engine. This keeps the rules testable headless and makes
//! future backends (TTS, mute, replay) a one-system change.
use crate::audio::{Bus, Synth};
use babilon_core::UmpireCall;
use bevy::prelude::*;

/// A logical "play this" request, before volumes/mode/swap are applied.
#[derive(Event, Debug, Clone, PartialEq)]
pub(crate) enum Sound {
    /// Positioned synth blip at the bus's current volume.
    Synth { kind: Synth, pan: f32, bus: Bus },
    /// HIT cue: full volume in Access mode, a discreet hint in Realist.
    Cue { pan: f32 },
    /// Positioned MP3 from `assets/sunete/` at the bus's volume.
    Clip { key: String, pan: f32, bus: Bus },
    /// Umpire voice (MP3, or console TTS when the call has no clip).
    Umpire(UmpireCall),
    /// Mid-flight guide beep; the service drops it in Realist mode.
    Guide { at: f32, pan: f32 },
    /// Delayed cue (e.g. the serve countdown), independent of audio mode.
    Schedule {
        at: f32,
        kind: Synth,
        pan: f32,
        bus: Bus,
    },
}

/// Ergonomic event helpers on `Commands`, so call sites read like the old
/// imperative audio context but only enqueue an event.
pub(crate) trait SoundExt {
    fn synth(&mut self, kind: Synth, pan: f32, bus: Bus);
    fn cue(&mut self, pan: f32);
    fn clip(&mut self, key: &str, pan: f32, bus: Bus);
    fn umpire(&mut self, call: &UmpireCall);
    fn guide(&mut self, at: f32, pan: f32);
    fn schedule(&mut self, at: f32, kind: Synth, pan: f32, bus: Bus);
}

impl<'w, 's> SoundExt for Commands<'w, 's> {
    fn synth(&mut self, kind: Synth, pan: f32, bus: Bus) {
        self.send_event(Sound::Synth { kind, pan, bus });
    }

    fn cue(&mut self, pan: f32) {
        self.send_event(Sound::Cue { pan });
    }

    fn clip(&mut self, key: &str, pan: f32, bus: Bus) {
        self.send_event(Sound::Clip {
            key: key.to_string(),
            pan,
            bus,
        });
    }

    fn umpire(&mut self, call: &UmpireCall) {
        self.send_event(Sound::Umpire(call.clone()));
    }

    fn guide(&mut self, at: f32, pan: f32) {
        self.send_event(Sound::Guide { at, pan });
    }

    fn schedule(&mut self, at: f32, kind: Synth, pan: f32, bus: Bus) {
        self.send_event(Sound::Schedule { at, kind, pan, bus });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Resource, Default)]
    struct Captured(Vec<Sound>);

    fn emit(mut commands: Commands) {
        commands.synth(Synth::Guide, -1.0, Bus::Ball);
        commands.schedule(1.0, Synth::Bounce, 0.0, Bus::Ball);
        commands.clip("applause", 0.0, Bus::Crowd);
    }

    fn collect(mut events: EventReader<Sound>, mut out: ResMut<Captured>) {
        out.0.extend(events.read().cloned());
    }

    #[test]
    fn commands_enqueue_sound_events() {
        let mut app = App::new();
        app.add_event::<Sound>();
        app.init_resource::<Captured>();
        // `.chain()` applies the deferred commands from `emit` before
        // `collect` reads them, exactly like the real Update schedule.
        app.add_systems(Update, (emit, collect).chain());
        app.update();
        let got = &app.world().resource::<Captured>().0;
        assert_eq!(got.len(), 3);
        assert_eq!(
            got[0],
            Sound::Synth {
                kind: Synth::Guide,
                pan: -1.0,
                bus: Bus::Ball
            }
        );
        assert_eq!(
            got[2],
            Sound::Clip {
                key: "applause".to_string(),
                pan: 0.0,
                bus: Bus::Crowd
            }
        );
    }
}
