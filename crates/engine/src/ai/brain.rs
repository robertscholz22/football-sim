//! Player brains (docs/player-brain.md section 4), updated every `AiConfig::team_tick_s` and immediately at pass
//! release (PRS-02, ATT-22).
//!
//! Each player keeps his beliefs about his team's situation and evaluates them from his previous beliefs and what he
//! perceives (the world cache): the phase of play (PH-01, BLK-01), the block geometry (DEF-05, DEF-06, BLK-02, BLK-04,
//! BLK-05), every team-mate's duty out of possession (DEF-01..04, IND-06), pressing (PRS-01, PRS-02, PRS-07),
//! transitions (TRN-01..03, TR-04) and the in-possession structure (TR-05, ATT-09, ATT-18). A player's duty
//! assignment is what the concept prescribes as he computes it (he estimates team-mates as far as he knows them),
//! corrected by the team-mates' claims he has read (`read_team_mates`, docs/player-brain.md section 7).

use glam::Vec2;

use super::analysis::value_at;
use super::positioning::{cover_point, in_shooting_range, opponent_pass_in_flight, shot_block_point};
use super::reading;
use super::shape::slot_point;
use super::world::{Times, World};
use super::reach::time_to_reach;
use crate::ball::{BALL_RADIUS, BallMode, Trajectory};
use crate::config::{SimConfig, TacticsConfig};
use crate::match_state::{MatchState, PLAYERS_PER_TEAM, PlayPhase, RestartKind, side_of, team_range};
use crate::pitch::{
    GOAL_AREA_DEPTH, HALF_LENGTH, HALF_WIDTH, LENGTH, PENALTY_AREA_DEPTH, PENALTY_AREA_HALF_WIDTH, Side, TeamFrame, Third, third_of,
};
use crate::player::{Body, KickKind, Player, PlayerId, norm};
use crate::rules::referee;
use crate::team::{
    BlockHeight, DefendingTactics, Duty, Formation, Lane, ResolvedTactics, TeamInstructions, Unit, adherence, applied_defending, resolve,
    resolve_player,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InPhase {
    BuildUp,
    Progression,
    FinalThird,
    CounterAttack,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutPhase {
    CounterPress,
    Recovery,
    Block,
}

/// PH-01 / BLK-01 phase of play.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TeamPhase {
    In(InPhase),
    Out(OutPhase),
}

/// DEF-01..03 duties out of possession.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DefDuty {
    /// Pressure / delay the ball (DEF-02, IND-01..03, PRS-03, PRS-07).
    First,
    /// Cover behind and inside the first defender (DEF-02).
    Second,
    /// Balance on the weak side (DEF-02).
    Third,
    /// Mark an opponent in the own zone (DEF-04, IND-04).
    Mark(PlayerId),
    /// Track a runner out of the own zone towards goal until a team-mate takes him over (DEF-04, IND-04).
    Track(PlayerId),
    /// Double a carrier standing closed to play from the side he faces and go for the ball (PRS-01).
    Double,
    /// Recovery run goal-side (IND-06).
    Recover,
    /// Hold the block (DEF-05, DEF-06, PRS-04).
    Shape,
    Keeper,
}

/// Duties in possession.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttDuty {
    Carrier,
    Receiver,
    Support,
    /// TR-05 rest defence.
    RestDefence,
    Keeper,
}

/// A rectangular block in the team frame: back line x, front line x, centre y and half width.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Block {
    pub line_x: f32,
    pub front_x: f32,
    pub centre_y: f32,
    pub half_width: f32,
}

/// A player's reading of his team's situation: the phase, the block, every team-mate's duty, pressing, the loose-ball
/// claim. Perception is exact in v1, so all players of a side agree on the situation; their beliefs about each other's
/// duties can differ (section 7).
#[derive(Clone, Debug, PartialEq)]
pub struct Beliefs {
    pub in_possession: bool,
    pub phase: TeamPhase,
    pub phase_since: f32,
    pub counter_start: f32,
    pub counter_until: f32,
    pub counter_progress_x: f32,
    pub counter_press_until: f32,
    pub recovery_until: f32,
    /// TRN-01 criteria met at the last ball loss.
    pub trn01_score: u32,
    /// Ball reference (team frame): BLK-04 landing point of long balls, ATT-22 reception point of own passes.
    pub ball_ref: Vec2,
    /// Out-of-possession block with manager instructions (layer 2) and the pure layer-1 block.
    pub def_block: Block,
    pub def_block_l1: Block,
    /// In-possession shape with and without instructions.
    pub att_block: Block,
    pub att_block_l1: Block,
    /// Every team-mate's duty as he believes it, by formation slot (section 8.3); his own is his claim.
    pub def_duties: [DefDuty; PLAYERS_PER_TEAM],
    pub att_duties: [AttDuty; PLAYERS_PER_TEAM],
    /// The duties the concept prescribes as he computes them (section 7: what he expects of a team-mate he has not
    /// read).
    pub concept_def: [DefDuty; PLAYERS_PER_TEAM],
    pub concept_att: [AttDuty; PLAYERS_PER_TEAM],
    /// Since when his own claim (his duty in the phase) has been what it is.
    pub claim_since: f32,
    /// Calls (M6, section 7, set by the simulation after the update): he voiced his claim, his line call (as the line
    /// leader; when it last changed) or "mine" (as the chaser). Team-mates hear a call at their next update instead of
    /// reading it after their delay.
    pub claim_called: bool,
    pub line_called: bool,
    pub line_called_at: f32,
    pub chase_called: bool,
    /// Slot of the first defender.
    pub first: Option<usize>,
    /// DEF-03: the opponent carrier the first defender engaged.
    pub first_on: Option<PlayerId>,
    pub press: bool,
    pub press_rules: Vec<&'static str>,
    pub trigger: Option<&'static str>,
    pub trigger_until: f32,
    /// BLK-02: is the opposing ball carrier under pressure?
    pub pressured: bool,
    /// BLK-02: slot of the player leading the last line, his call (true: step up, the carrier is under pressure;
    /// false: drop) and since when the reading has differed from the call.
    pub line_leader: Option<usize>,
    pub line_step: bool,
    pub line_reading_since: f32,
    /// Opponent offside line (team frame x).
    pub offside_line: f32,
    /// Opponent units (team frame x, ascending).
    pub opp_units: Vec<f32>,
    /// ATT-17: the opponents' deepest outfield player in the goal corridor (team frame x, `analysis::last_defender`).
    pub opp_last: f32,
    /// ATT-18: runs in behind are on.
    pub runs_on: bool,
    /// Slot of the player chasing a loose ball / intercepting, the point (world) and when the ball is there.
    pub chaser: Option<usize>,
    pub chase_point: Vec2,
    pub chase_t: f32,
    /// IND-07: the chaser is the first defender attacking an opponent's high ball.
    pub challenge: bool,
    /// GK-01 (keeper only): a ball going into his goal, the point of its path he goes to (world) and when the ball is
    /// there (`gk_path`).
    pub gk_path: Option<(Vec2, f32)>,
    /// Time of the last block update (DEF-05 shift speed); negative = never updated.
    pub shape_t: f32,
}

impl Default for Beliefs {
    fn default() -> Self {
        Self {
            in_possession: false,
            phase: TeamPhase::Out(OutPhase::Block),
            phase_since: 0.0,
            counter_start: 0.0,
            counter_until: 0.0,
            counter_progress_x: 0.0,
            counter_press_until: 0.0,
            recovery_until: 0.0,
            trn01_score: 0,
            ball_ref: Vec2::ZERO,
            def_block: Block::default(),
            def_block_l1: Block::default(),
            att_block: Block::default(),
            att_block_l1: Block::default(),
            def_duties: [DefDuty::Shape; PLAYERS_PER_TEAM],
            att_duties: [AttDuty::Support; PLAYERS_PER_TEAM],
            concept_def: [DefDuty::Shape; PLAYERS_PER_TEAM],
            concept_att: [AttDuty::Support; PLAYERS_PER_TEAM],
            claim_since: 0.0,
            claim_called: false,
            line_called: false,
            line_called_at: 0.0,
            chase_called: false,
            first: None,
            first_on: None,
            press: false,
            press_rules: Vec::new(),
            trigger: None,
            trigger_until: 0.0,
            pressured: false,
            line_leader: None,
            line_step: false,
            line_reading_since: 0.0,
            offside_line: 0.0,
            opp_units: Vec::new(),
            opp_last: 0.0,
            runs_on: false,
            chaser: None,
            challenge: false,
            chase_point: Vec2::ZERO,
            chase_t: 0.0,
            gk_path: None,
            shape_t: -1.0,
        }
    }
}

/// DEF-05: the block moves as a unit at a bounded speed, never faster than its players can follow. Its lines may drop
/// towards the own goal by up to `max_drop` (BOX-04: as fast as the ball comes near goal).
impl Block {
    /// Largest change of any line, centre or half-width (m) from `self` to `other`.
    fn max_diff(&self, other: &Block) -> f32 {
        (self.line_x - other.line_x)
            .abs()
            .max((self.front_x - other.front_x).abs())
            .max((self.centre_y - other.centre_y).abs())
            .max((self.half_width - other.half_width).abs())
    }
}

fn follow(prev: Block, target: Block, max_step: f32, max_drop: f32) -> Block {
    let step = |a: f32, b: f32| a + (b - a).clamp(-max_step, max_step);
    let drop = |a: f32, b: f32| a + (b - a).clamp(-max_drop.max(max_step), max_step);
    Block {
        line_x: drop(prev.line_x, target.line_x),
        front_x: drop(prev.front_x, target.front_x),
        centre_y: step(prev.centre_y, target.centre_y),
        half_width: step(prev.half_width, target.half_width),
    }
}

/// BOX-04 weight of the ball's position: 0 beyond `box_far` from the own goal centre, 1 within `box01_range`.
pub fn box04_weight(ball: Vec2, d: &DefendingTactics, tc: &TacticsConfig) -> f32 {
    let dist = (ball - Vec2::new(-HALF_LENGTH, 0.0)).length();
    ((d.box_far - dist) / (d.box_far - tc.box01_range).max(1.0)).clamp(0.0, 1.0)
}

/// Out-of-possession shape of a formation in block units: the widest gap between neighbouring back-line slots
/// (half-width units) and the mean depth of the midfield slots (length units), by the slots' out-of-possession roles.
pub fn shape_units(formation: &Formation) -> (f32, f32) {
    let mut w: Vec<f32> = formation.slots.iter().filter(|s| s.role(false).is_back_line()).map(|s| s.out_pos.1).collect();
    w.sort_by(f32::total_cmp);
    let gap = w.windows(2).map(|p| p[1] - p[0]).fold(0.0, f32::max);
    let mid: Vec<f32> =
        formation.slots.iter().filter(|s| s.role(false).unit() == Unit::Midfield).map(|s| s.out_pos.0).collect();
    let depth = if mid.is_empty() { 0.5 } else { mid.iter().sum::<f32>() / mid.len() as f32 };
    (gap, depth)
}

/// BOX-04: towards the own goal the block narrows until neighbouring back-line players are at most `box_max_gap`
/// apart, shifts less to the ball side so the line stays in front of goal, and shortens until the midfield line is
/// `box_mid_distance` in front of the back line. `units` = `shape_units` of the formation.
pub fn box04_block(block: Block, ball: Vec2, units: (f32, f32), d: &DefendingTactics, tc: &TacticsConfig) -> Block {
    let t = box04_weight(ball, d, tc);
    if t <= 0.0 {
        return block;
    }
    let (back_gap, mid_depth) = units;
    let narrow = (d.box_max_gap / back_gap.max(0.1)).min(block.half_width);
    let half_width = block.half_width + t * (narrow - block.half_width);
    let shift = d.shift_factor + t * (d.box_shift_factor - d.shift_factor);
    let centre_y = (shift * ball.y).clamp(-(HALF_WIDTH - half_width), HALF_WIDTH - half_width);
    let length = block.front_x - block.line_x;
    let short = (d.box_mid_distance / mid_depth.max(0.1)).min(length);
    let front_x = block.line_x + length + t * (short - length);
    Block { centre_y, half_width, front_x, ..block }
}

/// BLK-05 press zone for a block height.
fn press_zone(h: BlockHeight, tc: &TacticsConfig) -> f32 {
    match h {
        BlockHeight::High => tc.blk05_press_zone_high,
        BlockHeight::Mid => tc.blk05_press_zone_mid,
        BlockHeight::Low => tc.blk05_press_zone_low,
    }
}

/// Out-of-possession block (DEF-05, DEF-06, BLK-02, BLK-05, TRN-03) of a team defending with `d`.
pub fn defensive_block(d: &DefendingTactics, ball: Vec2, pressured: bool, recovery: bool, tc: &TacticsConfig) -> Block {
    // BLK-02: hold the block line, but stay a pressure-dependent gap behind the ball. The space behind the line only
    // needs protecting where the keeper cannot cover it, so near goal the drop is a share of the ball's distance.
    let gap = if pressured { d.gap_pressured } else { d.gap_unpressured };
    let gap = gap.min(d.max_drop_share * (ball.x + HALF_LENGTH).max(0.0));
    let mut line_x = d.line.min(ball.x - gap).max(-HALF_LENGTH + GOAL_AREA_DEPTH);
    // CRS-01: against a ball in a crossing position the line drops to cover the penalty-spot area [FIFA-GK], whatever
    // the pressure on the crosser.
    if third_of(ball.x) == Third::Defensive && ball.y.abs() > PENALTY_AREA_HALF_WIDTH {
        line_x = line_x.min(-HALF_LENGTH + d.cross_line_depth);
    }
    // DEF-05 compactness; TRN-03 squeeze in recovery.
    let length = d.length * if recovery { tc.trn03_squeeze_factor } else { 1.0 };
    let half_width = d.width / 2.0;
    // DEF-06 ball-side shift, block kept on the pitch.
    let centre_y = (d.shift_factor * ball.y).clamp(-(HALF_WIDTH - half_width), HALF_WIDTH - half_width);
    Block { line_x, front_x: line_x + length, centre_y, half_width }
}

/// ATT-11: a per-third value (own, middle, final third) at team-frame x: linear between the ball positions `at` it
/// holds at, held beyond the outer ones.
pub fn by_third(v: [f32; 3], at: [f32; 3], x: f32) -> f32 {
    if x < at[1] {
        v[1] + (v[0] - v[1]) * ((x - at[1]) / (at[0] - at[1])).clamp(0.0, 1.0)
    } else {
        v[1] + (v[2] - v[1]) * ((x - at[1]) / (at[2] - at[1])).clamp(0.0, 1.0)
    }
}

/// In-possession shape (ATT-11 width and depth, TR-05 via the back line): the back line at the height and the team at
/// the width real teams keep with the ball where it is, the front on the opponents' offside line.
pub fn attacking_block(ball: Vec2, offside: f32, t: &ResolvedTactics, tc: &TacticsConfig) -> Block {
    let line_x = by_third(t.attacking.line, tc.att11_ball_x, ball.x) - HALF_LENGTH;
    let front_x = (offside - tc.att11_onside_margin).clamp(line_x + 25.0, HALF_LENGTH - 6.0);
    let half_width = 0.5 * by_third(tc.att11_width, tc.att11_ball_x, ball.x) * (0.55 + 0.45 * t.width);
    let room = (HALF_WIDTH - tc.att11_touchline_margin - half_width).max(0.0);
    Block { line_x, front_x, centre_y: (0.15 * ball.y).clamp(-room, room), half_width }
}

/// PRS-01: does player `c` stand closed to play, facing his own goal?
pub fn closed_to_play(state: &MatchState, c: PlayerId) -> bool {
    let p = &state.players[c];
    state.frame(p.side).to_team(p.facing).x < -0.5
}

/// PRS-01 pressing triggers, from the defending team's perspective.
fn detect_trigger(state: &MatchState, side: Side, cfg: &SimConfig, now: f32) -> Option<&'static str> {
    let tc = &cfg.tactics;
    if let Some(f) = state.flight
        && side_of(f.kicker) != side
        && f.kind != KickKind::Shot
    {
        if f.backward {
            return Some("back pass");
        }
        if f.speed < tc.prs01_slow_pass_speed {
            return Some("slow pass");
        }
        // The receiver of a high ball has to bring it down: "not having the ball under control", "press immediately
        // while the attacker waits to receive" [FA-EFL].
        if f.kind == KickKind::LoftedPass {
            return Some("high ball");
        }
    }
    if let Some(c) = state.control.filter(|&c| side_of(c) != side) {
        let cp = &state.players[c];
        if cp.is_gk() {
            return Some("goalkeeper in possession");
        }
        if now - cp.control_since < 1.0 && state.last_touch_quality < tc.prs01_poor_touch_quality {
            return Some("poor touch");
        }
        if closed_to_play(state, c) {
            return Some("receiver facing own goal");
        }
    }
    if state.ball.pos.y.abs() > tc.prs01_touchline_zone {
        return Some("ball near touchline");
    }
    None
}

/// Player `id`'s beliefs after a brain update, from his previous beliefs and what he perceives (`world`).
pub fn update(state: &MatchState, world: &mut World, id: PlayerId, cfg: &SimConfig, now: f32) -> Beliefs {
    let tc = &cfg.tactics;
    let side = state.players[id].side;
    let team = state.team(side);
    let prev = &state.players[id].beliefs;
    let mut b = prev.clone();
    let fr = team.frame;
    let squad = state.squad(side);
    let opps = state.squad(side.opponent());
    let in_poss = state.possession == side;
    let ball = state.ball_in(side);

    // --- Transitions (TR-04, TRN-01) ---
    if in_poss != prev.in_possession {
        b.in_possession = in_poss;
        if in_poss {
            // TR-04: counter if enough players are ahead of the ball compared with the opponents behind it.
            let own_ahead = squad.iter().filter(|p| !p.is_gk() && fr.to_team(p.pos).x > ball.x).count() as i32;
            let opp_between = opps.iter().filter(|p| fr.to_team(p.pos).x > ball.x).count() as i32;
            let deep = third_of(ball.x) == Third::Defensive
                && super::analysis::pressure_at(state, side, state.ball.ground_pos()) > 0.6;
            b.counter_press_until = 0.0;
            b.recovery_until = 0.0;
            if own_ahead >= opp_between - tc.tr04_numbers_margin && !deep {
                b.counter_start = now;
                b.counter_until = now + tc.ph01_attacking_transition_s;
                b.counter_progress_x = ball.x;
            } else {
                b.counter_until = 0.0;
            }
        } else {
            // TRN-01: score the counter-press criteria at the moment of loss.
            let near = |p: &crate::player::Player| (fr.to_team(p.pos) - ball).length() < tc.trn01_near_radius;
            let near_own: Vec<Vec2> = squad.iter().filter(|p| !p.is_gk() && near(p)).map(|p| fr.to_team(p.pos)).collect();
            let near_opp = opps.iter().filter(|p| near(p)).count();
            let spread = |f: fn(Vec2) -> f32| {
                let (lo, hi) = near_own.iter().fold((f32::MAX, f32::MIN), |(lo, hi), &v| (lo.min(f(v)), hi.max(f(v))));
                hi - lo
            };
            let criteria = [
                third_of(ball.x) == Third::Attacking,
                near_own.len() >= near_opp,
                near_own.len() >= 2 && spread(|v| v.x) >= 6.0 && spread(|v| v.y) >= 6.0,
                near_own.iter().any(|&v| (v - ball).length() < 2.0 * tc.blk02_pressure_radius),
                ball.y.abs() > tc.att12_inside_half_width,
                opps.iter().filter(|p| !p.is_gk()).map(|p| fr.to_team(p.pos).x).sum::<f32>() / 10.0 > 10.0,
            ];
            b.trn01_score = criteria.iter().filter(|&&c| c).count() as u32;
            b.counter_until = 0.0;
            if b.trn01_score >= tc.trn01_min_criteria {
                b.counter_press_until = now + tc.trn02_window_s;
                b.recovery_until = 0.0;
            } else {
                b.counter_press_until = 0.0;
                b.recovery_until = now + tc.ph01_attacking_transition_s;
            }
        }
    }

    // --- Phase (PH-01, BLK-01) ---
    let goal_side = squad.iter().filter(|p| !p.is_gk() && fr.to_team(p.pos).x < ball.x).count();
    let phase = if in_poss {
        if now < b.counter_until {
            if ball.x > b.counter_progress_x + 3.0 {
                b.counter_progress_x = ball.x;
                b.counter_until = (now + 3.0).max(b.counter_until).min(b.counter_start + 2.0 * tc.ph01_attacking_transition_s);
            }
            TeamPhase::In(InPhase::CounterAttack)
        } else {
            match third_of(ball.x) {
                Third::Defensive => TeamPhase::In(InPhase::BuildUp),
                Third::Middle => TeamPhase::In(InPhase::Progression),
                Third::Attacking => TeamPhase::In(InPhase::FinalThird),
            }
        }
    } else if now < b.counter_press_until {
        TeamPhase::Out(OutPhase::CounterPress)
    } else if now < b.recovery_until && goal_side < 8 {
        TeamPhase::Out(OutPhase::Recovery)
    } else {
        TeamPhase::Out(OutPhase::Block)
    };
    if phase != prev.phase {
        b.phase_since = now;
    }
    b.phase = phase;

    // --- Ball reference (BLK-04, ATT-22) ---
    b.ball_ref = ball;
    // Time until an opponent's long ball arrives, while it is in flight (BLK-04).
    let mut long_ball_in = None;
    if let Some(f) = state.flight
        && f.kind != KickKind::Shot
    {
        let remaining = f.arrival_t - (now - f.start_t);
        // Own pass: play to where it will be (ATT-22). Opponent's long ball: drop to its landing point (BLK-04).
        let long_ball = matches!(f.kind, KickKind::LoftedPass | KickKind::Clearance) && f.arrival_t >= tc.blk04_min_flight_s && remaining > 0.0;
        if side_of(f.kicker) == side || long_ball {
            b.ball_ref = fr.to_team(f.target);
        }
        if long_ball && side_of(f.kicker) != side {
            long_ball_in = Some(remaining);
        }
    }

    b.offside_line = world.offside_line(side);
    b.opp_units.clear();
    b.opp_units.extend_from_slice(world.opp_units(side));
    b.opp_last = world.opp_last(side);

    // --- BLK-02: is the opposing carrier pressured? ---
    b.pressured = world.pressured(side);
    let line_step = if !in_poss {
        lead_the_line(state, side, cfg, &mut b, prev, now);
        // The others follow the leader's call as they read it (section 7), at once when he voiced it.
        if let Some(l) = b.line_leader.filter(|&l| l != state.players[id].slot) {
            let m = &squad[l];
            let read = m.beliefs.line_called || now - m.beliefs.line_called_at >= reading::delay(state, &cfg.ai, id, m.id);
            b.line_step = if m.beliefs.line_step != prev.line_step && read { m.beliefs.line_step } else { prev.line_step };
        }
        b.line_step
    } else {
        b.line_leader = None;
        b.line_step = b.pressured;
        b.line_reading_since = now;
        b.pressured
    };

    // --- Blocks (layer 2 and layer 1) ---
    let l2 = team.tactics;
    let l1 = resolve(&TeamInstructions::default(), tc);
    let recovery = b.phase == TeamPhase::Out(OutPhase::Recovery);
    let kickoff = matches!(state.phase, PlayPhase::Restart { kind: RestartKind::Kickoff, .. });
    let dt = (now - prev.shape_t).max(0.0);
    let max_step = if prev.shape_t < 0.0 || kickoff { f32::INFINITY } else { tc.def05_shift_speed * dt };
    // BOX-04: near goal the line drops as fast as the ball comes towards it ("the block moves at the speed of the
    // ball", DEF-06).
    let ball_in = (-fr.to_team(state.ball.vel.truncate()).x).max(0.0);
    let units = shape_units(&team.formation);
    let def = |prev: Block, d: &DefendingTactics| {
        let max_drop = if box04_weight(ball, d, tc) > 0.0 { ball_in * dt } else { 0.0 };
        let target = box04_block(defensive_block(d, b.ball_ref, line_step, recovery, tc), b.ball_ref, units, d, tc);
        // BLK-04: during a long ball's flight the whole block moves to its new place by the time the ball arrives
        // ("the block moves at the speed of the ball" [FIFA-MB]), above the DEF-05 shift-rate cap.
        let max_step = long_ball_in.map_or(max_step, |r| max_step.max(prev.max_diff(&target) * dt / r.max(dt)));
        follow(prev, target, max_step, max_drop)
    };
    b.def_block = def(prev.def_block, &l2.defending);
    b.def_block_l1 = def(prev.def_block_l1, &l1.defending);
    b.att_block = follow(prev.att_block, attacking_block(b.ball_ref, b.offside_line, &l2, tc), max_step, 0.0);
    b.att_block_l1 = follow(prev.att_block_l1, attacking_block(b.ball_ref, b.offside_line, &l1, tc), max_step, 0.0);
    b.shape_t = now;

    // --- Roles ---
    assign_duties(state, world, id, cfg, &mut b, prev, now, &Fixed::default());
    b.concept_def = b.def_duties;
    b.concept_att = b.att_duties;

    // --- Loose balls and interceptions ---
    b.chaser = None;
    b.challenge = false;
    if state.is_live() && state.control.is_none() && state.ball.mode != BallMode::Held {
        let own_pass_to_receiver = state.flight.is_some_and(|f| side_of(f.kicker) == side && f.receiver.is_some());
        let opp_pass_to_receiver = state.flight.is_some_and(|f| side_of(f.kicker) != side && f.receiver.is_some());
        if !own_pass_to_receiver {
            let mut best: Option<(f32, usize, Vec2)> = None;
            let hits = world.intercepts(side);
            // He knows his own time to the ball; his team-mates' he estimates as far as he knows them (section 7).
            let est = reading::estimated(state, id, std::array::from_fn(|i| hits[i].map_or(f32::MAX, |h| h.0)));
            let receiver = state
                .flight
                .and_then(|f| f.receiver.filter(|_| side_of(f.kicker) != side))
                .and_then(|r| world.reception(r));
            for (i, p) in squad.iter().enumerate() {
                let Some((_, pt)) = hits[i] else {
                    continue;
                };
                let t = est[i];
                let tp = fr.to_team(pt);
                if p.is_gk() && !(tp.x < -HALF_LENGTH + PENALTY_AREA_DEPTH && tp.y.abs() < PENALTY_AREA_HALF_WIDTH) {
                    continue;
                }
                // GK-04: the keeper comes for an opponent's pass only once it is played and when he is there clearly
                // first, the more clearly the less he commands his area [FIFA-GK].
                if p.is_gk() && receiver.is_some_and(|(rt, _)| t > rt - tc.gk04_claim_margin_s * (1.0 - norm(p.attrs.aerial_command))) {
                    continue;
                }
                if best.is_none_or(|(bt, _, _)| t < bt) {
                    best = Some((t, i, pt));
                }
            }
            if let Some((t, i, pt)) = best {
                // PRS-02: against an opponent's pass, only go for it if we arrive no later than slack after the receiver.
                // One who is first to the ball plays it; one who is later goes to the reception point to arrive on the
                // first touch, not to where the ball would run on past the receiver.
                match receiver {
                    None => {
                        b.chaser = Some(i);
                        b.chase_point = pt;
                        b.chase_t = now + t;
                    }
                    Some((rt, rp)) if t <= rt + tc.prs02_arrival_slack_s => {
                        b.chaser = Some(i);
                        (b.chase_point, b.chase_t) = if t <= rt { (pt, now + t) } else { (rp, now + rt) };
                    }
                    _ => {}
                }
            } else if let Some(end) = world.traj().end().filter(|_| world.traj().exit_time.is_none() && !opp_pass_to_receiver) {
                // Ball still rolling beyond the prediction horizon: nearest player goes to where it will be. (Not for a
                // ball going out of play, nor for an opponent's pass that its receiver takes first: the first
                // defender's PRS-02 approach handles that one.)
                let pt = end.pos.truncate();
                let times = reading::estimated(state, id, world.time_to(side, pt));
                b.chaser = (0..PLAYERS_PER_TEAM)
                    .filter(|&i| !squad[i].is_gk())
                    .min_by(|&a, &c| times[a].total_cmp(&times[c]).then(a.cmp(&c)));
                b.chase_point = pt;
                b.chase_t = now + end.t;
            }
        }
        // IND-07: against a high ball the player who can be there soonest duels for it, "just before your opponent"
        // if he can; later than the receiver he still goes onto him at his reception point, straight and without
        // stopping at the contain distance, while the receiver brings the dropping ball down.
        if b.chaser.is_none()
            && let Some(f) = state.flight.filter(|f| side_of(f.kicker) != side && f.kind == KickKind::LoftedPass)
            && let Some(r) = f.receiver
            && let Some((rt, rp)) = world.reception(r)
        {
            let times = reading::estimated(state, id, outfield(world.time_to_contest(side, rp), squad));
            if let Some(i) = (0..PLAYERS_PER_TEAM).min_by(|&a, &c| times[a].total_cmp(&times[c]).then(a.cmp(&c))) {
                let d = &squad[i];
                if times[i] <= rt + applied_defending(&team.tactics, &d.attrs, d.instruction_familiarity(false), tc).challenge_slack {
                    b.chaser = Some(i);
                    b.chase_point = rp;
                    b.chase_t = now + rt;
                    b.challenge = true;
                }
            }
        }
        // "Mine!": a team-mate who called the ball has it; he goes for it unless he called it himself (section 7).
        let me = state.players[id].slot;
        if let Some(j) = (0..PLAYERS_PER_TEAM).find(|&j| j != me && squad[j].beliefs.chase_called && squad[j].beliefs.chaser == Some(j))
            && !(prev.chase_called && prev.chaser == Some(me))
        {
            let m = &squad[j].beliefs;
            (b.chaser, b.chase_point, b.chase_t, b.challenge) = (Some(j), m.chase_point, m.chase_t, m.challenge);
        }
    }
    // GK-01: a ball going into his goal, at any speed and whoever played it last, the keeper goes to its path. Not a
    // claim on it: team-mates still go for it.
    b.gk_path = None;
    if state.players[id].is_gk()
        && state.is_live()
        && state.control.is_none()
        && state.ball.mode != BallMode::Held
        && world.traj().on_goal
        && referee::defending_side(state, world.traj().goal_x_sign) == side
        && !state.flight.is_some_and(|f| f.receiver == Some(id))
    {
        b.gk_path = gk_path(state, world.traj(), id, cfg).map(|(pt, t)| (pt, now + t));
    }
    b
}

/// GK-01: where keeper `id` goes to meet a ball going into his goal, and in how long it is there: the earliest point
/// of its path in his penalty area that he reaches before it (within his hands' reach, or his feet's for a
/// team-mate's pass he may not handle), else the one he misses by the least time (he dives for it,
/// `Simulation::goalkeepers`).
fn gk_path(state: &MatchState, traj: &Trajectory, id: PlayerId, cfg: &SimConfig) -> Option<(Vec2, f32)> {
    let p = &state.players[id];
    let fr = state.frame(p.side);
    let pc = &cfg.player;
    let own_pass = state.flight.is_some_and(|f| side_of(f.kicker) == p.side && f.kind != KickKind::Shot);
    let reach = if own_pass { pc.control_radius } else { pc.gk_standing_reach };
    let mut best: Option<(f32, Vec2, f32)> = None;
    for s in &traj.samples {
        if traj.exit_time.is_some_and(|e| s.t > e) {
            break;
        }
        let pt = s.pos.truncate();
        let tp = fr.to_team(pt);
        if tp.x > -HALF_LENGTH + PENALTY_AREA_DEPTH || tp.y.abs() > PENALTY_AREA_HALF_WIDTH || s.pos.z - BALL_RADIUS > pc.gk_max_height {
            continue;
        }
        let late = time_to_reach(p, pt, reach, true) - s.t;
        if late <= 0.0 {
            return Some((pt, s.t));
        }
        if best.is_none_or(|(l, _, _)| late < l) {
            best = Some((late, pt, s.t));
        }
    }
    best.map(|(_, pt, t)| (pt, t))
}

/// Team-mates' claims a player has read and holds fixed when he resolves the duties (section 7), by slot.
#[derive(Clone, Copy, Debug, Default)]
struct Fixed {
    def: [Option<DefDuty>; PLAYERS_PER_TEAM],
    att: [Option<AttDuty>; PLAYERS_PER_TEAM],
}

/// The duties of player `id`'s team as he resolves them: the concept's assignment (DEF-01..04, IND-06 out of
/// possession; TR-05 in possession) from his estimates, with the claims in `fixed` held.
#[allow(clippy::too_many_arguments)]
fn assign_duties(state: &MatchState, world: &mut World, id: PlayerId, cfg: &SimConfig, b: &mut Beliefs, prev: &Beliefs, now: f32, fixed: &Fixed) {
    let side = state.players[id].side;
    let squad = state.squad(side);
    b.def_duties = [DefDuty::Shape; PLAYERS_PER_TEAM];
    b.att_duties = [AttDuty::Support; PLAYERS_PER_TEAM];
    for (i, p) in squad.iter().enumerate() {
        if p.is_gk() {
            b.def_duties[i] = DefDuty::Keeper;
            b.att_duties[i] = AttDuty::Keeper;
        }
    }
    b.press_rules.clear();
    if b.in_possession {
        b.first = None;
        b.first_on = None;
        b.press = false;
        assign_attack_duties(state, id, cfg, b, now, &fixed.att);
    } else {
        b.first = prev.first;
        b.first_on = prev.first_on;
        let opp_carrier = state.control.filter(|&c| side_of(c) != side);
        assign_defence_duties(state, world, id, cfg, b, prev, opp_carrier, now, &fixed.def);
    }
}

/// Calls (M6): is the claim of the player in `slot` one he voices when he takes it on: an exclusive defending duty,
/// or staying back as rest defence?
pub fn voiced_claim(b: &Beliefs, slot: usize) -> bool {
    if b.in_possession { b.att_duties[slot] == AttDuty::RestDefence } else { exclusive(b.def_duties[slot]) }
}

/// Is `d` a duty only one player can hold (the first defender, the doubler, a mark, a track)?
fn exclusive(d: DefDuty) -> bool {
    matches!(d, DefDuty::First | DefDuty::Double | DefDuty::Mark(_) | DefDuty::Track(_))
}

/// Reading team-mates (docs/player-brain.md section 7), after every player's own update. Player i reads team-mate
/// j's claim (j's duty as j holds it) once j has held it for the reading delay d_ij (`reading::delay`). A claim that
/// differs from what i's concept prescribes for j, now and at i's previous update (a claim that only lags a change of
/// the concept is no deviation), corrects i's belief, and i resolves his own duty again with those claims held: a
/// team-mate seen doing something else is no candidate for the duty i expected of him, one seen holding a duty holds
/// it. Of two players claiming the same first-defender, mark or track duty, the one who claimed it first keeps it.
/// Finally every player's claim time. Among perfect readers who know each other all players compute the same
/// assignment and nothing deviates.
pub fn read_team_mates(state: &MatchState, world: &mut World, cfg: &SimConfig, beliefs: &mut [Beliefs], now: f32) {
    for (i, (mine, p)) in beliefs.iter_mut().zip(&state.players).enumerate() {
        let prev = &p.beliefs;
        let in_poss = mine.in_possession;
        if p.is_gk() || prev.in_possession != in_poss {
            continue;
        }
        let (mut fixed, mut read) = (Fixed::default(), Fixed::default());
        for (j, m) in state.squad(p.side).iter().enumerate() {
            let unread = !m.beliefs.claim_called && now - m.beliefs.claim_since < reading::delay(state, &cfg.ai, i, m.id);
            if m.id == i || m.is_gk() || m.beliefs.in_possession != in_poss || unread {
                continue;
            }
            if in_poss {
                let v = m.beliefs.att_duties[j];
                let expected = mine.concept_att[j];
                if matches!(v, AttDuty::Support | AttDuty::RestDefence)
                    && matches!(expected, AttDuty::Support | AttDuty::RestDefence)
                    && v != expected
                    && v != prev.concept_att[j]
                {
                    (fixed.att[j], read.att[j]) = (Some(v), Some(v));
                }
            } else {
                let v = m.beliefs.def_duties[j];
                if v != mine.concept_def[j] && v != prev.concept_def[j] {
                    read.def[j] = Some(v);
                    // He keeps a duty he claimed before the team-mate did.
                    let own = prev.def_duties[p.slot];
                    let earlier = (prev.claim_since, p.slot) < (m.beliefs.claim_since, j);
                    if !(exclusive(v) && own == v && earlier) {
                        fixed.def[j] = Some(v);
                    }
                }
            }
        }
        if read.def.iter().any(Option::is_some) || read.att.iter().any(Option::is_some) {
            let mut b = mine.clone();
            assign_duties(state, world, i, cfg, &mut b, prev, now, &fixed);
            for j in 0..PLAYERS_PER_TEAM {
                if let Some(v) = read.def[j] {
                    b.def_duties[j] = v;
                }
                if let Some(v) = read.att[j] {
                    b.att_duties[j] = v;
                }
            }
            *mine = b;
        }
    }
    for (b, p) in beliefs.iter_mut().zip(&state.players) {
        let prev = &p.beliefs;
        let same = prev.in_possession == b.in_possession
            && if b.in_possession { prev.att_duties[p.slot] == b.att_duties[p.slot] } else { prev.def_duties[p.slot] == b.def_duties[p.slot] };
        b.claim_since = if same { prev.claim_since } else { now };
    }
}

/// The team's situation as observers see it (`MatchState::team_view`): the first player's beliefs, with every duty,
/// the first defender and the loose-ball chaser as their players claim them.
pub fn view(beliefs: &[Beliefs], side: Side) -> Beliefs {
    let mine = &beliefs[team_range(side)];
    let mut v = mine[0].clone();
    for (slot, b) in mine.iter().enumerate() {
        v.def_duties[slot] = b.def_duties[slot];
        v.att_duties[slot] = b.att_duties[slot];
    }
    v.first = (0..PLAYERS_PER_TEAM).find(|&s| mine[s].first == Some(s));
    v.first_on = v.first.and_then(|s| mine[s].first_on);
    v.chaser = (0..PLAYERS_PER_TEAM).find(|&s| mine[s].chaser == Some(s));
    match v.chaser {
        Some(s) => (v.chase_point, v.chase_t, v.challenge) = (mine[s].chase_point, mine[s].chase_t, mine[s].challenge),
        None => v.challenge = false,
    }
    v
}

/// BLK-02: one player leads the last line: of the roles with the line-leading duty, the one nearest the middle of the
/// block (he keeps it until another is 3 m nearer). He calls step up while the carrier is under pressure and drop while
/// he has time: "If the player in possession is closed down, the defence may push up or hold … If … not closed down,
/// … the defence must therefore be ready to retreat" [FIFA-MB]. A back pass he calls at once ("the whole team moves
/// up" [FFA-433]), any other change once the new reading has held `blk02_call_hold_s`. The others follow his call as
/// they read him (exactly until phase F).
fn lead_the_line(state: &MatchState, side: Side, cfg: &SimConfig, b: &mut Beliefs, prev: &Beliefs, now: f32) {
    let fr = state.frame(side);
    let squad = state.squad(side);
    let off = |i: usize| (fr.to_team(squad[i].pos).y - prev.def_block.centre_y).abs();
    let duty = if squad.iter().any(|p| p.out_role.has_duty(Duty::LeadLine)) { Duty::LeadLine } else { Duty::LastLine };
    let pool: Vec<usize> = (0..PLAYERS_PER_TEAM).filter(|&i| squad[i].out_role.has_duty(duty)).collect();
    let best = pool.iter().copied().min_by(|&a, &c| off(a).total_cmp(&off(c)).then(a.cmp(&c)));
    b.line_leader = match (prev.line_leader, best) {
        (Some(l), Some(n)) if pool.contains(&l) && off(l) <= off(n) + 3.0 => Some(l),
        (_, n) => n,
    };
    let back_pass = state.flight.is_some_and(|f| side_of(f.kicker) != side && f.backward && f.kind != KickKind::Shot);
    let reading = b.pressured || back_pass;
    if reading == prev.line_step {
        b.line_reading_since = now;
    } else if back_pass || now - prev.line_reading_since >= cfg.tactics.blk02_call_hold_s {
        b.line_step = reading;
        b.line_reading_since = now;
    }
}

/// `times` with the keeper's left out (`f32::MAX`): he leaves his area only for GK-02.
fn outfield(mut times: Times, squad: &[Player]) -> Times {
    for (t, p) in times.iter_mut().zip(squad) {
        if p.is_gk() {
            *t = f32::MAX;
        }
    }
    times
}

/// DEF-03: is the defender in `slot` engaged with the carrier `carrier` (the same one as `engaged_on`): between him
/// and the goal (ahead of him on his line to the goal centre, at most `def03_engaged_half_width` to the side), within
/// `ind01_track_distance`, and not recovering from a lost tackle?
fn engaged(
    state: &MatchState,
    side: Side,
    slot: usize,
    carrier: Option<PlayerId>,
    engaged_on: Option<PlayerId>,
    tc: &TacticsConfig,
) -> bool {
    let Some(c) = carrier.filter(|&c| engaged_on == Some(c)) else { return false };
    let fr = state.frame(side);
    let d = &state.squad(side)[slot];
    let (dp, cp) = (fr.to_team(d.pos), fr.to_team(state.players[c].pos));
    let line = (TeamFrame::own_goal() - cp).normalize_or(Vec2::NEG_X);
    let rel = dp - cp;
    !matches!(d.body, Body::Recovering { .. })
        && rel.length() < tc.ind01_track_distance
        && rel.dot(line) > 0.0
        && rel.perp_dot(line).abs() < tc.def03_engaged_half_width
}

/// Out-of-possession duties as player `me` resolves them, with the read claims in `fixed` held.
#[allow(clippy::too_many_arguments)]
fn assign_defence_duties(
    state: &MatchState,
    world: &mut World,
    me: PlayerId,
    cfg: &SimConfig,
    b: &mut Beliefs,
    prev: &Beliefs,
    opp_carrier: Option<PlayerId>,
    now: f32,
    fixed: &[Option<DefDuty>; PLAYERS_PER_TEAM],
) {
    let tc = &cfg.tactics;
    let side = state.players[me].side;
    let fr = state.frame(side);
    let squad = state.squad(side);
    let team = state.team(side);
    // Reference point of the first defender: the carrier, else the reception point of the opponent's pass (PRS-02).
    let ref_w = match (opp_carrier, state.flight) {
        (Some(c), _) => state.players[c].pos,
        (None, Some(f)) if side_of(f.kicker) != side && f.kind != KickKind::Shot => f.target,
        _ => state.ball.ground_pos(),
    };
    // DEF-02/IND-01: pressure is applied from goal-side, so "closest" is measured to the contain point goal-side of
    // the ball; in the counter-press (TRN-02) the nearest player presses directly, from any side.
    let engage_w = if b.phase == TeamPhase::Out(OutPhase::CounterPress) {
        ref_w
    } else {
        let r = fr.to_team(ref_w);
        fr.to_world(r + (TeamFrame::own_goal() - r).normalize_or(Vec2::NEG_X) * tc.ind01_contain_distance)
    };
    // DEF-03: the contain point moves with a running carrier, so "time to it" is the time to meet it there: a
    // defender in his path takes over from one chasing from behind.
    let carrier_vel = opp_carrier.map_or(Vec2::ZERO, |c| state.players[c].vel);
    // Section 7: team-mates' times as he estimates them; one seen holding another duty is no candidate.
    let mut times = reading::estimated(state, me, outfield(world.time_to_meet(side, engage_w, carrier_vel), squad));
    for (t, f) in times.iter_mut().zip(fixed) {
        if f.is_some() {
            *t = f32::MAX;
        }
    }

    // DEF-01/DEF-03: first defender by time-to-ball with hysteresis. An engaged first defender keeps the role until
    // the ball is played past him (he is no longer goal-side, or lost a tackle) or goes to another player. A team-mate
    // seen pressing is the first defender.
    let mut first = (0..PLAYERS_PER_TEAM).min_by(|&a, &c| times[a].total_cmp(&times[c]).then(a.cmp(&c))).unwrap_or(1);
    if let Some(prev) = b.first
        && prev != first
        && fixed[prev].is_none()
        && (times[prev] <= times[first] + tc.def03_role_hysteresis_s || engaged(state, side, prev, opp_carrier, b.first_on, tc))
    {
        first = prev;
    }
    if let Some(j) = fixed.iter().position(|f| *f == Some(DefDuty::First)) {
        first = j;
    }
    b.first = Some(first);
    b.first_on = opp_carrier.filter(|_| engaged(state, side, first, opp_carrier, opp_carrier, tc));
    b.def_duties[first] = DefDuty::First;

    // DEF-02: second defender covers behind and inside the first; BOX-02: in shooting range, on the shooting line.
    let first_t = fr.to_team(squad[first].pos);
    let ref_t = fr.to_team(ref_w);
    let cover = if opp_carrier.is_some() && in_shooting_range(ref_t, tc) {
        fr.to_world(shot_block_point(ref_t, &team.tactics.defending))
    } else {
        fr.to_world(cover_point(first_t, ref_t, tc))
    };
    let cover_times = reading::estimated(state, me, world.time_to_meet(side, cover, carrier_vel));
    // DEF-03: the second defender with the first's hysteresis: he keeps the role while no team-mate is clearly sooner.
    let second = fixed.iter().position(|f| *f == Some(DefDuty::Second)).or_else(|| {
        let eligible = |i: usize| i != first && !squad[i].is_gk() && fixed[i].is_none();
        let best = (0..PLAYERS_PER_TEAM)
            .filter(|&i| eligible(i))
            .min_by(|&a, &c| cover_times[a].total_cmp(&cover_times[c]).then(a.cmp(&c)))?;
        let kept = prev.def_duties.iter().position(|d| *d == DefDuty::Second).filter(|&i| {
            i != best && eligible(i) && cover_times[i] <= cover_times[best] + tc.def03_role_hysteresis_s
        });
        Some(kept.unwrap_or(best))
    });
    if let Some(s) = second {
        b.def_duties[s] = DefDuty::Second;
    }

    // DEF-02: "When the ball is wide", the furthest defender of the last line gives balance on the weak side.
    if b.ball_ref.y.abs() > tc.att12_inside_half_width {
        let third = (0..PLAYERS_PER_TEAM)
            .filter(|&i| b.def_duties[i] == DefDuty::Shape && squad[i].out_role.has_duty(Duty::LastLine) && fixed[i].is_none())
            .max_by(|&a, &c| {
                let da = (fr.to_team(squad[a].pos).y - b.ball_ref.y).abs();
                let dc = (fr.to_team(squad[c].pos).y - b.ball_ref.y).abs();
                da.total_cmp(&dc).then(c.cmp(&a))
            });
        if let Some(t) = third {
            b.def_duties[t] = DefDuty::Third;
        }
    }

    // PRS-01: a carrier standing closed to play is doubled [BDFL-PETER]: while the first defender, engaged goal-side,
    // keeps him from turning, the team-mate on the side he faces who reaches him soonest closes in from there and goes
    // for the ball.
    let double_time = team.tactics.defending.double_time;
    if let Some(c) = opp_carrier.filter(|&c| {
        double_time > 0.0 && !state.players[c].is_gk() && b.first_on == Some(c) && closed_to_play(state, c)
    }) {
        let cp = &state.players[c];
        let front = cp.pos + cp.facing * tc.prs01_double_distance;
        let front_times = reading::estimated(state, me, world.time_to_meet(side, front, carrier_vel));
        let double = fixed.iter().position(|f| *f == Some(DefDuty::Double)).or_else(|| {
            (0..PLAYERS_PER_TEAM)
                .filter(|&i| b.def_duties[i] == DefDuty::Shape && !squad[i].is_gk() && fixed[i].is_none())
                .filter(|&i| (squad[i].pos - cp.pos).dot(cp.facing) > 0.0 && front_times[i] <= double_time)
                .min_by(|&a, &c| front_times[a].total_cmp(&front_times[c]).then(a.cmp(&c)))
        });
        if let Some(d) = double {
            b.def_duties[d] = DefDuty::Double;
        }
    }

    let recovery = b.phase == TeamPhase::Out(OutPhase::Recovery);
    // IND-06: ahead of the ball judged against where an opponent's pass in flight will be received.
    let recovery_ref = opponent_pass_in_flight(state, side).map_or(b.ball_ref, |(to, _)| fr.to_team(to));
    let mut marked: Vec<PlayerId> = opp_carrier.into_iter().collect();
    let ball_ref = b.ball_ref;
    // DEF-04: is opponent `o` in the zone of a defender anchored at `anchor` (zone widened by `margin`)?
    let in_zone = |o: PlayerId, anchor: Vec2, margin: f32| {
        let q = fr.to_team(state.players[o].pos);
        (q - anchor).length() < tc.def04_zone_radius + margin && (q - ball_ref).length() < 30.0 + margin
    };
    // DEF-04 / IND-04: an opponent running towards the own goal out of his marker's zone is handed over to a team-mate
    // goal-side of him; with nobody there his marker tracks him ([FA-EFL], [BDFL-PETER09]). A runner coming short is let
    // go.
    let running = |o: PlayerId, speed: f32| -fr.to_team(state.players[o].vel).x >= speed;
    let taken_over = |i: usize, o: PlayerId| {
        let q = fr.to_team(state.players[o].pos);
        squad.iter().enumerate().any(|(j, m)| {
            let r = fr.to_team(m.pos);
            j != i && !m.is_gk() && r.x < q.x && (r - q).length() < tc.def04_handover_distance
        })
    };
    // BLK-03: a back-line defender follows a man who has dropped between the lines only as far as his `step_out`,
    // only while no other line defender is out, and only if a line team-mate near his slot can shift across [FIFA-MB];
    // otherwise he passes him on to the midfield.
    let line_x = b.def_block.line_x;
    let ahead = |o: PlayerId| fr.to_team(state.players[o].pos).x - line_x;
    let slots: Vec<Vec2> = (0..PLAYERS_PER_TEAM).map(|i| slot_point(&team.formation.slots[i], false, &b.def_block)).collect();
    let back = |i: usize| squad[i].out_role.is_back_line();
    let out = |j: usize, d: &[DefDuty; PLAYERS_PER_TEAM]| back(j) && matches!(d[j], DefDuty::Mark(o) if ahead(o) > tc.blk03_front_m);
    // One already out keeps following within his reach; whether to go is decided as his man drops.
    let may_follow = |i: usize, o: PlayerId, d: &[DefDuty; PLAYERS_PER_TEAM]| {
        if !back(i) || ahead(o) <= tc.blk03_front_m {
            return true;
        }
        let p = &squad[i];
        let reach = applied_defending(&team.tactics, &p.attrs, p.instruction_familiarity(false), tc).step_out;
        let is_out = fr.to_team(p.pos).x - line_x > tc.blk03_front_m;
        ahead(o) <= reach
            && (is_out
                || (!(0..PLAYERS_PER_TEAM).any(|j| j != i && out(j, d))
                    && (0..PLAYERS_PER_TEAM).any(|j| j != i && back(j) && (slots[j].y - slots[i].y).abs() < tc.blk03_shift_m)))
    };
    // Read claims hold: their duties, and the men they mark or track are taken.
    for (i, f) in fixed.iter().enumerate() {
        if let Some(d) = *f {
            b.def_duties[i] = d;
            if let DefDuty::Mark(o) | DefDuty::Track(o) = d {
                marked.push(o);
            }
        }
    }
    let mut free = Vec::new();
    for (i, p) in squad.iter().enumerate() {
        if b.def_duties[i] != DefDuty::Shape || fixed[i].is_some() {
            continue;
        }
        let anchor = slot_point(&team.formation.slots[i], false, &b.def_block);
        let pos = fr.to_team(p.pos);
        let track_speed = applied_defending(&team.tactics, &p.attrs, p.instruction_familiarity(false), tc).track_speed;
        let runner = match prev.def_duties[i] {
            DefDuty::Mark(o) if !in_zone(o, anchor, tc.def04_release_margin) && running(o, track_speed) => Some(o),
            DefDuty::Track(o) if running(o, 0.5 * track_speed) => Some(o),
            _ => None,
        };
        if let Some(o) = runner.filter(|&o| !marked.contains(&o) && !taken_over(i, o)) {
            marked.push(o);
            b.def_duties[i] = DefDuty::Track(o);
            continue;
        }
        // IND-06: ahead of the ball although the shape wants him behind it → recovery run.
        if pos.x > recovery_ref.x + 1.0 && anchor.x < recovery_ref.x {
            b.def_duties[i] = DefDuty::Recover;
            continue;
        }
        if recovery {
            continue;
        }
        // DEF-04: a marker keeps his man until he is clearly out of the zone (handover), so marks do not flicker
        // at the zone's edge.
        match prev.def_duties[i] {
            DefDuty::Mark(o) if !marked.contains(&o) && in_zone(o, anchor, tc.def04_release_margin) && may_follow(i, o, &b.def_duties) => {
                marked.push(o);
                b.def_duties[i] = DefDuty::Mark(o);
            }
            _ => free.push((i, anchor)),
        }
    }
    // DEF-04: an opponent in the zone near the ball becomes the marking reference.
    // Claims: the most dangerous opponents first (denial value, docs/player-brain.md section 6: what his team could
    // make of him, the possession value where he stands as far as the lane from the ball to him is open), each to
    // the nearest free defender whose zone holds him.
    let opps = state.squad(side.opponent());
    let ofr = state.frame(side.opponent());
    let from = opp_carrier.map_or(state.ball.ground_pos(), |c| state.players[c].pos);
    let mut pairs = Vec::new();
    for o in opps.iter().filter(|o| !o.is_gk() && !marked.contains(&o.id)) {
        let zones: Vec<usize> = free.iter().filter(|&&(_, a)| in_zone(o.id, a, 0.0)).map(|&(i, _)| i).collect();
        if zones.is_empty() {
            continue;
        }
        let threat = value_at(ofr.to_team(o.pos), world.opp_last(side.opponent()), &cfg.ai) * world.lane_openness(side.opponent(), from, o.pos);
        pairs.extend(zones.into_iter().map(|i| (threat, (o.pos - squad[i].pos).length(), i, o.id)));
    }
    pairs.sort_by(|a, c| c.0.total_cmp(&a.0).then(a.1.total_cmp(&c.1)).then(a.2.cmp(&c.2)).then(a.3.cmp(&c.3)));
    for (_, _, i, o) in pairs {
        if b.def_duties[i] == DefDuty::Shape && !marked.contains(&o) && may_follow(i, o, &b.def_duties) {
            marked.push(o);
            b.def_duties[i] = DefDuty::Mark(o);
        }
    }

    // Pressing: BLK-05 zone, PRS-01 triggers, TRN-01/02 counter-press, TRN-03/PRS-07 delay.
    if let Some(t) = detect_trigger(state, side, cfg, now) {
        b.trigger = Some(t);
        b.trigger_until = now + tc.prs01_trigger_duration_s;
    }
    let trigger_active = now < b.trigger_until;
    let zone = press_zone(team.tactics.block_height, tc) + (team.tactics.pressing_intensity - 0.5) * 40.0;
    let in_zone = b.ball_ref.x <= zone;
    let trigger_reach = b.ball_ref.x <= zone + tc.prs01_zone_extension;
    match b.phase {
        TeamPhase::Out(OutPhase::CounterPress) => {
            b.press = true;
            b.press_rules.extend(["TRN-01", "TRN-02"]);
            if let Some(s) = second {
                b.def_duties[s] = DefDuty::First;
            }
        }
        TeamPhase::Out(OutPhase::Recovery) => {
            b.press = false;
            b.press_rules.extend(["TRN-03", "PRS-07"]);
        }
        _ => {
            b.press = in_zone || (trigger_active && trigger_reach);
            if in_zone {
                b.press_rules.push("BLK-05");
            }
            if trigger_active && trigger_reach {
                b.press_rules.push("PRS-01");
            }
            if !b.press {
                b.press_rules.push("PRS-07");
            }
        }
    }
}

/// In-possession duties as player `me` resolves them, with the read claims in `fixed` held.
fn assign_attack_duties(
    state: &MatchState,
    me: PlayerId,
    cfg: &SimConfig,
    b: &mut Beliefs,
    now: f32,
    fixed: &[Option<AttDuty>; PLAYERS_PER_TEAM],
) {
    let tc = &cfg.tactics;
    let side = state.players[me].side;
    let fr = state.frame(side);
    let squad = state.squad(side);
    let team = state.team(side);
    let base = side.index() * PLAYERS_PER_TEAM;
    let carrier = state.control.filter(|&c| side_of(c) == side);
    if let Some(c) = carrier {
        b.att_duties[c - base] = AttDuty::Carrier;
    }
    let receiver = state.flight.filter(|f| side_of(f.kicker) == side).and_then(|f| f.receiver);
    if let Some(r) = receiver {
        b.att_duties[r - base] = AttDuty::Receiver;
    }

    // TR-05: rest defence, 5 if the opponents leave five or more players high. The roles with the rest-defence duty
    // claim it first, the deepest by the formation first; others only if they are too few. A wide player on the
    // ball's side claims later (`tr05_ball_side_release`) and the player instructions move the
    // claim (`tr05_instruction_shift`); a team-mate's instructions he knows as far as the team has practised the
    // concept (section 7). Read claims hold: rest defenders seen count first, supporters seen do not claim.
    let high = state
        .squad(side.opponent())
        .iter()
        .filter(|o| !o.is_gk() && fr.to_team(o.pos).x < (b.ball_ref.x - 10.0).min(0.0))
        .count();
    let n_rest = if high >= 5 { tc.tr05_max_players } else { tc.tr05_min_players };
    let mut order: Vec<usize> =
        (0..PLAYERS_PER_TEAM).filter(|&i| b.att_duties[i] == AttDuty::Support && !squad[i].is_gk() && fixed[i].is_none()).collect();
    order.sort_by(|&a, &c| {
        let claim = |i: usize| {
            let (p, slot) = (&squad[i], &team.formation.slots[i]);
            let mut depth = slot.in_pos.0;
            let ball_side =
                p.role.lane() == Lane::Wide && slot.in_pos.1 * b.ball_ref.y > 0.0 && b.ball_ref.y.abs() >= tc.att12_central_half_width;
            let r = resolve_player(&p.instructions);
            let known = if p.id == me { 1.0 } else { team.practice.concept };
            let go = (r.forward_runs + r.overlap).clamp(-1.0, 1.0) * adherence(&p.attrs, p.instruction_familiarity(true), false) * known;
            depth += tc.tr05_ball_side_release * f32::from(u8::from(ball_side)) + tc.tr05_instruction_shift * go;
            (!p.role.has_duty(Duty::RestDefence), depth)
        };
        let (ca, cc) = (claim(a), claim(c));
        ca.0.cmp(&cc.0).then(ca.1.total_cmp(&cc.1)).then(a.cmp(&c))
    });
    let held = fixed.iter().filter(|f| **f == Some(AttDuty::RestDefence)).count();
    for &i in order.iter().take(n_rest.saturating_sub(held)) {
        b.att_duties[i] = AttDuty::RestDefence;
    }
    for (i, f) in fixed.iter().enumerate() {
        if let Some(d) = *f {
            b.att_duties[i] = d;
        }
    }

    // ATT-18: runs in behind when the carrier faces forward with time on the ball.
    b.runs_on = carrier.is_some_and(|c| {
        let cp = &state.players[c];
        fr.to_team(cp.facing).x > 0.5 && now - cp.control_since >= tc.att18_min_time_on_ball_s
    });

}

/// Fraction of the pitch length the ball has travelled in the team's attacking direction (stats helper).
pub fn progress(ball_team_x: f32) -> f32 {
    (ball_team_x + HALF_LENGTH) / LENGTH
}
