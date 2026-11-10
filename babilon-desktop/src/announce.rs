//! The game's announcement channel.
//!
//! Rules never touch the HUD or stdout directly: they `send_event` an
//! [`Announce`], and [`announce_sink`] is the single place that applies it
//! to the live status text (`Announcer`), the terminal screen-reader line,
//! and any future backends (TTS, logs, captions).
use bevy::prelude::*;

/// A line the game wants the player to hear/see, e.g. "Acum lovește!".
#[derive(Event, Debug, Clone)]
pub(crate) struct Announce(pub String);

/// The latest announcement, mirrored into the side-panel status box.
#[derive(Resource, Default)]
pub(crate) struct Announcer {
    pub(crate) status_ro: String,
}

/// Queue an announcement from anywhere holding `Commands`.
pub(crate) fn announce(commands: &mut Commands, text: impl Into<String>) {
    commands.send_event(Announce(text.into()));
}

/// Apply every queued announcement to the status line and the terminal.
///
/// Runs in the `Update` chain after gameplay and before `refresh_ui`, so a
/// key press or landing is visible in the same frame.
pub(crate) fn announce_sink(mut events: EventReader<Announce>, mut announcer: ResMut<Announcer>) {
    for Announce(text) in events.read() {
        // stdout line = picked up by Orca / terminal screen readers, in
        // addition to the Bevy UI text node (AccessKit live region).
        println!("[ANUNȚ] {text}");
        announcer.status_ro.clone_from(text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sink_keeps_the_last_line_and_prints_all() {
        let mut app = App::new();
        app.add_event::<Announce>();
        app.init_resource::<Announcer>();
        app.add_systems(Update, announce_sink);
        app.world_mut().send_event(Announce("prima".into()));
        app.world_mut().send_event(Announce("a doua".into()));
        app.update();
        assert_eq!(app.world().resource::<Announcer>().status_ro, "a doua");
    }
}
