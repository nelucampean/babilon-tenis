//! Babilon Tennis — accessible desktop port (Bevy 2D).
//!
//! Same model as `index.html` v0.5:
//! S serve / Space hit / ←→ direction / 1-2-3 shot / R repeat / P pause / T test.
//! Screen readers get the status text via Bevy UI (AccessKit) + stdout line.
//! Stereo MP3s live in `assets/sunete/` (copied from the web version).

use babilon_core::{Difficulty, Dir, Match, Phase, RallyOutcome, Shot, LANE_STEP, MAX_DIR};
use accesskit::{Action, Live, Role};
use bevy::a11y::{AccessibilityNode, ActionRequest as A11yActionRequest};
use bevy::input_focus::{InputDispatchPlugin, InputFocus};
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::relationship::RelatedSpawnerCommands;
use bevy::log::LogPlugin;
use bevy::prelude::*;
use bevy::render::mesh::Mesh2d;
use bevy::render::view::visibility::{InheritedVisibility, Visibility};
use bevy::sprite::MeshMaterial2d;
use rand::Rng;

mod audio;
use audio::{AudioMode, Pending, Sfx, Synth, VolGroup, Volumes};
mod settings;

// ---- resources -------------------------------------------------------------

#[derive(Resource)]
struct Tennis {
    m: Match,
    difficulty: Difficulty,
    paused: bool,
    status_ro: String,
    hit_window: Option<Timer>,
}

impl Tennis {
    fn announce(&mut self, text: String) {
        self.status_ro = text.clone();
        // stdout line = picked up by Orca / terminal screen readers,
        // in addition to the Bevy UI text node (AccessKit).
        println!("[ANNOUNȚ] {text}");
    }
}

/// One ball flight, animated in `ball_flight`. Mirrors `trajectory()` in JS.
#[derive(Resource)]
struct Flight {
    active: bool,
    t: f32,
    duration: f32,
    from: Vec2,
    to: Vec2,
    bounce_at: Option<f32>,
    bounced: bool,
    after: FlightAfter,
    /// Contact quality of the swing that started this flight (0.44..1.0).
    hit_quality: f32,
    /// Aim + shot snapshotted at contact. The rally MUST resolve with what
    /// the racket did at hit time — not with whatever ←/→/1-2-3 the player
    /// mashes during the 0.92s return flight.
    hit_dir: Dir,
    hit_shot: Shot,
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

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum FlightAfter {
    #[default]
    None,
    ServeLanded,
    IncomingLanded,
    ReturnLanded,
}

// ---- entities --------------------------------------------------------------

#[derive(Component)]
struct Ball;
#[derive(Component)]
struct BallShadow;
#[derive(Component)]
struct PlayerPaddle;
#[derive(Component)]
struct OpponentPaddle;
#[derive(Component)]
struct ScoreText;
#[derive(Component)]
struct StatusText;
#[derive(Component)]
struct ShotText;
#[derive(Component)]
struct PhaseText;
#[derive(Component)]
struct ModeText;

/// Y of the net visual (see `setup`: net sprite at y=20). A return swing is
/// only legal once the ball is past it, on your side.
const NET_Y: f32 = 20.0;

/// Where paddles drift toward. Mirrors `player.target` / `opponent.target`
/// in index.html — direction keys move the player + aim even in Ready.
#[derive(Resource)]
struct Targets {
    player_x: f32,
    opp_x: f32,
}

/// Swing/serve pose timestamps (seconds on the Bevy clock). Mirrors
/// `swingUntil` / `serveUntil` in the web athlete renderer.
#[derive(Resource, Default)]
struct Poses {
    p_swing_until: f32,
    p_serve_until: f32,
    o_swing_until: f32,
}

/// Entity handles for one composite athlete (root + animated parts).
struct Rig {
    root: Entity,
    racket: Entity,
    arm_r: Entity,
    leg_l: Entity,
    leg_r: Entity,
    base_y: f32,
}

#[derive(Resource)]
struct Rigs {
    player: Rig,
    opp: Rig,
}

/// Spark particles. Mirrors `particles` + `spark()` in index.html.
struct Fx {
    entity: Entity,
    vel: Vec2,
    life: f32,
    max: f32,
}

#[derive(Resource, Default)]
struct Particles {
    items: Vec<Fx>,
}

#[derive(Resource)]
struct ParticleAssets {
    mesh: Handle<Mesh>,
    gold: Handle<ColorMaterial>,
    pale: Handle<ColorMaterial>,
}

/// Bundled UI typeface (Inter, full coverage incl. Romanian ș/ț/ă/â/î).
/// Loaded once in `setup`; every HUD/world label uses it.
#[derive(Resource)]
struct UiFonts {
    regular: Handle<Font>,
    bold: Handle<Font>,
}

/// Read-only edit box for one side-panel line.
///
/// Bevy 0.16 only auto-creates AccessKit nodes for `Button` / `Image` /
/// `Label` entities — a bare `Text` is invisible to JAWS/NVDA. A `Label`
/// node (`Role::Label`) is static text: screen readers can browse it with
/// the virtual cursor but it never takes focus, so Tab/focus navigation
/// skips it and the tree has no focusable leaves. A read-only
/// `MultilineTextInput` is focusable *and* readable (`value` = visible
/// text), so JAWS/NVDA can both browse and move focus onto each line —
/// a working accessible focus tree.
///
/// No `Label` component is attached on purpose: `bevy_ui::label_changed`
/// resets any explicit node on a `Label` entity back to `Role::Label` in
/// `PostUpdate`. A bare explicit `AccessibilityNode` still joins the tree
/// via its `ChildOf` chain, and keeps whatever role we give it.
/// The node value is kept in sync with the `Text` in `refresh_ui`.
fn readonly_box(label: &str, value: &str) -> AccessibilityNode {
    let mut node = accesskit::Node::new(Role::MultilineTextInput);
    node.set_label(label.to_string().into_boxed_str());
    node.set_value(value.to_string().into_boxed_str());
    node.set_read_only();
    node.add_action(Action::Focus);
    AccessibilityNode::from(node)
}

/// Live announcement box for the status line ("Acum lovește!", scores…).
///
/// Same read-only edit box as above, plus `Live::Polite` + atomic so
/// NVDA/JAWS speak every update without moving focus, like an ARIA live
/// region. Focusable too: `I` moves Bevy's `InputFocus` (mirrored to
/// AccessKit focus) here from the keyboard side, and a screen reader can
/// move focus onto it from its side.
fn live_box(label: &str, value: &str) -> AccessibilityNode {
    let mut node = accesskit::Node::new(Role::MultilineTextInput);
    node.set_label(label.to_string().into_boxed_str());
    node.set_value(value.to_string().into_boxed_str());
    node.set_read_only();
    node.set_live(Live::Polite);
    node.set_live_atomic();
    node.add_action(Action::Focus);
    AccessibilityNode::from(node)
}

/// Handles of the side info block, so a key press can move screen-reader
/// focus through it (`I` cycles, `Esc` back to the game).
///
/// `order` is the full Tab/focus order (visual order); `status` is the
/// live announcements box where the cycle starts.
#[derive(Resource)]
struct InfoPanel {
    status: Entity,
    order: Vec<Entity>,
}

fn set_node_value(node: &mut AccessibilityNode, text: &str) {
    node.set_value(text.to_string().into_boxed_str());
}

fn spawn_sparks(
    commands: &mut Commands,
    assets: &ParticleAssets,
    particles: &mut Particles,
    pos: Vec2,
    gold: bool,
    n: usize,
) {
    let mut rng = rand::thread_rng();
    for _ in 0..n {
        let a: f32 = rng.gen_range(0.0..std::f32::consts::TAU);
        let sp = rng.gen_range(60.0..220.0);
        let e = commands
            .spawn((
                Mesh2d(assets.mesh.clone()),
                MeshMaterial2d(if gold {
                    assets.gold.clone()
                } else {
                    assets.pale.clone()
                }),
                Transform::from_xyz(pos.x, pos.y, 5.0),
            ))
            .id();
        particles.items.push(Fx {
            entity: e,
            vel: Vec2::new(a.cos() * sp, a.sin() * sp + 60.0),
            life: 0.42,
            max: 0.42,
        });
    }
}

fn mesh_part(
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

fn rect_part(
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

fn circ_part(
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
fn spawn_athlete(
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
        mesh_part(p, mats, shadow_mesh, Color::srgb(0.0, 0.15, 0.18), 2., -50., -0.5);
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

// ---- setup -----------------------------------------------------------------

fn main() {
    // Last session's setup (difficulty, audio mode, volumes, stereo swap).
    let saved = settings::load();
    let vols = saved.vols;
    let mut app = App::new();
    // Quiet the MP3 demuxer chatter (unsupported GEOB id3 frames, VBR
    // duration estimates) — it prints one line pair per clip otherwise.
    // JACK/ALSA probing noise on stderr is harmless card-scanning.
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Babilon Tennis — accesibil".into(),
                    resolution: (1160_f32, 800_f32).into(),
                    ..default()
                }),
                ..default()
            })
            .set(LogPlugin {
                filter: "info,symphonia_core=warn,symphonia_bundle_mp3=warn,symphonia_metadata=warn".into(),
                ..default()
            }),
    )
    .insert_resource(Tennis {
        m: Match::new(),
        difficulty: saved.difficulty,
        paused: false,
        status_ro: "Meci nou. Apasă S pentru serviciu.".into(),
        hit_window: None,
    })
    .insert_resource(Flight::default())
    .insert_resource(Targets {
        player_x: 0.0,
        opp_x: 0.0,
    })
    .insert_resource(Poses::default())
    .insert_resource(Particles::default())
    .insert_resource(vols)
    .insert_resource(saved.mode)
    .insert_resource(Pending::default())
    // Input focus tracking: required for ANY AccessKit tree updates
    // (bevy_winit skips them entirely without `InputFocus`), and lets
    // `I` / `Esc` move screen-reader focus between game and info panel.
    .add_plugins(InputDispatchPlugin)
    .add_systems(Startup, setup)
    // chained: keyboard Space and the 1.4s miss-timer race otherwise,
    // and flight landings must run before the timeout check.
    .add_systems(
        Update,
        (
            keyboard,
            settings_keys,
            info_focus_keys,
            a11y_focus_requests,
            drift_paddles,
            ball_flight,
            particle_fx,
            hit_timeout,
            audio::play_pending,
            refresh_ui,
            animate_athletes,
        )
            .chain(),
    );
    // Stadium sound on its own thread (silent without an audio device).
    // Systems take `Option<Res<Sfx>>`; headless tests simply omit it.
    let sfx = Sfx::start(vols.crowd);
    sfx.set_swapped(saved.swap_stereo);
    app.insert_resource(sfx);
    app.run();
}

fn setup(
    mut commands: Commands,
    server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    commands.spawn(Camera2d);

    // Bundled Inter (full Romanian ș/ț/ă/â/î coverage, OFL-licensed,
    // same files on Linux / Windows / macOS / mobile).
    let fonts = UiFonts {
        regular: server.load("fonts/Inter-Regular.ttf"),
        bold: server.load("fonts/Inter-Bold.ttf"),
    };
    commands.insert_resource(UiFonts {
        regular: fonts.regular.clone(),
        bold: fonts.bold.clone(),
    });

    // Court: outer blue surround + inner court + white lines + net.
    // Mirrors `court()` in index.html (longitudinal court, player bottom).
    let court_w = 460.0;
    let court_h = 560.0;
    commands.spawn((
        Sprite {
            color: Color::srgb(0.03, 0.47, 0.55),
            custom_size: Some(Vec2::new(court_w + 70.0, court_h + 60.0)),
            ..default()
        },
        Transform::from_xyz(0., 20., -2.),
    ));
    commands.spawn((
        Sprite {
            color: Color::srgb(0.06, 0.49, 0.56),
            custom_size: Some(Vec2::new(court_w, court_h)),
            ..default()
        },
        Transform::from_xyz(0., 20., -1.),
    ));
    // Alternating lane bands, like the web court's striped bands.
    for i in 0..8 {
        commands.spawn((
            Sprite {
                color: if i % 2 == 0 {
                    Color::srgb(0.05, 0.52, 0.57)
                } else {
                    Color::srgb(0.04, 0.48, 0.53)
                },
                custom_size: Some(Vec2::new(court_w, court_h / 8.0)),
                ..default()
            },
            Transform::from_xyz(0., 20. - court_h / 2. + court_h / 16.0 + i as f32 * court_h / 8.0, -0.5),
        ));
    }
    // Border lines (4 thin white sprites).
    let line = Color::srgb(0.88, 1.0, 0.96);
    let mk_line = |w: f32, h: f32, x: f32, y: f32| {
        (
            Sprite {
                color: line,
                custom_size: Some(Vec2::new(w, h)),
                ..default()
            },
            Transform::from_xyz(x, y, 0.),
        )
    };
    commands.spawn(mk_line(court_w, 4., 0., 20. + court_h / 2.));
    commands.spawn(mk_line(court_w, 4., 0., 20. - court_h / 2.));
    commands.spawn(mk_line(4., court_h, -court_w / 2., 20.));
    commands.spawn(mk_line(4., court_h, court_w / 2., 20.));
    // Singles sidelines.
    commands.spawn(mk_line(3., court_h, -187., 20.));
    commands.spawn(mk_line(3., court_h, 187., 20.));
    // Service lines + centre service line.
    commands.spawn(mk_line(court_w, 3., 0., 20. + court_h * 0.24));
    commands.spawn(mk_line(court_w, 3., 0., 20. - court_h * 0.24));
    commands.spawn(mk_line(3., court_h * 0.48, 0., 20.));
    // Centre marks on both baselines.
    commands.spawn(mk_line(3., 14., 0., 20. + court_h / 2. - 7.));
    commands.spawn(mk_line(3., 14., 0., 20. - court_h / 2. + 7.));
    // Net bar + posts (net line sits at NET_Y = 20).
    commands.spawn((
        Sprite {
            color: Color::srgb(0.04, 0.13, 0.19),
            custom_size: Some(Vec2::new(court_w + 40.0, 12.0)),
            ..default()
        },
        Transform::from_xyz(0., NET_Y, 1.),
    ));
    commands.spawn(mk_line(7., 35., -court_w / 2. - 20., NET_Y));
    commands.spawn(mk_line(7., 35., court_w / 2. + 20., NET_Y));
    // Net mesh slats.
    for i in 0..19 {
        let x = -court_w / 2. + i as f32 * court_w / 18.0;
        commands.spawn((
            Sprite {
                color: Color::srgba(0.95, 1.0, 1.0, 0.35),
                custom_size: Some(Vec2::new(2., 12.)),
                ..default()
            },
            Transform::from_xyz(x, NET_Y, 1.1),
        ));
    }
    // Perimeter dashes (stadium trim).
    for i in 0..15 {
        let y = -330. + i as f32 * 46.0;
        commands.spawn(mk_line(5., 21., -court_w / 2. - 52., y));
        commands.spawn(mk_line(5., 21., court_w / 2. + 52., y));
    }
    // Arena banner (world-space text, like the web overlay). Name tags
    // live on the athletes themselves so they can never overlap them.
    commands.spawn((
        Text2d::new("B A B I L O N  O P E N"),
        TextFont {
            font: fonts.bold.clone(),
            font_size: 24.0,
            ..default()
        },
        TextColor(Color::srgb(0.61, 0.91, 0.86)),
        Transform::from_xyz(0., 385., 4.),
    ));

    // Composite athletes with rackets (see athlete() in index.html).
    let player_rig = spawn_athlete(
        &mut commands,
        &mut meshes,
        &mut materials,
        0.,
        -280.,
        Color::srgb(0.96, 0.79, 0.42),
        Color::srgb(0.66, 0.49, 0.26),
    );
    commands.entity(player_rig.root).insert(PlayerPaddle);
    let opp_rig = spawn_athlete(
        &mut commands,
        &mut meshes,
        &mut materials,
        0.,
        320.,
        Color::srgb(1.0, 0.43, 0.41),
        Color::srgb(0.72, 0.18, 0.22),
    );
    commands.entity(opp_rig.root).insert(OpponentPaddle);
    commands.insert_resource(Rigs {
        player: player_rig,
        opp: opp_rig,
    });

    // Ball: glowing yellow circle with a seam, plus a ground shadow that
    // tracks the court position while the ball itself arcs with height.
    let ball_mat = materials.add(ColorMaterial::from(Color::srgb(0.94, 1.0, 0.38)));
    let ball = commands
        .spawn((
            Ball,
            Mesh2d(meshes.add(Circle::new(12.0))),
            MeshMaterial2d(ball_mat),
            Transform::from_xyz(0., -260., 3.),
        ))
        .id();
    commands.entity(ball).with_children(|p| {
        p.spawn((
            Mesh2d(meshes.add(Rectangle::new(20.0, 2.5))),
            MeshMaterial2d(materials.add(ColorMaterial::from(Color::srgb(0.82, 0.9, 0.56)))),
            Transform::from_xyz(-1., 0., 0.1).with_rotation(Quat::from_rotation_z(-0.6)),
        ));
    });
    commands.spawn((
        BallShadow,
        Mesh2d(meshes.add(Ellipse::new(16.0, 7.0))),
        MeshMaterial2d(materials.add(ColorMaterial::from(Color::srgb(0.0, 0.15, 0.18)))),
        Transform::from_xyz(0., -260., 2.5),
    ));
    // Shared spark assets.
    commands.insert_resource(ParticleAssets {
        mesh: meshes.add(Circle::new(2.8)),
        gold: materials.add(ColorMaterial::from(Color::srgb(1.0, 0.89, 0.55))),
        pale: materials.add(ColorMaterial::from(Color::srgb(0.92, 1.0, 0.67))),
    });

    // Accessible HUD: side panel with Inter (full Romanian diacritics),
    // docked right of the court so no text covers the game. Every line is a
    // read-only multiline edit box (see `readonly_box`): focusable, so the
    // AccessKit tree has a working Tab/focus order, not just browsable
    // static labels. The status box is additionally a polite live region so
    // announcements speak automatically. The panel itself is a Region so the
    // block navigates as one group.
    let gold = Color::srgb(0.98, 0.79, 0.40);
    let ink = Color::srgb(0.93, 0.97, 0.97);
    let muted = Color::srgb(0.72, 0.82, 0.85);
    let panel = Color::srgba(0.03, 0.10, 0.16, 0.96);
    let reg = fonts.regular.clone();
    let bold = fonts.bold.clone();
    let title_txt = "BABILON TENNIS";
    let score_txt = "Seturi 0 – 0\nGame-uri 0 – 0\nTu 0, calculator 0";
    let status_txt = "Meci nou. Apasă S pentru serviciu.";
    let shot_txt = "Direcție: centru • Lovitură: plată";
    let phase_txt = "PREGĂTIT PENTRU SERVICIU";
    let mode_txt = "Accesibil • Normal • Volum 100/100/65 • Stereo normal";
    let help_txt = "S serviciu • Spațiu lovire\n←/→ direcție • 1/2/3 lovitură\nR repetă • P pauză • T test audio\nG dificultate • M mod audio • N stereo\nC grup volum • Z/X volum\nI focusează anunțurile (repetat = câmpul următor) • Esc înapoi la joc";
    let mut order: Vec<Entity> = Vec::with_capacity(7);
    let mut info_status = Entity::PLACEHOLDER;
    commands
        .spawn(Node {
            width: Val::Percent(100.),
            height: Val::Percent(100.),
            justify_content: JustifyContent::FlexEnd,
            ..default()
        })
        .with_children(|root| {
            let mut panel_node = accesskit::Node::new(Role::Region);
            panel_node.set_label("Panou informații joc");
            // Focusable, so JAWS/NVDA can land on the whole block and then
            // browse its lines with the virtual cursor.
            panel_node.add_action(Action::Focus);
            root.spawn((
                Node {
                    width: Val::Px(268.),
                    height: Val::Percent(100.),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(Val::Px(14.)),
                    row_gap: Val::Px(10.),
                    ..default()
                },
                BackgroundColor(panel),
                AccessibilityNode::from(panel_node),
            ))
            .with_children(|p| {
                order.push(
                    p.spawn((
                        Text::new(title_txt),
                        TextFont { font: bold.clone(), font_size: 15.0, ..default() },
                        TextColor(gold),
                        readonly_box("Titlu joc", title_txt),
                    ))
                    .id(),
                );
                order.push(
                    p.spawn((
                        ScoreText,
                        Text::new(score_txt),
                        TextFont { font: bold.clone(), font_size: 19.0, ..default() },
                        TextColor(ink),
                        readonly_box("Scor", score_txt),
                    ))
                    .id(),
                );
                info_status = p.spawn((
                    StatusText,
                    Text::new(status_txt),
                    TextFont { font: reg.clone(), font_size: 19.0, ..default() },
                    TextColor(ink),
                    live_box("Anunțuri", status_txt),
                )).id();
                order.push(info_status);
                order.push(
                    p.spawn((
                        ShotText,
                        Text::new(shot_txt),
                        TextFont { font: reg.clone(), font_size: 14.0, ..default() },
                        TextColor(ink),
                        readonly_box("Direcție și lovitură", shot_txt),
                    ))
                    .id(),
                );
                order.push(
                    p.spawn((
                        PhaseText,
                        Text::new(phase_txt),
                        TextFont { font: bold.clone(), font_size: 13.0, ..default() },
                        TextColor(gold),
                        readonly_box("Fază joc", phase_txt),
                    ))
                    .id(),
                );
                order.push(
                    p.spawn((
                        ModeText,
                        Text::new(mode_txt),
                        TextFont { font: reg.clone(), font_size: 13.0, ..default() },
                        TextColor(muted),
                        readonly_box("Mod audio și volum", mode_txt),
                    ))
                    .id(),
                );
                order.push(
                    p.spawn((
                        Text::new(help_txt),
                        TextFont { font: reg.clone(), font_size: 12.0, ..default() },
                        TextColor(muted),
                        readonly_box("Ajutor taste", help_txt),
                    ))
                    .id(),
                );
            });
        });
    // Remember the order + live box so `I` can walk screen-reader focus
    // through the whole block and `Esc` returns to the game.
    commands.insert_resource(InfoPanel { status: info_status, order });
}

// ---- game flow (mirrors serve()/incoming()/hit()/point() in JS) ------------
// Sound comes from `audio::Sfx` (rodio): positioned synth SFX + MP3 clips.
// `sfx: Option<&Sfx>` is `None` headless, so every call is skip-safe.

fn start_flight(flight: &mut Flight, from: Vec2, to: Vec2, duration: f32, after: FlightAfter, bounce_at: Option<f32>) {
    flight.active = true;
    flight.t = 0.0;
    flight.duration = duration;
    flight.from = from;
    flight.to = to;
    flight.bounce_at = bounce_at;
    flight.bounced = false;
    flight.after = after;
}

fn do_serve(
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
        tennis.announce("Meci încheiat. Apasă butonul de meci nou (în curând) sau repornește jocul.".into());
        return;
    }
    if tennis.m.phase != Phase::Ready {
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

fn do_incoming(
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
    tennis.announce(format!("Mingea vine spre {}. Pregătește-te!", dir_label_ro(side)));
    // Mid-flight guide beep, like the web `setTimeout(..., 490)` — Access
    // mode only; Realist keeps just bounces and the landing cue.
    if mode == AudioMode::Access {
        pending.0.push(audio::Scheduled { at: now + 0.49, kind: Synth::Guide, pan: side as f32 / MAX_DIR as f32, vol: vols.ball });
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

/// After a point, park the ball at the player so the next serve starts clean.
/// Mirrors JS, where the ball hides and reappears at the server.
fn reset_ball_to_player(transforms: &mut Query<&mut Transform>, ball: Entity, targets: &Targets) {
    if let Ok(mut b) = transforms.get_mut(ball) {
        b.translation.x = targets.player_x;
        b.translation.y = -260.0;
        b.scale = Vec3::ONE;
    }
}

fn land_serve(
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
        reset_ball_to_player(transforms, ball, targets);
    } else {
        do_incoming(tennis, targets, poses, now, mode, vols, pending, flight, ball, transforms);
    }
}

/// Umpire voice, always centered. Tie-break scores have no MP3 (same as the
/// web version) — falls back to a TTS-style console line.
fn umpire_call(sfx: Option<&Sfx>, vols: &Volumes, call: &babilon_core::UmpireCall) {
    if call.clip_key.is_some() {
        Sfx::umpire(sfx, call, vols.umpire);
    } else {
        println!("[ARBITRU TTS] {}", call.text_en);
    }
}

fn land_incoming(
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
    Sfx::synth(sfx, Synth::Cue, tennis.m.incoming_side as f32 / MAX_DIR as f32, amp);
}

fn land_return(
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
            let reason = if outcome == RallyOutcome::Out {
                "AUT! Mingea ta a ieșit afară"
            } else if w == 0 {
                "Punct câștigat"
            } else {
                "Adversarul a câștigat punctul"
            };
            let r = tennis.m.score_point(w, reason);
            let msg = r.message_ro.clone();
            tennis.announce(msg);
            if reason.contains("AUT") {
                Sfx::clip(sfx, "out", 0.0, vols.umpire);
            }
            umpire_call(sfx, vols, &r.umpire);
            if w == 0 {
                Sfx::clip(sfx, "applause", 0.0, vols.crowd);
                Sfx::synth(sfx, Synth::Good, 0.0, vols.ball);
                spawn_sparks(commands, pa, particles, flight.to, true, 7);
            } else {
                Sfx::synth(sfx, Synth::Bad, 0.0, vols.ball);
                if rand::thread_rng().gen::<f32>() < 0.40 {
                    Sfx::clip(sfx, "reaction", 0.0, vols.crowd);
                }
            }
            flight.active = false;
            reset_ball_to_player(transforms, ball, targets);
        }
        None => do_incoming(tennis, targets, poses, now, mode, vols, pending, flight, ball, transforms),
    }
}

fn do_hit(
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
    let mut quality = match tennis.m.phase {
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
                && flight.after == FlightAfter::IncomingLanded
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
            tennis.announce("Apasă S pentru serviciu, apoi Spațiu la semnal.".into());
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
        reset_ball_to_player(transforms, ball, &targets);
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
    Sfx::synth(sfx, Synth::Hit, tennis.m.incoming_side as f32 / MAX_DIR as f32, vols.ball);
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

// ---- systems ---------------------------------------------------------------

fn keyboard(
    mut commands: Commands,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut tennis: ResMut<Tennis>,
    mut targets: ResMut<Targets>,
    mut poses: ResMut<Poses>,
    vols: Res<Volumes>,
    sfx: Option<Res<Sfx>>,
    pa: Res<ParticleAssets>,
    mut particles: ResMut<Particles>,
    mut flight: ResMut<Flight>,
    balls: Query<Entity, With<Ball>>,
    players: Query<Entity, With<PlayerPaddle>>,
    opps: Query<Entity, With<OpponentPaddle>>,
    mut transforms: Query<&mut Transform>,
) {
    let sfx = sfx.as_deref();
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
        Sfx::synth(sfx, Synth::Foe(lane), lane as f32 / MAX_DIR as f32, vols.ball);
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
            tennis.announce("Pauză. Apasă P pentru reluare.".into());
        } else {
            tennis.m.phase = Phase::Ready;
            tennis.announce("Meci reluat. Apasă S pentru serviciu.".into());
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
        Sfx::synth(sfx, Synth::Lane(d), d as f32 / MAX_DIR as f32, vols.ball);
        tennis.announce(format!("Direcție: {}", babilon_core::dir_label_ro(d)));
    }
    if keys.just_pressed(KeyCode::ArrowRight) {
        tennis.m.nudge_dir(1);
        let d = tennis.m.dir;
        targets.player_x = d as f32 * LANE_STEP;
        Sfx::synth(sfx, Synth::Lane(d), d as f32 / MAX_DIR as f32, vols.ball);
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
            do_serve(sfx, &vols, &mut tennis, &mut targets, &mut poses, time.elapsed_secs(), &mut flight, ball, &mut transforms);
        }
    }
    if keys.just_pressed(KeyCode::Space) {
        if let (Ok(ball), Ok(player)) = (balls.single(), players.single()) {
            do_hit(&mut commands, sfx, &vols, &mut tennis, &mut targets, &mut poses, time.elapsed_secs(), &mut flight, ball, player, &mut transforms, &pa, &mut particles);
        }
    }
}

/// Housekeeping keys: stadium test, difficulty, audio mode, volume groups,
/// output device. Split out of `keyboard` — Bevy caps function systems at
/// 16 parameters, and settings deserve their own system anyway.
///
/// Every branch that mutates setup calls [`persist_settings`] so the next
/// launch resumes where you left off.
fn persist_settings(tennis: &Tennis, vols: &Volumes, mode: &AudioMode, sfx: Option<&Sfx>) {
    settings::save(
        tennis.difficulty,
        *mode,
        vols,
        sfx.map(|s| s.swapped()).unwrap_or(false),
    );
}

#[allow(clippy::too_many_arguments)]
fn settings_keys(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut tennis: ResMut<Tennis>,
    mut vols: ResMut<Volumes>,
    mut mode: ResMut<AudioMode>,
    mut pending: ResMut<Pending>,
    sfx: Option<Res<Sfx>>,
) {
    let sfx = sfx.as_deref();
    if keys.just_pressed(KeyCode::KeyT) {
        // Stadium test + stereo channel check: hard-left blip now,
        // hard-right blip half a second later. If "stânga" comes out of
        // your right ear, your headset/speakers are swapped.
        Sfx::synth(sfx, Synth::Guide, -1.0, vols.ball);
        let now = time.elapsed_secs();
        pending.0.push(audio::Scheduled { at: now + 0.5, kind: Synth::Guide, pan: 1.0, vol: vols.ball });
        Sfx::synth(sfx, Synth::Bounce, 0.65, vols.ball);
        Sfx::umpire(sfx, &babilon_core::UmpireCall { text_en: "Fifteen love".into(), clip_key: Some("fifteen-love".into()) }, vols.umpire);
        Sfx::clip(sfx, "applause", 0.0, vols.crowd);
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
        tennis.announce(format!("Grup volum: {}. {}%. Z scade, X crește.", vols.group.label_ro(), (v * 100.0) as u32));
        persist_settings(&tennis, &vols, &mode, sfx);
        return;
    }
    if keys.just_pressed(KeyCode::KeyZ) || keys.just_pressed(KeyCode::KeyX) {
        let delta = if keys.just_pressed(KeyCode::KeyZ) { -0.1 } else { 0.1 };
        let (group, v) = vols.adjust(delta);
        if let Some(s) = sfx {
            s.refresh_ambient(vols.crowd);
        }
        tennis.announce(format!("Volum {}: {}%.", group.label_ro(), (v * 100.0) as u32));
        persist_settings(&tennis, &vols, &mode, sfx);
    }
}

/// Screen-reader focus control for the info block.
///
/// * `I` walks Bevy's `InputFocus` (mirrored to AccessKit focus) through
///   the read-only boxes in visual order, starting at the live status box,
///   so JAWS/NVDA jump to the announcements and keep speaking them, then
///   to score / shot / phase / volumes on repeated presses.
/// * `Esc` clears focus back to the game window, so play keys stay first.
/// Game input reads `ButtonInput` directly, so focus never breaks play.
fn info_focus_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut focus: ResMut<InputFocus>,
    panel: Option<Res<InfoPanel>>,
) {
    if keys.just_pressed(KeyCode::KeyI) {
        if let Some(p) = panel {
            if p.order.is_empty() {
                return;
            }
            let next = match focus.get() {
                Some(cur) => p
                    .order
                    .iter()
                    .position(|&e| e == cur)
                    .map(|i| p.order[(i + 1) % p.order.len()])
                    .unwrap_or(p.status),
                None => p.status,
            };
            focus.set(next);
        }
    } else if keys.just_pressed(KeyCode::Escape) {
        focus.clear();
    }
}

/// Screen-reader → game focus requests.
///
/// When JAWS/NVDA itself moves (virtual cursor, touch exploration), winit
/// forwards an AccessKit `Focus` action as an `ActionRequest` event — honor
/// it so focus stays in sync both ways. Unknown/stale targets are ignored.
fn a11y_focus_requests(
    mut events: EventReader<A11yActionRequest>,
    mut focus: ResMut<InputFocus>,
    nodes: Query<(), With<AccessibilityNode>>,
) {
    for req in events.read() {
        if req.action == Action::Focus {
            let e = Entity::from_bits(req.target.0);
            if nodes.contains(e) {
                focus.set(e);
            }
        }
    }
}

/// Paddles drift toward their targets (JS approach lerp), NOT toward
/// the ball — the old ball-follow hid the ←/→ aim completely.
/// Runs even with no flight active, so ←/→ moves the paddle + aim
/// marker visibly in Ready phase too.
#[allow(clippy::too_many_arguments)]
fn drift_paddles(
    time: Res<Time>,
    tennis: Res<Tennis>,
    targets: Res<Targets>,
    flight: Res<Flight>,
    balls: Query<Entity, With<Ball>>,
    players: Query<Entity, With<PlayerPaddle>>,
    opps: Query<Entity, With<OpponentPaddle>>,
    mut transforms: Query<&mut Transform>,
) {
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
    // Serve ball parked on your paddle: ←/→ in Ready phase moves the
    // ball too, so the serve starts exactly where you stand.
    if tennis.m.phase == Phase::Ready && !flight.active {
        if let Some(e) = ball {
            if let Ok(mut b) = transforms.get_mut(e) {
                b.translation.x = targets.player_x;
                b.translation.y = -260.0;
                b.scale = Vec3::ONE;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn ball_flight(
    mut commands: Commands,
    time: Res<Time>,
    mut tennis: ResMut<Tennis>,
    mut targets: ResMut<Targets>,
    mut poses: ResMut<Poses>,
    vols: Res<Volumes>,
    mode: Res<AudioMode>,
    mut pending: ResMut<Pending>,
    sfx: Option<Res<Sfx>>,
    pa: Res<ParticleAssets>,
    mut particles: ResMut<Particles>,
    mut flight: ResMut<Flight>,
    balls: Query<Entity, With<Ball>>,
    shadows: Query<Entity, With<BallShadow>>,
    mut transforms: Query<&mut Transform>,
) {
    let sfx = sfx.as_deref();
    let ball = balls.single().ok();
    let shadow = shadows.single().ok();
    if !flight.active || tennis.paused {
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
            Sfx::synth(sfx, Synth::Bounce, pan, vols.ball);
        }
    }

    if t >= 1.0 {
        let after = flight.after;
        let now = time.elapsed_secs();
        flight.active = false;
        if let Some(b) = ball {
            match after {
                FlightAfter::ServeLanded => land_serve(&mut commands, sfx, &vols, *mode, &mut pending, &mut tennis, &mut targets, &mut poses, now, &mut flight, b, &mut transforms, &pa, &mut particles),
                FlightAfter::IncomingLanded => land_incoming(sfx, &vols, *mode, &mut tennis),
                FlightAfter::ReturnLanded => {
                    land_return(&mut commands, sfx, &vols, *mode, &mut pending, &mut tennis, &mut targets, &mut poses, now, &mut flight, b, &mut transforms, &pa, &mut particles);
                }
                FlightAfter::None => {}
            }
        }
    }
}

/// Spark particles: rise, fall with gravity, shrink and despawn.
fn particle_fx(
    mut commands: Commands,
    time: Res<Time>,
    mut particles: ResMut<Particles>,
    mut q: Query<&mut Transform>,
) {
    let dt = time.delta_secs();
    particles.items.retain_mut(|p| {
        p.life -= dt;
        if p.life <= 0.0 {
            commands.entity(p.entity).despawn();
            return false;
        }
        p.vel.y -= 520.0 * dt;
        if let Ok(mut tr) = q.get_mut(p.entity) {
            tr.translation.x += p.vel.x * dt;
            tr.translation.y += p.vel.y * dt;
            let k = (p.life / p.max).clamp(0.0, 1.0);
            tr.scale = Vec3::splat(k.max(0.15));
        }
        true
    });
}

/// Athlete animation: stride while running to a target, racket poses for
/// idle / swing / serve. Mirrors athlete() in index.html.
fn animate_athletes(
    time: Res<Time>,
    poses: Res<Poses>,
    targets: Res<Targets>,
    rigs: Res<Rigs>,
    mut q: Query<&mut Transform>,
) {
    let t = time.elapsed_secs();
    let sides = [
        (&rigs.player, targets.player_x, poses.p_swing_until, poses.p_serve_until),
        (&rigs.opp, targets.opp_x, poses.o_swing_until, f32::NEG_INFINITY),
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

fn hit_timeout(
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
        let r = tennis.m.score_point(1, "Timp expirat! N-ai lovit la semnal");
        let msg = r.message_ro.clone();
        tennis.announce(msg);
        let sfx = sfx.as_deref();
        umpire_call(sfx, &vols, &r.umpire);
        Sfx::synth(sfx, Synth::Bad, 0.0, vols.ball);
        // park ball for the next serve (was left mid-court)
        if let Ok(ball) = balls.single() {
            reset_ball_to_player(&mut transforms, ball, &targets);
        }
    }
}

fn refresh_box(node: &mut AccessibilityNode, label: &str, value: &str, live: bool) {
    // Self-healing: re-assert the read-only-box shape every refresh so the
    // focus tree stays intact even if another system touched the node.
    node.set_role(Role::MultilineTextInput);
    node.set_label(label.to_string().into_boxed_str());
    node.set_read_only();
    node.add_action(Action::Focus);
    if live {
        node.set_live(Live::Polite);
        node.set_live_atomic();
    }
    set_node_value(node, value);
}

fn refresh_ui(
    tennis: Res<Tennis>,
    vols: Res<Volumes>,
    mode: Res<AudioMode>,
    sfx: Option<Res<Sfx>>,
    mut score_q: Query<(&mut Text, Option<&mut AccessibilityNode>), With<ScoreText>>,
    mut status_q: Query<(&mut Text, Option<&mut AccessibilityNode>), (With<StatusText>, Without<ScoreText>)>,
    mut shot_q: Query<(&mut Text, Option<&mut AccessibilityNode>), (With<ShotText>, Without<ScoreText>, Without<StatusText>)>,
    mut phase_q: Query<(&mut Text, Option<&mut AccessibilityNode>), (With<PhaseText>, Without<ScoreText>, Without<StatusText>, Without<ShotText>)>,
    mut mode_q: Query<(&mut Text, Option<&mut AccessibilityNode>), (With<ModeText>, Without<ScoreText>, Without<StatusText>, Without<ShotText>, Without<PhaseText>)>,
) {
    if !tennis.is_changed() && !vols.is_changed() && !mode.is_changed() {
        return;
    }
    let score_line = format!(
        "Seturi {} – {}\nGame-uri {} – {}\n{}",
        tennis.m.sets[0],
        tennis.m.sets[1],
        tennis.m.games[0],
        tennis.m.games[1],
        tennis.m.score_text_ro()
    );
    for (mut t, a11y) in &mut score_q {
        **t = score_line.clone();
        if let Some(mut node) = a11y {
            refresh_box(&mut node, "Scor", &score_line, false);
        }
    }
    for (mut t, a11y) in &mut status_q {
        **t = tennis.status_ro.clone();
        if let Some(mut node) = a11y {
            refresh_box(&mut node, "Anunțuri", &tennis.status_ro, true);
        }
    }
    let shot_line = format!(
        "Direcție: {} • Lovitură: {}",
        babilon_core::dir_label_ro(tennis.m.dir),
        tennis.m.shot.label_ro()
    );
    for (mut t, a11y) in &mut shot_q {
        **t = shot_line.clone();
        if let Some(mut node) = a11y {
            refresh_box(&mut node, "Direcție și lovitură", &shot_line, false);
        }
    }
    let phase_line = if tennis.paused {
        "PAUZĂ".to_string()
    } else {
        tennis.m.phase.visual_label_ro().to_string()
    };
    for (mut t, a11y) in &mut phase_q {
        **t = phase_line.clone();
        if let Some(mut node) = a11y {
            refresh_box(&mut node, "Fază joc", &phase_line, false);
        }
    }
    let diff_label = match tennis.difficulty {
        Difficulty::Easy => "Ușor",
        Difficulty::Normal => "Normal",
        Difficulty::Hard => "Greu",
    };
    let swapped = sfx.as_deref().map(|s| s.swapped()).unwrap_or(false);
    let mode_line = format!(
        "{} • {} • Volum {}/{}/{} • Stereo {}",
        mode.label_ro(),
        diff_label,
        (vols.ball * 100.0) as u32,
        (vols.umpire * 100.0) as u32,
        (vols.crowd * 100.0) as u32,
        if swapped { "invers" } else { "normal" }
    );
    for (mut t, a11y) in &mut mode_q {
        **t = mode_line.clone();
        if let Some(mut node) = a11y {
            refresh_box(&mut node, "Mod audio și volum", &mode_line, false);
        }
    }
}

#[cfg(test)]
mod smoke {
    //! Headless boot of the real system schedule. Catches query conflicts
    //! (B0001 — only raised at first schedule run, not by `cargo check`)
    //! and plays a full point without a display or audio device.
    use super::*;
    use bevy::input::InputPlugin;

    fn rig_entities(app: &mut App, x: f32, y: f32) -> Rig {
        let root = app.world_mut().spawn(Transform::from_xyz(x, y, 2.)).id();
        let racket = app.world_mut().spawn(Transform::default()).id();
        let arm_r = app.world_mut().spawn(Transform::default()).id();
        let leg_l = app.world_mut().spawn(Transform::default()).id();
        let leg_r = app.world_mut().spawn(Transform::default()).id();
        Rig {
            root,
            racket,
            arm_r,
            leg_l,
            leg_r,
            base_y: y,
        }
    }

    fn test_app() -> App {
        let mut app = App::new();
        // No audio device headless: no `Sfx` resource, so every `Sfx::*`
        // call takes the `None` path (systems use `Option<Res<Sfx>>`).
        app.add_plugins((MinimalPlugins, InputPlugin));
        app.insert_resource(Tennis {
            m: Match::new(),
            difficulty: Difficulty::Normal,
            paused: false,
            status_ro: String::new(),
            hit_window: None,
        });
        app.insert_resource(Flight::default());
        app.insert_resource(Targets {
            player_x: 0.0,
            opp_x: 0.0,
        });
        app.insert_resource(Poses::default());
        app.insert_resource(Particles::default());
        app.insert_resource(Volumes::default());
        app.insert_resource(AudioMode::Access);
        app.insert_resource(Pending::default());
        app.insert_resource(ParticleAssets {
            mesh: Handle::default(),
            gold: Handle::default(),
            pale: Handle::default(),
        });
        // Focus plumbing (mirrors the real app: InputDispatchPlugin would
        // init these; headless we add the pieces the new systems need).
        app.init_resource::<InputFocus>();
        app.add_event::<A11yActionRequest>();
        let status_e = app
            .world_mut()
            .spawn((StatusText, live_box("Anunțuri", "test")))
            .id();
        let score_e = app
            .world_mut()
            .spawn((ScoreText, readonly_box("Scor", "test")))
            .id();
        app.insert_resource(InfoPanel {
            status: status_e,
            order: vec![score_e, status_e],
        });
        app.add_systems(
            Update,
            (
                keyboard,
                settings_keys,
                info_focus_keys,
                a11y_focus_requests,
                drift_paddles,
                ball_flight,
                particle_fx,
                hit_timeout,
                audio::play_pending,
                refresh_ui,
                animate_athletes,
            )
                .chain(),
        );
        app.world_mut().spawn((Ball, Transform::from_xyz(0., -260., 3.)));
        let player = rig_entities(&mut app, 0., -280.);
        let opp = rig_entities(&mut app, 0., 320.);
        // link rig roots to the paddle markers the systems drive
        app.world_mut().entity_mut(player.root).insert(PlayerPaddle);
        app.world_mut().entity_mut(opp.root).insert(OpponentPaddle);
        app.insert_resource(Rigs { player, opp });
        app
    }

    fn press(app: &mut App, key: KeyCode) {
        // NOTE: run only the `Update` schedule — `InputPlugin` clears
        // `just_pressed` in `PreUpdate`, so a full `app.update()` would wipe
        // the manual press before `keyboard` sees it. `update()` at the end
        // replicates the frame-end clear so stale keys don't re-fire.
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(key);
        app.world_mut().run_schedule(Update);
        // `reset_all` (not `clear`: that one keeps `pressed`, so a repeat
        // press of the same key would never set `just_pressed` again).
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().reset_all();
    }

    #[test]
    fn systems_boot_without_query_conflict() {
        bevy::ecs::system::assert_is_system(keyboard);
        bevy::ecs::system::assert_is_system(settings_keys);
        bevy::ecs::system::assert_is_system(info_focus_keys);
        bevy::ecs::system::assert_is_system(a11y_focus_requests);
        bevy::ecs::system::assert_is_system(drift_paddles);
        bevy::ecs::system::assert_is_system(ball_flight);
        bevy::ecs::system::assert_is_system(particle_fx);
        bevy::ecs::system::assert_is_system(hit_timeout);
        bevy::ecs::system::assert_is_system(audio::play_pending);
        bevy::ecs::system::assert_is_system(refresh_ui);
        bevy::ecs::system::assert_is_system(animate_athletes);
        let mut app = test_app();
        app.update();
        app.update();
    }

    #[test]
    fn arrows_move_paddle_target_and_announce() {
        let mut app = test_app();
        press(&mut app, KeyCode::ArrowLeft);
        assert_eq!(app.world().resource::<Targets>().player_x, -LANE_STEP);
        assert!(app.world().resource::<Tennis>().status_ro.contains("mijloc-stânga"));
        press(&mut app, KeyCode::ArrowLeft);
        assert_eq!(app.world().resource::<Targets>().player_x, -2.0 * LANE_STEP);
        assert!(app.world().resource::<Tennis>().status_ro.contains("stânga"));
        for _ in 0..4 {
            press(&mut app, KeyCode::ArrowRight);
        }
        assert_eq!(app.world().resource::<Targets>().player_x, 2.0 * LANE_STEP);
        assert!(app.world().resource::<Tennis>().status_ro.contains("dreapta"));
    }

    #[test]
    fn far_from_ball_swings_at_air() {
        // Swinging from >120px away is a whiff: point to the opponent,
        // no phantom contact no matter how well timed the press is.
        let mut app = test_app();
        press(&mut app, KeyCode::KeyS);
        app.world_mut().resource_mut::<Flight>().t = 100.0;
        app.update();
        if app.world().resource::<Tennis>().m.phase == Phase::Ready {
            assert_eq!(app.world().resource::<Tennis>().m.pts[0], 1);
            return; // serve ace (8%), nothing to whiff at
        }
        app.world_mut().resource_mut::<Flight>().t = 100.0;
        app.update();
        assert_eq!(app.world().resource::<Tennis>().m.phase, Phase::Hit);
        // park the paddle 200px from where the ball landed: unwinnable
        // position, regardless of timing
        let landing_x = app.world().resource::<Flight>().to.x;
        let paddle = app.world().resource::<Rigs>().player.root;
        app.world_mut().get_mut::<Transform>(paddle).unwrap().translation.x = landing_x + 200.0;
        press(&mut app, KeyCode::Space);
        let t = app.world().resource::<Tennis>();
        assert_eq!(t.m.phase, Phase::Ready, "whiff ends the point, no return flight");
        assert_eq!(t.m.pts, [0, 1]);
        assert!(t.status_ro.contains("departe"));
        assert!(t.hit_window.is_none());
    }

    #[test]
    fn early_space_swings_instead_of_scolding() {
        // Space once the ball crossed the net (even before the HIT cue)
        // starts the return — never "too early", never a silent timeout.
        let mut app = test_app();
        press(&mut app, KeyCode::KeyS);
        assert_eq!(app.world().resource::<Tennis>().m.phase, Phase::Serve);
        // serve flight lands -> incoming starts (8% ace ends the point)
        app.world_mut().resource_mut::<Flight>().t = 100.0;
        app.update();
        if app.world().resource::<Tennis>().m.phase == Phase::Ready {
            assert_eq!(app.world().resource::<Tennis>().m.pts[0], 1);
            return;
        }
        assert_eq!(app.world().resource::<Tennis>().m.phase, Phase::Incoming);
        // too early: ball still on the opponent's side -> clear message, no swing
        press(&mut app, KeyCode::Space);
        assert_eq!(app.world().resource::<Tennis>().m.phase, Phase::Incoming);
        assert!(app.world().resource::<Tennis>().status_ro.contains("fileul"));
        // ball past the net (progress 0.7, y well below NET_Y): step to
        // where it will land, then the swing fires
        {
            let dur = app.world().resource::<Flight>().duration;
            app.world_mut().resource_mut::<Flight>().t = 0.7 * dur;
        }
        app.update();
        {
            let landing_x = app.world().resource::<Flight>().to.x;
            let paddle = app.world().resource::<Rigs>().player.root;
            app.world_mut().get_mut::<Transform>(paddle).unwrap().translation.x = landing_x;
        }
        press(&mut app, KeyCode::Space);
        assert_eq!(app.world().resource::<Tennis>().m.phase, Phase::Return);
    }

    #[test]
    fn sound_keys_update_volumes_mode_difficulty() {
        let mut app = test_app();
        press(&mut app, KeyCode::KeyG);
        assert_eq!(app.world().resource::<Tennis>().difficulty, Difficulty::Hard);
        press(&mut app, KeyCode::KeyM);
        assert_eq!(*app.world().resource::<AudioMode>(), AudioMode::Real);
        press(&mut app, KeyCode::KeyC);
        assert_eq!(app.world().resource::<Volumes>().group, VolGroup::Umpire);
        press(&mut app, KeyCode::KeyX);
        assert!((app.world().resource::<Volumes>().umpire - 1.1).abs() < 1e-6);
        press(&mut app, KeyCode::KeyZ);
        assert!((app.world().resource::<Volumes>().umpire - 1.0).abs() < 1e-6);
        // N with no audio engine: answers gracefully, nothing toggles nowhere.
        press(&mut app, KeyCode::KeyN);
        assert!(app.world().resource::<Tennis>().status_ro.contains("Stereo normal"));
        // T exercises every Sfx path headless (synth + clip with no engine).
        press(&mut app, KeyCode::KeyT);
        // Stereo channel check scheduled: hard-left now, hard-right in 0.5s.
        assert_eq!(app.world().resource::<Pending>().0.len(), 1);
        let p = &app.world().resource::<Pending>().0[0];
        assert!((p.pan - 1.0).abs() < 1e-6);
    }

    #[test]
    fn device_key_is_safe_with_no_devices() {
        // D is disabled: pressing it must be a harmless no-op, never panic.
        let mut app = test_app();
        press(&mut app, KeyCode::KeyD);
        assert!(app.world().resource::<Tennis>().m.phase == Phase::Ready);
    }

    #[test]
    fn info_keys_move_screen_reader_focus() {
        // I focuses the live status box for JAWS/NVDA; repeated I walks the
        // whole focus order; Esc returns to game.
        let mut app = test_app();
        assert!(app.world().resource::<InputFocus>().get().is_none());
        let status = app.world().resource::<InfoPanel>().status;
        let order = app.world().resource::<InfoPanel>().order.clone();
        press(&mut app, KeyCode::KeyI);
        assert_eq!(app.world().resource::<InputFocus>().get(), Some(status));
        press(&mut app, KeyCode::KeyI);
        let second = order
            .iter()
            .position(|&e| e == status)
            .map(|i| order[(i + 1) % order.len()])
            .unwrap();
        assert_eq!(app.world().resource::<InputFocus>().get(), Some(second));
        press(&mut app, KeyCode::Escape);
        assert!(app.world().resource::<InputFocus>().get().is_none());
    }

    #[test]
    fn info_boxes_are_readonly_focusable_inputs() {
        // The focus tree must be built from read-only edit boxes, not
        // static labels: every test-app box is a focusable
        // MultilineTextInput with ReadOnly set, and the live box also
        // carries a polite live region.
        use accesskit::Role;
        let app = test_app();
        let panel = app.world().resource::<InfoPanel>();
        assert!(panel.order.len() >= 2);
        for &e in &panel.order {
            let node = app.world().get::<AccessibilityNode>(e).expect("box needs a node");
            assert_eq!(node.role(), Role::MultilineTextInput, "box must be an edit box");
            assert!(node.is_read_only(), "box must be read-only");
            assert!(
                node.supports_action(accesskit::Action::Focus),
                "box must be focusable"
            );
        }
        let status_node = app.world().get::<AccessibilityNode>(panel.status).unwrap();
        assert_eq!(status_node.live(), Some(accesskit::Live::Polite));
        assert!(status_node.is_live_atomic());
    }

    #[test]
    fn full_point_flow_without_free_loss() {        let mut app = test_app();
        press(&mut app, KeyCode::KeyS);
        assert_eq!(app.world().resource::<Tennis>().m.phase, Phase::Serve);
        // fast-forward the serve flight to its landing
        app.world_mut().resource_mut::<Flight>().t = 100.0;
        app.update();
        if app.world().resource::<Tennis>().m.phase == Phase::Ready {
            // serve ace (8% on Normal) — point scored at once
            assert_eq!(app.world().resource::<Tennis>().m.pts[0], 1);
            return;
        }
        assert_eq!(app.world().resource::<Tennis>().m.phase, Phase::Incoming);
        // fast-forward the incoming flight to the HIT cue
        app.world_mut().resource_mut::<Flight>().t = 100.0;
        app.update();
        assert_eq!(app.world().resource::<Tennis>().m.phase, Phase::Hit);
        assert!(app.world().resource::<Tennis>().hit_window.is_some());
        // step to the ball like a player would, then hit on the cue
        {
            let landing_x = app.world().resource::<Flight>().to.x;
            let paddle = app.world().resource::<Rigs>().player.root;
            app.world_mut().get_mut::<Transform>(paddle).unwrap().translation.x = landing_x;
        }
        press(&mut app, KeyCode::Space);
        {
            let t = app.world().resource::<Tennis>();
            assert_eq!(t.m.phase, Phase::Return);
            assert!(t.hit_window.is_none());
        }
        // fast-forward the return flight to the rally resolution
        app.world_mut().resource_mut::<Flight>().t = 100.0;
        app.update();
        // Point scored either way, or the rally continues — but never a
        // silent miss while we hit on time.
        let t = app.world().resource::<Tennis>();
        let scored = t.m.pts[0] + t.m.pts[1] == 1;
        let continued = t.m.phase == Phase::Incoming;
        assert!(scored || continued, "phase={:?} pts={:?}", t.m.phase, t.m.pts);
    }

    #[test]
    fn return_uses_hit_time_aim_not_midflight_keys() {
        // Aim + shot are snapshotted at contact: mashing arrows / 1-2-3
        // during the 0.92s return flight must not change the outcome,
        // nor disagree with the "Lovitură … spre …" announcement.
        let mut app = test_app();
        press(&mut app, KeyCode::KeyS);
        app.world_mut().resource_mut::<Flight>().t = 100.0;
        app.update();
        if app.world().resource::<Tennis>().m.phase == Phase::Ready {
            return; // serve ace (8% on Normal), no rally to snapshot
        }
        app.world_mut().resource_mut::<Flight>().t = 100.0;
        app.update();
        assert_eq!(app.world().resource::<Tennis>().m.phase, Phase::Hit);
        {
            let landing_x = app.world().resource::<Flight>().to.x;
            let paddle = app.world().resource::<Rigs>().player.root;
            app.world_mut().get_mut::<Transform>(paddle).unwrap().translation.x = landing_x;
        }
        press(&mut app, KeyCode::ArrowRight);
        press(&mut app, KeyCode::ArrowRight);
        press(&mut app, KeyCode::Digit2); // topspin
        assert_eq!(app.world().resource::<Tennis>().m.dir, 2);
        press(&mut app, KeyCode::Space);
        assert_eq!(app.world().resource::<Tennis>().m.phase, Phase::Return);
        {
            let f = app.world().resource::<Flight>();
            assert_eq!(f.hit_dir, 2);
            assert_eq!(f.hit_shot, Shot::Topspin);
        }
        // Mash keys mid-flight: live aim changes, snapshot must not.
        press(&mut app, KeyCode::ArrowLeft);
        press(&mut app, KeyCode::ArrowLeft);
        press(&mut app, KeyCode::ArrowLeft);
        press(&mut app, KeyCode::ArrowLeft);
        press(&mut app, KeyCode::Digit1); // flat
        assert_eq!(app.world().resource::<Tennis>().m.dir, -2);
        {
            let f = app.world().resource::<Flight>();
            assert_eq!(f.hit_dir, 2, "snapshot must survive mid-flight keys");
            assert_eq!(f.hit_shot, Shot::Topspin);
        }
        // Landing resolves without panic; point or continued rally.
        app.world_mut().resource_mut::<Flight>().t = 100.0;
        app.update();
        let t = app.world().resource::<Tennis>();
        let scored = t.m.pts[0] + t.m.pts[1] == 1;
        let continued = t.m.phase == Phase::Incoming;
        assert!(scored || continued, "phase={:?} pts={:?}", t.m.phase, t.m.pts);
    }
}
