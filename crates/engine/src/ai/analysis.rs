//! Analysis models shared by every player: possession value (xT-like), xG, pressure, pitch control,
//! pass success along predicted trajectories, opposition units and the offside line.
//! These are standard analytics models, not tactical principles; their coefficients live in `AiConfig`.

use glam::{Vec2, Vec3};

use super::execution::{ExecInput, natural_foot};
use super::reach::{pass_reaction, reception_point, time_to_reach, time_to_reach_after, time_to_reach_from};
use crate::ball::{BALL_MASS, BALL_RADIUS, Ball, BallParams, KickSolution, predict};
use crate::config::{AiConfig, SimConfig, TacticsConfig, ValueGrid};
use crate::duel::{aerial_win_prob, control_prob, logistic, pass_touch_prob};
use crate::match_state::{MatchState, side_of};
use crate::pitch::{GOAL_HALF_WIDTH, HALF_LENGTH, HALF_WIDTH, LENGTH, Side, TeamFrame, in_bounds};
use crate::player::locomotion::carry_on;
use crate::player::{Body, Foot, KickKind, Player, PlayerId, norm};

/// Angle (rad) subtended by the opponent goal mouth from a team-frame point.
pub fn goal_angle(p: Vec2) -> f32 {
    if p.x >= HALF_LENGTH {
        return 0.0;
    }
    let a = Vec2::new(HALF_LENGTH, GOAL_HALF_WIDTH) - p;
    let b = Vec2::new(HALF_LENGTH, -GOAL_HALF_WIDTH) - p;
    a.angle_to(b).abs()
}

/// Unblocked xG of a reference finisher's strong-foot shot from a team-frame position, keeper set: distance, goal
/// angle, and how far off the centre line (a wide shot is worth less than its angle alone suggests).
pub fn xg_basic(p: Vec2, ai: &AiConfig) -> f32 {
    let d = (TeamFrame::opp_goal() - p).length();
    if d > ai.max_shot_distance * 1.5 {
        return 0.0;
    }
    logistic(ai.xg_a + ai.xg_b * goal_angle(p) - ai.xg_c * d - ai.xg_lateral * p.y.abs())
}

/// Value of having the ball at a team-frame point ≈ probability that the possession ends in a goal: the learned field
/// if there is one (M8), otherwise the parametric model.
pub fn possession_value(p: Vec2, ai: &AiConfig) -> f32 {
    if let Some(g) = &ai.value_grid {
        return g.at(p);
    }
    parametric_value(p, ai)
}

/// The value of the ball at team-frame point `p` with the deepest outfield opponent in the goal corridor at `last_x`
/// (`last_defender`): the place's `possession_value`, plus ATT-17's premium for being in behind the last line
/// (`AiConfig::value_behind`), phased in over the 2 m around him.
pub fn value_at(p: Vec2, last_x: f32, ai: &AiConfig) -> f32 {
    let behind = ((p.x - last_x + 1.5) / 2.0).clamp(0.0, 1.0);
    possession_value(p, ai) + behind * ai.value_behind * (-(p.y / ai.value_behind_width).powi(2)).exp()
}

/// ATT-17: the deepest outfield opponent of `side` within `value_behind_corridor` m of the goal's centre line (`side`'s
/// team frame x); behind the own goal line if there is none.
pub fn last_defender(state: &MatchState, side: Side, ai: &AiConfig) -> f32 {
    let fr = state.frame(side);
    state
        .squad(side.opponent())
        .iter()
        .filter(|p| !p.is_gk() && !p.sent_off)
        .map(|p| fr.to_team(p.pos))
        .filter(|q| q.y.abs() < ai.value_behind_corridor)
        .map(|q| q.x)
        .fold(-HALF_LENGTH - 1.0, f32::max)
}

/// The parametric possession value model (`AiConfig::value_base` and following).
pub fn parametric_value(p: Vec2, ai: &AiConfig) -> f32 {
    let s = ((p.x + HALF_LENGTH) / LENGTH).clamp(0.0, 1.0);
    let c = (-(p.y / ai.value_width).powi(2)).exp();
    ai.value_base + ai.value_progress * s.powf(ai.value_power) * (ai.value_wide + (1.0 - ai.value_wide) * c) + ai.value_shot_share * xg_basic(p, ai)
}

impl ValueGrid {
    /// An all-zero grid of `cell_m` squares covering one half-width of the pitch.
    pub fn new(cell_m: f32) -> Self {
        let (nx, ny) = Self::dims(cell_m);
        Self { cell_m, nx, ny, v: vec![0.0; nx * ny] }
    }

    /// Cells along x and across one half-width for squares of `cell_m`.
    pub fn dims(cell_m: f32) -> (usize, usize) {
        ((LENGTH / cell_m).ceil() as usize, (HALF_WIDTH / cell_m).ceil() as usize)
    }

    /// Index of the cell holding team-frame point `p` (clamped onto the pitch) in a grid of `cell_m` squares.
    pub fn cell_of(cell_m: f32, p: Vec2) -> usize {
        let (nx, ny) = Self::dims(cell_m);
        let ix = (((p.x + HALF_LENGTH).max(0.0) / cell_m) as usize).min(nx - 1);
        let iy = ((p.y.abs() / cell_m) as usize).min(ny - 1);
        ix * ny + iy
    }

    /// Team-frame centre of cell `i` (on the positive-y wing).
    pub fn centre(&self, i: usize) -> Vec2 {
        let (ix, iy) = (i / self.ny, i % self.ny);
        Vec2::new((ix as f32 + 0.5) * self.cell_m - HALF_LENGTH, (iy as f32 + 0.5) * self.cell_m)
    }

    /// Value at team-frame point `p`: bilinear between the cell centres, flat beyond the outermost ones.
    pub fn at(&self, p: Vec2) -> f32 {
        let fx = ((p.x + HALF_LENGTH) / self.cell_m - 0.5).clamp(0.0, (self.nx - 1) as f32);
        let fy = (p.y.abs() / self.cell_m - 0.5).clamp(0.0, (self.ny - 1) as f32);
        let (ix, iy) = ((fx as usize).min(self.nx - 2), (fy as usize).min(self.ny - 2));
        let (tx, ty) = (fx - ix as f32, fy - iy as f32);
        let v = |x: usize, y: usize| self.v[x * self.ny + y];
        let lo = v(ix, iy) + (v(ix, iy + 1) - v(ix, iy)) * ty;
        let hi = v(ix + 1, iy) + (v(ix + 1, iy + 1) - v(ix + 1, iy)) * ty;
        lo + (hi - lo) * tx
    }
}

/// Cost of losing the ball at a team-frame point = the value of the possession the opponents' regain there starts
/// (`AiConfig::regain_base` and following), or their learned possession value there.
pub fn turnover_cost(p: Vec2, ai: &AiConfig) -> f32 {
    if let Some(g) = &ai.value_grid {
        return g.at(-p);
    }
    let q = -p;
    let s = ((q.x + HALF_LENGTH) / LENGTH).clamp(0.0, 1.0);
    ai.regain_base + ai.regain_progress * s.powf(ai.regain_power) * (-(q.y / ai.regain_width).powi(2)).exp()
}

/// Pressure on a player (0..1) from the nearest opponents; goal-side opponents count fully.
pub fn pressure_at(state: &MatchState, side: Side, pos: Vec2) -> f32 {
    let fr = state.frame(side);
    let tp = fr.to_team(pos);
    let mut best: f32 = 0.0;
    for o in state.squad(side.opponent()) {
        let d = (o.pos - pos).length();
        let goal_side = fr.to_team(o.pos).x > tp.x - 0.5;
        let w = if goal_side { 1.0 } else { 0.7 };
        best = best.max(w * (1.0 - (d - 1.0) / 5.0).clamp(0.0, 1.0));
    }
    best
}

/// Pitch control for `side` at a world point (0..1), from the fastest player of each team.
pub fn pitch_control(state: &MatchState, side: Side, point: Vec2) -> f32 {
    let t = |s: Side| state.squad(s).iter().map(|p| time_to_reach(p, point, 0.5, true)).fold(f32::MAX, f32::min);
    logistic((t(side.opponent()) - t(side)) / 0.45)
}

/// Can `p` see `point`? Vision cone from the vision attribute, always true for very close points.
pub fn in_vision(p: &Player, point: Vec2, ai: &AiConfig) -> bool {
    in_vision_from(p, p.pos, p.facing, point, ai)
}

/// As `in_vision`, for `p` standing at `pos` and facing `facing` (world frame).
pub fn in_vision_from(p: &Player, pos: Vec2, facing: Vec2, point: Vec2, ai: &AiConfig) -> bool {
    let to = point - pos;
    if to.length() < 10.0 {
        return true;
    }
    let half = (ai.vision_half_angle_min + (ai.vision_half_angle_max - ai.vision_half_angle_min) * norm(p.attrs.intelligence))
        .to_radians();
    facing.angle_to(to).abs() <= half
}

/// Team-frame x of the opponents' units (lines), ascending, by 1-D clustering of their outfield x positions
/// (split at the two largest gaps of at least 4 m). Used by ATT-15 and ATT-17.
pub fn opponent_units(state: &MatchState, side: Side) -> Vec<f32> {
    let fr = state.frame(side);
    let mut xs: Vec<f32> = state.squad(side.opponent()).iter().filter(|p| !p.is_gk()).map(|p| fr.to_team(p.pos).x).collect();
    xs.sort_by(f32::total_cmp);
    let mut gaps: Vec<(f32, usize)> = xs.windows(2).enumerate().map(|(i, w)| (w[1] - w[0], i + 1)).collect();
    gaps.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    let mut cuts: Vec<usize> = gaps.iter().take(2).filter(|g| g.0 >= 4.0).map(|g| g.1).collect();
    cuts.sort();
    let mut units = Vec::new();
    let mut start = 0;
    for c in cuts.into_iter().chain(std::iter::once(xs.len())) {
        let group = &xs[start..c];
        if !group.is_empty() {
            units.push(group.iter().sum::<f32>() / group.len() as f32);
        }
        start = c;
    }
    units
}

/// Opponent offside line in the team frame of `side`: second-last opponent, not behind the ball or halfway.
pub fn offside_line(state: &MatchState, side: Side) -> f32 {
    let fr = state.frame(side);
    let mut xs: Vec<f32> = state.squad(side.opponent()).iter().filter(|p| !p.sent_off).map(|p| fr.to_team(p.pos).x).collect();
    xs.sort_by(|a, b| b.total_cmp(a));
    let second = xs.get(1).copied().unwrap_or(0.0);
    second.max(state.ball_in(side).x).max(0.0)
}

/// Number of opposition units broken by a pass from `from_x` to `to_x` and whether the last one is broken (ATT-17).
pub fn lines_broken(units: &[f32], from_x: f32, to_x: f32) -> (usize, bool) {
    let mut n = 0;
    let mut last = false;
    for (i, &u) in units.iter().enumerate() {
        if from_x < u - 1.0 && to_x > u + 1.0 {
            n += 1;
            if i + 1 == units.len() {
                last = true;
            }
        }
    }
    (n, last)
}

/// Probability that a ball sent along the straight lane `from → to` at roughly 14 m/s gets through the opponents
/// of `side` (cheap version of `evaluate_pass` for positioning).
pub fn lane_openness(state: &MatchState, side: Side, from: Vec2, to: Vec2, cfg: &SimConfig) -> f32 {
    let lane = to - from;
    let len = lane.length().max(0.1);
    let mut open = 1.0;
    for o in state.squad(side.opponent()) {
        let u = ((o.pos - from).dot(lane) / (len * len)).clamp(0.0, 1.0);
        let q = from + lane * u;
        let tb = u * len / 14.0 + 0.1;
        if (o.pos - q).length() > o.kin.top_speed * (tb + 0.3) + cfg.player.control_radius {
            continue;
        }
        let to_ = time_to_reach_after(o, q, cfg.player.control_radius, pass_reaction(o, 0.0, &cfg.player));
        open *= 1.0 - logistic((tb - to_) / cfg.ai.race_sigma);
    }
    open
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PassEval {
    pub p_success: f32,
    /// The part of `p_success` that is a knock-down after a header rather than a reception: his side gets the loose
    /// ball, but not the ball at the target.
    pub p_air: f32,
    pub p_intercept: f32,
    /// Ball flight time to the target.
    pub arrival_t: f32,
    pub arrival_speed: f32,
    /// Most likely loss point (world frame).
    pub loss_point: Vec2,
    /// Where the receiver most likely gets the ball (world frame), as `Simulation::reception` has him take it (ATT-24):
    /// free, at the point he reaches nearest the target; pressed there, where he meets it first (the points along the
    /// path where he is first, weighted by that chance, `race`). The target for a ball into space.
    pub recv_point: Vec2,
    /// ATT-06 / ATT-25: how pressed (0..1) the receiver is when the ball arrives (an opponent can reach the target
    /// within `att06_pressure_time_s` of it).
    pub pressed: f32,
}

/// ATT-06 / ATT-25: how pressed (0..1) a player in possession at a point is, `t` s from now, when the nearest opponent
/// needs `opp_t` s to get there: pressed if he can arrive within `att06_pressure_time_s`, softened by the race sigma.
pub fn pressed_at(t: f32, opp_t: f32, cfg: &SimConfig) -> f32 {
    logistic((t + cfg.tactics.att06_pressure_time_s - opp_t) / cfg.ai.race_sigma)
}

/// Success probability of a kick (`sol`, from `from`) towards `target` for `receiver` (or into space if `None`),
/// racing every opponent along the predicted trajectory. The kick is struck `delay` seconds from now (set-up time).
/// A pass to a receiver is the first-arrival race the sim settles (`race`), a lofted one (`spread`: its landing error)
/// averaged over where it may come down; the receiver starts once he has read the pass (`reading::receive_delay`).
#[allow(clippy::too_many_arguments)]
pub fn evaluate_pass(
    state: &MatchState,
    cfg: &SimConfig,
    plan: &BallParams,
    kicker: PlayerId,
    receiver: Option<PlayerId>,
    from: Vec3,
    sol: &KickSolution,
    target: Vec2,
    delay: f32,
    spread: Option<Vec2>,
) -> PassEval {
    let side = side_of(kicker);
    let mut ball = Ball::at_rest(from.truncate());
    ball.pos.z = from.z.max(BALL_RADIUS);
    ball.kick(sol.vel, sol.spin);
    let tail = if spread.is_some() { cfg.ai.lofted_race_tail_s } else { 0.4 };
    let traj = predict(&ball, plan, sol.time + tail, 0.1);
    let arrival = sol.time;
    let mut eval = PassEval {
        p_success: 0.0,
        p_air: 0.0,
        p_intercept: 1.0,
        arrival_t: arrival,
        arrival_speed: sol.arrival_vel.length(),
        loss_point: target,
        recv_point: target,
        pressed: 0.0,
    };
    if let Some(r) = receiver {
        let rp = &state.players[r];
        let spread = spread.unwrap_or(Vec2::ZERO);
        let read = super::reading::receive_delay(state, &cfg.ai, r, kicker);
        let (p_first, p_air, p_opp, loss, recv) = race(state, cfg, &traj, rp, read, side, from.truncate(), target, arrival, delay, spread);
        let p_clear = 1.0 - mate_in_path(state, kicker, receiver, &traj, delay, &cfg.player);
        let (p_first, p_air) = (p_first * p_clear, p_air * p_clear);
        let opp_time = state
            .squad(side.opponent())
            .iter()
            .map(|o| opponent_time(o, target, cfg.player.control_radius, delay, &cfg.player))
            .fold(f32::MAX, f32::min);
        let pressure = (1.0 - (opp_time - arrival - delay) / 1.0).clamp(0.0, 1.0);
        eval.pressed = pressed_at(arrival + delay, opp_time, cfg);
        let height = (sol.arrival_pos.z - BALL_RADIUS).max(0.0) + 0.6 * sol.arrival_vel.z.abs() / 5.0;
        let p_ctrl = control_prob(&rp.attrs, rp.confidence, rp.energy, eval.arrival_speed, height, pressure, &cfg.duel);
        eval.p_intercept = p_opp;
        eval.loss_point = loss;
        // His margin counts from now; the ball's samples from the release, `delay` later, and he reads it `read` late.
        let margin = cfg.tactics.att24_set_margin_s + read - delay;
        let free = reception_point(rp, &traj, target, cfg.player.control_height, margin).map_or(recv, |(_, pt)| pt);
        eval.recv_point = free.lerp(recv, eval.pressed);
        // A failed first touch leaves a loose ball near the receiver, recovered by his team about a third of the time.
        eval.p_success = p_first * (0.35 + 0.65 * p_ctrl) + p_air;
        eval.p_air = p_air;
        return eval;
    }
    if traj.exit_time.is_some_and(|e| e < arrival + 0.2) {
        return eval;
    }
    let seg = target - from.truncate();
    let seg_len = seg.length().max(0.1);
    let mut worst = 0.0;
    for o in state.squad(side.opponent()) {
        // Quick reject: too far from the lane to matter.
        let u = ((o.pos - from.truncate()).dot(seg) / (seg_len * seg_len)).clamp(0.0, 1.0);
        let q = from.truncate() + seg * u;
        if (o.pos - q).length() > o.kin.top_speed * (arrival + delay + 0.4) + 2.0 {
            continue;
        }
        // An outfield opponent contests a high ball up to his header height.
        let reach_h = if o.is_gk() { cfg.player.gk_max_height } else { o.header_height() };
        let (mut best, mut best_pt) = block(o, &traj, delay, &cfg.player).map_or((0.0f32, q), |(p, _, g)| (p, g));
        let (react, start, vel) = opponent_start(o, delay, &cfg.player);
        for s in &traj.samples {
            if s.t > arrival + 0.1 {
                break;
            }
            if s.pos.z - BALL_RADIUS > reach_h {
                continue;
            }
            let g = s.pos.truncate();
            let to = react + time_to_reach_from(&o.kin, start, vel, g, cfg.player.control_radius, 0.0);
            let p = logistic((s.t + delay - to) / cfg.ai.race_sigma) * pass_touch_prob(s.vel.length(), o.attrs.instinct, &cfg.player);
            if p > best {
                best = p;
                best_pt = g;
            }
        }
        if best > worst {
            worst = best;
            eval.loss_point = best_pt;
        }
    }
    // PRS-02: one opponent goes for the ball, the best placed.
    let p_none = (1.0 - worst) * (1.0 - mate_in_path(state, kicker, receiver, &traj, delay, &cfg.player));
    eval.p_intercept = worst;

    let opp_time = state
        .squad(side.opponent())
        .iter()
        .map(|o| opponent_time(o, target, cfg.player.control_radius, delay, &cfg.player))
        .fold(f32::MAX, f32::min);
    eval.pressed = pressed_at(arrival + delay, opp_time, cfg);
    let own_time = state
        .squad(side)
        .iter()
        .filter(|p| p.id != kicker)
        .map(|p| time_to_reach(p, target, cfg.player.control_radius, true))
        .fold(f32::MAX, f32::min);
    // A failed first touch leaves a loose ball near the receiver, recovered by his team about a third of the time.
    eval.p_success = p_none * logistic((opp_time - own_time) / 0.5) * (0.35 + 0.65 * 0.8);
    eval
}

/// IND-08: the time (s from now) opponent `o` needs to reach `point` (within `radius`) against a kick struck `delay` s
/// from now: he reads it at the release (`pass_reaction` from the kick, as the sim lets him) and until then carries
/// on with his movement (`locomotion::carry_on`).
fn opponent_time(o: &Player, point: Vec2, radius: f32, delay: f32, pc: &crate::config::PlayerConfig) -> f32 {
    let (react, pos, vel) = opponent_start(o, delay, pc);
    react + time_to_reach_from(&o.kin, pos, vel, point, radius, 0.0)
}

/// IND-08: when (s from now) opponent `o` starts for a kick struck `delay` s from now, where he is and how he moves
/// then (`opponent_time`).
fn opponent_start(o: &Player, delay: f32, pc: &crate::config::PlayerConfig) -> (f32, Vec2, Vec2) {
    let react = delay + pass_reaction(o, 0.0, pc);
    let (pos, vel) = carry_on(o, react);
    (react, pos, vel)
}

/// The sim's block (`Simulation::foot_contact`): a low kick that reaches an outfield opponent within his reaction time
/// of the strike hits him if it passes within `control_radius` (his body within `body_radius`, `shot_body_block_p`,
/// otherwise a leg, `shot_leg_block_p` by instinct); he has no time to go for it and carries on with his movement,
/// through the set-up (`delay`) too. Returns the chance, when (s after the strike) and where.
fn block(o: &Player, traj: &crate::ball::Trajectory, delay: f32, pc: &crate::config::PlayerConfig) -> Option<(f32, f32, Vec2)> {
    if o.is_gk() {
        return None;
    }
    in_path(o, traj, delay, pc, pc.control_radius, pc.control_height)
}

/// K5: the passer sees his team-mates too. A team-mate the pass is not for that it reaches within his reaction time
/// of the strike hits his body if it passes within `body_radius` below his height (`Simulation::mate_hit`): the chance
/// that one does, the most likely of them.
fn mate_in_path(state: &MatchState, kicker: PlayerId, receiver: Option<PlayerId>, traj: &crate::ball::Trajectory, delay: f32, pc: &crate::config::PlayerConfig) -> f32 {
    state
        .squad(side_of(kicker))
        .iter()
        .filter(|m| m.id != kicker && Some(m.id) != receiver)
        .filter_map(|m| in_path(m, traj, delay, pc, pc.body_radius, m.attrs.height_cm / 100.0))
        .map(|b| b.0)
        .fold(0.0, f32::max)
}

/// `block` and `mate_in_path`: the first point of the path within `o`'s reaction time of the strike where the ball
/// passes within `radius` of him (where he will be, carrying on with his movement) no higher than `top`.
fn in_path(o: &Player, traj: &crate::ball::Trajectory, delay: f32, pc: &crate::config::PlayerConfig, radius: f32, top: f32) -> Option<(f32, f32, Vec2)> {
    for w in traj.samples.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        if a.t >= o.kin.reaction {
            break;
        }
        let at = carry_on(o, delay + 0.5 * (a.t + b.t)).0;
        let (g0, seg) = (a.pos.truncate(), (b.pos - a.pos).truncate());
        let u = ((at - g0).dot(seg) / seg.length_squared().max(1e-6)).clamp(0.0, 1.0);
        let (t, g) = (a.t + u * (b.t - a.t), g0 + seg * u);
        let d = (at - g).length();
        if t < o.kin.reaction && d < radius && a.pos.z + u * (b.pos.z - a.pos.z) - BALL_RADIUS <= top {
            let p = if d < pc.body_radius { pc.shot_body_block_p } else { pc.shot_leg_block_p * (0.75 + 0.25 * norm(o.attrs.instinct)) };
            return Some((p, t, g));
        }
    }
    None
}

/// A pass to `r` as the sim settles it: a first-arrival race along the ball's path. The receiver plays it at the
/// earliest point he can reach below control height (ATT-24: he meets the ball when pressed, `reach::reception_point`
/// / `intercept`), an opponent at the earliest he can reach below his header height (the brains' loose-ball chaser,
/// PRS-02); a ball that leaves the pitch first is lost. Each player's chance of being able to reach the ball by a sample grows
/// along the path (race logistic); the receiver is first where his chance rises while no opponent's has (ties split).
/// So a marker beside the receiver wins only the part of the path he reaches before both the ball and the receiver.
/// A lofted pass is averaged over five landing points spread by the kicker's execution error (`spread`: one σ along
/// and across the pass, `execution::landing_spread`; zero for a ground pass): the aimed one and ±√3 σ on each axis
/// (unscented transform, weights 1/3 and 1/6), each shifting the path by its landing offset in proportion to flight
/// time. The receiver starts `read` s after the set-up begins (it is legible to him, ATT-24), an opponent only at the
/// strike (IND-08), carrying on through the set-up. Returns the probability that the receiver is first, that an
/// opponent is, where an opponent most likely wins it, and where the receiver most likely gets it (his first-arrival
/// points weighted by their chance; the target if he is never first).
#[allow(clippy::too_many_arguments)]
fn race(
    state: &MatchState,
    cfg: &SimConfig,
    traj: &crate::ball::Trajectory,
    r: &Player,
    read: f32,
    side: Side,
    from: Vec2,
    target: Vec2,
    arrival: f32,
    delay: f32,
    spread: Vec2,
) -> (f32, f32, f32, Vec2, Vec2) {
    let (pc, sigma) = (&cfg.player, cfg.ai.race_sigma);
    let dir = (target - from).normalize_or(Vec2::X);
    let (a, c) = (dir * spread.x * 3f32.sqrt(), dir.perp() * spread.y * 3f32.sqrt());
    let cases = if spread == Vec2::ZERO {
        vec![(Vec2::ZERO, 1.0)]
    } else {
        vec![(Vec2::ZERO, 1.0 / 3.0), (a, 1.0 / 6.0), (-a, 1.0 / 6.0), (c, 1.0 / 6.0), (-c, 1.0 / 6.0)]
    };
    let opps: Vec<&Player> = state.squad(side.opponent()).iter().collect();
    // The receiver's chance in a header against each opponent, and the slack a player within the contest radius has.
    let pw: Vec<f32> = opps.iter().map(|o| aerial_win_prob(r, o, &cfg.duel)).collect();
    let cr = cfg.duel.aerial_contest_radius;
    let (kw, kl) = (cfg.ai.aerial_won_keep, cfg.ai.aerial_lost_keep);
    let mut f_opp = vec![0.0f32; opps.len()];
    let blocks: Vec<Option<(f32, f32, Vec2)>> = opps.iter().map(|o| block(o, traj, delay, pc)).collect();
    let starts: Vec<(f32, Vec2, Vec2)> = opps.iter().map(|o| opponent_start(o, delay, pc)).collect();
    let (mut p_recv, mut p_air, mut p_opp) = (0.0, 0.0, 0.0);
    let mut loss = (0.0, target);
    let (mut recv_sum, mut recv_w) = (Vec2::ZERO, 0.0f32);
    for (shift, w) in cases {
        f_opp.fill(0.0);
        let (mut f_r, mut none_before) = (0.0f32, 1.0f32);
        for s in &traj.samples {
            let g = s.pos.truncate() + shift * (s.t / arrival).min(1.0);
            if !in_bounds(g) {
                break;
            }
            let h = s.pos.z - BALL_RADIUS;
            let t = s.t + delay;
            // Above control height the ball is headed, and the sim settles a header with an opponent inside the
            // contest radius as an aerial duel (`aerial_win_prob`), not by who got there first.
            let aerial = h > pc.control_height;
            // Within the contest radius of the first man to the ball: this much earlier or later along its path.
            let slack = cr / s.vel.truncate().length().max(1.0);
            // PRS-02: one opponent goes for the ball (`brain` chaser), so the race is against the best placed one.
            let mut f_max = 0.0f32;
            let (mut near_none, mut pw_sum, mut f_sum, mut f_gk) = (1.0f32, 0.0f32, 0.0f32, 0.0f32);
            for ((((o, f), pwo), bl), &(react, start, vel)) in opps.iter().zip(f_opp.iter_mut()).zip(&pw).zip(&blocks).zip(&starts) {
                if let Some((bp, bt, _)) = *bl
                    && s.t >= bt
                {
                    *f = f.max(bp);
                }
                let reach_h = if o.is_gk() { pc.gk_max_height } else { o.header_height() };
                // IND-08: he reads the pass at its release and carries on with his movement until then
                // (`opponent_start`). Skip the race when even running flat out from then he could not get near it.
                let lower = react + ((start - g).length() - pc.control_radius) / o.kin.top_speed;
                if h <= reach_h && t - lower > -4.0 * sigma {
                    let to = react + time_to_reach_from(&o.kin, start, vel, g, pc.control_radius, 0.0);
                    *f = f.max(logistic((t - to) / sigma) * pass_touch_prob(s.vel.length(), o.attrs.instinct, pc));
                    if aerial && !o.is_gk() {
                        near_none *= 1.0 - logistic((t + slack - to) / sigma);
                        pw_sum += *f * pwo;
                        f_sum += *f;
                    }
                }
                if o.is_gk() {
                    f_gk = *f;
                }
                f_max = f_max.max(*f);
            }
            let none = 1.0 - f_max;
            let near_o = 1.0 - near_none;
            let mut f_r_now = f_r;
            let mut near_r = 0.0;
            if aerial {
                if h <= r.header_height() {
                    // Only a pressed receiver attacks the ball at head height (`Simulation::reception`); free, he
                    // lets it drop to him, so that share of his arrival waits for the ground samples.
                    let to_r = time_to_reach_after(r, g, pc.control_radius, read);
                    f_r_now = f_r + (logistic((t - to_r) / sigma) - f_r).max(0.0) * near_o;
                    near_r = logistic((t + slack - to_r) / sigma);
                }
            } else if h <= pc.control_height {
                f_r_now = f_r.max(logistic((t - time_to_reach_after(r, g, pc.control_radius, read)) / sigma));
            }
            let r_first = (f_r_now - f_r) * 0.5 * (none + none_before);
            let o_first = (none_before - none) * (1.0 - 0.5 * (f_r + f_r_now));
            recv_sum += g * (w * r_first);
            recv_w += w * r_first;
            let lost = if aerial {
                // A header the receiver contests (he is first, or within the contest radius of the opponent who is)
                // is an aerial duel and then a loose knock-down: his side keeps it `aerial_won_keep` of the time
                // after a won one and `aerial_lost_keep` after a lost one.
                let pw = if f_sum > 0.0 { pw_sum / f_sum } else { 0.5 };
                let keep = pw * kw + (1.0 - pw) * kl;
                let contested = r_first + o_first * near_r;
                // An opponent's free header is a loose ball too (`aerial_lost_keep`), a keeper's catch is not.
                let free_opp = o_first * (1.0 - near_r);
                let back = free_opp * kl * (1.0 - f_gk);
                p_air += w * (contested * keep + back);
                contested * (1.0 - keep) + free_opp - back
            } else {
                p_recv += w * r_first;
                o_first
            };
            p_opp += w * lost;
            if w * lost > loss.0 {
                loss = (w * lost, g);
            }
            (f_r, none_before) = (f_r_now, none);
        }
    }
    let recv = if recv_w > 1e-4 { recv_sum / recv_w } else { target };
    (p_recv, p_air, p_opp, loss.1, recv)
}

/// xG of a shot by `shooter` from team-frame point `p`, struck now with the foot his body position gives, with
/// blockers, goalkeeper position, pressure and the shooter's own finishing (the decision model is optimal *for that
/// player*).
pub fn shot_xg(state: &MatchState, cfg: &SimConfig, shooter: PlayerId, p: Vec2) -> f32 {
    shot_xg_parts(state, cfg, shooter, p, ShotHow::default()).total
}

/// How a shot is struck, as far as xG is concerned.
#[derive(Clone, Copy, Debug, Default)]
pub struct ShotHow {
    /// Kicking foot; `None`: the one the body position gives for a shot at the goal centre.
    pub foot: Option<Foot>,
    /// Body angle (rad) beyond what that foot can strike (`execution::setup_turn`), for a kick without set-up.
    pub body_excess: f32,
    pub first_time: bool,
    /// Time (s) until the shot is struck: the keeper keeps adjusting meanwhile.
    pub lead: f32,
}

/// `shot_xg` step by step: the running value after each factor.
#[derive(Clone, Copy, Debug, Default)]
pub struct XgParts {
    /// Unblocked, average-finisher xG from distance and angle (`xg_basic`).
    pub basic: f32,
    pub after_blockers: f32,
    pub after_keeper: f32,
    pub after_pressure: f32,
    /// After the shooter's finishing: skill and foot, relative to the reference finisher (`finish_factor`).
    pub total: f32,
    pub finish_factor: f32,
    pub weak_foot: bool,
    /// Share of the goal the actual keeper / a set keeper on his GK-01 spot cannot cover.
    pub open_share: f32,
    pub open_share_set: f32,
}

pub fn shot_xg_parts(state: &MatchState, cfg: &SimConfig, shooter: PlayerId, p: Vec2, how: ShotHow) -> XgParts {
    let ai = &cfg.ai;
    let side = side_of(shooter);
    let fr = state.frame(side);
    let mut parts = XgParts::default();
    let mut xg = xg_basic(p, ai);
    parts.basic = xg;
    if xg <= 0.0 {
        return parts;
    }
    let post_l = Vec2::new(HALF_LENGTH, GOAL_HALF_WIDTH);
    let post_r = Vec2::new(HALF_LENGTH, -GOAL_HALF_WIDTH);
    let goal = TeamFrame::opp_goal();
    let mut gk_pos = None;
    for o in state.squad(side.opponent()) {
        let q = fr.to_team(o.pos);
        if o.is_gk() {
            gk_pos = Some(q);
            continue;
        }
        if in_triangle(q, p, post_l, post_r) {
            let closeness = 1.0 - ((q - p).length() / 20.0).clamp(0.0, 0.7);
            xg *= 1.0 - ai.xg_blocker_factor * closeness;
        }
    }
    parts.after_blockers = xg;
    // Keeper: `xg_basic` assumes one standing on his GK-01 spot. A keeper who cannot get across in time leaves part of
    // the goal open; xG rises towards the open-goal value by the share of the goal he leaves open beyond that.
    let open_goal = ai.xg_open_goal * (goal_angle(p) / ai.xg_open_angle).min(1.0).powf(ai.xg_open_exp);
    let sh = &state.players[shooter];
    let speed = ai.xg_shot_speed_factor * sh.kin.max_kick_speed;
    match gk_pos.and_then(|_| state.squad(side.opponent()).iter().find(|o| o.is_gk())) {
        Some(gk) => {
            let actual = keeper_open_share(gk, fr, p, speed, state.time, how.lead, cfg);
            let mut set = gk.clone();
            set.pos = fr.to_world(gk01_spot(p, cfg));
            set.vel = Vec2::ZERO;
            set.body = Body::Normal;
            let reference = keeper_open_share(&set, fr, p, speed, state.time, how.lead, cfg);
            (parts.open_share, parts.open_share_set) = (actual, reference);
            xg += (open_goal - xg).max(0.0) * (actual - reference).max(0.0);
        }
        None => xg = xg.max(open_goal),
    }
    parts.after_keeper = xg;
    let pressure = pressure_at(state, side, fr.to_world(p));
    xg *= 1.0 - ai.xg_pressure_factor * pressure;
    parts.after_pressure = xg;
    let dir = (fr.to_world(goal) - fr.to_world(p)).normalize_or_zero();
    let foot = how.foot.unwrap_or_else(|| natural_foot(sh.attrs.preferred_foot, sh.facing, dir, speed, sh.kin.max_kick_speed, &cfg.execution));
    let dist = (goal - p).length();
    let mut inp =
        ExecInput::for_player(&sh.attrs, KickKind::Shot, dist, false, foot, how.first_time, 0.0, sh.confidence, sh.energy, &cfg.execution);
    inp.body_excess = how.body_excess;
    parts.weak_foot = inp.weak_foot;
    parts.finish_factor = finish_factor(&inp, cfg);
    xg *= parts.finish_factor;
    parts.total = xg.clamp(0.0, 0.97);
    parts
}

/// Finishing relative to the reference finisher of `xg_basic` (strong foot, `xg_ref_skill`, set, rested): the ratio
/// of his execution error scale (`ExecInput::scale`: skill, weak foot, first time, body angle, fatigue, confidence)
/// to the shooter's, to the power `xg_finish_exp` (fitted to the `finishing` bench: a goal needs the ball on target
/// *and* past the keeper, so halving the error does not double the goals).
pub fn finish_factor(inp: &ExecInput, cfg: &SimConfig) -> f32 {
    let reference = ExecInput {
        skill: cfg.ai.xg_ref_skill,
        weak_foot: false,
        first_time: false,
        body_excess: 0.0,
        pressure: 0.0,
        energy: 1.0,
        confidence: 0.5,
        speed_frac: 0.0,
        ..*inp
    };
    let ex = &cfg.execution;
    (reference.scale(ex) / inp.scale(ex).max(1e-3)).powf(cfg.ai.xg_finish_exp)
}

/// GK-01: how far off his line (m) a keeper narrowing the angle stands against a ball `dist` metres from the goal
/// centre. Calibrated to StatsBomb freeze frames at open-play shots (median depth ≈ 1.2 m from 0–6 m, ≈ 1.6 m at
/// 9–12 m, ≈ 2.1–2.3 m at 12–25 m, ≈ 1.8–2.0 m beyond).
pub fn gk01_depth(dist: f32, tc: &TacticsConfig) -> f32 {
    (tc.gk01_narrow_depth + tc.gk01_depth_per_m * (dist - 11.0).max(0.0)).min(tc.gk01_max_depth).min(tc.gk01_depth_frac * dist)
}

/// GK-01: where a set keeper stands against a shot from team-frame `p` (as `positioning` places him within shooting
/// range): on the ball–goal line, a few metres off his line, a little higher the further away the shooter.
pub fn gk01_spot(p: Vec2, cfg: &SimConfig) -> Vec2 {
    let goal = TeamFrame::opp_goal();
    let dist = (p - goal).length();
    let depth = gk01_depth(dist, &cfg.tactics);
    let dir = (p - goal).normalize_or(-Vec2::X);
    let along = (depth / dir.x.abs().max(0.3)).min(dist - 1.0).max(0.5);
    let mut t = goal + dir * along;
    t.y = t.y.clamp(-GOAL_HALF_WIDTH - 0.5, GOAL_HALF_WIDTH + 0.5);
    t
}

/// Share of the goal mouth keeper `gk` cannot cover against a shot from team-frame `p` travelling at about `speed`,
/// struck `lead` seconds from now: for each aim point between the posts he races the ball to its path, anywhere up
/// to the goal line, needing his reaction time (and to finish whatever he is doing), then running and diving. Before
/// the strike he can finish what he is doing and move, but not react to the shot: at least his reaction and dive
/// fall after it. The race is soft (`xg_keeper_race_sigma`): a keeper who is just too late still gets a hand to some.
#[allow(clippy::too_many_arguments)]
fn keeper_open_share(gk: &Player, fr: TeamFrame, p: Vec2, speed: f32, now: f32, lead: f32, cfg: &SimConfig) -> f32 {
    const AIMS: usize = 9;
    let pc = &cfg.player;
    let busy = match gk.body {
        Body::Normal => 0.0,
        Body::Kicking { until } | Body::Tackling { until } | Body::Recovering { until } | Body::Diving { until, .. } => {
            (until - now).max(0.0)
        }
    };
    let reach = pc.gk_standing_reach + gk.kin.gk_dive_reach;
    let dive_t = gk.kin.gk_dive_reach / pc.gk_dive_speed;
    // Flight time to a point `s` m along the path under quadratic air drag (dv/ds = −k·v): (e^{ks} − 1) / (k·v0). At
    // constant speed a 40 m shot reached the line in 1.6 s instead of 2.1 s, and the keeper looked beaten.
    let k = 0.5 * cfg.ball.air_density * cfg.ball.drag_coefficient * std::f32::consts::PI * BALL_RADIUS * BALL_RADIUS / BALL_MASS;
    let flight = |s: f32| if k > 1e-6 { ((k * s).exp() - 1.0) / (k * speed) } else { s / speed };
    let gk_t = fr.to_team(gk.pos);
    let mut open = 0.0;
    for k in 0..AIMS {
        let y = (GOAL_HALF_WIDTH - 0.3) * (2.0 * k as f32 / (AIMS - 1) as f32 - 1.0);
        let aim = Vec2::new(HALF_LENGTH, y);
        let len = (aim - p).length();
        let dir = (aim - p) / len.max(0.1);
        // Only the stretch of the path from level with the keeper (less his reach) to the line can be saved.
        let from = ((gk_t - p).dot(dir) - reach).clamp(0.0, len);
        let steps = ((len - from) / 0.5).ceil().max(1.0) as usize;
        let margin = (0..=steps)
            .map(|i| {
                let s = from + (len - from) * i as f32 / steps as f32;
                let pt = p + dir * s;
                let near = (pt - gk_t).length();
                let dive = dive_t.min((near - pc.gk_standing_reach).max(0.0) / pc.gk_dive_speed);
                let t_gk = busy + time_to_reach(gk, fr.to_world(pt), reach, true) + dive;
                flight(s) - (t_gk - lead).max(gk.kin.reaction + dive)
            })
            .fold(f32::MIN, f32::max);
        open += 1.0 - logistic(margin / cfg.ai.xg_keeper_race_sigma);
    }
    open / AIMS as f32
}

fn in_triangle(q: Vec2, a: Vec2, b: Vec2, c: Vec2) -> bool {
    let d1 = (q - a).perp_dot(b - a);
    let d2 = (q - b).perp_dot(c - b);
    let d3 = (q - c).perp_dot(a - c);
    let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(neg && pos)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_grid_interpolates_between_cell_centres_and_folds_the_wings() {
        let mut g = ValueGrid::new(3.0);
        assert_eq!((g.nx, g.ny), (35, 12));
        for i in 0..g.v.len() {
            let c = g.centre(i);
            g.v[i] = 0.01 * (c.x + HALF_LENGTH) + 0.001 * c.y;
        }
        for i in [0, 13, 200, g.v.len() - 1] {
            let c = g.centre(i);
            assert_eq!(ValueGrid::cell_of(3.0, c), i);
            assert!((g.at(c) - g.v[i]).abs() < 1e-6);
        }
        // Linear in between, the same on both wings, flat beyond the outermost centres.
        let p = Vec2::new(10.2, 7.7);
        assert!((g.at(p) - (0.01 * (p.x + HALF_LENGTH) + 0.001 * p.y)).abs() < 1e-5);
        assert_eq!(g.at(p), g.at(Vec2::new(p.x, -p.y)));
        assert_eq!(g.at(Vec2::new(HALF_LENGTH, 0.0)), g.at(Vec2::new(HALF_LENGTH - 1.5, 0.0)));
    }

    #[test]
    fn value_rises_towards_goal() {
        let ai = AiConfig::default();
        let own = possession_value(Vec2::new(-40.0, 0.0), &ai);
        let mid = possession_value(Vec2::new(0.0, 0.0), &ai);
        let edge = possession_value(Vec2::new(36.0, 0.0), &ai);
        let spot = possession_value(Vec2::new(41.5, 0.0), &ai);
        assert!(own < mid && mid < edge && edge < spot);
        assert!(spot < 0.5);
        let wide = possession_value(Vec2::new(41.5, 30.0), &ai);
        assert!(wide < spot);
    }

    #[test]
    fn xg_is_plausible() {
        // Footed shots, keeper set, nobody in the way: StatsBomb open-play conversion of such clean shots
        // (docs/design.md, xG). Central …
        let ai = AiConfig::default();
        let xg = |d: f32, y: f32| xg_basic(Vec2::new(HALF_LENGTH - d, y), &ai);
        let (six, penalty, twenty, thirty) = (xg(6.0, 0.0), xg(11.0, 0.0), xg(20.0, 0.0), xg(30.0, 0.0));
        assert!((0.4..0.6).contains(&six), "{six}");
        assert!((0.2..0.35).contains(&penalty), "{penalty}");
        assert!((0.06..0.12).contains(&twenty), "{twenty}");
        assert!((0.02..0.045).contains(&thirty), "{thirty}");
        // … and wide: worth less than the goal angle alone suggests.
        assert!(xg(16.5, 12.0) < 0.06, "{}", xg(16.5, 12.0));
        assert!(xg(25.0, 15.0) < 0.02, "{}", xg(25.0, 15.0));
    }

    #[test]
    fn units_are_clustered() {
        // Clustering is tested through a full scenario in the integration tests; here only lines_broken.
        let units = [-20.0, 0.0, 20.0];
        assert_eq!(lines_broken(&units, -25.0, 5.0), (2, false));
        assert_eq!(lines_broken(&units, 5.0, 30.0), (1, true));
    }
}
