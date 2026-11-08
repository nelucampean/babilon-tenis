//! Babilon Tennis core — pure rules ported from `index.html` (v0.5).
//!
//! Mirrors the JS exactly:
//! - one set to 6 games (win by 2), 6–6 triggers tie-break to 7 (win by 2)
//! - player serves every point (simplified controls)
//! - deuce / advantage / umpire clip keys (`sunete/*.mp3`)
//! - serve ace chance + return outcome math from `serve()` / `hit()`
//!
//! No wall-clock, no audio, no rendering here: the desktop (and later
//! mobile) crate owns timers, input, sound and screen-reader output.
//! All randomness is injected as `roll: f32` in `[0,1)` so tests are deterministic.

/// Lane direction: -2 = stânga, -1 = mijloc-stânga, 0 = centru,
/// +1 = mijloc-dreapta, +2 = dreapta. Five stops so aim and footwork
/// feel analogue instead of ternary.
pub type Dir = i8;

/// Outer lane index (absolute value range of [`Dir`]).
pub const MAX_DIR: Dir = 2;
/// Lateral pixels per lane step on the Bevy court (outer lanes at ±151).
pub const LANE_STEP: f32 = 75.5;

/// 0 = plată, 1 = liftată, 2 = slice. Matches `shot` in index.html.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shot {
    Flat = 0,
    Topspin = 1,
    Slice = 2,
}

impl Shot {
    pub fn from_index(i: u8) -> Self {
        match i {
            1 => Shot::Topspin,
            2 => Shot::Slice,
            _ => Shot::Flat,
        }
    }
    pub fn label_ro(self) -> &'static str {
        match self {
            Shot::Flat => "plată",
            Shot::Topspin => "liftată",
            Shot::Slice => "slice",
        }
    }
    /// `typeBonus` in `hit()`: liftată +0.04, slice +0.025.
    pub fn type_bonus(self) -> f32 {
        match self {
            Shot::Topspin => 0.04,
            Shot::Slice => 0.025,
            Shot::Flat => 0.0,
        }
    }
}

pub fn dir_label_ro(dir: Dir) -> &'static str {
    match dir {
        d if d <= -2 => "stânga",
        -1 => "mijloc-stânga",
        0 => "centru",
        1 => "mijloc-dreapta",
        _ => "dreapta",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Difficulty {
    Easy,
    Normal,
    Hard,
}

impl Difficulty {
    /// Ace chance on serve. Matches `serve()` in index.html; Hard reads
    /// serves better, so free aces against it are rarer.
    pub fn ace_chance(self) -> f32 {
        match self {
            Difficulty::Easy => 0.13,
            Difficulty::Normal => 0.08,
            Difficulty::Hard => 0.025,
        }
    }
    /// Opponent `defense` in `hit()`: how much of your timing quality the
    /// bot eats before a ball becomes a winner.
    pub fn defense(self) -> f32 {
        match self {
            Difficulty::Easy => 0.24,
            Difficulty::Normal => 0.58,
            Difficulty::Hard => 0.82,
        }
    }
    /// Counter-attack multiplier in `opponent_counter_chance()`: on Hard the
    /// bot punishes weak returns much more often.
    pub fn counter_factor(self) -> f32 {
        match self {
            Difficulty::Easy => 0.8,
            Difficulty::Normal => 1.0,
            Difficulty::Hard => 1.3,
        }
    }
    /// How hard the bot aims away from you (0 = picks lanes uniformly).
    /// Easy hits anywhere; Normal prefers the open court; Hard hunts the
    /// far corner almost every ball.
    pub fn aim_bias(self) -> f32 {
        match self {
            Difficulty::Easy => 0.0,
            Difficulty::Normal => 0.5,
            Difficulty::Hard => 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Ready,
    Serve,
    Incoming,
    Hit,
    Return,
    Finished,
}

impl Phase {
    pub fn visual_label_ro(self) -> &'static str {
        match self {
            Phase::Ready => "PREGĂTIT PENTRU SERVICIU",
            Phase::Serve => "SERVICIU",
            Phase::Incoming => "MINGEA SE APROPIE",
            Phase::Hit => "LOVEȘTE ACUM",
            Phase::Return => "MINGEA SPRE ADVERSAR",
            Phase::Finished => "MECI ÎNCHEIAT",
        }
    }
}

/// English umpire call + MP3 clip key (file stem in `sunete/`).
/// `clip_key == None` means "synthesized/TTS fallback" (tie-break score).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UmpireCall {
    pub text_en: String,
    pub clip_key: Option<String>,
}

/// Every clip key the umpire/crowd can request (mirrors `clips` in index.html).
pub const ALL_CLIP_KEYS: &[&str] = &[
    "hit", "out", "fault", "applause", "reaction", "ambient",
    "advantage-server", "advantage-receiver", "game-server", "game-receiver",
    "set-server", "set-receiver", "match-server", "match-receiver",
    "double-fault", "let", "net", "first-service", "second-service",
    "tie-break", "new-balls", "deuce",
    "love-all", "love-fifteen", "love-thirty", "love-forty",
    "fifteen-love", "fifteen-all", "fifteen-thirty", "fifteen-forty",
    "thirty-love", "thirty-fifteen", "thirty-all", "thirty-forty",
    "forty-love", "forty-fifteen", "forty-thirty",
];

/// Map a clip key to its MP3 filename. Returns `None` for TTS-only calls.
pub fn clip_filename(key: &str) -> Option<&'static str> {
    match key {
        "hit" => Some("serve.mp3"),
        "out" => Some("out.mp3"),
        "fault" => Some("fault.mp3"),
        "applause" => Some("crowd-small.mp3"),
        "reaction" => Some("crowd-reaction.mp3"),
        "ambient" => Some("crowd-ambient.mp3"),
        "advantage-server" => Some("advantage-server.mp3"),
        "advantage-receiver" => Some("advantage-receiver.mp3"),
        "game-server" => Some("game-server.mp3"),
        "game-receiver" => Some("game-receiver.mp3"),
        "set-server" => Some("set-server.mp3"),
        "set-receiver" => Some("set-receiver.mp3"),
        "match-server" => Some("game-set-and-match-server.mp3"),
        "match-receiver" => Some("game-set-and-match-receiver.mp3"),
        "double-fault" => Some("double-fault.mp3"),
        "let" => Some("let.mp3"),
        "net" => Some("net.mp3"),
        "first-service" => Some("first-service.mp3"),
        "second-service" => Some("second-service.mp3"),
        "tie-break" => Some("tie-break.mp3"),
        "new-balls" => Some("new-balls.mp3"),
        "deuce" => Some("deuce.mp3"),
        "love-all" => Some("love-all.mp3"),
        "love-fifteen" => Some("love-fifteen.mp3"),
        "love-thirty" => Some("love-thirty.mp3"),
        "love-forty" => Some("love-forty.mp3"),
        "fifteen-love" => Some("fifteen-love.mp3"),
        "fifteen-all" => Some("fifteen-all.mp3"),
        "fifteen-thirty" => Some("fifteen-thirty.mp3"),
        "fifteen-forty" => Some("fifteen-forty.mp3"),
        "thirty-love" => Some("thirty-love.mp3"),
        "thirty-fifteen" => Some("thirty-fifteen.mp3"),
        "thirty-all" => Some("thirty-all.mp3"),
        "thirty-forty" => Some("thirty-forty.mp3"),
        "forty-love" => Some("forty-love.mp3"),
        "forty-fifteen" => Some("forty-fifteen.mp3"),
        "forty-thirty" => Some("forty-thirty.mp3"),
        _ => None,
    }
}

const POINT_NAMES_RO: [&str; 4] = ["0", "15", "30", "40"];
const POINT_NAMES_EN: [&str; 4] = ["love", "fifteen", "thirty", "forty"];

/// A finished point: what to announce (RO for screen reader) and what the
/// umpire says (EN + clip key for `sunete/`).
#[derive(Debug, Clone)]
pub struct PointResult {
    /// Romanian sentence, e.g. "Game pentru tine! Seturi: ...".
    pub message_ro: String,
    pub game_won: bool,
    pub set_won: bool,
    pub match_finished: bool,
    pub umpire: UmpireCall,
}

/// Whole-match state. Index 0 = player ("tu"/server), 1 = opponent.
#[derive(Debug, Clone)]
pub struct Match {
    pub pts: [u32; 2],
    pub games: [u32; 2],
    pub sets: [u32; 2],
    pub tiebreak: bool,
    pub phase: Phase,
    pub dir: Dir,
    pub shot: Shot,
    pub incoming_side: Dir,
    pub history_ro: String,
}

impl Default for Match {
    fn default() -> Self {
        Self::new()
    }
}

impl Match {
    pub fn new() -> Self {
        Self {
            pts: [0, 0],
            games: [0, 0],
            sets: [0, 0],
            tiebreak: false,
            phase: Phase::Ready,
            dir: 0,
            shot: Shot::Flat,
            incoming_side: 0,
            history_ro: "Meci nou. Apasă S pentru serviciu.".to_string(),
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    pub fn set_dir(&mut self, dir: Dir) {
        self.dir = dir.clamp(-MAX_DIR, MAX_DIR);
    }

    pub fn nudge_dir(&mut self, delta: i8) {
        self.set_dir(self.dir + delta);
    }

    pub fn set_shot(&mut self, shot: Shot) {
        self.shot = shot;
    }

    /// Romanian score line. Mirrors `scoreText()` in index.html.
    pub fn score_text_ro(&self) -> String {
        let v = if self.tiebreak {
            format!("Tie-break: tu {}, calculator {}", self.pts[0], self.pts[1])
        } else if self.pts[0] >= 3 && self.pts[1] >= 3 {
            if self.pts[0] == self.pts[1] {
                "Egalitate".to_string()
            } else if self.pts[0] > self.pts[1] {
                "Avantaj tu".to_string()
            } else {
                "Avantaj calculator".to_string()
            }
        } else {
            format!(
                "Tu {}, calculator {}",
                POINT_NAMES_RO[self.pts[0].min(3) as usize],
                POINT_NAMES_RO[self.pts[1].min(3) as usize]
            )
        };
        format!(
            "Seturi: tu {}, calculator {}. Game-uri: tu {}, calculator {}. {}.",
            self.sets[0], self.sets[1], self.games[0], self.games[1], v
        )
    }

    /// English umpire call + clip. Mirrors `umpireScore()` in index.html.
    pub fn umpire_score(&self) -> UmpireCall {
        if self.tiebreak {
            return UmpireCall {
                text_en: format!("Tie break. {} to {}", self.pts[0], self.pts[1]),
                clip_key: None, // generated vocally per CITESTE-MA / index.html notice
            };
        }
        if self.pts[0] >= 3 && self.pts[1] >= 3 {
            if self.pts[0] == self.pts[1] {
                return UmpireCall {
                    text_en: "Deuce".to_string(),
                    clip_key: Some("deuce".to_string()),
                };
            }
            if self.pts[0] > self.pts[1] {
                return UmpireCall {
                    text_en: "Advantage server".to_string(),
                    clip_key: Some("advantage-server".to_string()),
                };
            }
            return UmpireCall {
                text_en: "Advantage receiver".to_string(),
                clip_key: Some("advantage-receiver".to_string()),
            };
        }
        let x = POINT_NAMES_EN[self.pts[0].min(3) as usize];
        let y = POINT_NAMES_EN[self.pts[1].min(3) as usize];
        let (text_en, key) = if x == y {
            if x == "love" {
                ("Love all".to_string(), "love-all".to_string())
            } else {
                (format!("{x} all"), format!("{x}-all"))
            }
        } else {
            (format!("{x} {y}"), format!("{x}-{y}"))
        };
        UmpireCall {
            text_en,
            clip_key: Some(key),
        }
    }

    /// Score a finished point. Mirrors `point(winner, reason)` in index.html.
    pub fn score_point(&mut self, winner: usize, reason_ro: &str) -> PointResult {
        debug_assert!(winner <= 1);
        self.pts[winner] += 1;
        let mut msg = format!("{reason_ro}. ");
        let (mut game_won, mut set_won) = (false, false);

        if self.tiebreak {
            if self.pts[winner] >= 7 && self.pts[winner].saturating_sub(self.pts[1 - winner]) >= 2 {
                self.games[winner] += 1;
                self.sets[winner] += 1;
                self.phase = Phase::Finished;
                set_won = true;
                msg += if winner == 0 {
                    "Ai câștigat tie-break-ul și meciul! "
                } else {
                    "Calculatorul a câștigat tie-break-ul și meciul. "
                };
            }
        } else if self.pts[winner] >= 4
            && self.pts[winner].saturating_sub(self.pts[1 - winner]) >= 2
        {
            self.games[winner] += 1;
            self.pts = [0, 0];
            game_won = true;
            msg += if winner == 0 {
                "Game pentru tine! "
            } else {
                "Game pentru calculator. "
            };
            if self.games[winner] >= 6
                && self.games[winner].saturating_sub(self.games[1 - winner]) >= 2
            {
                self.sets[winner] += 1;
                self.phase = Phase::Finished;
                set_won = true;
                msg += if winner == 0 {
                    "Ai câștigat setul și meciul! "
                } else {
                    "Calculatorul a câștigat setul și meciul. "
                };
            } else if self.games[0] == 6 && self.games[1] == 6 {
                self.tiebreak = true;
                msg += "Începe tie-break-ul. ";
            }
        }

        if self.phase != Phase::Finished {
            self.phase = Phase::Ready;
        }

        let umpire = if set_won {
            UmpireCall {
                text_en: "Game, set and match".to_string(),
                clip_key: Some(
                    if winner == 0 {
                        "match-server"
                    } else {
                        "match-receiver"
                    }
                    .to_string(),
                ),
            }
        } else if game_won {
            UmpireCall {
                text_en: "Game".to_string(),
                clip_key: Some(
                    if winner == 0 {
                        "game-server"
                    } else {
                        "game-receiver"
                    }
                    .to_string(),
                ),
            }
        } else {
            self.umpire_score()
        };

        msg += &self.score_text_ro();
        if self.phase == Phase::Ready {
            msg += " Apasă S pentru serviciu.";
        }
        self.history_ro = msg.clone();

        PointResult {
            message_ro: msg,
            game_won,
            set_won,
            match_finished: self.phase == Phase::Finished,
            umpire,
        }
    }

    /// Serve ace? `roll` in [0,1). Mirrors the ace check in `serve()`.
    pub fn serve_is_ace(difficulty: Difficulty, roll: f32) -> bool {
        roll < difficulty.ace_chance()
    }

    /// Timing quality from `hit()`: ms since the "LOVEȘTE ACUM" cue.
    pub fn hit_quality(elapsed_ms: f32) -> f32 {
        if elapsed_ms < 330.0 {
            1.0
        } else if elapsed_ms < 750.0 {
            0.87
        } else if elapsed_ms < 1100.0 {
            0.66
        } else {
            0.44
        }
    }

    /// Contact quality from paddle-to-ball distance (px) at swing time.
    /// Same bands as `hit_quality`: set on the ball = perfect, a lane
    /// away = good, lunging = poor, over 120px = whiff territory
    /// (the caller awards the point instead of swinging at air).
    pub fn position_quality(dx: f32) -> f32 {
        if dx < 25.0 {
            1.0
        } else if dx < 60.0 {
            0.87
        } else if dx < 120.0 {
            0.66
        } else {
            0.44
        }
    }

    /// Whiff line: farther than this from the ball and the swing hits air.
    pub const WHIFF_DISTANCE: f32 = 120.0;

    /// Chance the return goes OUT. Mirrors `outChance` in `hit()`.
    pub fn out_chance(quality: f32, shot: Shot) -> f32 {
        0.035 + (1.0 - quality) * 0.26 + if shot == Shot::Flat { 0.025 } else { 0.0 }
    }

    /// Chance the return is an outright winner. Mirrors `winnerChance`.
    pub fn winner_chance(
        quality: f32,
        difficulty: Difficulty,
        alignment: u8,
        shot: Shot,
    ) -> f32 {
        (0.28 + quality * 0.46 - difficulty.defense() * 0.50
            + f32::from(alignment) * 0.055
            + shot.type_bonus())
        .clamp(0.09, 0.89)
    }

    /// Chance the opponent wins the rally outright after a non-winner return.
    /// Scales with difficulty: the Hard bot punishes weak balls far more.
    pub fn opponent_counter_chance(quality: f32, difficulty: Difficulty) -> f32 {
        (0.17 + (1.0 - quality) * 0.15) * difficulty.counter_factor()
    }

    /// Which lane the bot sends the ball to. `player_lane` is where you
    /// stand (-2..=2); `roll` in [0,1) picks deterministically so tests
    /// stay reproducible. Easy ignores you, harder bots weight the far
    /// lanes from your position — you get wrong-footed if you camp.
    pub fn pick_incoming_side(difficulty: Difficulty, player_lane: Dir, roll: f32) -> Dir {
        const SIDES: [Dir; 5] = [-2, -1, 0, 1, 2];
        let bias = difficulty.aim_bias();
        let mut acc = 0.0;
        let mut edges = [0.0f32; 5];
        for (i, side) in SIDES.iter().enumerate() {
            let dist = side.abs_diff(player_lane).min(4) as f32;
            acc += 1.0 + bias * dist;
            edges[i] = acc;
        }
        let x = roll.clamp(0.0, 0.999_999) * acc;
        let mut pick = SIDES[4];
        for (i, &edge) in edges.iter().enumerate() {
            if x < edge {
                pick = SIDES[i];
                break;
            }
        }
        pick.clamp(-MAX_DIR, MAX_DIR)
    }
}

/// Outcome of a return rally, resolved with three rolls in [0,1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RallyOutcome {
    /// Player's ball went out → point to opponent.
    Out,
    /// Clean winner → point to player.
    Winner,
    /// Opponent countered → point to opponent.
    Countered,
    /// Rally continues → opponent sends another incoming ball.
    Continue,
}

impl RallyOutcome {
    pub fn resolve(
        difficulty: Difficulty,
        dir: Dir,
        incoming_side: Dir,
        shot: Shot,
        quality: f32,
        roll_out: f32,
        roll_win: f32,
        roll_counter: f32,
    ) -> Self {
        if roll_out < Match::out_chance(quality, shot) {
            return RallyOutcome::Out;
        }
        let alignment = dir.abs_diff(incoming_side).min(2);
        if roll_win < Match::winner_chance(quality, difficulty, alignment, shot) {
            return RallyOutcome::Winner;
        }
        if roll_counter < Match::opponent_counter_chance(quality, difficulty) {
            return RallyOutcome::Countered;
        }
        RallyOutcome::Continue
    }

    pub fn point_winner(self) -> Option<usize> {
        match self {
            RallyOutcome::Out | RallyOutcome::Countered => Some(1),
            RallyOutcome::Winner => Some(0),
            RallyOutcome::Continue => None,
        }
    }

    pub fn reason_ro(self) -> &'static str {
        match self {
            RallyOutcome::Out => "AUT! Mingea ta a ieșit afară",
            RallyOutcome::Winner => "Punct câștigat",
            RallyOutcome::Countered => "Adversarul a câștigat punctul",
            RallyOutcome::Continue => "Schimbul continuă",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn five_lanes_labels_and_clamp() {
        assert_eq!(dir_label_ro(-2), "stânga");
        assert_eq!(dir_label_ro(-1), "mijloc-stânga");
        assert_eq!(dir_label_ro(0), "centru");
        assert_eq!(dir_label_ro(1), "mijloc-dreapta");
        assert_eq!(dir_label_ro(2), "dreapta");
        // out-of-range input still clamps to the outer lanes
        assert_eq!(dir_label_ro(-9), "stânga");
        assert_eq!(dir_label_ro(9), "dreapta");
        let mut m = Match::new();
        m.set_dir(9);
        assert_eq!(m.dir, 2);
        m.set_dir(-9);
        assert_eq!(m.dir, -2);
        m.set_dir(0);
        m.nudge_dir(-1);
        m.nudge_dir(-1);
        m.nudge_dir(-1);
        assert_eq!(m.dir, -2, "nudging past the edge sticks at stânga");
        for _ in 0..9 {
            m.nudge_dir(1);
        }
        assert_eq!(m.dir, 2, "nudging past the edge sticks at dreapta");
    }

    #[test]
    fn bot_aim_targets_away_from_player() {
        // Easy: uniform lanes, ignores you. Sides [-2..2], weight 1 each.
        assert_eq!(Match::pick_incoming_side(Difficulty::Easy, 0, 0.0), -2);
        assert_eq!(Match::pick_incoming_side(Difficulty::Easy, 0, 0.39), -1);
        assert_eq!(Match::pick_incoming_side(Difficulty::Easy, 1, 0.99), 2);
        // Hard, you camp stânga (-2): weights [1,2,3,4,5], total 15.
        assert_eq!(Match::pick_incoming_side(Difficulty::Hard, -2, 0.0), -2);
        assert_eq!(Match::pick_incoming_side(Difficulty::Hard, -2, 0.5), 1);
        assert_eq!(Match::pick_incoming_side(Difficulty::Hard, -2, 0.99), 2);
        // Hard, you camp dreapta (+2): weights [5,4,3,2,1], total 15 —
        // mirror image, hunts stânga.
        assert_eq!(Match::pick_incoming_side(Difficulty::Hard, 2, 0.1), -2);
        assert_eq!(Match::pick_incoming_side(Difficulty::Hard, 2, 0.5), -1);
        // Normal, you hold centru (0): weights [2,1.5,1,1.5,2], total 7.
        assert_eq!(Match::pick_incoming_side(Difficulty::Normal, 0, 0.5), 0);
        assert_eq!(Match::pick_incoming_side(Difficulty::Normal, 0, 0.99), 2);
        // Always a legal lane, whatever the roll.
        for &d in &[Difficulty::Easy, Difficulty::Normal, Difficulty::Hard] {
            for i in 0..=10 {
                let s = Match::pick_incoming_side(d, (i % 5) as i8 - 2, i as f32 / 10.0);
                assert!((-2..=2).contains(&s), "{d:?} lane {s}");
            }
        }
    }

    #[test]
    fn hard_bot_counters_and_reads_serves_better() {
        assert_eq!(Difficulty::Hard.counter_factor(), 1.3);
        assert_eq!(Difficulty::Normal.counter_factor(), 1.0);
        // Same weak ball: Hard counters ~30% more often than base rate.
        let base = Match::opponent_counter_chance(0.5, Difficulty::Normal);
        let hard = Match::opponent_counter_chance(0.5, Difficulty::Hard);
        assert!((hard / base - 1.3).abs() < 1e-6, "{hard} vs {base}");
        // Hard gives away almost no free aces.
        assert!(Match::serve_is_ace(Difficulty::Hard, 0.02));
        assert!(!Match::serve_is_ace(Difficulty::Hard, 0.03));
    }

    #[test]
    fn new_match_score_text() {
        let m = Match::new();
        assert!(m.score_text_ro().contains("Tu 0, calculator 0"));
        let u = m.umpire_score();
        assert_eq!(u.text_en, "Love all");
        assert_eq!(u.clip_key.as_deref(), Some("love-all"));
    }

    #[test]
    fn deuce_advantage_calls() {
        let mut m = Match::new();
        m.pts = [3, 3];
        assert_eq!(m.umpire_score().clip_key.as_deref(), Some("deuce"));
        m.pts = [4, 3];
        let u = m.umpire_score();
        assert_eq!(u.text_en, "Advantage server");
        m.pts = [3, 4];
        assert_eq!(
            m.umpire_score().clip_key.as_deref(),
            Some("advantage-receiver")
        );
    }

    #[test]
    fn game_win_resets_points() {
        let mut m = Match::new();
        m.pts = [3, 0];
        let r = m.score_point(0, "Punct câștigat");
        assert!(r.game_won);
        assert_eq!(m.games, [1, 0]);
        assert_eq!(m.pts, [0, 0]);
        assert_eq!(r.umpire.clip_key.as_deref(), Some("game-server"));
    }

    #[test]
    fn needs_two_point_margin() {
        let mut m = Match::new();
        m.pts = [3, 3];
        let r = m.score_point(0, "Punct");
        assert!(!r.game_won, "40-40 + 1 punct nu e game");
        let r = m.score_point(0, "Punct");
        assert!(r.game_won);
    }

    #[test]
    fn six_six_starts_tiebreak() {
        let mut m = Match::new();
        m.games = [5, 6];
        m.pts = [3, 1];
        let r = m.score_point(0, "Punct");
        assert!(r.game_won);
        assert!(m.tiebreak);
        assert!(!r.match_finished);
    }

    #[test]
    fn tiebreak_win_ends_match() {
        let mut m = Match::new();
        m.tiebreak = true;
        m.games = [6, 6];
        m.pts = [6, 4];
        let r = m.score_point(0, "Punct câștigat");
        assert!(r.set_won && r.match_finished);
        assert_eq!(m.sets, [1, 0]);
        assert_eq!(r.umpire.clip_key.as_deref(), Some("match-server"));
    }

    #[test]
    fn serve_ace_thresholds() {
        assert!(Match::serve_is_ace(Difficulty::Easy, 0.12));
        assert!(!Match::serve_is_ace(Difficulty::Easy, 0.14));
        assert!(!Match::serve_is_ace(Difficulty::Normal, 0.08));
        assert!(Match::serve_is_ace(Difficulty::Normal, 0.07));
    }

    #[test]
    fn hit_quality_windows_match_js() {
        assert_eq!(Match::hit_quality(100.0), 1.0);
        assert_eq!(Match::hit_quality(500.0), 0.87);
        assert_eq!(Match::hit_quality(900.0), 0.66);
        assert_eq!(Match::hit_quality(1300.0), 0.44);
    }

    #[test]
    fn rally_out_winner_counter_continue() {
        // Forced OUT: roll_out below out_chance.
        assert_eq!(
            RallyOutcome::resolve(Difficulty::Normal, 0, 0, Shot::Flat, 1.0, 0.0, 0.99, 0.99),
            RallyOutcome::Out
        );
        // Winner: perfect contact, aligned, topspin bonus.
        assert_eq!(
            RallyOutcome::resolve(Difficulty::Easy, 1, 1, Shot::Topspin, 1.0, 0.99, 0.0, 0.99),
            RallyOutcome::Winner
        );
        // Countered: poor contact on hard difficulty.
        assert_eq!(
            RallyOutcome::resolve(Difficulty::Hard, 0, 0, Shot::Flat, 0.44, 0.99, 0.99, 0.0),
            RallyOutcome::Countered
        );
    }

    #[test]
    fn position_quality_matches_hit_bands() {
        assert_eq!(Match::position_quality(0.0), 1.0);
        assert_eq!(Match::position_quality(24.9), 1.0);
        assert_eq!(Match::position_quality(25.0), 0.87);
        assert_eq!(Match::position_quality(59.9), 0.87);
        assert_eq!(Match::position_quality(60.0), 0.66);
        assert_eq!(Match::position_quality(119.9), 0.66);
        assert_eq!(Match::position_quality(120.0), 0.44);
        assert_eq!(Match::position_quality(500.0), 0.44);
        assert!(Match::WHIFF_DISTANCE > 0.0);
    }

    #[test]
    fn all_umpire_clip_keys_have_files() {        let mut m = Match::new();
        // Sweep representative scorelines; every non-tiebreak key must map to a file.
        for a in 0..5 {
            for b in 0..5 {
                m.pts = [a, b];
                m.tiebreak = false;
                let u = m.umpire_score();
                let key = u.clip_key.expect("non-tiebreak needs a clip");
                assert!(clip_filename(&key).is_some(), "missing file for {key}");
            }
        }
        for key in ["game-server", "game-receiver", "match-server", "match-receiver"] {
            assert!(clip_filename(key).is_some());
        }
        // The audio engine's preload list covers every known clip.
        assert_eq!(ALL_CLIP_KEYS.len(), 37);
        for key in ALL_CLIP_KEYS {
            assert!(clip_filename(key).is_some(), "unknown key {key}");
        }
    }
}
