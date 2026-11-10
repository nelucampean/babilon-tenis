//! Babilon Tennis — accessible desktop port (Bevy 2D).
//!
//! Same model as `index.html` v0.5:
//! S serve / Space hit / ←→ direction / 1-2-3 shot / R repeat / P pause / T test.
//! Screen readers get the status text via Bevy UI (AccessKit) + stdout line.
//! Stereo MP3s live in `assets/sunete/` (copied from the web version).

mod a11y;
mod app;
mod athlete;
mod audio;
mod ball;
mod gameplay;
mod hud;
mod input;
mod particles;
mod scoreboard;
mod settings;
mod theme;

// Flat re-exports keep the in-file test harness (and any future internal
// module) able to use `use super::*` like the old single-file layout.
pub(crate) use a11y::*;
pub(crate) use athlete::*;
pub(crate) use audio::*;
pub(crate) use ball::*;
pub(crate) use gameplay::*;
pub(crate) use hud::*;
pub(crate) use input::*;
pub(crate) use particles::*;
pub(crate) use scoreboard::*;

fn main() {
    app::run();
}

#[cfg(test)]
mod smoke {
    //! Headless boot of the real system schedule. Catches query conflicts
    //! (B0001 — only raised at first schedule run, not by `cargo check`)
    //! and plays a full point without a display or audio device.
    use super::*;
    use accesskit::Role;
    use babilon_core::*;
    use bevy::a11y::{AccessibilityNode, ActionRequest as A11yActionRequest};
    use bevy::input::InputPlugin;
    use bevy::input_focus::InputFocus;
    use bevy::prelude::*;

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
            opp_serve_timer: None,
            opp_serve_origin: 1,
            opp_serve_target: 0,
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
        app.world_mut()
            .spawn((Ball, Transform::from_xyz(0., -260., 3.)));
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
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
    }

    #[test]
    fn systems_boot_without_query_conflict() {
        bevy::ecs::system::assert_is_system(keyboard);
        bevy::ecs::system::assert_is_system(settings_keys);
        bevy::ecs::system::assert_is_system(info_focus_keys);
        bevy::ecs::system::assert_is_system(a11y_focus_requests);
        bevy::ecs::system::assert_is_system(opponent_serve_tick);
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
        assert!(app
            .world()
            .resource::<Tennis>()
            .status_ro
            .contains("mijloc-stânga"));
        press(&mut app, KeyCode::ArrowLeft);
        assert_eq!(app.world().resource::<Targets>().player_x, -2.0 * LANE_STEP);
        assert!(app
            .world()
            .resource::<Tennis>()
            .status_ro
            .contains("stânga"));
        for _ in 0..4 {
            press(&mut app, KeyCode::ArrowRight);
        }
        assert_eq!(app.world().resource::<Targets>().player_x, 2.0 * LANE_STEP);
        assert!(app
            .world()
            .resource::<Tennis>()
            .status_ro
            .contains("dreapta"));
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
        app.world_mut()
            .get_mut::<Transform>(paddle)
            .unwrap()
            .translation
            .x = landing_x + 200.0;
        press(&mut app, KeyCode::Space);
        let t = app.world().resource::<Tennis>();
        assert_eq!(
            t.m.phase,
            Phase::Ready,
            "whiff ends the point, no return flight"
        );
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
        assert!(app
            .world()
            .resource::<Tennis>()
            .status_ro
            .contains("fileul"));
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
            app.world_mut()
                .get_mut::<Transform>(paddle)
                .unwrap()
                .translation
                .x = landing_x;
        }
        press(&mut app, KeyCode::Space);
        assert_eq!(app.world().resource::<Tennis>().m.phase, Phase::Return);
    }

    #[test]
    fn sound_keys_update_volumes_mode_difficulty() {
        let mut app = test_app();
        press(&mut app, KeyCode::KeyG);
        assert_eq!(
            app.world().resource::<Tennis>().difficulty,
            Difficulty::Hard
        );
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
        assert!(app
            .world()
            .resource::<Tennis>()
            .status_ro
            .contains("Stereo normal"));
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
            let node = app
                .world()
                .get::<AccessibilityNode>(e)
                .expect("box needs a node");
            assert_eq!(
                node.role(),
                Role::MultilineTextInput,
                "box must be an edit box"
            );
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
    fn full_point_flow_without_free_loss() {
        let mut app = test_app();
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
            app.world_mut()
                .get_mut::<Transform>(paddle)
                .unwrap()
                .translation
                .x = landing_x;
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
        // OUT / countered take a short visual epilogue flight before the
        // point is scored — follow it to its landing too (but leave a
        // continued rally's fresh incoming flight alone).
        let is_epilogue = matches!(
            app.world().resource::<Flight>().after,
            FlightAfter::OutLanded | FlightAfter::CounterLanded
        );
        if is_epilogue && app.world().resource::<Flight>().active {
            app.world_mut().resource_mut::<Flight>().t = 100.0;
            app.update();
        }
        // Point scored either way, or the rally continues — but never a
        // silent miss while we hit on time.
        let t = app.world().resource::<Tennis>();
        let scored = t.m.pts[0] + t.m.pts[1] == 1;
        let continued = t.m.phase == Phase::Incoming;
        assert!(
            scored || continued,
            "phase={:?} pts={:?}",
            t.m.phase,
            t.m.pts
        );
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
            app.world_mut()
                .get_mut::<Transform>(paddle)
                .unwrap()
                .translation
                .x = landing_x;
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
        // OUT / countered take a short visual epilogue flight before the
        // point is scored — follow it to its landing too (but leave a
        // continued rally's fresh incoming flight alone).
        let is_epilogue = matches!(
            app.world().resource::<Flight>().after,
            FlightAfter::OutLanded | FlightAfter::CounterLanded
        );
        if is_epilogue && app.world().resource::<Flight>().active {
            app.world_mut().resource_mut::<Flight>().t = 100.0;
            app.update();
        }
        let t = app.world().resource::<Tennis>();
        let scored = t.m.pts[0] + t.m.pts[1] == 1;
        let continued = t.m.phase == Phase::Incoming;
        assert!(
            scored || continued,
            "phase={:?} pts={:?}",
            t.m.phase,
            t.m.pts
        );
    }

    fn ball_xy(app: &mut App) -> (f32, f32) {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&Transform, With<Ball>>();
        let tr = q.single(world).unwrap();
        (tr.translation.x, tr.translation.y)
    }

    #[test]
    fn epilogue_landings_score_park_and_ready() {
        // Out + counter epilogues score WHERE THEY LAND (quiet window after
        // the flight, so the flight stays audible): landing awards the point
        // to the opponent, parks the ball, and readies the serve — or stays
        // Finished after match point.
        for (after, word) in [
            (FlightAfter::OutLanded, "AUT"),
            (FlightAfter::CounterLanded, "Contraatac"),
        ] {
            let mut app = test_app();
            app.world_mut().resource_mut::<Tennis>().m.phase = Phase::Return;
            {
                let mut f = app.world_mut().resource_mut::<Flight>();
                f.active = true;
                f.t = 100.0;
                f.duration = 1.0;
                f.from = Vec2::new(0.0, 300.0);
                f.to = Vec2::new(0.0, -330.0);
                f.after = after;
                f.bounce_at = None;
            }
            app.update();
            assert!(
                !app.world().resource::<Flight>().active,
                "{after:?} still flying"
            );
            let t = app.world().resource::<Tennis>();
            assert_eq!(t.m.pts, [0, 1], "{after:?} point not scored");
            assert!(
                t.status_ro.contains(word),
                "{after:?} call missing: {}",
                t.status_ro
            );
            assert_eq!(t.m.phase, Phase::Ready);
            let (bx, by) = ball_xy(&mut app);
            assert_eq!((bx, by), (0.0, -260.0), "{after:?} ball not parked");
        }
        // Match already over: epilogue landing must not revive Ready.
        let mut app = test_app();
        app.world_mut().resource_mut::<Tennis>().m.phase = Phase::Finished;
        {
            let mut f = app.world_mut().resource_mut::<Flight>();
            f.active = true;
            f.t = 100.0;
            f.duration = 1.0;
            f.from = Vec2::new(0.0, 300.0);
            f.to = Vec2::new(0.0, -330.0);
            f.after = FlightAfter::CounterLanded;
            f.bounce_at = None;
        }
        app.update();
        assert_eq!(app.world().resource::<Tennis>().m.phase, Phase::Finished);
    }

    #[test]
    fn opponent_serve_signals_stance_before_flight() {
        // His service game: entering Ready auto-starts a position signal —
        // stance announced + 3-bip countdown, ball at his racket, flight
        // only after the timer.
        let mut app = test_app();
        app.world_mut().resource_mut::<Tennis>().m.server = 1;
        assert!(!app.world().resource::<Tennis>().m.is_player_serve());
        app.update();
        let t = app.world().resource::<Tennis>();
        assert_eq!(t.m.phase, Phase::Serve);
        assert!(
            t.status_ro.contains("Adversarul servește de la"),
            "stance not signaled: {}",
            t.status_ro
        );
        assert!(t.opp_serve_timer.is_some(), "no signal timer");
        assert!(
            !app.world().resource::<Flight>().active,
            "flight must wait for the signal timer"
        );
        // 3-bip countdown: one immediate synth + two scheduled at +0.4/+0.8s.
        assert_eq!(
            app.world().resource::<Pending>().0.len(),
            2,
            "expected two scheduled countdown bips"
        );
        // Ball waits at his feet, on his side of the net.
        let (bx, by) = ball_xy(&mut app);
        assert!(by > 200.0, "ball not at server: ({bx}, {by})");
        assert_eq!(bx, app.world().resource::<Targets>().opp_x);
    }

    #[test]
    fn opponent_serve_fire_leads_to_hit_or_ace() {
        // Signal -> timer lapses -> serve flight -> landing resolves to
        // either your HIT cue or his ace, never a stuck Serve.
        let mut app = test_app();
        app.world_mut().resource_mut::<Tennis>().m.server = 1;
        app.update(); // auto-signal
        assert_eq!(app.world().resource::<Tennis>().m.phase, Phase::Serve);
        // Expire the 1.2s signal wait at once (headless frames tick ms).
        app.world_mut().resource_mut::<Tennis>().opp_serve_timer =
            Some(Timer::from_seconds(0.0, TimerMode::Once));
        app.update(); // fires the serve
        {
            let f = app.world().resource::<Flight>();
            assert!(f.active, "serve never fired");
            assert_eq!(f.after, FlightAfter::OpponentServeLanded);
        }
        // Fast-forward the serve flight to its landing.
        app.world_mut().resource_mut::<Flight>().t = 100.0;
        app.update();
        let t = app.world().resource::<Tennis>();
        if t.m.phase == Phase::Hit {
            assert!(t.hit_window.is_some(), "HIT cue without a window");
            assert!(
                t.status_ro.contains("lovește"),
                "no HIT call: {}",
                t.status_ro
            );
        } else {
            // Ace (8% on Normal): point to him, parked, ready again.
            assert_eq!(t.m.pts, [0, 1], "phase={:?} pts={:?}", t.m.phase, t.m.pts);
            assert_eq!(t.m.phase, Phase::Ready);
        }
    }

    #[test]
    fn tv_scoreboard_texts_follow_server() {
        // Start of match: you serve, everything zero.
        let sb = scoreboard_texts(&Match::new());
        assert!(sb.serve_you);
        assert_eq!(sb.sets, ["0".to_string(), "0".to_string()]);
        assert_eq!(sb.points, ["0".to_string(), "0".to_string()]);
        assert!(sb.summary.contains("servește TU"), "{}", sb.summary);
        // His serve, he leads 30-0: columns follow the match, dot flips.
        let mut m = Match::new();
        m.server = 1;
        m.games = [2, 1];
        m.pts = [0, 2];
        let sb = scoreboard_texts(&m);
        assert!(!sb.serve_you);
        assert_eq!(sb.games, ["2".to_string(), "1".to_string()]);
        assert_eq!(sb.points, ["0".to_string(), "30".to_string()]);
        assert!(sb.summary.contains("servește CPU"), "{}", sb.summary);
        assert!(sb.summary.contains("puncte 30"), "{}", sb.summary);
        // Deuce and advantage render like the broadcast bug.
        m.server = 0;
        m.pts = [3, 3];
        assert_eq!(
            scoreboard_texts(&m).points,
            ["40".to_string(), "40".to_string()]
        );
        m.pts = [4, 3];
        assert_eq!(
            scoreboard_texts(&m).points,
            ["AD".to_string(), "40".to_string()]
        );
        // Tie-break shows bare numbers.
        m.tiebreak = true;
        m.pts = [5, 6];
        assert_eq!(
            scoreboard_texts(&m).points,
            ["5".to_string(), "6".to_string()]
        );
    }

    fn col_name(c: SbCol) -> &'static str {
        match c {
            SbCol::Serve => "serve",
            SbCol::Sets => "sets",
            SbCol::Games => "games",
            SbCol::Points => "points",
        }
    }

    #[test]
    fn tv_scoreboard_cells_update_without_conflict() {
        // Spawn a minimal scoreboard (dots + value cells + labeled root)
        // and run the real schedule: refresh_ui must fill every cell,
        // light only the server's square, and never trip a query conflict.
        let mut app = test_app();
        {
            let w = app.world_mut();
            w.spawn((
                SbCell {
                    row: 0,
                    col: SbCol::Serve,
                },
                BackgroundColor(Color::WHITE),
            ));
            w.spawn((
                SbCell {
                    row: 1,
                    col: SbCol::Serve,
                },
                BackgroundColor(Color::WHITE),
            ));
            for row in [0u8, 1u8] {
                for col in [SbCol::Sets, SbCol::Games, SbCol::Points] {
                    w.spawn((SbCell { row, col }, Text::new("?")));
                }
            }
            let mut node = accesskit::Node::new(Role::Label);
            node.set_label("Tabela de scor");
            w.spawn((SbRoot, AccessibilityNode::from(node)));
        }
        let mut m = Match::new();
        m.server = 1;
        m.games = [2, 1];
        m.pts = [0, 2];
        m.phase = Phase::Finished; // keep serve-tick/timeout systems quiet
        app.world_mut().resource_mut::<Tennis>().m = m;
        app.update();
        let w = app.world_mut();
        let mut q = w.query::<(&Text, &SbCell)>();
        let mut seen = 0;
        for (t, cell) in q.iter(w) {
            let want = match (cell.row, cell.col) {
                (0, SbCol::Sets) => "0",
                (1, SbCol::Sets) => "0",
                (0, SbCol::Games) => "2",
                (1, SbCol::Games) => "1",
                (0, SbCol::Points) => "0",
                (1, SbCol::Points) => "30",
                _ => continue,
            };
            assert_eq!(
                t.as_str(),
                want,
                "row={} col={}",
                cell.row,
                col_name(cell.col)
            );
            seen += 1;
        }
        assert_eq!(seen, 6, "not every scoreboard cell updated");
        let w = app.world_mut();
        let mut q = w.query::<(&BackgroundColor, &SbCell)>();
        let mut dots = 0;
        for (bg, cell) in q.iter(w) {
            // Text cells carry an automatic transparent BackgroundColor —
            // only the Serve squares count here.
            if cell.col != SbCol::Serve {
                continue;
            }
            let lit = bg.0 == Color::srgb(0.98, 0.79, 0.40);
            assert_eq!(lit, cell.row == 1, "wrong serve square lit");
            dots += 1;
        }
        assert_eq!(dots, 2, "serve squares missing");
    }

    #[test]
    fn serve_key_does_not_steal_opponent_serve() {
        // S on his game never starts YOUR serve flight — it waits for (or
        // nudges) his signal instead.
        let mut app = test_app();
        app.world_mut().resource_mut::<Tennis>().m.server = 1;
        press(&mut app, KeyCode::KeyS);
        let t = app.world().resource::<Tennis>();
        assert!(
            t.status_ro.contains("Adversarul servește"),
            "wrong S answer: {}",
            t.status_ro
        );
        assert_eq!(t.m.pts, [0, 0], "S scored a point on his serve");
        let f = app.world().resource::<Flight>();
        // Either still signaling (no flight) or his serve flight is up —
        // never your player-serve flight to y=+300 from the baseline.
        if f.active {
            assert_eq!(f.after, FlightAfter::OpponentServeLanded);
        }
    }
}
