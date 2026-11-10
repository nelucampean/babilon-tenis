//! Game flow: serve / incoming / hit / rally resolution and the match
//! resources that drive it. Mirrors serve()/incoming()/hit()/point() in JS.
use babilon_core::{Difficulty, Dir, Match, Phase, RallyOutcome, LANE_STEP, MAX_DIR};
use bevy::prelude::*;
use rand::Rng;

use crate::athlete::Poses;
use crate::audio::{self, AudioMode, Pending, Sfx, Synth, Volumes};
use crate::ball::{Ball, Flight, FlightAfter, NET_Y};
use crate::particles::{spawn_sparks, ParticleAssets, Particles};

// ---- resources -------------------------------------------------------------

#[derive(Resource)]
pub(crate) struct Tennis {
    pub(crate) m: Match,
    pub(crate) difficulty: Difficulty,
    pub(crate) paused: bool,
    pub(crate) status_ro: String,
    pub(crate) hit_window: Option<Timer>,
    /// Countdown between the opponent's position signal and his serve
    /// flight. `None` outside opponent service games.
    pub(crate) opp_serve_timer: Option<Timer>,
    /// Serve stance + target lanes announced for the opponent's serve.
    pub(crate) opp_serve_origin: Dir,
    pub(crate) opp_serve_target: Dir,
}

impl Tennis {
    pub(crate) fn announce(&mut self, text: String) {
        self.status_ro = text.clone();
        // stdout line = picked up by Orca / terminal screen readers,
        // in addition to the Bevy UI text node (AccessKit).
        println!("[ANNOUNȚ] {text}");
    }
}

/// Where paddles drift toward. Mirrors `player.target` / `opponent.target`
/// in index.html — direction keys move the player + aim even in Ready.
#[derive(Resource)]
pub(crate) struct Targets {
    pub(crate) player_x: f32,
    pub(crate) opp_x: f32,
}

// ---- game flow (mirrors serve()/incoming()/hit()/point() in JS) ------------
// Sound comes from `audio::Sfx` (rodio): positioned synth SFX + MP3 clips.
// `sfx: Option<&Sfx>` is `None` headless, so every call is skip-safe.

pub(crate) fn start_flight(
    flight: &mut Flight,
    from: Vec2,
    to: Vec2,
    duration: f32,
    after: FlightAfter,
    bounce_at: Option<f32>,
) {
    flight.active = true;
    flight.t = 0.0;
    flight.duration = duration;
    flight.from = from;
    flight.to = to;
    flight.bounce_at = bounce_at;
    flight.bounced = false;
    flight.after = after;
}

pub(crate) fn do_serve(
    sfx: Option<&Sfx>,
    vols: &Volumes,
    tennis: &mut Tennis,
    targets: &mut Targets,
    poses: &mut Poses,
    now: f32,
    flight: &mut Flight,
    ball: Entity,
    transforms: &mut Query<&mut Transform>,
) {
    if tennis.paused {
        return;
    }
    if tennis.m.phase == Phase::Finished {
        tennis.announce(
            "Meci încheiat. Apasă butonul de meci nou (în curând) sau repornește jocul.".into(),
        );
        return;
    }
    if tennis.m.phase != Phase::Ready {
        return;
    }
    // Service games alternate: on his game the opponent serves himself —
    // S is not your serve. The auto-signal system serves a beat later.
    if !tennis.m.is_player_serve() {
        tennis.announce("Adversarul servește. Pregătește-te!".into());
        return;
    }
    tennis.m.phase = Phase::Serve;
    targets.player_x = tennis.m.dir as f32 * LANE_STEP;
    poses.p_serve_until = now + 0.42;
    tennis.announce("Serviciu!".into());
    let pan = tennis.m.dir as f32 / MAX_DIR as f32;
    Sfx::clip(sfx, "hit", pan, vols.ball);
    Sfx::synth(sfx, Synth::Hit, pan, vols.ball);
    if let Ok(t) = transforms.get(ball) {
        start_flight(
            flight,
            Vec2::new(t.translation.x, t.translation.y),
            Vec2::new(tennis.m.dir as f32 * LANE_STEP, 300.0),
            0.88,
            FlightAfter::ServeLanded,
            Some(0.58),
        );
    }
}

pub(crate) fn do_incoming(
    tennis: &mut Tennis,
    targets: &mut Targets,
    poses: &mut Poses,
    now: f32,
    mode: AudioMode,
    vols: &Volumes,
    pending: &mut Pending,
    flight: &mut Flight,
    ball: Entity,
    transforms: &mut Query<&mut Transform>,
) {
    use babilon_core::dir_label_ro;
    if tennis.paused || tennis.m.phase == Phase::Finished {
        return;
    }
    tennis.m.phase = Phase::Incoming;
    let mut rng = rand::thread_rng();
    let side: i8 = {
        // The bot reads your position (paddle drift target) and, on higher
        // difficulties, wrong-foots you toward the far corner. Easy hits
        // anywhere — it doesn't scout you at all.
        let player_lane = (targets.player_x / LANE_STEP).round() as i8;
        Match::pick_incoming_side(tennis.difficulty, player_lane, rng.gen())
    };
    tennis.m.incoming_side = side;
    // Chase the ball like the web's opponent.target (near the landing
    // spot, not stranded mid-court): full lane, like the player's run.
    targets.opp_x = side as f32 * LANE_STEP;
    poses.o_swing_until = now + 0.35;
    tennis.announce(format!(
        "Mingea vine spre {}. Pregătește-te!",
        dir_label_ro(side)
    ));
    // Mid-flight guide beep, like the web `setTimeout(..., 490)` — Access
    // mode only; Realist keeps just bounces and the landing cue.
    if mode == AudioMode::Access {
        pending.0.push(audio::Scheduled {
            at: now + 0.49,
            kind: Synth::Guide,
            pan: side as f32 / MAX_DIR as f32,
            vol: vols.ball,
        });
    }
    if let Ok(t) = transforms.get(ball) {
        start_flight(
            flight,
            Vec2::new(t.translation.x, t.translation.y),
            Vec2::new(side as f32 * LANE_STEP, -240.0),
            1.08,
            FlightAfter::IncomingLanded,
            Some(0.69),
        );
    }
}

/// After a point, park the ball at whoever serves next.
/// Mirrors JS, where the ball hides and reappears at the server.
pub(crate) fn reset_ball_to_server(
    transforms: &mut Query<&mut Transform>,
    ball: Entity,
    targets: &Targets,
    server: usize,
) {
    if let Ok(mut b) = transforms.get_mut(ball) {
        if server == 0 {
            b.translation.x = targets.player_x;
            b.translation.y = -260.0;
        } else {
            b.translation.x = targets.opp_x;
            b.translation.y = 300.0;
        }
        b.scale = Vec3::ONE;
    }
}

/// Opponent's service game, part 1 — position signal.
///
/// Picks his stance (deuce/ad by points parity) and his target on your
/// side, plants him visibly there, and tells you BEFORE he hits: THREE
/// lower-octave lane bips panned at his feet (one now, two more at
/// +0.4s/+0.8s — never confused with your own blip) plus "Adversarul
/// servește de la …". The flight itself starts ~1.2s later in
/// [`fire_opponent_serve`], announced like any incoming ball, so you get
/// stance and trajectory as two spaced cues.
#[allow(clippy::too_many_arguments)]
pub(crate) fn start_opponent_serve_signal(
    sfx: Option<&Sfx>,
    vols: &Volumes,
    tennis: &mut Tennis,
    targets: &mut Targets,
    poses: &mut Poses,
    now: f32,
    pending: &mut Pending,
    ball: Entity,
    transforms: &mut Query<&mut Transform>,
) {
    use babilon_core::dir_label_ro;
    let origin = tennis.m.serve_origin_lane();
    let player_lane = (targets.player_x / LANE_STEP).round() as i8;
    let target =
        Match::pick_incoming_side(tennis.difficulty, player_lane, rand::thread_rng().gen());
    tennis.m.incoming_side = target;
    tennis.opp_serve_origin = origin;
    tennis.opp_serve_target = target;
    tennis.m.phase = Phase::Serve;
    targets.opp_x = origin as f32 * LANE_STEP;
    poses.o_swing_until = now + 1.2;
    // Park the ball at his racket now; drift/skating won't touch it
    // mid-signal because the phase is Serve, not Ready.
    if let Ok(mut b) = transforms.get_mut(ball) {
        b.translation.x = targets.opp_x;
        b.translation.y = 300.0;
        b.scale = Vec3::ONE;
    }
    tennis.announce(format!(
        "Adversarul servește de la {}. Pregătește-te!",
        dir_label_ro(origin)
    ));
    // 3-bip countdown to his serve: one now, two scheduled. Same lane
    // coding as the O-key locator, panned at his feet.
    let pan = origin as f32 / MAX_DIR as f32;
    Sfx::synth(sfx, Synth::Foe(origin), pan, vols.ball);
    pending.0.push(audio::Scheduled {
        at: now + 0.4,
        kind: Synth::Foe(origin),
        pan,
        vol: vols.ball,
    });
    pending.0.push(audio::Scheduled {
        at: now + 0.8,
        kind: Synth::Foe(origin),
        pan,
        vol: vols.ball,
    });
    tennis.opp_serve_timer = Some(Timer::from_seconds(1.2, TimerMode::Once));
}

/// Opponent's service game, part 2 — the serve flight.
///
/// Fired by [`opponent_serve_tick`] when the signal timer lapses: racket
/// crack at his stance, then a full incoming-style flight to the
/// pre-announced target lane. Landing resolves in `ball_flight` via
/// [`FlightAfter::OpponentServeLanded`].
#[allow(clippy::too_many_arguments)]
pub(crate) fn fire_opponent_serve(
    sfx: Option<&Sfx>,
    vols: &Volumes,
    mode: AudioMode,
    pending: &mut Pending,
    tennis: &mut Tennis,
    targets: &mut Targets,
    now: f32,
    flight: &mut Flight,
    ball: Entity,
    transforms: &mut Query<&mut Transform>,
) {
    use babilon_core::dir_label_ro;
    let origin = tennis.opp_serve_origin;
    let target = tennis.opp_serve_target;
    tennis.m.incoming_side = target;
    targets.opp_x = origin as f32 * LANE_STEP;
    tennis.announce(format!(
        "Mingea vine spre {}. Pregătește-te!",
        dir_label_ro(target)
    ));
    let pan = origin as f32 / MAX_DIR as f32;
    Sfx::synth(sfx, Synth::Hit, pan, vols.ball);
    if mode == AudioMode::Access {
        pending.0.push(audio::Scheduled {
            at: now + 0.49,
            kind: Synth::Guide,
            pan: target as f32 / MAX_DIR as f32,
            vol: vols.ball,
        });
    }
    if let Ok(t) = transforms.get(ball) {
        start_flight(
            flight,
            Vec2::new(t.translation.x, t.translation.y),
            Vec2::new(target as f32 * LANE_STEP, -240.0),
            1.08,
            FlightAfter::OpponentServeLanded,
            Some(0.69),
        );
    }
}

/// Drives the opponent's service game: auto-starts his position signal
/// when a Ready point comes up on his serve, then fires the serve when
/// the signal timer lapses. Runs before `drift_paddles`/`ball_flight`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn opponent_serve_tick(
    time: Res<Time>,
    mut tennis: ResMut<Tennis>,
    mut targets: ResMut<Targets>,
    mut poses: ResMut<Poses>,
    vols: Res<Volumes>,
    mode: Res<AudioMode>,
    mut pending: ResMut<Pending>,
    sfx: Option<Res<Sfx>>,
    mut flight: ResMut<Flight>,
    balls: Query<Entity, With<Ball>>,
    mut transforms: Query<&mut Transform>,
) {
    if tennis.paused || tennis.m.phase == Phase::Finished {
        return;
    }
    let sfx = sfx.as_deref();
    if let Some(timer) = tennis.opp_serve_timer.as_mut() {
        timer.tick(time.delta());
        if timer.finished() {
            tennis.opp_serve_timer = None;
            if tennis.m.phase == Phase::Serve && !flight.active {
                if let Ok(ball) = balls.single() {
                    fire_opponent_serve(
                        sfx,
                        &vols,
                        *mode,
                        &mut pending,
                        &mut tennis,
                        &mut targets,
                        time.elapsed_secs(),
                        &mut flight,
                        ball,
                        &mut transforms,
                    );
                }
            }
        }
        return;
    }
    // No signal in progress: start one when his serve comes up.
    if tennis.m.phase == Phase::Ready && !flight.active && !tennis.m.is_player_serve() {
        if let Ok(ball) = balls.single() {
            start_opponent_serve_signal(
                sfx,
                &vols,
                &mut tennis,
                &mut targets,
                &mut poses,
                time.elapsed_secs(),
                &mut pending,
                ball,
                &mut transforms,
            );
        }
    }
}

/// Opponent-serve landing: a clean ace takes the point at once, otherwise
/// the ball is live and you get the normal HIT cue.
#[allow(clippy::too_many_arguments)]
pub(crate) fn land_opponent_serve(
    commands: &mut Commands,
    sfx: Option<&Sfx>,
    vols: &Volumes,
    mode: AudioMode,
    tennis: &mut Tennis,
    targets: &Targets,
    flight: &mut Flight,
    ball: Entity,
    transforms: &mut Query<&mut Transform>,
    pa: &ParticleAssets,
    particles: &mut Particles,
) {
    if Match::opponent_serve_is_ace(tennis.difficulty, rand::thread_rng().gen()) {
        let r = tennis
            .m
            .score_point(1, "As! Serviciul adversarului te-a prins");
        let msg = r.message_ro.clone();
        tennis.announce(msg);
        umpire_call(sfx, vols, &r.umpire);
        Sfx::synth(sfx, Synth::Bad, 0.0, vols.ball);
        if rand::thread_rng().gen::<f32>() < 0.40 {
            Sfx::clip(sfx, "reaction", 0.0, vols.crowd);
        }
        spawn_sparks(commands, pa, particles, flight.to, true, 7);
        flight.active = false;
        let server = tennis.m.current_server();
        reset_ball_to_server(transforms, ball, targets, server);
    } else {
        land_incoming(sfx, vols, mode, tennis);
    }
}

pub(crate) fn land_serve(
    commands: &mut Commands,
    sfx: Option<&Sfx>,
    vols: &Volumes,
    mode: AudioMode,
    pending: &mut Pending,
    tennis: &mut Tennis,
    targets: &mut Targets,
    poses: &mut Poses,
    now: f32,
    flight: &mut Flight,
    ball: Entity,
    transforms: &mut Query<&mut Transform>,
    pa: &ParticleAssets,
    particles: &mut Particles,
) {
    let mut rng = rand::thread_rng();
    let roll: f32 = rng.gen();
    if Match::serve_is_ace(tennis.difficulty, roll) {
        let r = tennis.m.score_point(0, "As!");
        let msg = r.message_ro.clone();
        tennis.announce(msg);
        umpire_call(sfx, vols, &r.umpire);
        Sfx::clip(sfx, "applause", 0.0, vols.crowd);
        Sfx::synth(sfx, Synth::Good, 0.0, vols.ball);
        spawn_sparks(commands, pa, particles, flight.to, true, 7);
        flight.active = false;
        let server = tennis.m.current_server();
        reset_ball_to_server(transforms, ball, targets, server);
    } else {
        do_incoming(
            tennis, targets, poses, now, mode, vols, pending, flight, ball, transforms,
        );
    }
}

/// Umpire voice, always centered. Tie-break scores have no MP3 (same as the
/// web version) — falls back to a TTS-style console line.
pub(crate) fn umpire_call(sfx: Option<&Sfx>, vols: &Volumes, call: &babilon_core::UmpireCall) {
    if call.clip_key.is_some() {
        Sfx::umpire(sfx, call, vols.umpire);
    } else {
        println!("[ARBITRU TTS] {}", call.text_en);
    }
}

pub(crate) fn land_incoming(
    sfx: Option<&Sfx>,
    vols: &Volumes,
    mode: AudioMode,
    tennis: &mut Tennis,
) {
    tennis.m.phase = Phase::Hit;
    tennis.hit_window = Some(Timer::from_seconds(1.4, TimerMode::Once));
    tennis.announce("Acum lovește!".into());
    // Audible cue (screen readers speak the text above too late): full
    // volume in Access mode, a discreet hint in Realist mode.
    let amp = vols.ball * if mode == AudioMode::Access { 1.0 } else { 0.4 };
    Sfx::synth(
        sfx,
        Synth::Cue,
        tennis.m.incoming_side as f32 / MAX_DIR as f32,
        amp,
    );
}

pub(crate) fn land_return(
    commands: &mut Commands,
    sfx: Option<&Sfx>,
    vols: &Volumes,
    mode: AudioMode,
    pending: &mut Pending,
    tennis: &mut Tennis,
    targets: &mut Targets,
    poses: &mut Poses,
    now: f32,
    flight: &mut Flight,
    ball: Entity,
    transforms: &mut Query<&mut Transform>,
    pa: &ParticleAssets,
    particles: &mut Particles,
) {
    let mut rng = rand::thread_rng();
    // Resolve with the contact snapshot (aim/shot/quality at hit time) —
    // never with the live keys, which the player may have moved since.
    let outcome = RallyOutcome::resolve(
        tennis.difficulty,
        flight.hit_dir,
        tennis.m.incoming_side,
        flight.hit_shot,
        flight.hit_quality,
        rng.gen(),
        rng.gen(),
        rng.gen(),
    );
    match outcome.point_winner() {
        Some(w) => {
            if w == 0 {
                let r = tennis.m.score_point(0, "Punct câștigat");
                let msg = r.message_ro.clone();
                tennis.announce(msg);
                umpire_call(sfx, vols, &r.umpire);
                Sfx::clip(sfx, "applause", 0.0, vols.crowd);
                Sfx::synth(sfx, Synth::Good, 0.0, vols.ball);
                spawn_sparks(commands, pa, particles, flight.to, true, 7);
                flight.active = false;
                let server = tennis.m.current_server();
                reset_ball_to_server(transforms, ball, targets, server);
            } else if outcome == RallyOutcome::Out {
                // Takeoff gets ONLY the line call + flight noise. The score,
                // umpire and jingles wait for the landing — otherwise the
                // 90ms Hit drowns under the voice + Bad + crowd starting in
                // the same frame and the ball is never heard.
                tennis.announce("AUT! Mingea a ieșit.".into());
                Sfx::clip(sfx, "out", 0.0, vols.umpire);
                let from = flight.to;
                let to = if rand::thread_rng().gen::<f32>() < 0.5 {
                    Vec2::new(from.x, 392.0)
                } else {
                    let side = if from.x >= 0.0 { 1.0 } else { -1.0 };
                    Vec2::new(side * 305.0, 262.0)
                };
                start_flight(flight, from, to, 0.42, FlightAfter::OutLanded, Some(0.75));
                // Epilogue in flight: phase Return so Space/S wait for it.
                tennis.m.phase = Phase::Return;
            } else {
                // His reply, staged like any incoming ball: warning text,
                // his racket crack, full-volume Cue, mid-flight guide, then
                // the bounce — the score call waits for the landing.
                tennis.announce("Contraatac! Mingea vine spre tine!".into());
                poses.o_swing_until = now + 0.45;
                let pan = (flight.to.x / 160.0).clamp(-1.0, 1.0);
                Sfx::synth(sfx, Synth::Hit, pan, vols.ball);
                let amp = vols.ball * if mode == AudioMode::Access { 1.0 } else { 0.4 };
                Sfx::synth(sfx, Synth::Cue, pan, amp);
                if mode == AudioMode::Access {
                    pending.0.push(audio::Scheduled {
                        at: now + 0.3,
                        kind: Synth::Guide,
                        pan: 0.0,
                        vol: vols.ball,
                    });
                }
                let from = flight.to;
                start_flight(
                    flight,
                    from,
                    Vec2::new(targets.player_x, -330.0),
                    0.65,
                    FlightAfter::CounterLanded,
                    Some(0.55),
                );
                tennis.m.phase = Phase::Return;
            }
        }
        None => do_incoming(
            tennis, targets, poses, now, mode, vols, pending, flight, ball, transforms,
        ),
    }
}

pub(crate) fn do_hit(
    commands: &mut Commands,
    sfx: Option<&Sfx>,
    vols: &Volumes,
    tennis: &mut Tennis,
    targets: &mut Targets,
    poses: &mut Poses,
    now: f32,
    flight: &mut Flight,
    ball: Entity,
    player: Entity,
    transforms: &mut Query<&mut Transform>,
    pa: &ParticleAssets,
    particles: &mut Particles,
) {
    if tennis.paused {
        return;
    }
    // Whiff check first: swinging at air is not a hit, no matter when.
    // dx is measured against the ball's live position — positioning is
    // the skill, not mashing Space.
    let dx = match (transforms.get(ball), transforms.get(player)) {
        (Ok(b), Ok(p)) => (b.translation.x - p.translation.x).abs(),
        _ => f32::INFINITY,
    };
    // Net rule: Space during Incoming swings only once the ball crossed
    // the net onto your side; sooner presses get a clear "too early".
    let quality = match tennis.m.phase {
        Phase::Hit => {
            let timed = match &tennis.hit_window {
                Some(t) => Match::hit_quality(t.elapsed_secs() * 1000.0),
                None => 0.5,
            };
            // Fast hands help, but lunging caps what the racket can do.
            timed.min(Match::position_quality(dx))
        }
        Phase::Incoming => {
            if !(flight.active
                && (flight.after == FlightAfter::IncomingLanded
                    || flight.after == FlightAfter::OpponentServeLanded)
                && flight.duration > 0.0)
            {
                tennis.announce("Mingea e în aer. Așteaptă.".into());
                return;
            }
            let crossed = transforms
                .get(ball)
                .map(|b| b.translation.y <= NET_Y + 10.0)
                .unwrap_or(false);
            if !crossed {
                tennis.announce("Prea devreme — mingea n-a trecut încă fileul.".into());
                return;
            }
            // Pre-cue, meeting the ball IS the skill: only position counts.
            Match::position_quality(dx)
        }
        Phase::Ready => {
            if tennis.m.is_player_serve() {
                tennis.announce("Apasă S pentru serviciu, apoi Spațiu la semnal.".into());
            } else {
                tennis.announce("Adversarul servește. Pregătește-te!".into());
            }
            return;
        }
        Phase::Serve | Phase::Return => {
            tennis.announce("Mingea e în aer. Așteaptă.".into());
            return;
        }
        Phase::Finished => {
            tennis.announce("Meci încheiat.".into());
            return;
        }
    };
    if dx > babilon_core::Match::WHIFF_DISTANCE {
        // Swing and a miss: the ball goes through, point to the opponent.
        tennis.hit_window = None;
        let r = tennis.m.score_point(1, "Ai ratat mingea");
        let msg = r.message_ro.clone();
        tennis.announce(format!("{msg} Erai prea departe de minge."));
        umpire_call(sfx, vols, &r.umpire);
        Sfx::synth(sfx, Synth::Bad, 0.0, vols.ball);
        flight.active = false;
        // Whiff target deref: `targets` is `&mut Targets` here.
        let server = tennis.m.current_server();
        reset_ball_to_server(transforms, ball, &*targets, server);
        return;
    }
    tennis.hit_window = None;
    tennis.m.phase = Phase::Return;
    poses.p_swing_until = now + 0.45;
    // JS: player runs to the ball, opponent anticipates our target.
    targets.player_x = tennis.m.incoming_side as f32 * LANE_STEP;
    targets.opp_x = tennis.m.dir as f32 * LANE_STEP;
    tennis.announce(format!(
        "Lovitură {} spre {}.",
        tennis.m.shot.label_ro(),
        babilon_core::dir_label_ro(tennis.m.dir)
    ));
    Sfx::synth(
        sfx,
        Synth::Hit,
        tennis.m.incoming_side as f32 / MAX_DIR as f32,
        vols.ball,
    );
    if let Ok(t) = transforms.get(ball) {
        let at = t.translation.truncate();
        spawn_sparks(commands, pa, particles, at, false, 3);
        start_flight(
            flight,
            Vec2::new(t.translation.x, t.translation.y),
            Vec2::new(tennis.m.dir as f32 * LANE_STEP, 300.0),
            0.92,
            FlightAfter::ReturnLanded,
            Some(0.61),
        );
    }
    // stash the contact snapshot in the flight for the rally resolution
    flight.hit_quality = quality;
    flight.hit_dir = tennis.m.dir;
    flight.hit_shot = tennis.m.shot;
}

pub(crate) fn hit_timeout(
    time: Res<Time>,
    mut tennis: ResMut<Tennis>,
    targets: Res<Targets>,
    vols: Res<Volumes>,
    sfx: Option<Res<Sfx>>,
    balls: Query<Entity, With<Ball>>,
    mut transforms: Query<&mut Transform>,
) {
    if tennis.paused {
        return;
    }
    let expired = match &mut tennis.hit_window {
        Some(timer) => {
            timer.tick(time.delta());
            timer.finished()
        }
        None => false,
    };
    if expired {
        tennis.hit_window = None;
        // Distinct from the whiff ("too far") message: here the 1.4s hitting
        // window lapsed with no swing at all (Space never pressed in time),
        // so say so — otherwise a slightly-late press feels like a stolen point.
        let r = tennis
            .m
            .score_point(1, "Timp expirat! N-ai lovit la semnal");
        let msg = r.message_ro.clone();
        tennis.announce(msg);
        let sfx = sfx.as_deref();
        umpire_call(sfx, &vols, &r.umpire);
        Sfx::synth(sfx, Synth::Bad, 0.0, vols.ball);
        // park ball for the next serve (was left mid-court)
        if let Ok(ball) = balls.single() {
            let server = tennis.m.current_server();
            reset_ball_to_server(&mut transforms, ball, &targets, server);
        }
    }
}
