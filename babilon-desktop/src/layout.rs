//! Fixed court, window and panel geometry.
//!
//! Keeping the numbers in one module means the court drawing, the net
//! physics (`NET_Y`), the athletes and the side panel all agree — and a
//! future settings/menu screen can reuse the same window metrics.
/// Default window size (logical pixels).
pub(crate) const WINDOW_W: f32 = 1160.0;
pub(crate) const WINDOW_H: f32 = 800.0;

/// Court rectangle (longitudinal, player at the bottom).
pub(crate) const COURT_W: f32 = 460.0;
pub(crate) const COURT_H: f32 = 560.0;
/// Y of the court centre, and of the net line (`NET_Y = COURT_Y`).
pub(crate) const COURT_Y: f32 = 20.0;
pub(crate) const NET_Y: f32 = COURT_Y;

/// Blue surround beyond the court surface.
pub(crate) const SURROUND_W: f32 = COURT_W + 70.0;
pub(crate) const SURROUND_H: f32 = COURT_H + 60.0;

/// Singles sideline X (court half-width is 230; singles narrows it).
pub(crate) const SINGLES_X: f32 = 187.0;
/// Alternating surface bands across the court length.
pub(crate) const LANES: usize = 8;
/// Service line as a fraction of the court length from the net.
pub(crate) const SERVICE_LINE_RATIO: f32 = 0.24;

/// Side panel width in the HUD layout.
pub(crate) const PANEL_W: f32 = 268.0;
