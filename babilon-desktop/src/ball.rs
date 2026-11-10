//! The ball, its shadow, and one animated 2D flight (with landings).
use babilon_core::{Dir, Phase, Shot};
use bevy::prelude::*;
use rand::Rng;

use crate::audio::{AudioCtx, Bus, Synth};
use crate::gameplay::{
    land_incoming, land_opponent_serve, land_return, land_serve, reset_ball_to_server, GameWorld,
};
use crate::particles::spawn_sparks;

#[derive(Component)]
pub(crate) struct Ball;
#[derive(Component)]
pub(crate) struct BallShadow;

/// One ball flight, animated in `ball_flight`. Mirrors `trajectory()` in JS.
#[derive(Resource)]
pub(crate) struct Flight {
    pub(crate) active: bool,
    pub(crate) t: f32,
    pub(crate) duration: f32,
    pub(crate) from: Vec2,
    pub(crate) to: Vec2,
    pub(crate) bounce_at: Option<f32>,
    pub(crate) bounced: bool,
    pub(crate) after: FlightAfter,
    /// Contact quality of the swing that started this flight (0.44..1.0).
    pub(crate) hit_quality: f32,
    /// Aim + shot snapshotted at contact. The rally MUST resolve with what
    /// the racket did at hit time — not with whatever ←/→/1-2-3 the player
    /// mashes during the 0.92s return flight.
    pub(crate) hit_dir: Dir,
    pub(crate) hit_shot: Shot,
}

impl Default for Flight {
    fn default() -> Self {
        Self {
            active: false,
            t: 0.0,
            duration: 0.0,
            from: Vec2::ZERO,
            to: Vec2::ZERO,
            bounce_at: None,
            bounced: false,
            after: FlightAfter::None,
            hit_quality: 0.0,
            hit_dir: 0,
            hit_shot: Shot::Flat,
        }
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FlightAfter {
    #[default]
    None,
    ServeLanded,
    IncomingLanded,
    ReturnLanded,
    /// Opponent's serve flight landing on your side. Ace check happens
    /// here (point to him) — otherwise it becomes your HIT cue.
    OpponentServeLanded,
    /// Visual epilogue: your ball sails long/wide after the OUT call.
    /// The point was already scored at takeoff; landing just parks the ball.
    OutLanded,
    /// Visual epilogue: his counter comes back at your feet and beats you.
    /// The point was already scored at takeoff; landing just parks the ball.
    CounterLanded,
}

/// Y of the net visual (see `setup`: net sprite at y=20). A return swing is
/// only legal once the ball is past it, on your side.
#[allow(clippy::too_many_arguments)]
pub(crate) fn ball_flight(world: GameWorld) {
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
        shadows,
        mut transforms,
        ..
    } = world;
    let mut audio = AudioCtx::new(sfx.as_deref(), *vols, *mode, &mut pending);

    let ball = balls.single().ok();
    let shadow = shadows.single().ok();
    if !flight.active {
        return;
    }
    flight.t += time.delta_secs();
    let t = (flight.t / flight.duration).clamp(0.0, 1.0);
    // smoothstep like JS `e=t*t*(3-2t)`
    let e = t * t * (3.0 - 2.0 * t);
    let pos = flight.from.lerp(flight.to, e);
    // arc height -> ball scale pulse (cheap 2D height cue)
    let height = (t * std::f32::consts::PI).sin() * 33.0;

    // Ball arcs with real lift: the mesh rises by `height` while the
    // shadow stays on the court (mirrors ballDraw() in index.html).
    if let Some(e) = ball {
        if let Ok(mut b) = transforms.get_mut(e) {
            b.translation.x = pos.x;
            b.translation.y = pos.y + height;
            b.scale = Vec3::splat(1.0 + height / 90.0);
        }
    }
    if let Some(e) = shadow {
        if let Ok(mut s) = transforms.get_mut(e) {
            s.translation.x = pos.x;
            s.translation.y = pos.y;
            let k = 1.0 + height * 0.012;
            s.scale = Vec3::new(k, k, 1.0);
        }
    }
    if let Some(bt) = flight.bounce_at {
        if !flight.bounced && t >= bt {
            flight.bounced = true;
            let pan = (pos.x / 160.0).clamp(-1.0, 1.0);
            spawn_sparks(&mut commands, &pa, &mut particles, pos, false, 4);
            audio.synth(Synth::Bounce, pan, Bus::Ball);
        }
    }

    if t >= 1.0 {
        let after = flight.after;
        let now = time.elapsed_secs();
        flight.active = false;
        if let Some(b) = ball {
            match after {
                FlightAfter::ServeLanded => land_serve(
                    &mut commands,
                    &mut audio,
                    &mut tennis,
                    &mut targets,
                    &mut poses,
                    now,
                    &mut flight,
                    b,
                    &mut transforms,
                    &pa,
                    &mut particles,
                ),
                FlightAfter::IncomingLanded => land_incoming(&mut commands, &audio, &mut tennis),
                FlightAfter::OpponentServeLanded => land_opponent_serve(
                    &mut commands,
                    &mut audio,
                    &mut tennis,
                    &targets,
                    &mut flight,
                    b,
                    &mut transforms,
                    &pa,
                    &mut particles,
                ),
                FlightAfter::ReturnLanded => {
                    land_return(
                        &mut commands,
                        &mut audio,
                        &mut tennis,
                        &mut targets,
                        &mut poses,
                        now,
                        &mut flight,
                        b,
                        &mut transforms,
                        &pa,
                        &mut particles,
                    );
                }
                FlightAfter::OutLanded => {
                    // Miss confirmed where it lands: score + call now, in
                    // the quiet window after the flight — not at takeoff
                    // under the line call.
                    let r = tennis.m.score_point(1, "AUT! Mingea ta a ieșit afară");
                    let msg = r.message_ro.clone();
                    crate::announce::announce(&mut commands, msg);
                    audio.umpire(&r.umpire);
                    audio.synth(Synth::Bad, 0.0, Bus::Ball);
                    if rand::thread_rng().gen::<f32>() < 0.40 {
                        audio.clip("reaction", 0.0, Bus::Crowd);
                    }
                    let server = tennis.m.current_server();
                    reset_ball_to_server(&mut transforms, b, &targets, server);
                    if tennis.m.phase != Phase::Finished {
                        tennis.m.phase = Phase::Ready;
                    }
                }
                FlightAfter::CounterLanded => {
                    // His reply beat you: score + call now, in the quiet
                    // window after the flight — the takeoff stayed clean
                    // (racket + Cue + guide + bounce) so it could be heard.
                    let r = tennis
                        .m
                        .score_point(1, "Contraatac! Adversarul a câștigat punctul");
                    let msg = r.message_ro.clone();
                    crate::announce::announce(&mut commands, msg);
                    audio.umpire(&r.umpire);
                    audio.synth(Synth::Bad, 0.0, Bus::Ball);
                    if rand::thread_rng().gen::<f32>() < 0.40 {
                        audio.clip("reaction", 0.0, Bus::Crowd);
                    }
                    let server = tennis.m.current_server();
                    reset_ball_to_server(&mut transforms, b, &targets, server);
                    if tennis.m.phase != Phase::Finished {
                        tennis.m.phase = Phase::Ready;
                    }
                }
                FlightAfter::None => {}
            }
        }
    }
}
