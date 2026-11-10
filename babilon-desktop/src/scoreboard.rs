//! TV scoreboard overlay (top-left broadcast bug): spawn helpers + the
//! pure text mapping shared by the overlay and its screen-reader summary.
use babilon_core::Match;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::relationship::RelatedSpawnerCommands;
use bevy::prelude::*;

use crate::theme::UiFonts;

// ---- TV scoreboard (top-left broadcast bug) ---------------------------------

/// All dynamic scoreboard strings, computed pure from the match so both
/// the overlay and its screen-reader summary stay in sync. Row 0 = you.
pub(crate) struct SbTexts {
    pub(crate) serve_you: bool,
    pub(crate) sets: [String; 2],
    pub(crate) games: [String; 2],
    pub(crate) points: [String; 2],
    pub(crate) summary: String,
}

pub(crate) fn scoreboard_texts(m: &Match) -> SbTexts {
    let (pp, cp) = m.game_points_display();
    let serve_you = m.is_player_serve();
    let summary = format!(
        "Tabela: TU seturi {}, game-uri {}, puncte {}; CPU seturi {}, game-uri {}, puncte {}; servește {}.",
        m.sets[0],
        m.games[0],
        pp,
        m.sets[1],
        m.games[1],
        cp,
        if serve_you { "TU" } else { "CPU" },
    );
    SbTexts {
        serve_you,
        sets: [m.sets[0].to_string(), m.sets[1].to_string()],
        games: [m.games[0].to_string(), m.games[1].to_string()],
        points: [pp, cp],
        summary,
    }
}

pub(crate) fn sb_header_cell(
    parent: &mut RelatedSpawnerCommands<ChildOf>,
    fonts: &UiFonts,
    muted: Color,
    text: &str,
    w: f32,
) {
    parent.spawn((
        Node {
            width: Val::Px(w),
            justify_content: JustifyContent::Center,
            ..default()
        },
        Text::new(text),
        TextFont {
            font: fonts.regular.clone(),
            font_size: 10.0,
            ..default()
        },
        TextColor(muted),
    ));
}

/// One scoreboard row: serve square + name + SET/GAME/PCT cells.
pub(crate) fn sb_row(
    parent: &mut RelatedSpawnerCommands<ChildOf>,
    fonts: &UiFonts,
    ink: Color,
    dim: Color,
    row: u8,
    name: &str,
    serving: bool,
) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(6.),
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|r| {
            r.spawn((
                SbCell {
                    row,
                    col: SbCol::Serve,
                },
                Node {
                    width: Val::Px(12.),
                    height: Val::Px(12.),
                    ..default()
                },
                BackgroundColor(if serving {
                    Color::srgb(0.98, 0.79, 0.40)
                } else {
                    dim
                }),
            ));
            r.spawn((
                Node {
                    width: Val::Px(64.),
                    ..default()
                },
                Text::new(name),
                TextFont {
                    font: fonts.bold.clone(),
                    font_size: 15.0,
                    ..default()
                },
                TextColor(ink),
            ));
            for (col, w, init) in [
                (SbCol::Sets, 36., "0"),
                (SbCol::Games, 52., "0"),
                (SbCol::Points, 56., "0"),
            ] {
                r.spawn((
                    SbCell { row, col },
                    Node {
                        width: Val::Px(w),
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    Text::new(init),
                    TextFont {
                        font: fonts.bold.clone(),
                        font_size: 17.0,
                        ..default()
                    },
                    TextColor(ink),
                ));
            }
        });
}

/// TV scoreboard cell (top-left broadcast bug): row 0 = you, row 1 = CPU.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SbCell {
    pub(crate) row: u8,
    pub(crate) col: SbCol,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SbCol {
    /// Serve indicator square (a `BackgroundColor` node, no text).
    Serve,
    Sets,
    Games,
    Points,
}

/// Root of the TV scoreboard overlay. Carries a non-focusable
/// screen-reader label with the same score, so the bug stays a visual
/// convenience without adding a tab stop to the focus order.
#[derive(Component)]
pub(crate) struct SbRoot;
