//! World cache (docs/player-brain.md section 4): physics and geometry of the true state, never decisions, which the
//! brains read through their beliefs. A cache lives for one brain update. Each query is evaluated when a brain first
//! asks it and reused after, so the same question from 22 brains costs one evaluation. Perception is exact in v1, so
//! every brain reads the cache unchanged.

use glam::Vec2;

use super::analysis::{lane_openness, offside_line, opponent_units};
use super::reach::{intercept, pass_reaction, time_to_meet, time_to_reach, time_to_reach_after};
use crate::ball::Trajectory;
use crate::config::SimConfig;
use crate::match_state::{MatchState, PLAYERS_PER_TEAM, side_of};
use crate::pitch::Side;
use crate::player::{Intent, KickKind, PlayerId};

/// A time (s) for each player of a side, by slot.
pub type Times = [f32; PLAYERS_PER_TEAM];
/// Each player's earliest reach of the ball's path, by slot: time from now (s) and point (world).
pub type Intercepts = [Option<(f32, Vec2)>; PLAYERS_PER_TEAM];

pub struct World<'a> {
    state: &'a MatchState,
    cfg: &'a SimConfig,
    traj: &'a Trajectory,
    now: f32,
    offside_line: [f32; 2],
    opp_units: [Vec<f32>; 2],
    pressured: [bool; 2],
    intercepts: [Option<Intercepts>; 2],
    receptions: Vec<(PlayerId, Option<(f32, Vec2)>)>,
    reach: Vec<(Side, Vec2, Times)>,
    meet: Vec<(Side, Vec2, Vec2, Times)>,
    contest: Vec<(Side, Vec2, Times)>,
    lanes: Vec<(Side, Vec2, Vec2, f32)>,
}

impl<'a> World<'a> {
    /// The cache for `state` at time `now`; `traj` is the current ball prediction.
    pub fn new(state: &'a MatchState, cfg: &'a SimConfig, traj: &'a Trajectory, now: f32) -> Self {
        let sides = [Side::Home, Side::Away];
        Self {
            state,
            cfg,
            traj,
            now,
            offside_line: sides.map(|s| offside_line(state, s)),
            opp_units: sides.map(|s| opponent_units(state, s)),
            pressured: sides.map(|s| carrier_pressured(state, s, cfg)),
            intercepts: [None, None],
            receptions: Vec::new(),
            reach: Vec::new(),
            meet: Vec::new(),
            contest: Vec::new(),
            lanes: Vec::new(),
        }
    }

    /// The predicted ball trajectory.
    pub fn traj(&self) -> &'a Trajectory {
        self.traj
    }

    /// The opponents' offside line (`side`'s team frame x).
    pub fn offside_line(&self, side: Side) -> f32 {
        self.offside_line[side.index()]
    }

    /// The opponents' units (`side`'s team frame x, ascending).
    pub fn opp_units(&self, side: Side) -> &[f32] {
        &self.opp_units[side.index()]
    }

    /// BLK-02: is a player of `side` within `blk02_pressure_radius` of the opposing carrier and no further forward?
    pub fn pressured(&self, side: Side) -> bool {
        self.pressured[side.index()]
    }

    /// Where and when each player of `side` first reaches the ball's path: an outfield player up to the height he
    /// reaches with a header, the keeper at control height; a pass he read before it was struck sooner (IND-08).
    pub fn intercepts(&mut self, side: Side) -> Intercepts {
        if let Some(v) = self.intercepts[side.index()] {
            return v;
        }
        let (state, cfg, traj, now) = (self.state, self.cfg, self.traj, self.now);
        let pass = state.flight.filter(|f| f.kind != KickKind::Shot);
        let squad = state.squad(side);
        let v = std::array::from_fn(|i| {
            let p = &squad[i];
            let reach_h = if p.is_gk() { cfg.player.control_height } else { p.header_height() };
            let reaction = pass.map_or(p.kin.reaction, |f| pass_reaction(p, now - f.start_t, &cfg.player));
            intercept(p, traj, reach_h, cfg.player.control_radius, reaction)
        });
        self.intercepts[side.index()] = Some(v);
        v
    }

    /// Where (world) and in how long (s) the receiver `r` of a pass in flight takes it: his planned reception
    /// (ATT-24), else the earliest point he reaches at control height.
    pub fn reception(&mut self, r: PlayerId) -> Option<(f32, Vec2)> {
        if let Some(&(_, v)) = self.receptions.iter().find(|(q, _)| *q == r) {
            return v;
        }
        let p = &self.state.players[r];
        let pc = &self.cfg.player;
        let v = match p.intent {
            Intent::Receive { point, t, .. } if t >= self.now => Some((t - self.now, point)),
            _ => intercept(p, self.traj, pc.control_height, pc.control_radius, 0.0),
        };
        self.receptions.push((r, v));
        v
    }

    /// Each player's time to reach `point`, reaction included.
    pub fn time_to(&mut self, side: Side, point: Vec2) -> Times {
        if let Some(&(_, _, v)) = self.reach.iter().find(|(s, q, _)| *s == side && *q == point) {
            return v;
        }
        let squad = self.state.squad(side);
        let v = std::array::from_fn(|i| time_to_reach(&squad[i], point, 0.0, true));
        self.reach.push((side, point, v));
        v
    }

    /// Each player's time to meet a point that moves at `vel`, reaction included.
    pub fn time_to_meet(&mut self, side: Side, point: Vec2, vel: Vec2) -> Times {
        if let Some(&(_, _, _, v)) = self.meet.iter().find(|(s, q, w, _)| *s == side && *q == point && *w == vel) {
            return v;
        }
        let squad = self.state.squad(side);
        let v = std::array::from_fn(|i| time_to_meet(&squad[i], point, vel, true));
        self.meet.push((side, point, vel, v));
        v
    }

    /// IND-07: each player's time to contest a high ball at `point` (within `aerial_contest_radius`), reacting to the
    /// pass in flight as he read it (IND-08).
    pub fn time_to_contest(&mut self, side: Side, point: Vec2) -> Times {
        if let Some(&(_, _, v)) = self.contest.iter().find(|(s, q, _)| *s == side && *q == point) {
            return v;
        }
        let cfg = self.cfg;
        let since = self.state.flight.map_or(0.0, |f| self.now - f.start_t);
        let squad = self.state.squad(side);
        let v = std::array::from_fn(|i| {
            let p = &squad[i];
            time_to_reach_after(p, point, cfg.duel.aerial_contest_radius, pass_reaction(p, since, &cfg.player))
        });
        self.contest.push((side, point, v));
        v
    }

    /// `analysis::lane_openness` of a pass by `side` from `from` to `to`.
    pub fn lane_openness(&mut self, side: Side, from: Vec2, to: Vec2) -> f32 {
        if let Some(&(_, _, _, v)) = self.lanes.iter().find(|(s, a, b, _)| *s == side && *a == from && *b == to) {
            return v;
        }
        let v = lane_openness(self.state, side, from, to, self.cfg);
        self.lanes.push((side, from, to, v));
        v
    }
}

fn carrier_pressured(state: &MatchState, side: Side, cfg: &SimConfig) -> bool {
    let fr = state.frame(side);
    state.control.filter(|&c| side_of(c) != side).is_some_and(|c| {
        let cp = fr.to_team(state.players[c].pos);
        state.squad(side).iter().any(|p| {
            let q = fr.to_team(p.pos);
            (q - cp).length() < cfg.tactics.blk02_pressure_radius && q.x < cp.x + 0.5
        })
    })
}
