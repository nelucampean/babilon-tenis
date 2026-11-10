//! Keyboard handling: play keys and the settings/housekeeping keys.
use babilon_core::{Difficulty, Phase, Shot, LANE_STEP, MAX_DIR};
use bevy::prelude::*;

use crate::announce::{announce, Announcer};
use crate::audio::{AudioMode, Bus, Synth, VolGroup};
use crate::ball::Flight;
use crate::sound::SoundExt;

use crate::gameplay::{do_hit, do_serve, GameWorld, Tennis};

use crate::settings::Settings;
use crate::state::AppState;
use crate::text;

// ---- systems ---------------------------------------------------------------

/// Pause/resume. Runs in both states so P works while frozen; gameplay
/// itself only runs in [`AppState::Playing`] (see `app.rs`).
pub(crate) fn pause_key(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<AppState>>,
    mut next: ResMut<NextState<AppState>>,
    mut tennis: ResMut<Tennis>,
    mut flight: ResMut<Flight>,
) {
    if !keys.just_pressed(KeyCode::KeyP) || tennis.m.phase == Phase::Finished {
        return;
    }
    match state.get() {
        AppState::Playing => {
            next.set(AppState::Paused);
            flight.active = false;
            tennis.hit_window = None;
            tennis.opp_serve_timer = None;
            announce(&mut commands, "Pauză. Apasă P pentru reluare.");
        }
        AppState::Paused => {
            next.set(AppState::Playing);
            tennis.m.phase = Phase::Ready;
            if tennis.m.is_player_serve() {
                announce(&mut commands, "Meci reluat. Apasă S pentru serviciu.");
            } else {
                announce(&mut commands, "Meci reluat. Adversarul servește.");
            }
        }
    }
}

pub(crate) fn keyboard(
    world: GameWorld,
    keys: Res<ButtonInput<KeyCode>>,
    announcer: Res<Announcer>,
) {
    let GameWorld {
        mut commands,
        time,
        mut tennis,
        mut targets,
        mut poses,
        mut flight,
        mut particles,
        pa,
        balls,
        players,
        opps,
        mut transforms,
        ..
    } = world;
    if keys.just_pressed(KeyCode::KeyR) {
        // Repeat what was last announced, then the current score line so
        // the listener keeps their bearings.
        let msg = text::repeat_message_ro(&announcer.status_ro, &tennis.m);
        announce(&mut commands, msg);
        return;
    }
    if keys.just_pressed(KeyCode::KeyO) {
        // Opponent locator: lower-octave lane blip panned at his feet,
        // plus the lane name — impossible to confuse with your own blip.
        let lane = opps
            .single()
            .ok()
            .and_then(|e| transforms.get(e).ok())
            .map(|t| (t.translation.x / LANE_STEP).round() as i8)
            .unwrap_or(0)
            .clamp(-MAX_DIR, MAX_DIR);
        commands.synth(Synth::Foe(lane), lane as f32 / MAX_DIR as f32, Bus::Ball);
        announce(
            &mut commands,
            format!("Adversar: {}.", text::dir_label_ro(lane)),
        );
        return;
    }
    if keys.just_pressed(KeyCode::ArrowLeft) {
        tennis.m.nudge_dir(-1);
        let d = tennis.m.dir;
        // Full lane reach: the incoming ball lands at ±151, so the paddle
        // must be able to get there — ±72 left users stranded mid-court.
        targets.player_x = d as f32 * LANE_STEP;
        // Position blip: pitch (low/mid/high) + pan both encode the lane,
        // so you always know where you stand without looking.
        commands.synth(Synth::Lane(d), d as f32 / MAX_DIR as f32, Bus::Ball);
        announce(
            &mut commands,
            format!("Direcție: {}", text::dir_label_ro(d)),
        );
    }
    if keys.just_pressed(KeyCode::ArrowRight) {
        tennis.m.nudge_dir(1);
        let d = tennis.m.dir;
        targets.player_x = d as f32 * LANE_STEP;
        commands.synth(Synth::Lane(d), d as f32 / MAX_DIR as f32, Bus::Ball);
        announce(
            &mut commands,
            format!("Direcție: {}", text::dir_label_ro(d)),
        );
    }
    if keys.just_pressed(KeyCode::Digit1) {
        tennis.m.set_shot(Shot::Flat);
        announce(&mut commands, "Lovitură plată");
    }
    if keys.just_pressed(KeyCode::Digit2) {
        tennis.m.set_shot(Shot::Topspin);
        announce(&mut commands, "Lovitură liftată");
    }
    if keys.just_pressed(KeyCode::Digit3) {
        tennis.m.set_shot(Shot::Slice);
        announce(&mut commands, "Lovitură slice");
    }
    if keys.just_pressed(KeyCode::KeyS) {
        if let Ok(ball) = balls.single() {
            do_serve(
                &mut commands,
                &mut tennis,
                &mut targets,
                &mut poses,
                time.elapsed_secs(),
                &mut flight,
                ball,
                &mut transforms,
            );
        }
    }
    if keys.just_pressed(KeyCode::Space) {
        if let (Ok(ball), Ok(player)) = (balls.single(), players.single()) {
            do_hit(
                &mut commands,
                &mut tennis,
                &mut targets,
                &mut poses,
                time.elapsed_secs(),
                &mut flight,
                ball,
                player,
                &mut transforms,
                &pa,
                &mut particles,
            );
        }
    }
}

/// Housekeeping keys: stadium test, difficulty, audio mode, volume groups,
/// output device. Split out of `keyboard` — Bevy caps function systems at
/// 16 parameters, and settings deserve their own system anyway.
///
/// Each branch only mutates [`Settings`]; `settings::autosave` persists it
/// at the end of the frame, so there is no manual save call to forget.
pub(crate) fn settings_keys(
    mut commands: Commands,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut settings: ResMut<Settings>,
) {
    if keys.just_pressed(KeyCode::KeyT) {
        // Stadium test + stereo channel check: hard-left blip now,
        // hard-right blip half a second later. If "stânga" comes out of
        // your right ear, your headset/speakers are swapped.
        commands.synth(Synth::Guide, -1.0, Bus::Ball);
        let now = time.elapsed_secs();
        commands.schedule(now + 0.5, Synth::Guide, 1.0, Bus::Ball);
        commands.synth(Synth::Bounce, 0.65, Bus::Ball);
        commands.umpire(&babilon_core::UmpireCall {
            text_en: "Fifteen love".into(),
            clip_key: Some("fifteen-love".into()),
        });
        commands.clip("applause", 0.0, Bus::Crowd);
        announce(
            &mut commands,
            "Test audio: stânga … apoi dreapta, ricoșeu, arbitru și aplauze.",
        );
        return;
    }
    if keys.just_pressed(KeyCode::KeyG) {
        settings.difficulty = match settings.difficulty {
            Difficulty::Easy => Difficulty::Normal,
            Difficulty::Normal => Difficulty::Hard,
            Difficulty::Hard => Difficulty::Easy,
        };
        let label = match settings.difficulty {
            Difficulty::Easy => "Ușor",
            Difficulty::Normal => "Normal",
            Difficulty::Hard => "Greu",
        };
        announce(&mut commands, format!("Dificultate: {label}."));
        return;
    }
    if keys.just_pressed(KeyCode::KeyM) {
        settings.mode = match settings.mode {
            AudioMode::Access => AudioMode::Real,
            AudioMode::Real => AudioMode::Access,
        };
        announce(
            &mut commands,
            format!("Mod audio {}.", settings.mode.label_ro()),
        );
        return;
    }
    if keys.just_pressed(KeyCode::KeyN) {
        // Stereo L/R swap for swapped headsets, OS remaps, or single-sided
        // hearing. Verify with T: the "stânga" blip must come from the left.
        settings.swap_stereo = !settings.swap_stereo;
        announce(
            &mut commands,
            if settings.swap_stereo {
                "Stereo inversat (stânga-dreapta)."
            } else {
                "Stereo normal."
            },
        );
        return;
    }
    if keys.just_pressed(KeyCode::KeyC) {
        settings.vols.group = settings.vols.group.next();
        let v = match settings.vols.group {
            VolGroup::Ball => settings.vols.ball,
            VolGroup::Umpire => settings.vols.umpire,
            VolGroup::Crowd => settings.vols.crowd,
        };
        announce(
            &mut commands,
            format!(
                "Grup volum: {}. {}%. Z scade, X crește.",
                settings.vols.group.label_ro(),
                (v * 100.0) as u32
            ),
        );
        return;
    }
    if keys.just_pressed(KeyCode::KeyZ) || keys.just_pressed(KeyCode::KeyX) {
        let delta = if keys.just_pressed(KeyCode::KeyZ) {
            -0.1
        } else {
            0.1
        };
        let (group, v) = settings.vols.adjust(delta);
        announce(
            &mut commands,
            format!("Volum {}: {}%.", group.label_ro(), (v * 100.0) as u32),
        );
    }
}
