//! Romanian UI strings and score formatting.
//!
//! `babilon-core` stays a pure rules crate: it owns points, games, sets,
//! umpire clip keys and flags, but no user-facing Romanian text. Everything
//! the player reads or hears is formatted here (and can later be swapped
//! for another language without touching the rules).
use babilon_core::{Difficulty, Dir, Match, Phase, PointResult, Shot};

const POINT_NAMES_RO: [&str; 4] = ["0", "15", "30", "40"];

/// Lane name for direction announcements ("stânga" … "dreapta").
pub(crate) fn dir_label_ro(dir: Dir) -> &'static str {
    match dir {
        d if d <= -2 => "stânga",
        -1 => "mijloc-stânga",
        0 => "centru",
        1 => "mijloc-dreapta",
        _ => "dreapta",
    }
}

pub(crate) fn shot_label_ro(shot: Shot) -> &'static str {
    match shot {
        Shot::Flat => "plată",
        Shot::Topspin => "liftată",
        Shot::Slice => "slice",
    }
}

pub(crate) fn phase_label_ro(phase: Phase) -> &'static str {
    match phase {
        Phase::Ready => "PREGĂTIT PENTRU SERVICIU",
        Phase::Serve => "SERVICIU",
        Phase::Incoming => "MINGEA SE APROPIE",
        Phase::Hit => "LOVEȘTE ACUM",
        Phase::Return => "MINGEA SPRE ADVERSAR",
        Phase::Finished => "MECI ÎNCHEIAT",
    }
}

pub(crate) fn difficulty_label_ro(difficulty: Difficulty) -> &'static str {
    match difficulty {
        Difficulty::Easy => "Ușor",
        Difficulty::Normal => "Normal",
        Difficulty::Hard => "Greu",
    }
}

/// Romanian score line, server-first per tennis convention
/// ("15-0" = the server leads). Labels stay, so screen readers hear
/// unambiguously who has what.
pub(crate) fn score_text_ro(m: &Match) -> String {
    let (s, r) = m.ordered_pts();
    let v = if m.tiebreak {
        if m.server == 0 {
            format!("Tie-break: tu {s}, calculator {r}")
        } else {
            format!("Tie-break: calculator {s}, tu {r}")
        }
    } else if m.pts[0] >= 3 && m.pts[1] >= 3 {
        if m.pts[0] == m.pts[1] {
            "Egalitate".to_string()
        } else if m.pts[0] > m.pts[1] {
            "Avantaj tu".to_string()
        } else {
            "Avantaj calculator".to_string()
        }
    } else if m.server == 0 {
        format!(
            "Tu {}, calculator {}",
            POINT_NAMES_RO[s.min(3) as usize],
            POINT_NAMES_RO[r.min(3) as usize]
        )
    } else {
        format!(
            "Calculator {}, tu {}",
            POINT_NAMES_RO[s.min(3) as usize],
            POINT_NAMES_RO[r.min(3) as usize]
        )
    };
    let serve = if m.is_player_serve() {
        "Servește tu"
    } else {
        "Servește calculatorul"
    };
    format!(
        "Seturi: tu {}, calculator {}. Game-uri: tu {}, calculator {}. {}. {}.",
        m.sets[0], m.sets[1], m.games[0], m.games[1], v, serve
    )
}

/// TV-scoreboard point columns: `(player, opponent)` display strings —
/// "0"/"15"/"30"/"40", "AD" for the advantage holder, bare numbers in a
/// tie-break. Deuce shows "40"/"40", like broadcast graphics.
pub(crate) fn game_points_display(m: &Match) -> (String, String) {
    if m.tiebreak {
        return (m.pts[0].to_string(), m.pts[1].to_string());
    }
    if m.pts[0] >= 3 && m.pts[1] >= 3 {
        if m.pts[0] == m.pts[1] {
            return ("40".to_string(), "40".to_string());
        }
        if m.pts[0] > m.pts[1] {
            return ("AD".to_string(), "40".to_string());
        }
        return ("40".to_string(), "AD".to_string());
    }
    (
        POINT_NAMES_RO[m.pts[0].min(3) as usize].to_string(),
        POINT_NAMES_RO[m.pts[1].min(3) as usize].to_string(),
    )
}

/// Append `s` to `msg` as the next sentence, adding a period only when
/// the previous text does not already end with punctuation.
fn push_sentence(msg: &mut String, s: &str) {
    if !msg.is_empty() && !msg.ends_with(['.', '!', '?']) {
        msg.push('.');
    }
    if !msg.is_empty() {
        msg.push(' ');
    }
    msg.push_str(s);
}

/// Full Romanian announcement for a finished point: reason, game/set/tie-break
/// news, score line and (when ready) the serve prompt.
pub(crate) fn point_message_ro(m: &Match, reason_ro: &str, r: &PointResult) -> String {
    let mut msg = reason_ro.to_string();
    if r.game_won {
        push_sentence(
            &mut msg,
            if r.winner == 0 {
                "Game pentru tine!"
            } else {
                "Game pentru calculator."
            },
        );
    }
    if r.tiebreak_won {
        push_sentence(
            &mut msg,
            if r.winner == 0 {
                "Ai câștigat tie-break-ul și meciul!"
            } else {
                "Calculatorul a câștigat tie-break-ul și meciul."
            },
        );
    } else if r.set_won {
        push_sentence(
            &mut msg,
            if r.winner == 0 {
                "Ai câștigat setul și meciul!"
            } else {
                "Calculatorul a câștigat setul și meciul."
            },
        );
    } else if r.tiebreak_started {
        push_sentence(&mut msg, "Începe tie-break-ul.");
    }
    push_sentence(&mut msg, &score_text_ro(m));
    if m.phase == Phase::Ready {
        push_sentence(
            &mut msg,
            if m.is_player_serve() {
                "Apasă S pentru serviciu."
            } else {
                "Adversarul servește."
            },
        );
    }
    msg
}

/// The R key: repeat the last announcement, then the current score.
pub(crate) fn repeat_message_ro(last: &str, m: &Match) -> String {
    let score = score_text_ro(m);
    if last.is_empty() {
        return score;
    }
    let mut msg = last.to_string();
    push_sentence(&mut msg, &score);
    msg
}

/// Score a point and format its announcement in one call.
pub(crate) fn score_and_message(
    m: &mut Match,
    winner: usize,
    reason_ro: &str,
) -> (PointResult, String) {
    let r = m.score_point(winner);
    let msg = point_message_ro(m, reason_ro, &r);
    (r, msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_cover_every_lane_and_shot() {
        assert_eq!(dir_label_ro(-2), "stânga");
        assert_eq!(dir_label_ro(-1), "mijloc-stânga");
        assert_eq!(dir_label_ro(0), "centru");
        assert_eq!(dir_label_ro(1), "mijloc-dreapta");
        assert_eq!(dir_label_ro(2), "dreapta");
        assert_eq!(shot_label_ro(Shot::Flat), "plată");
        assert_eq!(shot_label_ro(Shot::Topspin), "liftată");
        assert_eq!(shot_label_ro(Shot::Slice), "slice");
        assert_eq!(difficulty_label_ro(Difficulty::Hard), "Greu");
        assert_eq!(phase_label_ro(Phase::Finished), "MECI ÎNCHEIAT");
    }

    #[test]
    fn score_line_is_server_first_and_announces_serve() {
        let mut m = Match::new();
        assert!(score_text_ro(&m).contains("Tu 0, calculator 0"));
        assert!(score_text_ro(&m).contains("Servește tu"));
        m.score_point(0);
        assert!(score_text_ro(&m).contains("Tu 15, calculator 0"));
        m.server = 1;
        assert!(score_text_ro(&m).contains("Servește calculatorul"));
    }

    #[test]
    fn point_message_adds_game_score_and_prompt() {
        let mut m = Match::new();
        let (r, msg) = score_and_message(&mut m, 0, "As!");
        assert!(msg.starts_with("As! "));
        assert!(msg.contains("Tu 15, calculator 0"));
        assert!(msg.contains("Apasă S pentru serviciu"));
        assert_eq!(r.winner, 0);
        assert!(!r.game_won);
    }

    #[test]
    fn tv_point_columns() {
        let mut m = Match::new();
        assert_eq!(game_points_display(&m), ("0".to_string(), "0".to_string()));
        m.pts = [1, 2];
        assert_eq!(
            game_points_display(&m),
            ("15".to_string(), "30".to_string())
        );
        m.pts = [3, 3];
        assert_eq!(
            game_points_display(&m),
            ("40".to_string(), "40".to_string())
        );
        m.pts = [4, 3];
        assert_eq!(
            game_points_display(&m),
            ("AD".to_string(), "40".to_string())
        );
        m.pts = [3, 4];
        assert_eq!(
            game_points_display(&m),
            ("40".to_string(), "AD".to_string())
        );
        m.tiebreak = true;
        m.pts = [5, 6];
        assert_eq!(game_points_display(&m), ("5".to_string(), "6".to_string()));
    }

    #[test]
    fn repeat_appends_score_without_double_punctuation() {
        let m = Match::new();
        assert_eq!(repeat_message_ro("", &m), score_text_ro(&m));
        assert!(repeat_message_ro("Acum lovește!", &m).starts_with("Acum lovește! Seturi"));
        assert!(repeat_message_ro("Pauză", &m).starts_with("Pauză. Seturi"));
    }

    #[test]
    fn tiebreak_start_is_announced() {
        let mut m = Match::new();
        // He leads 6-5 in games; winning the next game makes it 6-6.
        m.games = [5, 6];
        m.pts = [3, 0];
        let (_, msg) = score_and_message(&mut m, 0, "Game");
        assert!(msg.contains("Începe tie-break-ul"));
    }
}
