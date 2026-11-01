//! The referee: goals, kick-off, restarts when the ball goes out, and the Laws of M7 [IFAB]: offside (Law 11, LAW-01),
//! fouls and misconduct (Law 12, LAW-02) with advantage (Law 5.3), free kicks and penalty kicks (Laws 13–14).

use glam::Vec2;

use crate::ai::analysis::offside_line;
use crate::ai::shape::slot_point;
use crate::ai::brain::defensive_block;
use crate::config::{RulesConfig, SimConfig};
use crate::duel::TackleOutcome;
use crate::events::Card;
use crate::match_state::{MatchState, RestartKind, side_of};
use crate::pitch::{
    CENTRE_CIRCLE_RADIUS, GOAL_AREA_DEPTH, GOAL_AREA_HALF_WIDTH, HALF_LENGTH, HALF_WIDTH, PENALTY_AREA_DEPTH, PENALTY_AREA_HALF_WIDTH,
    PENALTY_SPOT_DISTANCE, Side, TeamFrame,
};
use crate::player::{Player, PlayerId, norm};
use crate::rng::SimRng;
use crate::team::DefendingTactics;

/// Law 11.1: in the opponents' half and nearer to their goal line than both the ball and the second-last opponent
/// (level is onside; the hands and arms do not count, bodies are points here).
pub fn offside_position(state: &MatchState, id: PlayerId) -> bool {
    let side = side_of(id);
    state.frame(side).to_team(state.players[id].pos).x > offside_line(state, side)
}

/// Is `at` (world) in the penalty area `side` defends?
pub fn in_penalty_area(state: &MatchState, side: Side, at: Vec2) -> bool {
    let t = state.frame(side).to_team(at);
    t.x < -HALF_LENGTH + PENALTY_AREA_DEPTH && t.y.abs() < PENALTY_AREA_HALF_WIDTH
}

/// The penalty mark in front of the goal `side` defends (world).
pub fn penalty_mark(state: &MatchState, side: Side) -> Vec2 {
    state.frame(side).to_world(Vec2::new(-HALF_LENGTH + PENALTY_SPOT_DISTANCE, 0.0))
}

/// LAW-02, Law 12.1: the probability that a challenge by `def` with this outcome is a foul (careless): mostly when he missed
/// the ball, more from behind, for poorer tacklers and for more aggressive players.
/// `tactical`: the TRN-04 probability if this is a chance for a tactical foul.
pub fn foul_chance(def: &Player, outcome: TackleOutcome, from_behind: bool, tactical: Option<f32>, in_box: bool, r: &RulesConfig) -> f32 {
    let base = match (outcome, tactical) {
        (TackleOutcome::Beaten, Some(p)) if !def.cautioned => p,
        _ => match outcome {
            TackleOutcome::Beaten => r.foul_beaten,
            TackleOutcome::Deflected => r.foul_deflected,
            TackleOutcome::Won => r.foul_won,
        },
    };
    let behind = if from_behind { r.foul_from_behind } else { 1.0 };
    let care = if def.cautioned { r.caution_care } else { 1.0 } * if in_box { r.foul_box } else { 1.0 };
    (base * behind * care * temperament(def, r) * (1.0 + r.foul_tackling * (1.0 - 2.0 * norm(def.attrs.tackling)))).clamp(0.0, 0.95)
}

/// 1 ± `foul_aggression` from the calmest to the most aggressive player.
pub fn temperament(p: &Player, r: &RulesConfig) -> f32 {
    1.0 + r.foul_aggression * (2.0 * norm(p.attrs.aggression) - 1.0)
}

/// Law 12.3: what a foul is beyond a careless one, most serious first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Misconduct {
    None,
    /// Stopping a promising attack.
    Spa,
    Reckless,
    /// Denying an obvious goal-scoring opportunity.
    Dogso,
    ExcessiveForce,
}

impl Misconduct {
    /// The card: a DOGSO in the penalty area (every engine foul is a challenge for the ball) or with advantage is a
    /// caution; an SPA then goes unpunished; a reckless foul is always cautioned.
    pub fn card(self, penalty: bool, advantage: bool) -> Option<Card> {
        match self {
            Self::None => None,
            Self::Spa => (!penalty && !advantage).then_some(Card::Yellow),
            Self::Reckless => Some(Card::Yellow),
            Self::Dogso => Some(if penalty || advantage { Card::Yellow } else { Card::Red }),
            Self::ExcessiveForce => Some(Card::Red),
        }
    }
}

/// The misconduct in a foul by `by` on `on`. DOGSO and SPA from the circumstances the Law lists: the distance to goal,
/// the direction of play (the fouled player heading for goal) and the defenders goal-side of him (outfield players
/// besides the offender); recklessness and excessive force by chance, more from behind and for aggressive players.
pub fn misconduct(state: &MatchState, r: &RulesConfig, by: PlayerId, on: PlayerId, from_behind: bool, rng: &mut SimRng) -> Misconduct {
    let (o, d) = (&state.players[on], &state.players[by]);
    let fr = state.frame(o.side);
    let pos = fr.to_team(o.pos);
    let dist = (TeamFrame::opp_goal() - pos).length();
    let heading = fr.to_team(o.vel).x > 1.0;
    let goal_side = state
        .squad(d.side)
        .iter()
        .filter(|q| q.id != by && !q.is_gk() && !q.sent_off && fr.to_team(q.pos).x > pos.x)
        .count();
    if rng.chance(r.red_excessive) {
        return Misconduct::ExcessiveForce;
    }
    if heading && dist < r.dogso_distance && goal_side == 0 {
        return Misconduct::Dogso;
    }
    let care = if d.cautioned { r.caution_care } else { 1.0 };
    let reckless = r.card_reckless * if from_behind { r.card_from_behind } else { 1.0 } * care * temperament(d, r);
    if rng.chance(reckless.clamp(0.0, 1.0)) {
        return Misconduct::Reckless;
    }
    if heading && dist < r.spa_distance && goal_side <= r.spa_defenders {
        return Misconduct::Spa;
    }
    Misconduct::None
}

/// Minimum distance (m) opponents keep from the ball at a restart.
pub fn keep_out_radius(kind: RestartKind) -> f32 {
    match kind {
        RestartKind::ThrowIn => 5.0,
        // Laws 13–14: 9.15 m, kept with a little margin.
        _ => CENTRE_CIRCLE_RADIUS + 0.5,
    }
}

/// The side whose own goal is at world x sign `x_sign`.
pub fn defending_side(state: &MatchState, x_sign: f32) -> Side {
    let own_goal_x = |s: Side| state.frame(s).to_world(TeamFrame::own_goal()).x;
    if own_goal_x(Side::Home).signum() == x_sign.signum() { Side::Home } else { Side::Away }
}

/// Restart after the ball left the pitch at `exit`.
pub fn restart_for_out(state: &MatchState, exit: Vec2, over_goal_line: bool) -> (RestartKind, Side, Vec2) {
    let last = state.last_touch.map(side_of).unwrap_or(state.possession);
    if over_goal_line {
        let defending = defending_side(state, exit.x);
        let fr = state.frame(defending);
        let e = fr.to_team(exit);
        if last == defending {
            // Corner for the attackers, at the corner on the exit side.
            let spot = fr.to_world(Vec2::new(-HALF_LENGTH + 0.3, e.y.signum() * (HALF_WIDTH - 0.3)));
            (RestartKind::Corner, defending.opponent(), spot)
        } else {
            let spot = fr.to_world(Vec2::new(-HALF_LENGTH + GOAL_AREA_DEPTH, e.y.signum() * (GOAL_AREA_HALF_WIDTH - 3.0)));
            (RestartKind::GoalKick, defending, spot)
        }
    } else {
        let x = exit.x.clamp(-HALF_LENGTH + 1.0, HALF_LENGTH - 1.0);
        let spot = Vec2::new(x, exit.y.signum() * (HALF_WIDTH - 0.3));
        (RestartKind::ThrowIn, last.opponent(), spot)
    }
}

/// Player of `side` who takes the restart: the goalkeeper for goal kicks and free kicks in his own box, the best
/// finisher for a penalty, the best crosser for a corner, the best crosser or long-range shooter for a free kick in
/// the attacking third; otherwise the nearest outfield player.
pub fn restart_taker(state: &MatchState, kind: RestartKind, side: Side, spot: Vec2) -> PlayerId {
    let squad = state.squad(side);
    let x = state.frame(side).to_team(spot).x;
    let best = |key: &dyn Fn(&Player) -> f32| {
        squad.iter().filter(|p| !p.is_gk() && !p.sent_off).max_by(|a, b| key(a).total_cmp(&key(b)).then(b.id.cmp(&a.id))).map(|p| p.id)
    };
    let specialist = match kind {
        RestartKind::Penalty => best(&|p| p.attrs.finishing + p.attrs.composure),
        RestartKind::Corner => best(&|p| p.attrs.crossing),
        RestartKind::FreeKick { .. } if x > HALF_LENGTH / 3.0 => best(&|p| p.attrs.crossing.max(p.attrs.long_shots)),
        RestartKind::FreeKick { .. } if in_penalty_area(state, side, spot) => squad.iter().find(|p| p.is_gk()).map(|p| p.id),
        _ => None,
    };
    if let Some(t) = specialist {
        return t;
    }
    if kind == RestartKind::GoalKick
        && let Some(gk) = squad.iter().find(|p| p.is_gk())
    {
        return gk.id;
    }
    if kind == RestartKind::Kickoff
        && let Some(st) = squad.iter().find(|p| p.role.is_striker())
    {
        return st.id;
    }
    squad
        .iter()
        .filter(|p| !p.is_gk() && !p.sent_off)
        .min_by(|a, b| (a.pos - spot).length().total_cmp(&(b.pos - spot).length()).then(a.id.cmp(&b.id)))
        .map(|p| p.id)
        .unwrap_or(squad[0].id)
}

/// Kick-off positions (world) for every player: formation out-of-possession shape in a mid block, in the own half,
/// opponents of the kicking side outside the centre circle; the taker at the ball.
pub fn kickoff_positions(state: &MatchState, cfg: &SimConfig, kicking: Side, taker: PlayerId) -> Vec<Vec2> {
    let tc = &cfg.tactics;
    // Each team's own defending shape, at the mid-block line and length whatever its instructed height.
    let blocks = [Side::Home, Side::Away].map(|s| {
        let d = DefendingTactics { line: tc.blk05_line_mid, length: tc.def05_length_mid, ..state.team(s).tactics.defending };
        defensive_block(&d, Vec2::ZERO, false, false, tc)
    });
    state
        .players
        .iter()
        .map(|p| {
            let fr = state.frame(p.side);
            let slot = &state.team(p.side).formation.slots[p.slot];
            let mut t = if p.is_gk() { Vec2::new(-HALF_LENGTH + 2.0, 0.0) } else { slot_point(slot, false, &blocks[p.side.index()]) };
            t.x = t.x.min(-1.0);
            if p.id == taker {
                t = Vec2::new(-0.4, 0.0);
            } else if p.side != kicking && t.length() < CENTRE_CIRCLE_RADIUS + 0.5 {
                t = t.normalize_or(Vec2::NEG_X) * (CENTRE_CIRCLE_RADIUS + 0.5);
                t.x = t.x.min(-1.0);
            }
            fr.to_world(t)
        })
        .collect()
}

/// Push a movement target out of the keep-out circle around a restart spot.
pub fn keep_out(target: Vec2, spot: Vec2, radius: f32) -> Vec2 {
    let d = target - spot;
    if d.length() >= radius {
        return target;
    }
    spot + d.normalize_or(Vec2::Y) * radius
}
