//! Accessible side panel: the HUD text components and their refresh.
use accesskit::Role;
use babilon_core::Difficulty;
use bevy::a11y::AccessibilityNode;
use bevy::prelude::*;

use crate::a11y::{refresh_box, set_node_value};
use crate::announce::Announcer;
use crate::audio::{AudioMode, Sfx, Volumes};
use crate::gameplay::Tennis;
use crate::scoreboard::{scoreboard_texts, SbCell, SbCol, SbRoot};

// ---- entities --------------------------------------------------------------

#[derive(Component)]
pub(crate) struct ScoreText;
#[derive(Component)]
pub(crate) struct StatusText;
#[derive(Component)]
pub(crate) struct ShotText;
#[derive(Component)]
pub(crate) struct PhaseText;
#[derive(Component)]
pub(crate) struct ModeText;

pub(crate) fn refresh_ui(
    tennis: Res<Tennis>,
    announcer: Res<Announcer>,
    vols: Res<Volumes>,
    mode: Res<AudioMode>,
    sfx: Option<Res<Sfx>>,
    mut score_q: Query<(&mut Text, Option<&mut AccessibilityNode>), With<ScoreText>>,
    mut status_q: Query<
        (&mut Text, Option<&mut AccessibilityNode>),
        (With<StatusText>, Without<ScoreText>),
    >,
    mut shot_q: Query<
        (&mut Text, Option<&mut AccessibilityNode>),
        (With<ShotText>, Without<ScoreText>, Without<StatusText>),
    >,
    mut phase_q: Query<
        (&mut Text, Option<&mut AccessibilityNode>),
        (
            With<PhaseText>,
            Without<ScoreText>,
            Without<StatusText>,
            Without<ShotText>,
        ),
    >,
    mut mode_q: Query<
        (&mut Text, Option<&mut AccessibilityNode>),
        (
            With<ModeText>,
            Without<ScoreText>,
            Without<StatusText>,
            Without<ShotText>,
            Without<PhaseText>,
        ),
    >,
    mut sb_text_q: Query<
        (&mut Text, &SbCell),
        (
            Without<ScoreText>,
            Without<StatusText>,
            Without<ShotText>,
            Without<PhaseText>,
            Without<ModeText>,
        ),
    >,
    mut sb_dot_q: Query<(&mut BackgroundColor, &SbCell)>,
    mut sb_root_q: Query<
        &mut AccessibilityNode,
        (
            With<SbRoot>,
            Without<ScoreText>,
            Without<StatusText>,
            Without<ShotText>,
            Without<PhaseText>,
            Without<ModeText>,
        ),
    >,
) {
    if !tennis.is_changed() && !announcer.is_changed() && !vols.is_changed() && !mode.is_changed() {
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
        **t = announcer.status_ro.clone();
        if let Some(mut node) = a11y {
            refresh_box(&mut node, "Anunțuri", &announcer.status_ro, true);
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
    // TV scoreboard cells + screen-reader summary (same gate as the rest).
    let sb = scoreboard_texts(&tennis.m);
    for (mut t, cell) in &mut sb_text_q {
        let v = match (cell.row, cell.col) {
            (0, SbCol::Sets) => &sb.sets[0],
            (1, SbCol::Sets) => &sb.sets[1],
            (0, SbCol::Games) => &sb.games[0],
            (1, SbCol::Games) => &sb.games[1],
            (0, SbCol::Points) => &sb.points[0],
            (1, SbCol::Points) => &sb.points[1],
            _ => continue,
        };
        **t = v.clone();
    }
    for (mut bg, cell) in &mut sb_dot_q {
        if cell.col != SbCol::Serve {
            continue;
        }
        let serving = (cell.row == 0) == sb.serve_you;
        *bg = BackgroundColor(if serving {
            Color::srgb(0.98, 0.79, 0.40)
        } else {
            Color::srgba(0.45, 0.55, 0.60, 0.45)
        });
    }
    for mut node in &mut sb_root_q {
        // Label role, no Focus action: browsable like the panel Region's
        // block, but never a tab stop — the right panel owns the focus order.
        node.set_role(Role::Label);
        node.set_label("Tabela de scor".to_string().into_boxed_str());
        set_node_value(&mut node, &sb.summary);
    }
}
