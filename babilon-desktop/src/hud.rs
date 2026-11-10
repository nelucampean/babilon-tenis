//! Accessible side panel: one data-driven text line per row.
//!
//! Every panel row is an [`InfoLine`] tag plus a `Text` (and usually an
//! `AccessibilityNode`). [`refresh_ui`] builds all five strings once and
//! walks them in a single query, so adding or reordering a row no longer
//! means another marker component and another mutually-exclusive query.
use accesskit::Role;
use babilon_core::Difficulty;
use bevy::a11y::AccessibilityNode;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::a11y::{refresh_box, set_node_value};
use crate::announce::Announcer;
use crate::audio::Sfx;
use crate::gameplay::Tennis;
use crate::scoreboard::{scoreboard_texts, SbCell, SbCol, SbRoot};
use crate::settings::Settings;
use crate::state::AppState;
use crate::theme::{SERVE_OFF, SERVE_ON};

/// Which side-panel row a text node renders.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InfoLine {
    Score,
    Status,
    Shot,
    Phase,
    Mode,
}

impl InfoLine {
    /// AccessKit label for this row (stable, not the dynamic value).
    fn label(self) -> &'static str {
        match self {
            Self::Score => "Scor",
            Self::Status => "Anunțuri",
            Self::Shot => "Direcție și lovitură",
            Self::Phase => "Fază joc",
            Self::Mode => "Mod audio și volum",
        }
    }

    /// Only the status row is a live region (announcements speak by
    /// themselves); score/shot/phase/mode are quiet and purely browsable.
    fn live(self) -> bool {
        self == Self::Status
    }

    fn text(self, lines: &Lines) -> &str {
        match self {
            Self::Score => &lines.score,
            Self::Status => &lines.status,
            Self::Shot => &lines.shot,
            Self::Phase => &lines.phase,
            Self::Mode => &lines.mode,
        }
    }
}

/// All side-panel strings for one frame.
struct Lines {
    score: String,
    status: String,
    shot: String,
    phase: String,
    mode: String,
}

/// Everything `refresh_ui` reads or writes, in one argument.
#[derive(SystemParam)]
pub(crate) struct HudWorld<'w, 's> {
    tennis: Res<'w, Tennis>,
    state: Res<'w, State<AppState>>,
    announcer: Res<'w, Announcer>,
    settings: Res<'w, Settings>,
    sfx: Option<Res<'w, Sfx>>,
    lines: Query<
        'w,
        's,
        (
            &'static InfoLine,
            &'static mut Text,
            Option<&'static mut AccessibilityNode>,
        ),
        Without<SbCell>,
    >,
    sb_text_q: Query<'w, 's, (&'static mut Text, &'static SbCell), Without<InfoLine>>,
    sb_dot_q: Query<'w, 's, (&'static mut BackgroundColor, &'static SbCell)>,
    sb_root_q: Query<'w, 's, &'static mut AccessibilityNode, (With<SbRoot>, Without<InfoLine>)>,
}

pub(crate) fn refresh_ui(mut hud: HudWorld) {
    if !hud.tennis.is_changed()
        && !hud.state.is_changed()
        && !hud.announcer.is_changed()
        && !hud.settings.is_changed()
    {
        return;
    }

    let diff_label = match hud.settings.difficulty {
        Difficulty::Easy => "Ușor",
        Difficulty::Normal => "Normal",
        Difficulty::Hard => "Greu",
    };
    let swapped = hud.sfx.as_deref().map(|s| s.swapped()).unwrap_or(false);
    let lines = Lines {
        score: format!(
            "Seturi {} – {}\nGame-uri {} – {}\n{}",
            hud.tennis.m.sets[0],
            hud.tennis.m.sets[1],
            hud.tennis.m.games[0],
            hud.tennis.m.games[1],
            hud.tennis.m.score_text_ro()
        ),
        status: hud.announcer.status_ro.clone(),
        shot: format!(
            "Direcție: {} • Lovitură: {}",
            babilon_core::dir_label_ro(hud.tennis.m.dir),
            hud.tennis.m.shot.label_ro()
        ),
        phase: if *hud.state.get() == AppState::Paused {
            "PAUZĂ".to_string()
        } else {
            hud.tennis.m.phase.visual_label_ro().to_string()
        },
        mode: format!(
            "{} • {} • Volum {}/{}/{} • Stereo {}",
            hud.settings.mode.label_ro(),
            diff_label,
            (hud.settings.vols.ball * 100.0) as u32,
            (hud.settings.vols.umpire * 100.0) as u32,
            (hud.settings.vols.crowd * 100.0) as u32,
            if swapped { "invers" } else { "normal" }
        ),
    };

    for (line, mut text, a11y) in &mut hud.lines {
        let value = line.text(&lines);
        **text = value.to_owned();
        if let Some(mut node) = a11y {
            refresh_box(&mut node, line.label(), value, line.live());
        }
    }

    // TV scoreboard cells + screen-reader summary (same gate as the rest).
    let sb = scoreboard_texts(&hud.tennis.m);
    for (mut text, cell) in &mut hud.sb_text_q {
        let value = match (cell.row, cell.col) {
            (0, SbCol::Sets) => &sb.sets[0],
            (1, SbCol::Sets) => &sb.sets[1],
            (0, SbCol::Games) => &sb.games[0],
            (1, SbCol::Games) => &sb.games[1],
            (0, SbCol::Points) => &sb.points[0],
            (1, SbCol::Points) => &sb.points[1],
            _ => continue,
        };
        **text = value.clone();
    }
    for (mut bg, cell) in &mut hud.sb_dot_q {
        if cell.col != SbCol::Serve {
            continue;
        }
        let serving = (cell.row == 0) == sb.serve_you;
        *bg = BackgroundColor(if serving { SERVE_ON } else { SERVE_OFF });
    }
    for mut node in &mut hud.sb_root_q {
        // Label role, no Focus action: browsable like the panel Region's
        // block, but never a tab stop — the right panel owns the focus order.
        node.set_role(Role::Label);
        node.set_label("Tabela de scor".to_string().into_boxed_str());
        set_node_value(&mut node, &sb.summary);
    }
}
