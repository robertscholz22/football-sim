//! Manager-facing options (M5): named choices that set the engine-unit instructions of `instructions.rs`. Each option
//! is a multiple of, or a step from, the layer-1 value its catalog rule gives, so the options follow the calibration.
//! Player presets (docs/player-brain.md section 5.4) are named combinations of the player instructions.

use serde::{Deserialize, Serialize};

use super::instructions::{PlayerInstructions, TeamInstructions};
use crate::config::TacticsConfig;

/// A team option. Options of one pair set the same instruction in opposite directions; applying one replaces the
/// other.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TeamOption {
    /// IND-07: attack every high ball / only when first to it.
    AttackEveryHighBall,
    OnlyWhenFirstToIt,
    /// DEF-04: track every runner / pass runners on to the line.
    TrackEveryRunner,
    PassRunnersOn,
    /// BLK-03: step out to a man dropping between the lines / hold the line and pass him on to the midfield.
    StepOutToMidfielders,
    HoldTheLine,
    /// PRS-05: trap at the touchline / no trap.
    TrapAtTheTouchline,
    NoTouchlineTrap,
    /// PRS-01: double up on a player with his back to goal from further away / never double up.
    DoubleUpOnBackToGoal,
    NoDoublingUp,
    /// CRS-01: defend crosses deep / higher.
    DefendCrossesDeep,
    DefendCrossesHigh,
    /// ATT-21: switch play / keep it on this side.
    SwitchPlay,
    KeepItOnThisSide,
    /// ATT-24: come short for the ball / hold position.
    ComeShortForTheBall,
    HoldPosition,
    /// ATT-25: release the ball early / keep the ball under pressure.
    ReleaseTheBallEarly,
    KeepTheBallUnderPressure,
    /// ATT-25: find the free man / play into feet under pressure.
    FindTheFreeMan,
    PlayIntoFeet,
    /// ATT-11: push up behind the ball / stay deep.
    PushUpBehindTheBall,
    StayDeep,
    /// TRN-04: stop counters with a foul / never foul on purpose.
    StopCountersWithFouls,
    NoTacticalFouls,
    /// SP-01: zonal marking / player-to-player marking at defending corners.
    ZonalAtCorners,
    ManToManAtCorners,
    /// SP-01: leave players up for the counter at defending corners / everyone back.
    LeavePlayersUp,
    EveryoneBack,
    /// SP-02: commit more / fewer players to the box at attacking corners and free kicks.
    CommitToTheBox,
    HoldBackAtSetPieces,
}

impl TeamOption {
    pub const ALL: [TeamOption; 30] = [
        TeamOption::AttackEveryHighBall,
        TeamOption::OnlyWhenFirstToIt,
        TeamOption::TrackEveryRunner,
        TeamOption::PassRunnersOn,
        TeamOption::StepOutToMidfielders,
        TeamOption::HoldTheLine,
        TeamOption::TrapAtTheTouchline,
        TeamOption::NoTouchlineTrap,
        TeamOption::DoubleUpOnBackToGoal,
        TeamOption::NoDoublingUp,
        TeamOption::DefendCrossesDeep,
        TeamOption::DefendCrossesHigh,
        TeamOption::SwitchPlay,
        TeamOption::KeepItOnThisSide,
        TeamOption::ComeShortForTheBall,
        TeamOption::HoldPosition,
        TeamOption::ReleaseTheBallEarly,
        TeamOption::KeepTheBallUnderPressure,
        TeamOption::FindTheFreeMan,
        TeamOption::PlayIntoFeet,
        TeamOption::PushUpBehindTheBall,
        TeamOption::StayDeep,
        TeamOption::StopCountersWithFouls,
        TeamOption::NoTacticalFouls,
        TeamOption::ZonalAtCorners,
        TeamOption::ManToManAtCorners,
        TeamOption::LeavePlayersUp,
        TeamOption::EveryoneBack,
        TeamOption::CommitToTheBox,
        TeamOption::HoldBackAtSetPieces,
    ];

    pub fn label(self) -> &'static str {
        match self {
            TeamOption::AttackEveryHighBall => "attack every high ball",
            TeamOption::OnlyWhenFirstToIt => "only when first to it",
            TeamOption::TrackEveryRunner => "track every runner",
            TeamOption::PassRunnersOn => "pass runners on",
            TeamOption::StepOutToMidfielders => "step out to midfielders",
            TeamOption::HoldTheLine => "hold the line",
            TeamOption::TrapAtTheTouchline => "trap at the touchline",
            TeamOption::NoTouchlineTrap => "no touchline trap",
            TeamOption::DoubleUpOnBackToGoal => "double up on a player with his back to goal",
            TeamOption::NoDoublingUp => "no doubling up",
            TeamOption::DefendCrossesDeep => "defend crosses deep",
            TeamOption::DefendCrossesHigh => "defend crosses higher",
            TeamOption::SwitchPlay => "switch play",
            TeamOption::KeepItOnThisSide => "keep it on this side",
            TeamOption::ComeShortForTheBall => "come short for the ball",
            TeamOption::HoldPosition => "hold position",
            TeamOption::ReleaseTheBallEarly => "release the ball early",
            TeamOption::KeepTheBallUnderPressure => "keep the ball under pressure",
            TeamOption::FindTheFreeMan => "find the free man",
            TeamOption::PlayIntoFeet => "play into feet under pressure",
            TeamOption::PushUpBehindTheBall => "push up behind the ball",
            TeamOption::StayDeep => "stay deep",
            TeamOption::StopCountersWithFouls => "stop counters with a foul",
            TeamOption::NoTacticalFouls => "no tactical fouls",
            TeamOption::ZonalAtCorners => "zonal marking at corners",
            TeamOption::ManToManAtCorners => "man-to-man at corners",
            TeamOption::LeavePlayersUp => "leave players up at corners",
            TeamOption::EveryoneBack => "everyone back at corners",
            TeamOption::CommitToTheBox => "commit players to the box",
            TeamOption::HoldBackAtSetPieces => "hold back at set pieces",
        }
    }

    /// Set this option's instruction in `t`.
    pub fn apply(self, t: &mut TeamInstructions, tc: &TacticsConfig) {
        let (d, a, s) = (&mut t.defending, &mut t.attacking, &mut t.set_pieces);
        match self {
            TeamOption::AttackEveryHighBall => d.challenge_slack = Some(tc.ind07_challenge_slack_s + 1.0),
            TeamOption::OnlyWhenFirstToIt => d.challenge_slack = Some(-0.2),
            TeamOption::TrackEveryRunner => d.track_speed = Some(0.5 * tc.def04_track_speed),
            TeamOption::PassRunnersOn => d.track_speed = Some(2.0 * tc.def04_track_speed),
            TeamOption::StepOutToMidfielders => d.step_out = Some(1.5 * tc.blk03_step_out_m),
            TeamOption::HoldTheLine => d.step_out = Some(0.0),
            TeamOption::TrapAtTheTouchline => d.trap = Some(1.0),
            TeamOption::NoTouchlineTrap => d.trap = Some(0.0),
            TeamOption::DoubleUpOnBackToGoal => d.double_time = Some(1.5 * tc.prs01_double_time_s),
            TeamOption::NoDoublingUp => d.double_time = Some(0.0),
            TeamOption::DefendCrossesDeep => d.cross_line_depth = Some(tc.crs01_line_depth - 4.0),
            TeamOption::DefendCrossesHigh => d.cross_line_depth = Some(tc.crs01_line_depth + 4.0),
            TeamOption::SwitchPlay => a.switch_bonus = Some(2.0 * tc.att21_switch_bonus),
            TeamOption::KeepItOnThisSide => a.switch_bonus = Some(0.0),
            TeamOption::ComeShortForTheBall => a.meet_m = Some(2.0 * tc.att24_meet_m),
            TeamOption::HoldPosition => a.meet_m = Some(0.0),
            TeamOption::ReleaseTheBallEarly => a.pressed_loss = Some((2.0 * tc.att25_pressed_loss).min(1.0)),
            TeamOption::KeepTheBallUnderPressure => a.pressed_loss = Some(0.5 * tc.att25_pressed_loss),
            TeamOption::FindTheFreeMan => a.receiver_loss = Some((2.0 * tc.att25_receiver_loss).min(1.0)),
            TeamOption::PlayIntoFeet => a.receiver_loss = Some(0.5 * tc.att25_receiver_loss),
            TeamOption::PushUpBehindTheBall => a.line = Some(tc.att11_line.map(|h| h + 8.0)),
            TeamOption::StayDeep => a.line = Some(tc.att11_line.map(|h| h - 8.0)),
            TeamOption::StopCountersWithFouls => d.tactical_foul = Some((2.0 * tc.trn04_foul_p).min(1.0)),
            TeamOption::NoTacticalFouls => d.tactical_foul = Some(0.0),
            TeamOption::ZonalAtCorners => s.zonal = Some(6),
            TeamOption::ManToManAtCorners => s.zonal = Some(0),
            TeamOption::LeavePlayersUp => s.counter = Some(tc.sp01_counter + 2),
            TeamOption::EveryoneBack => s.counter = Some(0),
            TeamOption::CommitToTheBox => s.attackers = Some(tc.sp02_attackers + 2),
            TeamOption::HoldBackAtSetPieces => s.attackers = Some(tc.sp02_attackers.saturating_sub(2).max(1)),
        }
    }
}

/// A named player preset: a combination of player instructions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlayerPreset {
    /// Front player who drops between the lines instead of pinning the last line (ATT-26, ATT-15).
    FalseNine,
    /// Front player who stays on the last line as the target and does not drop or dribble (ATT-26, ATT-07).
    TargetMan,
    /// Front player who looks for the ball in behind (ATT-18).
    Runner,
    /// Full-back or wing-back who moves inside beside the pivot and does not overlap (BU-06).
    InvertedFullBack,
    /// Full-back or wing-back who goes round the winger and stays wide (ATT-20).
    OverlappingFullBack,
    /// Any player who stays behind the ball (TR-05).
    StayBack,
}

impl PlayerPreset {
    pub const ALL: [PlayerPreset; 6] = [
        PlayerPreset::FalseNine,
        PlayerPreset::TargetMan,
        PlayerPreset::Runner,
        PlayerPreset::InvertedFullBack,
        PlayerPreset::OverlappingFullBack,
        PlayerPreset::StayBack,
    ];

    pub fn label(self) -> &'static str {
        match self {
            PlayerPreset::FalseNine => "false 9",
            PlayerPreset::TargetMan => "target man",
            PlayerPreset::Runner => "runner in behind",
            PlayerPreset::InvertedFullBack => "inverted full-back",
            PlayerPreset::OverlappingFullBack => "overlapping full-back",
            PlayerPreset::StayBack => "stay back",
        }
    }

    pub fn instructions(self) -> PlayerInstructions {
        let mut p = PlayerInstructions::default();
        match self {
            PlayerPreset::FalseNine => (p.come_short, p.forward_runs) = (Some(1.0), Some(-0.3)),
            PlayerPreset::TargetMan => (p.come_short, p.dribble_freq) = (Some(-1.0), Some(-0.5)),
            PlayerPreset::Runner => (p.forward_runs, p.come_short) = (Some(1.0), Some(-0.5)),
            PlayerPreset::InvertedFullBack => (p.invert, p.overlap) = (Some(1.0), Some(-1.0)),
            PlayerPreset::OverlappingFullBack => (p.overlap, p.invert) = (Some(1.0), Some(-1.0)),
            PlayerPreset::StayBack => p.forward_runs = Some(-1.0),
        }
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::team::tactics::resolve;

    /// Every option moves its instruction away from layer 1, and the two options of a pair move it in opposite
    /// directions.
    #[test]
    fn each_option_moves_its_value_and_pairs_move_it_opposite_ways() {
        let tc = TacticsConfig::default();
        let l1 = resolve(&TeamInstructions::default(), &tc);
        let values = |o: TeamOption| {
            let mut t = TeamInstructions::default();
            o.apply(&mut t, &tc);
            let r = resolve(&t, &tc);
            let (d, a, s) = (r.defending, r.attacking, r.set_pieces);
            let (d1, a1, s1) = (l1.defending, l1.attacking, l1.set_pieces);
            [
                d.challenge_slack - d1.challenge_slack,
                d.track_speed - d1.track_speed,
                d.step_out - d1.step_out,
                d.trap - d1.trap,
                d.double_time - d1.double_time,
                d.cross_line_depth - d1.cross_line_depth,
                a.switch_bonus - a1.switch_bonus,
                a.meet_m - a1.meet_m,
                a.pressed_loss - a1.pressed_loss,
                a.receiver_loss - a1.receiver_loss,
                a.line[1] - a1.line[1],
                d.tactical_foul - d1.tactical_foul,
                s.zonal as f32 - s1.zonal as f32,
                s.counter as f32 - s1.counter as f32,
                s.attackers as f32 - s1.attackers as f32,
            ]
        };
        for pair in TeamOption::ALL.chunks(2) {
            let (x, y) = (values(pair[0]), values(pair[1]));
            let moved = |v: &[f32; 15]| v.iter().filter(|d| d.abs() > 1e-4).count();
            // An option may equal layer 1 (the trap is on by default); then its partner moves.
            assert!(moved(&x) + moved(&y) >= 1 && moved(&x) <= 1 && moved(&y) <= 1, "{pair:?}: {x:?} / {y:?}");
            let k = (0..15).find(|&k| x[k].abs() > 1e-4 || y[k].abs() > 1e-4).unwrap();
            assert!(x[k] != y[k], "{pair:?}");
            assert!(x[k] * y[k] <= 0.0, "{pair:?}: both on one side of layer 1: {} / {}", x[k], y[k]);
        }
    }

    #[test]
    fn presets_resolve_to_their_instructions() {
        use crate::team::tactics::resolve_player;
        let f9 = resolve_player(&PlayerPreset::FalseNine.instructions());
        let tm = resolve_player(&PlayerPreset::TargetMan.instructions());
        assert!(f9.come_short > 0.0 && tm.come_short < 0.0);
        let ifb = resolve_player(&PlayerPreset::InvertedFullBack.instructions());
        let ofb = resolve_player(&PlayerPreset::OverlappingFullBack.instructions());
        assert!(ifb.invert > 0.0 && ifb.overlap < 0.0 && ofb.overlap > 0.0 && ofb.invert < 0.0);
        assert!(resolve_player(&PlayerPreset::StayBack.instructions()).forward_runs < 0.0);
    }
}
