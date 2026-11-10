//! Composite athlete rigs, their pose timers, and paddle drift/animation.
use babilon_core::{Phase, LANE_STEP};
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::relationship::RelatedSpawnerCommands;
use bevy::prelude::*;
use bevy::render::mesh::Mesh2d;
use bevy::render::view::visibility::{InheritedVisibility, Visibility};
use bevy::sprite::MeshMaterial2d;

use crate::ball::NET_Y;
use crate::gameplay::{GameWorld, Targets};

#[derive(Component)]
pub(crate) struct PlayerPaddle;
#[derive(Component)]
pub(crate) struct OpponentPaddle;

/// Swing/serve pose timestamps (seconds on the Bevy clock). Mirrors
/// `swingUntil` / `serveUntil` in the web athlete renderer.
#[derive(Resource, Default)]
pub(crate) struct Poses {
    pub(crate) p_swing_until: f32,
    pub(crate) p_serve_until: f32,
    pub(crate) o_swing_until: f32,
}

/// Entity handles for one composite athlete (root + animated parts).
pub(crate) struct Rig {
    pub(crate) root: Entity,
    pub(crate) racket: Entity,
    pub(crate) arm_r: Entity,
    pub(crate) leg_l: Entity,
    pub(crate) leg_r: Entity,
    pub(crate) base_y: f32,
}

#[derive(Resource)]
pub(crate) struct Rigs {
    pub(crate) player: Rig,
    pub(crate) opp: Rig,
}

pub(crate) fn mesh_part(
    parent: &mut RelatedSpawnerCommands<ChildOf>,
    mats: &mut Assets<ColorMaterial>,
    mesh: Handle<Mesh>,
    color: Color,
    x: f32,
    y: f32,
    z: f32,
) -> Entity {
    parent
        .spawn((
            Mesh2d(mesh),
            MeshMaterial2d(mats.add(ColorMaterial::from(color))),
            Transform::from_xyz(x, y, z),
        ))
        .id()
}

pub(crate) fn rect_part(
    parent: &mut RelatedSpawnerCommands<ChildOf>,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<ColorMaterial>,
    w: f32,
    h: f32,
    color: Color,
    x: f32,
    y: f32,
    z: f32,
) -> Entity {
    let mesh = meshes.add(Rectangle::new(w, h));
    mesh_part(parent, mats, mesh, color, x, y, z)
}

pub(crate) fn circ_part(
    parent: &mut RelatedSpawnerCommands<ChildOf>,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<ColorMaterial>,
    r: f32,
    color: Color,
    x: f32,
    y: f32,
    z: f32,
) -> Entity {
    let mesh = meshes.add(Circle::new(r));
    mesh_part(parent, mats, mesh, color, x, y, z)
}

/// Composite athlete: shadow, striding legs, shoes, shirt, arms, head with
/// cap, and a strung racket on a rotating mount (idle / swing / serve).
pub(crate) fn spawn_athlete(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<ColorMaterial>,
    x: f32,
    y: f32,
    shirt: Color,
    shirt_dark: Color,
) -> Rig {
    let skin = Color::srgb(0.95, 0.73, 0.56);
    let navy = Color::srgb(0.08, 0.18, 0.25);
    let white = Color::srgb(0.94, 1.0, 0.97);
    let frame = Color::srgb(0.08, 0.20, 0.30);
    let strings = Color::srgb(0.97, 1.0, 0.98);
    let root = commands
        .spawn((
            Transform::from_xyz(x, y, 2.),
            // Bare parents need these or children warn B0004 and can
            // render inconsistently (Mesh2d children get theirs via #[require]).
            Visibility::default(),
            InheritedVisibility::default(),
        ))
        .id();
    let mut racket = Entity::PLACEHOLDER;
    let mut arm_r = Entity::PLACEHOLDER;
    let mut leg_l = Entity::PLACEHOLDER;
    let mut leg_r = Entity::PLACEHOLDER;
    commands.entity(root).with_children(|p| {
        // ground shadow
        let shadow_mesh = meshes.add(Ellipse::new(30.0, 8.0));
        mesh_part(
            p,
            mats,
            shadow_mesh,
            Color::srgb(0.0, 0.15, 0.18),
            2.,
            -50.,
            -0.5,
        );
        // legs + shoes (stride animated)
        leg_l = rect_part(p, meshes, mats, 12., 32., navy, -9., -28., 0.1);
        leg_r = rect_part(p, meshes, mats, 12., 32., navy, 9., -28., 0.1);
        rect_part(p, meshes, mats, 20., 9., white, -11., -46., 0.2);
        rect_part(p, meshes, mats, 20., 9., white, 11., -46., 0.2);
        // shirt with darker belt band
        rect_part(p, meshes, mats, 42., 36., shirt, 0., -4., 0.3);
        rect_part(p, meshes, mats, 42., 8., shirt_dark, 0., -18., 0.4);
        // left arm (static) + right upper arm (posed with the swing)
        rect_part(p, meshes, mats, 10., 24., skin, -27., -8., 0.3);
        arm_r = rect_part(p, meshes, mats, 10., 24., skin, 26., -4., 0.3);
        // head + hair + cap
        circ_part(p, meshes, mats, 15., skin, 0., 28., 0.5);
        rect_part(p, meshes, mats, 32., 9., white, 0., 38., 0.6);
        // racket mount (rotated by the animate system)
        let mut mount = p.spawn((
            Transform::from_xyz(32., 4., 0.7).with_rotation(Quat::from_rotation_z(0.42)),
            Visibility::default(),
            InheritedVisibility::default(),
        ));
        racket = mount.id();
        mount.with_children(|q| {
            // Racket held in the hand (mount origin): handle grips down
            // through the hand, head points UP (frame above the hand).
            rect_part(q, meshes, mats, 7., 30., navy, 0., 0., 0.1); // handle
            circ_part(q, meshes, mats, 17., frame, 0., 30., 0.2); // frame
            circ_part(q, meshes, mats, 13., strings, 0., 30., 0.3); // strings bed
            rect_part(q, meshes, mats, 1.5, 22., white, -6., 30., 0.4);
            rect_part(q, meshes, mats, 1.5, 22., white, 0., 30., 0.4);
            rect_part(q, meshes, mats, 1.5, 22., white, 6., 30., 0.4);
            rect_part(q, meshes, mats, 22., 1.5, white, 0., 24., 0.4);
            rect_part(q, meshes, mats, 22., 1.5, white, 0., 36., 0.4);
        });
    });
    // orient arm + racket mounts
    Rig {
        root,
        racket,
        arm_r,
        leg_l,
        leg_r,
        base_y: y,
    }
}

/// Paddles drift toward their targets (JS approach lerp), NOT toward
/// the ball — the old ball-follow hid the ←/→ aim completely.
/// Runs even with no flight active, so ←/→ moves the paddle + aim
/// marker visibly in Ready phase too.
#[allow(clippy::too_many_arguments)]
pub(crate) fn drift_paddles(world: GameWorld) {
    let GameWorld {
        time,
        tennis,
        targets,
        flight,
        balls,
        players,
        opps,
        mut transforms,
        ..
    } = world;
    if tennis.paused {
        return;
    }
    // Frame-rate independent approach (same feel at 30 or 144 fps).
    let dt = time.delta_secs().max(0.0);
    let player_k = 1.0 - (-dt * 7.0).exp();
    let opp_k = 1.0 - (-dt * 10.0).exp();
    let ball = balls.single().ok();
    let player = players.single().ok();
    let opp = opps.single().ok();
    if let Some(e) = player {
        if let Ok(mut p) = transforms.get_mut(e) {
            p.translation.x += (targets.player_x - p.translation.x) * player_k;
        }
    }
    // The opponent shadows the LIVE ball on his half and runs to meet it,
    // so his racket is at the ball when he "hits" — no more teleporting
    // to a stale target after the flight already started.
    let mut opp_target = targets.opp_x;
    if flight.active {
        if let Some(e) = ball {
            if let Ok(b) = transforms.get(e) {
                if b.translation.y > NET_Y {
                    opp_target = b.translation.x.clamp(-LANE_STEP * 2.0, LANE_STEP * 2.0);
                }
            }
        }
    }
    if let Some(e) = opp {
        if let Ok(mut o) = transforms.get_mut(e) {
            o.translation.x += (opp_target - o.translation.x) * opp_k;
        }
    }
    // Serve ball parked at the server's paddle: ←/→ in Ready phase moves
    // the ball too on your serve; on his serve it waits at his feet.
    if tennis.m.phase == Phase::Ready && !flight.active {
        if let Some(e) = ball {
            if let Ok(mut b) = transforms.get_mut(e) {
                if tennis.m.is_player_serve() {
                    b.translation.x = targets.player_x;
                    b.translation.y = -260.0;
                } else {
                    b.translation.x = targets.opp_x;
                    b.translation.y = 300.0;
                }
                b.scale = Vec3::ONE;
            }
        }
    }
}

/// Athlete animation: stride while running to a target, racket poses for
/// idle / swing / serve. Mirrors athlete() in index.html.
pub(crate) fn animate_athletes(
    time: Res<Time>,
    poses: Res<Poses>,
    targets: Res<Targets>,
    rigs: Res<Rigs>,
    mut q: Query<&mut Transform>,
) {
    let t = time.elapsed_secs();
    let sides = [
        (
            &rigs.player,
            targets.player_x,
            poses.p_swing_until,
            poses.p_serve_until,
        ),
        (
            &rigs.opp,
            targets.opp_x,
            poses.o_swing_until,
            f32::NEG_INFINITY,
        ),
    ];
    for (rig, target_x, swing_until, serve_until) in sides {
        let swinging = t < swing_until;
        let serving = t < serve_until;
        let move_f = q
            .get(rig.root)
            .map(|tr| ((target_x - tr.translation.x).abs() / 60.0).clamp(0.0, 1.0))
            .unwrap_or(0.0);
        let stride = (t * 9.0).sin() * 0.30 * move_f;
        if let Ok(mut root) = q.get_mut(rig.root) {
            root.translation.y = rig.base_y + move_f * (t * 9.0).cos() * 2.0;
        }
        if let Ok(mut l) = q.get_mut(rig.leg_l) {
            l.rotation = Quat::from_rotation_z(stride);
        }
        if let Ok(mut l) = q.get_mut(rig.leg_r) {
            l.rotation = Quat::from_rotation_z(-stride);
        }
        if let Ok(mut r) = q.get_mut(rig.racket) {
            r.rotation = Quat::from_rotation_z(if swinging {
                -0.85
            } else if serving {
                -2.3
            } else {
                0.42
            });
        }
        if let Ok(mut a) = q.get_mut(rig.arm_r) {
            a.rotation = Quat::from_rotation_z(if swinging {
                -0.7
            } else if serving {
                -2.0
            } else {
                -0.3
            });
        }
    }
}
