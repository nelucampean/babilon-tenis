//! Thin binary shell; the game lives in the `babilon_desktop` library so
//! the headless test harness can boot the same app wiring.
fn main() {
    babilon_desktop::run();
}
