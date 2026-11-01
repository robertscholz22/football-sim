//! Resolution of layer 1 (optimal parameters from the principle catalog) with layer 2 (manager overrides).

use super::instructions::{
    AttackingInstructions, BlockHeight, Channel, DefendingInstructions, PlayerInstructions, SetPieceInstructions, TeamInstructions,
};
use crate::config::TacticsConfig;
use crate::pitch::{GOAL_AREA_DEPTH, HALF_LENGTH, WIDTH};
use crate::player::{Attributes, norm};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedTactics {
    pub block_height: BlockHeight,
    pub pressing_intensity: f32,
    pub width: f32,
    pub directness: f32,
    pub tempo: f32,
    pub channel: Channel,
    pub defending: DefendingTactics,
    pub attacking: AttackingTactics,
    pub set_pieces: SetPieceTactics,
}

/// Set-piece organisation of a team. Layer 1 takes every value from the catalog parameter named on the field;
/// `SetPieceInstructions` override them per team.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SetPieceTactics {
    /// SP-01: `sp01_zonal`, `sp01_edge`, `sp01_counter`.
    pub zonal: u8,
    pub edge: bool,
    pub counter: u8,
    /// SP-02: `sp02_attackers`, `sp02_short`, `sp02_short_share`, `sp02_delivery`.
    pub attackers: u8,
    pub short: bool,
    pub short_share: f32,
    pub delivery: [f32; 3],
    /// SP-03: `sp03_wall_max`, `sp03_line_m`.
    pub wall_max: u8,
    pub line_m: f32,
}

fn resolve_set_pieces(s: &SetPieceInstructions, tc: &TacticsConfig) -> SetPieceTactics {
    SetPieceTactics {
        zonal: s.zonal.unwrap_or(tc.sp01_zonal).min(10),
        edge: s.edge.unwrap_or(tc.sp01_edge),
        counter: s.counter.unwrap_or(tc.sp01_counter).min(3),
        attackers: s.attackers.unwrap_or(tc.sp02_attackers).clamp(1, 8),
        short: s.short.unwrap_or(tc.sp02_short),
        short_share: s.short_share.unwrap_or(tc.sp02_short_share).clamp(0.0, 1.0),
        delivery: s.delivery.unwrap_or(tc.sp02_delivery).map(|w| w.max(0.0)),
        wall_max: s.wall_max.unwrap_or(tc.sp03_wall_max).clamp(1, 7),
        line_m: s.line_m.unwrap_or(tc.sp03_line_m).clamp(GOAL_AREA_DEPTH, 30.0),
    }
}

/// Receiving and keeping the ball. Layer 1 takes every value from the catalog parameter named on the field
/// (`TacticsConfig`); `AttackingInstructions` override them per team.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AttackingTactics {
    /// ATT-24: `att24_meet_m`.
    pub meet_m: f32,
    /// ATT-25: `att25_pressed_loss`.
    pub pressed_loss: f32,
    /// ATT-25: `att25_receiver_loss`.
    pub receiver_loss: f32,
    /// ATT-25: `att25_receiver_base_loss` (no instruction: not a style).
    pub receiver_base_loss: f32,
    /// ATT-11: `att11_line`.
    pub line: [f32; 3],
    /// ATT-21: `att21_switch_bonus`.
    pub switch_bonus: f32,
}

impl AttackingTactics {
    /// Every value moved from `self` towards `other` by `t` (0 = `self`, 1 = `other`).
    pub fn lerp(&self, other: &Self, t: f32) -> Self {
        let l = |a: f32, b: f32| a + t * (b - a);
        Self {
            meet_m: l(self.meet_m, other.meet_m),
            pressed_loss: l(self.pressed_loss, other.pressed_loss),
            receiver_loss: l(self.receiver_loss, other.receiver_loss),
            receiver_base_loss: l(self.receiver_base_loss, other.receiver_base_loss),
            line: std::array::from_fn(|i| l(self.line[i], other.line[i])),
            switch_bonus: l(self.switch_bonus, other.switch_bonus),
        }
    }
}

/// Catalog values, overridden by any in-possession instruction.
fn resolve_attacking(a: &AttackingInstructions, tc: &TacticsConfig) -> AttackingTactics {
    AttackingTactics {
        meet_m: a.meet_m.unwrap_or(tc.att24_meet_m).clamp(0.0, 10.0),
        pressed_loss: a.pressed_loss.unwrap_or(tc.att25_pressed_loss).clamp(0.0, 1.0),
        receiver_loss: a.receiver_loss.unwrap_or(tc.att25_receiver_loss).clamp(0.0, 1.0),
        receiver_base_loss: tc.att25_receiver_base_loss.clamp(0.0, 1.0),
        line: a.line.unwrap_or(tc.att11_line).map(|h| h.clamp(5.0, HALF_LENGTH + 20.0)),
        switch_bonus: a.switch_bonus.unwrap_or(tc.att21_switch_bonus).clamp(0.0, 2.0),
    }
}

/// Out-of-possession shape and box defending of a team. Layer 1 takes every value from the catalog parameter named
/// on the field (`TacticsConfig`); `DefendingInstructions` override them per team.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DefendingTactics {
    /// BLK-05: block line (team frame x, m), from the block height.
    pub line: f32,
    /// DEF-05: block length (m), from the block height (`def05_length_*`).
    pub length: f32,
    /// DEF-05: block width (m), `def05_width`.
    pub width: f32,
    /// DEF-06: ball-side shift, `def06_shift_factor`.
    pub shift_factor: f32,
    /// BLK-02: `blk02_gap_pressured`, `blk02_gap_unpressured`, `blk02_max_drop_share`.
    pub gap_pressured: f32,
    pub gap_unpressured: f32,
    pub max_drop_share: f32,
    /// BOX-04: `box04_far`, `box04_max_gap`, `box04_shift_factor`, `box04_mid_distance`, `box04_drop_urgency`.
    pub box_far: f32,
    /// CRS-01: `crs01_line_depth`.
    pub cross_line_depth: f32,
    /// PRS-05: `prs05_trap`.
    pub trap: f32,
    /// PRS-01: `prs01_double_time_s`.
    pub double_time: f32,
    pub box_max_gap: f32,
    pub box_shift_factor: f32,
    pub box_mid_distance: f32,
    pub box_drop_urgency: f32,
    /// BOX-01: `box01_contain_distance`.
    pub contain_distance: f32,
    /// BOX-02: `box02_block_distance`.
    pub block_distance: f32,
    /// BOX-03: `box03_cover_radius`, `box03_tackle_min_p`.
    pub cover_radius: f32,
    pub tackle_min_p: f32,
    /// IND-07: `ind07_challenge_slack_s`.
    pub challenge_slack: f32,
    /// DEF-04: `def04_track_speed`.
    pub track_speed: f32,
    /// BLK-03: `blk03_step_out_m`.
    pub step_out: f32,
    /// BLK-03: `blk03_shuffle`.
    pub shuffle: f32,
    /// TRN-04: `trn04_foul_p`.
    pub tactical_foul: f32,
}

impl DefendingTactics {
    /// Every value moved from `self` towards `other` by `t` (0 = `self`, 1 = `other`).
    pub fn lerp(&self, other: &Self, t: f32) -> Self {
        let l = |a: f32, b: f32| a + t * (b - a);
        Self {
            line: l(self.line, other.line),
            length: l(self.length, other.length),
            width: l(self.width, other.width),
            shift_factor: l(self.shift_factor, other.shift_factor),
            gap_pressured: l(self.gap_pressured, other.gap_pressured),
            gap_unpressured: l(self.gap_unpressured, other.gap_unpressured),
            max_drop_share: l(self.max_drop_share, other.max_drop_share),
            box_far: l(self.box_far, other.box_far),
            cross_line_depth: l(self.cross_line_depth, other.cross_line_depth),
            trap: l(self.trap, other.trap),
            double_time: l(self.double_time, other.double_time),
            box_max_gap: l(self.box_max_gap, other.box_max_gap),
            box_shift_factor: l(self.box_shift_factor, other.box_shift_factor),
            box_mid_distance: l(self.box_mid_distance, other.box_mid_distance),
            box_drop_urgency: l(self.box_drop_urgency, other.box_drop_urgency),
            contain_distance: l(self.contain_distance, other.contain_distance),
            block_distance: l(self.block_distance, other.block_distance),
            cover_radius: l(self.cover_radius, other.cover_radius),
            tackle_min_p: l(self.tackle_min_p, other.tackle_min_p),
            challenge_slack: l(self.challenge_slack, other.challenge_slack),
            track_speed: l(self.track_speed, other.track_speed),
            step_out: l(self.step_out, other.step_out),
            shuffle: l(self.shuffle, other.shuffle),
            tactical_foul: l(self.tactical_foul, other.tactical_foul),
        }
    }
}

/// Catalog values for a block height, overridden by any defending instruction.
fn resolve_defending(h: BlockHeight, d: &DefendingInstructions, tc: &TacticsConfig) -> DefendingTactics {
    let (line, length) = match h {
        BlockHeight::High => (tc.blk05_line_high, tc.def05_length_high),
        BlockHeight::Mid => (tc.blk05_line_mid, tc.def05_length_mid),
        BlockHeight::Low => (tc.blk05_line_low, tc.def05_length_low),
    };
    let or = |v: Option<f32>, default: f32, lo: f32, hi: f32| v.unwrap_or(default).clamp(lo, hi);
    DefendingTactics {
        line: or(d.line, line, -HALF_LENGTH, HALF_LENGTH),
        length: or(d.length, length, 5.0, 60.0),
        width: or(d.width, tc.def05_width, 10.0, WIDTH),
        shift_factor: or(d.shift_factor, tc.def06_shift_factor, 0.0, 1.0),
        gap_pressured: or(d.gap_pressured, tc.blk02_gap_pressured, 0.0, 40.0),
        gap_unpressured: or(d.gap_unpressured, tc.blk02_gap_unpressured, 0.0, 40.0),
        max_drop_share: or(d.max_drop_share, tc.blk02_max_drop_share, 0.0, 1.0),
        box_far: or(d.box_far, tc.box04_far, 0.0, 2.0 * HALF_LENGTH),
        cross_line_depth: or(d.cross_line_depth, tc.crs01_line_depth, GOAL_AREA_DEPTH, HALF_LENGTH),
        trap: or(d.trap, tc.prs05_trap, 0.0, 1.0),
        double_time: or(d.double_time, tc.prs01_double_time_s, 0.0, 10.0),
        box_max_gap: or(d.box_max_gap, tc.box04_max_gap, 1.0, 30.0),
        box_shift_factor: or(d.box_shift_factor, tc.box04_shift_factor, 0.0, 1.0),
        box_mid_distance: or(d.box_mid_distance, tc.box04_mid_distance, 0.0, 60.0),
        box_drop_urgency: or(d.box_drop_urgency, tc.box04_drop_urgency, 0.0, 1.0),
        contain_distance: or(d.contain_distance, tc.box01_contain_distance, 0.5, 10.0),
        block_distance: or(d.block_distance, tc.box02_block_distance, 0.5, 15.0),
        cover_radius: or(d.cover_radius, tc.box03_cover_radius, 0.0, 30.0),
        tackle_min_p: or(d.tackle_min_p, tc.box03_tackle_min_p, 0.0, 1.0),
        challenge_slack: or(d.challenge_slack, tc.ind07_challenge_slack_s, -1.0, 3.0),
        track_speed: or(d.track_speed, tc.def04_track_speed, 0.0, 12.0),
        step_out: or(d.step_out, tc.blk03_step_out_m, 0.0, 30.0),
        shuffle: or(d.shuffle, tc.blk03_shuffle, 0.0, 1.0),
        tactical_foul: or(d.tactical_foul, tc.trn04_foul_p, 0.0, 1.0),
    }
}

/// Layer-1 optimum, overridden by any instruction the manager set.
pub fn resolve(instr: &TeamInstructions, tc: &TacticsConfig) -> ResolvedTactics {
    let block_height = instr.block_height.unwrap_or(BlockHeight::Mid);
    ResolvedTactics {
        // BLK-05: no absolute right height in the sources; the mid-block is the reference block.
        block_height,
        // PRS-07: press on triggers / in the zone only.
        pressing_intensity: instr.pressing_intensity.unwrap_or(0.5).clamp(0.0, 1.0),
        // ATT-11: "use all the width of the field".
        width: instr.width.unwrap_or(1.0).clamp(0.0, 1.0),
        // BU-05: the route (short or long) follows whichever lanes are open, so no bias.
        directness: instr.directness.unwrap_or(0.0).clamp(-1.0, 1.0),
        tempo: instr.tempo.unwrap_or(0.0).clamp(-1.0, 1.0),
        // DEF-07: generally steer play to the flanks.
        channel: instr.channel.unwrap_or(Channel::Wide),
        defending: resolve_defending(block_height, &instr.defending, tc),
        attacking: resolve_attacking(&instr.attacking, tc),
        set_pieces: resolve_set_pieces(&instr.set_pieces, tc),
    }
}

/// Layer 3: the defending values a player applies, the layer-1 optimum moved towards his team's layer-2 values as far
/// as he adheres to instructions (as his anchor blends the layer-1 and layer-2 blocks).
pub fn applied_defending(team: &ResolvedTactics, a: &Attributes, familiarity: f32, tc: &TacticsConfig) -> DefendingTactics {
    let l1 = resolve(&TeamInstructions::default(), tc).defending;
    l1.lerp(&team.defending, adherence(a, familiarity, false))
}

/// Layer 3: the in-possession values a player applies, blended like `applied_defending`.
pub fn applied_attacking(team: &ResolvedTactics, a: &Attributes, familiarity: f32, tc: &TacticsConfig) -> AttackingTactics {
    let l1 = resolve(&TeamInstructions::default(), tc).attacking;
    l1.lerp(&team.attacking, adherence(a, familiarity, false))
}

/// How strongly a player applies an instruction (layer 3): discipline and familiarity with his role raise adherence.
/// Instructions that ask for more passing are additionally resisted by egotism.
pub fn adherence(a: &Attributes, familiarity: f32, pass_more: bool) -> f32 {
    let mut x = (0.4 + 0.6 * norm(a.discipline)) * (0.6 + 0.4 * familiarity);
    if pass_more {
        x *= 1.0 - 0.6 * norm(a.egotism);
    }
    x.clamp(0.0, 1.0)
}

/// Player-level instructions with defaults.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedPlayer {
    pub forward_runs: f32,
    pub dribble_freq: f32,
    pub come_short: f32,
    pub overlap: f32,
    pub invert: f32,
}

pub fn resolve_player(p: &PlayerInstructions) -> ResolvedPlayer {
    ResolvedPlayer {
        forward_runs: p.forward_runs.unwrap_or(0.0).clamp(-1.0, 1.0),
        dribble_freq: p.dribble_freq.unwrap_or(0.0).clamp(-1.0, 1.0),
        come_short: p.come_short.unwrap_or(0.0).clamp(-1.0, 1.0),
        overlap: p.overlap.unwrap_or(0.0).clamp(-1.0, 1.0),
        invert: p.invert.unwrap_or(0.0).clamp(-1.0, 1.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overrides_replace_optimum() {
        let tc = TacticsConfig::default();
        let base = resolve(&TeamInstructions::default(), &tc);
        assert_eq!(base.block_height, BlockHeight::Mid);
        let high = resolve(&TeamInstructions { block_height: Some(BlockHeight::High), ..Default::default() }, &tc);
        assert_eq!(high.block_height, BlockHeight::High);
    }

    #[test]
    fn defending_takes_catalog_values_unless_overridden() {
        let tc = TacticsConfig::default();
        let base = resolve(&TeamInstructions::default(), &tc).defending;
        assert_eq!((base.line, base.length, base.width), (tc.blk05_line_mid, tc.def05_length_mid, tc.def05_width));
        assert_eq!((base.box_max_gap, base.tackle_min_p), (tc.box04_max_gap, tc.box03_tackle_min_p));
        let low = resolve(&TeamInstructions { block_height: Some(BlockHeight::Low), ..Default::default() }, &tc).defending;
        assert_eq!((low.line, low.length), (tc.blk05_line_low, tc.def05_length_low));
        // An explicit line beats the block height; the other values stay.
        let instr = TeamInstructions {
            block_height: Some(BlockHeight::Low),
            defending: DefendingInstructions { line: Some(-28.0), width: Some(34.0), ..Default::default() },
            ..Default::default()
        };
        let d = resolve(&instr, &tc).defending;
        assert_eq!((d.line, d.length, d.width, d.box_max_gap), (-28.0, tc.def05_length_low, 34.0, tc.box04_max_gap));
    }

    #[test]
    fn players_apply_defending_instructions_as_far_as_they_adhere() {
        let tc = TacticsConfig::default();
        let instr = TeamInstructions {
            defending: DefendingInstructions { contain_distance: Some(3.0), ..Default::default() },
            ..Default::default()
        };
        let team = resolve(&instr, &tc);
        let obedient = Attributes { discipline: 100.0, ..Default::default() };
        let wilful = Attributes { discipline: 0.0, ..Default::default() };
        let full = applied_defending(&team, &obedient, 1.0, &tc).contain_distance;
        let part = applied_defending(&team, &wilful, 0.0, &tc).contain_distance;
        assert!((full - 3.0).abs() < 1e-5, "fully adherent: {full}");
        assert!(part > tc.box01_contain_distance && part < full, "partly adherent: {part}");
        // Without instructions everyone applies the catalog value exactly.
        let none = resolve(&TeamInstructions::default(), &tc);
        assert_eq!(applied_defending(&none, &wilful, 0.0, &tc), none.defending);
    }

    #[test]
    fn attacking_takes_catalog_values_unless_overridden() {
        let tc = TacticsConfig::default();
        let base = resolve(&TeamInstructions::default(), &tc).attacking;
        assert_eq!((base.meet_m, base.pressed_loss, base.line), (tc.att24_meet_m, tc.att25_pressed_loss, tc.att11_line));
        let instr = TeamInstructions {
            attacking: AttackingInstructions { pressed_loss: Some(0.05), line: Some([16.0, 30.0, 45.0]), ..Default::default() },
            ..Default::default()
        };
        let team = resolve(&instr, &tc);
        assert_eq!((team.attacking.meet_m, team.attacking.pressed_loss), (tc.att24_meet_m, 0.05));
        assert_eq!(team.attacking.line, [16.0, 30.0, 45.0]);
        let obedient = Attributes { discipline: 100.0, ..Default::default() };
        let wilful = Attributes { discipline: 0.0, ..Default::default() };
        assert!((applied_attacking(&team, &obedient, 1.0, &tc).pressed_loss - 0.05).abs() < 1e-5);
        let part = applied_attacking(&team, &wilful, 0.0, &tc).pressed_loss;
        assert!(part > 0.05 && part < tc.att25_pressed_loss, "partly adherent: {part}");
    }

    #[test]
    fn egotists_resist_pass_more_instructions() {
        let mut a = Attributes { egotism: 20.0, ..Default::default() };
        let ego = adherence(&a, 1.0, true);
        a.egotism = 0.0;
        let team = adherence(&a, 1.0, true);
        assert!(ego < team);
    }
}
