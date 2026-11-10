//! Top-level app state machine.
//!
//! Gameplay systems run only in [`AppState::Playing`], so future screens
//! (menu, settings, results) can be added as extra variants without
//! remembering to disable every court system. Settings/focus/audio systems
//! stay outside the gate and keep working on every screen.
use bevy::prelude::*;

#[derive(States, Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum AppState {
    /// A live match: gameplay systems run.
    #[default]
    Playing,
    /// Player pressed P: the court freezes, settings still work.
    Paused,
}
