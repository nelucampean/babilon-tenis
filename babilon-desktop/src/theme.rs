//! Shared presentation assets and palette: the bundled typeface and every
//! colour used by the court, athletes, HUD and scoreboard.
use bevy::prelude::*;

/// Bundled UI typeface (Inter, full coverage incl. Romanian ș/ț/ă/â/î).
/// Loaded once in `setup`; every HUD/world label uses it.
#[derive(Resource)]
pub(crate) struct UiFonts {
    pub(crate) regular: Handle<Font>,
    pub(crate) bold: Handle<Font>,
}

// ---- court -----------------------------------------------------------------

pub(crate) const COURT_SURROUND: Color = Color::srgb(0.03, 0.47, 0.55);
pub(crate) const COURT_SURFACE: Color = Color::srgb(0.06, 0.49, 0.56);
pub(crate) const COURT_BAND_A: Color = Color::srgb(0.05, 0.52, 0.57);
pub(crate) const COURT_BAND_B: Color = Color::srgb(0.04, 0.48, 0.53);
pub(crate) const COURT_LINE: Color = Color::srgb(0.88, 1.0, 0.96);
pub(crate) const NET_BAR: Color = Color::srgb(0.04, 0.13, 0.19);
pub(crate) const NET_SLAT: Color = Color::srgba(0.95, 1.0, 1.0, 0.35);
pub(crate) const BANNER_TEXT: Color = Color::srgb(0.61, 0.91, 0.86);

// ---- athletes --------------------------------------------------------------

pub(crate) const PLAYER_BODY: Color = Color::srgb(0.96, 0.79, 0.42);
pub(crate) const PLAYER_TRIM: Color = Color::srgb(0.66, 0.49, 0.26);
pub(crate) const OPP_BODY: Color = Color::srgb(1.0, 0.43, 0.41);
pub(crate) const OPP_TRIM: Color = Color::srgb(0.72, 0.18, 0.22);
pub(crate) const SKIN: Color = Color::srgb(0.95, 0.73, 0.56);
pub(crate) const KIT_NAVY: Color = Color::srgb(0.08, 0.18, 0.25);
pub(crate) const KIT_WHITE: Color = Color::srgb(0.94, 1.0, 0.97);
pub(crate) const RACKET_FRAME: Color = Color::srgb(0.08, 0.20, 0.30);
pub(crate) const RACKET_STRINGS: Color = Color::srgb(0.97, 1.0, 0.98);

// ---- ball / particles ------------------------------------------------------

pub(crate) const BALL: Color = Color::srgb(0.94, 1.0, 0.38);
pub(crate) const BALL_SEAM: Color = Color::srgb(0.82, 0.9, 0.56);
pub(crate) const BALL_SHADOW: Color = Color::srgb(0.0, 0.15, 0.18);
pub(crate) const SPARK_GOLD: Color = Color::srgb(1.0, 0.89, 0.55);
pub(crate) const SPARK_PALE: Color = Color::srgb(0.92, 1.0, 0.67);

// ---- HUD / scoreboard ------------------------------------------------------

pub(crate) const GOLD: Color = Color::srgb(0.98, 0.79, 0.40);
pub(crate) const INK: Color = Color::srgb(0.93, 0.97, 0.97);
pub(crate) const MUTED: Color = Color::srgb(0.72, 0.82, 0.85);
pub(crate) const PANEL_BG: Color = Color::srgba(0.03, 0.10, 0.16, 0.96);
pub(crate) const BUG_BG: Color = Color::srgba(0.02, 0.06, 0.11, 0.94);
/// Unlit serve square / dimmed TV column.
pub(crate) const DIM: Color = Color::srgba(0.45, 0.55, 0.60, 0.45);
pub(crate) const SERVE_ON: Color = GOLD;
pub(crate) const SERVE_OFF: Color = DIM;
