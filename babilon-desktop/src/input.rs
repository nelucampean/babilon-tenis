//! Keyboard handling: play keys and the settings/housekeeping keys.
use babilon_core::{Difficulty, Phase, Shot, LANE_STEP, MAX_DIR};
use bevy::prelude::*;

use crate::audio::{AudioCtx, AudioMode, Bus, Pending, Sfx, Synth, VolGroup, Volumes};

use crate::gameplay::{do_hit, do_serve, GameWorld, Tennis};

use crate::settings;

// ---- systems ---------------------------------------------------------------

pub(crate) fn keyboard(world: GameWorld, keys: Res<ButtonInput<KeyCode>>) {
    let GameWorld {
        mut commands,
        time,
        mut tennis,
        mut targets,
        mut poses,
        mut flight,
        mut particles,
        pa,
        sfx,
        vols,
        mode,
        mut pending,
        balls,
        players,
        opps,
        mut transforms,
        ..
    } = world;
    let mut audio = AudioCtx::new(sfx.as_deref(), *vols, *mode, &mut pending);
    if keys.just_pressed(KeyCode::KeyR) {
        // history_ro already ends with the score line + serve prompt, so
        // repeating it verbatim avoids announcing the score twice.
        let msg = tennis.m.history_ro.clone();
        tennis.announce(msg);
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
        audio.synth(Synth::Foe(lane), lane as f32 / MAX_DIR as f32, Bus::Ball);
        tennis.announce(format!("Adversar: {}.", babilon_core::dir_label_ro(lane)));
        return;
    }
    if keys.just_pressed(KeyCode::KeyP) {
        if tennis.m.phase == Phase::Finished {
            return;
        }
        tennis.paused = !tennis.paused;
        flight.active = false;
        if tennis.paused {
            tennis.hit_window = None;
            tennis.opp_serve_timer = None;
            tennis.announce("Pauză. Apasă P pentru reluare.".into());
        } else {
            tennis.m.phase = Phase::Ready;
            if tennis.m.is_player_serve() {
                tennis.announce("Meci reluat. Apasă S pentru serviciu.".into());
            } else {
                tennis.announce("Meci reluat. Adversarul servește.".into());
            }
        }
        return;
    }
    if tennis.paused {
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
        audio.synth(Synth::Lane(d), d as f32 / MAX_DIR as f32, Bus::Ball);
        tennis.announce(format!("Direcție: {}", babilon_core::dir_label_ro(d)));
    }
    if keys.just_pressed(KeyCode::ArrowRight) {
        tennis.m.nudge_dir(1);
        let d = tennis.m.dir;
        targets.player_x = d as f32 * LANE_STEP;
        audio.synth(Synth::Lane(d), d as f32 / MAX_DIR as f32, Bus::Ball);
        tennis.announce(format!("Direcție: {}", babilon_core::dir_label_ro(d)));
    }
    if keys.just_pressed(KeyCode::Digit1) {
        tennis.m.set_shot(Shot::Flat);
        tennis.announce("Lovitură plată".into());
    }
    if keys.just_pressed(KeyCode::Digit2) {
        tennis.m.set_shot(Shot::Topspin);
        tennis.announce("Lovitură liftată".into());
    }
    if keys.just_pressed(KeyCode::Digit3) {
        tennis.m.set_shot(Shot::Slice);
        tennis.announce("Lovitură slice".into());
    }
    if keys.just_pressed(KeyCode::KeyS) {
        if let Ok(ball) = balls.single() {
            do_serve(
                &mut audio,
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
                &mut audio,
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
/// Every branch that mutates setup calls [`persist_settings`] so the next
/// launch resumes where you left off.
pub(crate) fn persist_settings(
    tennis: &Tennis,
    vols: &Volumes,
    mode: &AudioMode,
    sfx: Option<&Sfx>,
) {
    settings::save(
        tennis.difficulty,
        *mode,
        vols,
        sfx.map(|s| s.swapped()).unwrap_or(false),
    );
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn settings_keys(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut tennis: ResMut<Tennis>,
    mut vols: ResMut<Volumes>,
    mut mode: ResMut<AudioMode>,
    mut pending: ResMut<Pending>,
    sfx: Option<Res<Sfx>>,
) {
    let sfx = sfx.as_deref();
    let mut audio = AudioCtx::new(sfx, *vols, *mode, &mut pending);
    if keys.just_pressed(KeyCode::KeyT) {
        // Stadium test + stereo channel check: hard-left blip now,
        // hard-right blip half a second later. If "stânga" comes out of
        // your right ear, your headset/speakers are swapped.
        audio.synth(Synth::Guide, -1.0, Bus::Ball);
        let now = time.elapsed_secs();
        audio.schedule(now + 0.5, Synth::Guide, 1.0, Bus::Ball);
        audio.synth(Synth::Bounce, 0.65, Bus::Ball);
        audio.umpire(&babilon_core::UmpireCall {
            text_en: "Fifteen love".into(),
            clip_key: Some("fifteen-love".into()),
        });
        audio.clip("applause", 0.0, Bus::Crowd);
        tennis.announce("Test audio: stânga … apoi dreapta, ricoșeu, arbitru și aplauze.".into());
        return;
    }
    if keys.just_pressed(KeyCode::KeyG) {
        tennis.difficulty = match tennis.difficulty {
            Difficulty::Easy => Difficulty::Normal,
            Difficulty::Normal => Difficulty::Hard,
            Difficulty::Hard => Difficulty::Easy,
        };
        let label = match tennis.difficulty {
            Difficulty::Easy => "Ușor",
            Difficulty::Normal => "Normal",
            Difficulty::Hard => "Greu",
        };
        tennis.announce(format!("Dificultate: {label}."));
        persist_settings(&tennis, &vols, &mode, sfx);
        return;
    }
    if keys.just_pressed(KeyCode::KeyM) {
        *mode = match *mode {
            AudioMode::Access => AudioMode::Real,
            AudioMode::Real => AudioMode::Access,
        };
        tennis.announce(format!("Mod audio {}.", mode.label_ro()));
        persist_settings(&tennis, &vols, &mode, sfx);
        return;
    }
    if keys.just_pressed(KeyCode::KeyN) {
        // Stereo L/R swap for swapped headsets, OS remaps, or single-sided
        // hearing. Verify with T: the "stânga" blip must come from the left.
        if let Some(s) = sfx {
            let swapped = !s.swapped();
            s.set_swapped(swapped);
            tennis.announce(if swapped {
                "Stereo inversat (stânga-dreapta).".into()
            } else {
                "Stereo normal.".into()
            });
        } else {
            tennis.announce("Stereo normal (fără motor audio).".into());
        }
        persist_settings(&tennis, &vols, &mode, sfx);
        return;
    }
    if keys.just_pressed(KeyCode::KeyC) {
        vols.group = vols.group.next();
        let v = match vols.group {
            VolGroup::Ball => vols.ball,
            VolGroup::Umpire => vols.umpire,
            VolGroup::Crowd => vols.crowd,
        };
        tennis.announce(format!(
            "Grup volum: {}. {}%. Z scade, X crește.",
            vols.group.label_ro(),
            (v * 100.0) as u32
        ));
        persist_settings(&tennis, &vols, &mode, sfx);
        return;
    }
    if keys.just_pressed(KeyCode::KeyZ) || keys.just_pressed(KeyCode::KeyX) {
        let delta = if keys.just_pressed(KeyCode::KeyZ) {
            -0.1
        } else {
            0.1
        };
        let (group, v) = vols.adjust(delta);
        if let Some(s) = sfx {
            s.refresh_ambient(vols.crowd);
        }
        tennis.announce(format!(
            "Volum {}: {}%.",
            group.label_ro(),
            (v * 100.0) as u32
        ));
        persist_settings(&tennis, &vols, &mode, sfx);
    }
}
