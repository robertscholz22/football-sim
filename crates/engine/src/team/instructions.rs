//! Layer 2: manager instructions. Every field is an `Option`:
//! `None` = trust the layer-1 optimum, `Some` = the manager overrides it.

use serde::{Deserialize, Serialize};

/// Height of the defensive block (BLK-05).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockHeight {
    High,
    Mid,
    Low,
}

/// Which way the first defender directs the ball carrier (DEF-07).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Channel {
    Wide,
    Inside,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TeamInstructions {
    /// Defensive line / block height.
    pub block_height: Option<BlockHeight>,
    /// 0 = only press on triggers inside the zone, 1 = press everywhere with a second presser.
    pub pressing_intensity: Option<f32>,
    /// In-possession width, 0 = narrow, 1 = full width.
    pub width: Option<f32>,
    /// −1 = short passing, +1 = direct / long.
    pub directness: Option<f32>,
    /// −1 = patient, +1 = high tempo.
    pub tempo: Option<f32>,
    pub channel: Option<Channel>,
    /// Out-of-possession shape and box defending.
    pub defending: DefendingInstructions,
    /// Receiving and keeping the ball.
    pub attacking: AttackingInstructions,
    /// Set pieces.
    pub set_pieces: SetPieceInstructions,
}

/// Layer-2 overrides of the set-piece parameters (see `ResolvedTactics::set_pieces`); `None` keeps the catalog value.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SetPieceInstructions {
    /// SP-01: zonal players at defending corners (0 = player to player; 6+ = zonal).
    pub zonal: Option<u8>,
    /// SP-01: a player on the edge of the box at defending corners.
    pub edge: Option<bool>,
    /// SP-01: outfield players left up for the counter at defending corners.
    pub counter: Option<u8>,
    /// SP-02: attackers in the box at attacking corners and free kicks.
    pub attackers: Option<u8>,
    /// SP-02: a short option at attacking corners.
    pub short: Option<bool>,
    /// SP-02: share of the corners played short.
    pub short_share: Option<f32>,
    /// SP-02: weights of the deliveries to the first post, the centre and the second post.
    pub delivery: Option<[f32; 3]>,
    /// SP-03: the largest wall.
    pub wall_max: Option<u8>,
    /// SP-03: distance of the defensive line from the goal line at free kicks to be crossed (m).
    pub line_m: Option<f32>,
}

/// Layer-2 overrides of the in-possession parameters, in engine units (see `ResolvedTactics::attacking`, whose fields
/// they mirror). The manager-facing options (`TeamOption`) set these; `None` keeps the catalog value.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AttackingInstructions {
    /// ATT-24: how far (m) a pressed receiver comes towards the passer ("come short for the ball" / "hold position").
    pub meet_m: Option<f32>,
    /// ATT-25: share of a turnover expected when the player on the ball is pressed ("release it early" / "keep it
    /// under pressure").
    pub pressed_loss: Option<f32>,
    /// ATT-25: the same for the receiver of a pass ("find the free man" / "play into feet under pressure").
    pub receiver_loss: Option<f32>,
    /// ATT-11: height (m from the own goal line) of the in-possession back line with the ball in the own, middle and
    /// final third ("push up behind the ball" / "stay deep").
    pub line: Option<[f32; 3]>,
    /// ATT-21: bonus (fraction of the local value scale) for switching play once the ball side has drawn the
    /// opponents' pressure ("switch play" / "keep it on this side").
    pub switch_bonus: Option<f32>,
}

/// Layer-2 overrides of the defending parameters, in engine units (see `ResolvedTactics::defending`, whose fields
/// they mirror). The manager-facing options (`TeamOption`) set these; `None` keeps the catalog value.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DefendingInstructions {
    /// Block line (team frame x, m); replaces the line of `block_height`.
    pub line: Option<f32>,
    /// Block length (m); replaces the length of `block_height`.
    pub length: Option<f32>,
    pub width: Option<f32>,
    pub shift_factor: Option<f32>,
    pub gap_pressured: Option<f32>,
    pub gap_unpressured: Option<f32>,
    pub max_drop_share: Option<f32>,
    pub box_far: Option<f32>,
    pub cross_line_depth: Option<f32>,
    pub trap: Option<f32>,
    /// PRS-01: how far away (s) a team-mate may be to double a carrier standing closed to play (0: never double).
    pub double_time: Option<f32>,
    pub box_max_gap: Option<f32>,
    pub box_shift_factor: Option<f32>,
    pub box_mid_distance: Option<f32>,
    pub box_drop_urgency: Option<f32>,
    pub contain_distance: Option<f32>,
    pub block_distance: Option<f32>,
    pub cover_radius: Option<f32>,
    pub tackle_min_p: Option<f32>,
    /// IND-07: how late (s) after the receiver the first defender still goes for an opponent's high ball (negative:
    /// only when he gets there first; large: always).
    pub challenge_slack: Option<f32>,
    /// DEF-04: from what speed (m/s) a marker tracks an opponent running out of his zone (low: track every runner;
    /// high: pass runners on to the line).
    pub track_speed: Option<f32>,
    /// BLK-03: how far (m) in front of the line a back-line defender follows a man dropping between the lines (0: pass
    /// him on to the midfield).
    pub step_out: Option<f32>,
    /// BLK-03: how far (share of the gap) the line shuffles across into the slot of a defender who has stepped out.
    pub shuffle: Option<f32>,
    /// TRN-04: probability that a player beaten just after losing the ball high up stops the counter with a foul.
    pub tactical_foul: Option<f32>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlayerInstructions {
    /// −1 = stay back, +1 = get forward / make runs in behind.
    pub forward_runs: Option<f32>,
    /// −1 = dribble less, +1 = dribble more.
    pub dribble_freq: Option<f32>,
    /// ATT-26, front-line players: −1 = stay on the last line, +1 = come short / drop between the lines.
    pub come_short: Option<f32>,
    /// ATT-20, full-backs and wing-backs (ROLE-05 / 06): −1 = never overlap or underlap, +1 = overlap whenever on.
    pub overlap: Option<f32>,
    /// Full-backs and wing-backs: −1 = stay wide, +1 = move inside beside the pivot (BU-06 [FIFA-IFB-CWC25]).
    pub invert: Option<f32>,
}
