//! Shared presentation assets: the bundled typeface and (later) palette.
use bevy::prelude::*;

/// Bundled UI typeface (Inter, full coverage incl. Romanian ș/ț/ă/â/î).
/// Loaded once in `setup`; every HUD/world label uses it.
#[derive(Resource)]
pub(crate) struct UiFonts {
    pub(crate) regular: Handle<Font>,
    pub(crate) bold: Handle<Font>,
}
