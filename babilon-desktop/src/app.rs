//! App wiring: plugins, resources, startup scene, system schedule.
use accesskit::{Action, Role};
use bevy::a11y::AccessibilityNode;
use bevy::input_focus::InputDispatchPlugin;
use bevy::log::LogPlugin;
use bevy::prelude::*;
use bevy::render::mesh::Mesh2d;
use bevy::sprite::MeshMaterial2d;

use crate::a11y::*;
use crate::athlete::*;
use crate::audio::{self, Pending, Sfx};
use crate::ball::*;
use crate::gameplay::hit_timeout;
use crate::gameplay::*;
use crate::hud::*;
use crate::input::*;
use crate::particles::*;
use crate::scoreboard::*;
use crate::settings;
use crate::theme::*;
use babilon_core::Match;

// ---- setup -----------------------------------------------------------------

pub(crate) fn run() {
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
                filter:
                    "info,symphonia_core=warn,symphonia_bundle_mp3=warn,symphonia_metadata=warn"
                        .into(),
                ..default()
            }),
    )
    .insert_resource(Tennis {
        m: Match::new(),
        difficulty: saved.difficulty,
        paused: false,
        status_ro: "Meci nou. Apasă S pentru serviciu.".into(),
        hit_window: None,
        opp_serve_timer: None,
        opp_serve_origin: 1,
        opp_serve_target: 0,
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
            opponent_serve_tick,
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
    eprintln!("[APP] starting Bevy loop");
    app.run();
}

pub(crate) fn setup(
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
            Transform::from_xyz(
                0.,
                20. - court_h / 2. + court_h / 16.0 + i as f32 * court_h / 8.0,
                -0.5,
            ),
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
    let help_txt = "S serviciu (când servești tu) • Spațiu lovire\n←/→ direcție • 1/2/3 lovitură\nR repetă • P pauză • T test audio\nG dificultate • M mod audio • N stereo\nC grup volum • Z/X volum\nI focusează anunțurile (repetat = câmpul următor) • Esc înapoi la joc";
    let mut order: Vec<Entity> = Vec::with_capacity(7);
    let mut info_status = Entity::PLACEHOLDER;
    // TV-style score bug, top-left over the free background (never over the
    // court): serve square + name + SET / GAME / PCT columns, like the
    // broadcast graphics. You serve the first game, so your square starts lit.
    let mut sb_node = accesskit::Node::new(Role::Label);
    sb_node.set_label("Tabela de scor");
    sb_node.set_value("Tabela: TU seturi 0, game-uri 0, puncte 0; CPU seturi 0, game-uri 0, puncte 0; servește TU.");
    commands
        .spawn((
            SbRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(12.),
                top: Val::Px(12.),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(10.)),
                row_gap: Val::Px(4.),
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.06, 0.11, 0.94)),
            AccessibilityNode::from(sb_node),
        ))
        .with_children(|sb| {
            sb.spawn(Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(6.),
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|h| {
                h.spawn(Node {
                    width: Val::Px(12.),
                    ..default()
                });
                sb_header_cell(h, &fonts, muted, "", 64.);
                sb_header_cell(h, &fonts, muted, "SET", 36.);
                sb_header_cell(h, &fonts, muted, "GAME", 52.);
                sb_header_cell(h, &fonts, muted, "PCT", 56.);
            });
            let dim = Color::srgba(0.45, 0.55, 0.60, 0.45);
            sb_row(sb, &fonts, ink, dim, 0, "TU", true);
            sb_row(sb, &fonts, ink, dim, 1, "CPU", false);
        });
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
                        TextFont {
                            font: bold.clone(),
                            font_size: 15.0,
                            ..default()
                        },
                        TextColor(gold),
                        readonly_box("Titlu joc", title_txt),
                    ))
                    .id(),
                );
                order.push(
                    p.spawn((
                        ScoreText,
                        Text::new(score_txt),
                        TextFont {
                            font: bold.clone(),
                            font_size: 19.0,
                            ..default()
                        },
                        TextColor(ink),
                        readonly_box("Scor", score_txt),
                    ))
                    .id(),
                );
                info_status = p
                    .spawn((
                        StatusText,
                        Text::new(status_txt),
                        TextFont {
                            font: reg.clone(),
                            font_size: 19.0,
                            ..default()
                        },
                        TextColor(ink),
                        live_box("Anunțuri", status_txt),
                    ))
                    .id();
                order.push(info_status);
                order.push(
                    p.spawn((
                        ShotText,
                        Text::new(shot_txt),
                        TextFont {
                            font: reg.clone(),
                            font_size: 14.0,
                            ..default()
                        },
                        TextColor(ink),
                        readonly_box("Direcție și lovitură", shot_txt),
                    ))
                    .id(),
                );
                order.push(
                    p.spawn((
                        PhaseText,
                        Text::new(phase_txt),
                        TextFont {
                            font: bold.clone(),
                            font_size: 13.0,
                            ..default()
                        },
                        TextColor(gold),
                        readonly_box("Fază joc", phase_txt),
                    ))
                    .id(),
                );
                order.push(
                    p.spawn((
                        ModeText,
                        Text::new(mode_txt),
                        TextFont {
                            font: reg.clone(),
                            font_size: 13.0,
                            ..default()
                        },
                        TextColor(muted),
                        readonly_box("Mod audio și volum", mode_txt),
                    ))
                    .id(),
                );
                order.push(
                    p.spawn((
                        Text::new(help_txt),
                        TextFont {
                            font: reg.clone(),
                            font_size: 12.0,
                            ..default()
                        },
                        TextColor(muted),
                        readonly_box("Ajutor taste", help_txt),
                    ))
                    .id(),
                );
            });
        });
    // Remember the order + live box so `I` can walk screen-reader focus
    // through the whole block and `Esc` returns to the game.
    commands.insert_resource(InfoPanel {
        status: info_status,
        order,
    });
}
