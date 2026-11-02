//! On-ball decisions with the 3-layer model:
//! 1. optimum `U_opt = p·V(success) − (1−p)·C(turnover)`, shaped by ATT-01/02/03/04/06/07/17/21 and TR-01; keeping
//!    the ball (dribble, hold) adds the best pass he expects to have afterwards (look-ahead level 1, `ahead`);
//! 2. manager-instruction biases, scaled by adherence (discipline, role familiarity; egotism resists "pass more");
//! 3. personality biases (`personality`) and softmax noise whose temperature depends on decisions, composure,
//!    confidence and pressure. Execution error is applied afterwards (`execution`).

use glam::{Vec2, Vec3};

use super::analysis::{
    ShotHow, evaluate_pass, goal_angle, in_vision, lines_broken, possession_value, pressed_at, pressure_at, shot_xg_parts, value_at,
    turnover_cost,
};
use super::execution::{ExecInput, landing_spread, natural_foot, setup_angle, setup_time, setup_turn};
use super::personality::{BiasCtx, OptionKind, bias};
use super::deviation::state_factor;
use super::reach::{pass_reaction, time_to_reach, time_to_reach_from};
use super::scan;
use super::brain::{InPhase, TeamPhase};
use crate::ball::{BallParams, KickSolution, KickSpec, solve_kick};
use crate::config::SimConfig;
use crate::duel::{challenge_strength, logistic, tackle_win_prob};
use crate::match_state::MatchState;
use crate::pitch::{
    GOAL_HALF_WIDTH, HALF_LENGTH, HALF_WIDTH, PENALTY_AREA_DEPTH, PENALTY_AREA_HALF_WIDTH, Third, clamp_to_pitch, is_cross,
    third_of,
};
use crate::player::locomotion::carry_on;
use crate::player::{AttrClass, Foot, KickKind, KickPlan, Player, PlayerId, effective, norm};
use crate::rng::SimRng;
use crate::team::{AttackingTactics, Movement, adherence, applied_attacking, resolve_player};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Kick(KickPlan),
    /// Push the ball ahead in a world direction; `lead` = how far ahead of the player it should run.
    Dribble { dir: Vec2, lead: f32 },
    /// Keep the ball close (shield / settle). As a first-time alternative it means "control it".
    Hold,
}

#[derive(Clone, Debug)]
pub struct OptionEval {
    pub label: String,
    pub kind: OptionKind,
    pub action: Action,
    /// Success probability.
    pub p: f32,
    /// Layer 1: optimum utility.
    pub u_opt: f32,
    /// Layer 2: instruction bias.
    pub instr: f32,
    /// Layer 3: personality bias.
    pub pers: f32,
    /// Final choice probability (softmax).
    pub prob: f32,
    /// ATT-25: how pressed (0..1) whoever has the ball after this option is.
    pub pressed: f32,
    pub rules: Vec<&'static str>,
}

impl OptionEval {
    pub fn utility(&self) -> f32 {
        self.u_opt + self.instr + self.pers
    }
}

/// Where and how the decision is taken.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Situation {
    pub carrier: PlayerId,
    /// Ball position where the action happens (world).
    pub ball: Vec3,
    /// Deciding before the ball arrives (ATT-06): only kicks, or "control" (Hold).
    pub first_time: bool,
    /// Restart (kickoff, throw-in, goal kick, corner): passes only.
    pub restart: bool,
    /// Goalkeeper with the ball in his hands: passes and kicks only.
    pub gk_hands: bool,
    /// Longest pass considered (e.g. throw-ins).
    pub max_pass: f32,
    /// Goal kick, throw-in or corner: no offside from it (Law 11.3).
    pub offside_exempt: bool,
    /// Direct free kick: a shot at goal is allowed too (SP-03).
    pub direct: bool,
    /// Penalty kick: a shot only (Law 14, SP-04).
    pub penalty: bool,
    /// Corner kick: the delivery is the team's routine (SP-02, `set_piece::corner_delivery`).
    pub corner: bool,
    /// Throw-in (SP-06): thrown, from `ball` at hand height, at a throwing speed.
    pub throw_in: bool,
    /// Deciding before the ball arrives: time (s) until it reaches him and he plays it. The keeper keeps adjusting
    /// meanwhile, so a first-time shot is priced against where he will be, not where he stands now.
    pub arrives_in: f32,
}

impl Situation {
    /// Open-play decision with the ball at `ball`.
    pub fn open_play(carrier: PlayerId, ball: Vec3) -> Self {
        Self {
            carrier,
            ball,
            first_time: false,
            restart: false,
            gk_hands: false,
            max_pass: f32::MAX,
            offside_exempt: false,
            direct: false,
            penalty: false,
            corner: false,
            throw_in: false,
            arrives_in: 0.0,
        }
    }
}

#[derive(Clone)]
struct Ctx<'a> {
    state: &'a MatchState,
    cfg: &'a SimConfig,
    plan: &'a BallParams,
    sit: Situation,
    p: &'a Player,
    fr: crate::pitch::TeamFrame,
    from: Vec2,
    from_w: Vec2,
    pressure: f32,
    counter: bool,
    max_speed: f32,
    /// Local value scale (possession value at the ball + base): tactical bonuses, instruction and personality
    /// biases and the decision temperature are fractions of it, so they weigh the same anywhere on the pitch.
    scale: f32,
    /// The carrier's in-possession values (layer 1 blended with his team's instructions).
    att: AttackingTactics,
    /// Look-ahead level 1 (docs/player-brain.md 8.4): how much the best pass he expects to have at his next decision
    /// is worth beyond the value of the ball's location (≥ 0), holding (`ahead` at `onball_interval`) or at the end of
    /// a carry (at `carry_time`). What keeping the ball keeps available, or opens.
    cont_hold: f32,
    cont_carry: f32,
}

impl Ctx<'_> {
    /// Time the carrier needs to set his body up for a kick of `speed` in world direction `dir` with `foot` (`None`:
    /// either; `execution::setup_time`); defenders close in meanwhile. `None` when a first-time kick cannot be struck
    /// that way: the receiver faces the incoming ball and can open his body (ATT-05) only by `first_time_open_deg`.
    fn kick_delay(&self, dir: Vec2, speed: f32, foot: Option<Foot>) -> Option<f32> {
        let ex = &self.cfg.execution;
        if self.sit.restart || self.sit.gk_hands {
            return Some(0.0);
        }
        if self.sit.first_time {
            let towards_ball = -self.state.ball.ground_vel().normalize_or_zero();
            let reach = setup_angle(speed, self.max_speed, ex) + ex.first_time_open_deg.to_radians();
            return (towards_ball == Vec2::ZERO || towards_ball.angle_to(dir).abs() <= reach).then_some(0.0);
        }
        Some(setup_time(self.p, dir, speed, foot, ex))
    }

    /// Set-up delay and foot of a pass or clearance: the strong foot unless setting it up there takes more than
    /// `strong_foot_slack_s · (1 − weak-foot ability)` longer than striking with whichever foot can (`None`).
    fn pass_delay(&self, sol_vel: Vec3) -> Option<(f32, Option<Foot>)> {
        let (dir, speed) = (sol_vel.truncate().normalize_or_zero(), sol_vel.length());
        let either = self.kick_delay(dir, speed, None)?;
        if self.sit.first_time {
            return Some((either, None));
        }
        let strong = self.p.attrs.preferred_foot;
        let slack = self.cfg.execution.strong_foot_slack_s * (1.0 - norm(self.p.attrs.weak_foot));
        Some(match self.kick_delay(dir, speed, Some(strong)) {
            Some(s) if s <= either + slack => (s, Some(strong)),
            _ => (either, None),
        })
    }

    /// One-σ landing error of the lofted pass `sol` over `range` m (`execution::landing_spread`), struck with `foot`
    /// (`None`: whichever the kick direction gives) under the pressure on the ball now; `cross`: it is a cross.
    fn landing_spread(&self, sol: &KickSolution, foot: Option<Foot>, range: f32, cross: bool) -> Vec2 {
        let (p, ex) = (self.p, &self.cfg.execution);
        let dir = sol.vel.truncate().normalize_or(p.facing);
        let foot = foot.unwrap_or_else(|| natural_foot(p.attrs.preferred_foot, p.facing, dir, sol.vel.length(), p.kin.max_kick_speed, ex));
        let inp = ExecInput::for_player(
            &p.attrs,
            KickKind::LoftedPass,
            range,
            cross,
            foot,
            self.sit.first_time,
            self.pressure,
            p.confidence,
            p.energy,
            ex,
        );
        landing_spread(&inp, ex, sol, range)
    }

    /// ATT-25: the value `v` of having the ball at team-frame point `pt` for a player who may still lose it before he
    /// can play it on: a share `loss` of a turnover there is expected. For the receiver of a pass that is
    /// `att25_receiver_base_loss` + `att25_receiver_loss` × pressed, for the carrier `att25_pressed_loss` × pressed,
    /// with carrying under pressure valued by the pressure that remains at its end.
    fn secured(&self, v: f32, pt: Vec2, loss: f32) -> f32 {
        v - loss * (v + self.cost_at(pt))
    }

    /// Length (m) and duration (s) of a carry: knock-ons in space, short under pressure.
    fn carry(&self) -> (f32, f32) {
        let len = if self.pressure > 0.6 { 3.5 } else { 6.0 };
        (len, len / carry_speed(self.p) + 0.15)
    }

    /// ATT-02: turnover cost weighted by the third in which the ball is lost.
    fn cost_at(&self, team_pt: Vec2) -> f32 {
        risk_cost(team_pt, self.cfg)
    }
}

/// ATT-02: turnover cost at team-frame point `team_pt`, weighted by the third in which the ball is lost.
pub fn risk_cost(team_pt: Vec2, cfg: &SimConfig) -> f32 {
    let tc = &cfg.tactics;
    let w = match third_of(team_pt.x) {
        Third::Defensive => tc.att02_risk_weight_own_third,
        Third::Middle => tc.att02_risk_weight_middle_third,
        Third::Attacking => tc.att02_risk_weight_final_third,
    };
    turnover_cost(team_pt, &cfg.ai) * w
}

/// Evaluate all options and their choice probabilities.
pub fn evaluate(state: &MatchState, cfg: &SimConfig, plan: &BallParams, sit: Situation) -> Vec<OptionEval> {
    let p = &state.players[sit.carrier];
    let side = p.side;
    let fr = state.frame(side);
    let from_w = sit.ball.truncate();
    let brain = &p.beliefs;
    let mut ctx = Ctx {
        state,
        cfg,
        plan,
        sit,
        p,
        fr,
        from: fr.to_team(from_w),
        from_w,
        pressure: pressure_at(state, side, from_w),
        counter: brain.phase == TeamPhase::In(InPhase::CounterAttack),
        max_speed: if sit.throw_in {
            let tc = &cfg.tactics;
            tc.sp06_speed_min + (tc.sp06_speed_max - tc.sp06_speed_min) * norm(p.attrs.strength)
        } else {
            p.kin.max_kick_speed
        },
        scale: possession_value(fr.to_team(from_w), &cfg.ai) + cfg.ai.value_base,
        att: applied_attacking(&state.team(side).tactics, &p.attrs, p.instruction_familiarity(true), &cfg.tactics),
        cont_hold: 0.0,
        cont_carry: 0.0,
    };
    let mut opts = Vec::new();
    if sit.penalty {
        shots(&ctx, &mut opts);
        return opts;
    }
    passes(&ctx, &mut opts);
    if !sit.restart && !sit.gk_hands {
        let here = value_at(ctx.from, p.beliefs.opp_last, &cfg.ai);
        let mut proj = state.clone();
        ctx.cont_hold = ahead(&ctx, &mut proj, cfg.ai.onball_interval, here);
        if !sit.first_time {
            ctx.cont_carry = ahead(&ctx, &mut proj, ctx.carry().1, here);
        }
    }
    if sit.direct {
        shots(&ctx, &mut opts);
    }
    if !sit.restart && !sit.gk_hands {
        shots(&ctx, &mut opts);
        if !sit.first_time {
            dribbles(&ctx, &mut opts);
        }
        hold(&ctx, &mut opts);
        clearance(&ctx, &mut opts);
    }
    if sit.gk_hands {
        clearance(&ctx, &mut opts);
    }

    // ATT-04: nothing comfortable on and no pressure → dribble at the nearest opponent to bind him.
    let tc = &cfg.tactics;
    let comfortable = opts.iter().any(|o| matches!(o.kind, OptionKind::Pass { .. }) && o.p >= tc.att03_comfortable_p);
    if !comfortable && ctx.pressure < 0.3 {
        let nearest = state
            .squad(side.opponent())
            .iter()
            .min_by(|a, b| (a.pos - from_w).length().total_cmp(&(b.pos - from_w).length()).then(a.id.cmp(&b.id)));
        if let Some(o) = nearest {
            let to = (o.pos - from_w).normalize_or_zero();
            if let Some(best) = opts
                .iter_mut()
                .filter(|o| matches!(o.kind, OptionKind::Dribble { .. }))
                .max_by(|a, b| dir_of(a).dot(to).total_cmp(&dir_of(b).dot(to)))
            {
                best.u_opt += ctx.scale * tc.att04_bind_bonus;
                best.rules.push("ATT-04");
            }
        }
    }

    layer2_and_3(&ctx, &mut opts);
    opts
}

/// Look-ahead level 1 for the carrier (docs/player-brain.md 8.4: "the carrier is a brain too"): the best pass he
/// expects to have `t` s from now, beyond `here`, the value of the ball's location (≥ 0). Team-mates go towards the
/// offers he reads (`reading::seen`, so a team-mate he knows less he reads later), everybody else carries on with
/// his movement (`locomotion::carry_on`); `proj` is a copy of the state that takes those positions. A free carrier keeps what is on and
/// gains what his team-mates' movements open; defenders closing the lanes take it away. Real carriers keep the ball
/// until a movement is on or they are pressed.
fn ahead(ctx: &Ctx, proj: &mut MatchState, t: f32, here: f32) -> f32 {
    let (state, ai, me) = (ctx.state, &ctx.cfg.ai, ctx.p);
    for (q, out) in state.players.iter().zip(proj.players.iter_mut()) {
        if q.id == me.id {
            continue;
        }
        let offer = if q.side == me.side { super::reading::seen(state, ai, me.id, q).1 } else { None };
        let to = match offer {
            // At the pace of a positional move (`pace_position_s`, `pace_run_speed`), or faster if he already is.
            Some(o) => {
                let to = o - q.pos;
                let speed = q.vel.length().max((to.length() / ai.pace_position_s).min(ai.pace_run_speed));
                to.clamp_length_max(speed * t)
            }
            None => {
                let (pos, vel) = carry_on(q, t);
                out.vel = vel;
                pos - q.pos
            }
        };
        out.pos = clamp_to_pitch(q.pos + to, 0.0);
    }
    let c = Ctx { state: proj, ..ctx.clone() };
    let mut opts = Vec::new();
    passes(&c, &mut opts);
    (opts.iter().map(|o| o.u_opt).fold(f32::MIN, f32::max) - here).max(0.0)
}

/// Speed (m/s) at which `p` carries the ball.
fn carry_speed(p: &Player) -> f32 {
    let drib = effective(p.attrs.dribbling, AttrClass::Technical, p.confidence, p.energy);
    0.8 * p.kin.top_speed * (0.6 + 0.4 * drib)
}

fn dir_of(o: &OptionEval) -> Vec2 {
    match o.action {
        Action::Dribble { dir, .. } => dir,
        _ => Vec2::ZERO,
    }
}

/// LAW-01: does the passer judge team-mate `t` to be in an offside position? Against the second-last opponent as he
/// perceives the opponents (ATT-05: those out of view where his last scan saw them), with a judgement error
/// (`offside_judge_m`) fixed for this decision.
fn judged_offside(ctx: &Ctx, t: &Player) -> bool {
    let (state, p, fr, ai) = (ctx.state, ctx.p, ctx.fr, &ctx.cfg.ai);
    let mut xs: Vec<f32> = scan::opponents(state, p, ai).filter(|(_, _, o)| !o.sent_off).map(|(pos, _, _)| fr.to_team(pos).x).collect();
    xs.sort_by(|a, b| b.total_cmp(a));
    let line = xs.get(1).copied().unwrap_or(0.0).max(state.ball_in(p.side).x).max(0.0);
    let u1 = (SimRng::hashed_unit(p.decision_seed, &[b'o', t.id as u8]) as f32).max(1e-6);
    let u2 = SimRng::hashed_unit(p.decision_seed, &[b'f', t.id as u8]) as f32;
    let z = (-2.0 * u1.ln()).sqrt() * (std::f32::consts::TAU * u2).cos();
    let error = z * ai.offside_judge_m * (1.25 - 0.5 * norm(p.attrs.intelligence));
    fr.to_team(t.pos).x > line + error
}

fn passes(ctx: &Ctx, opts: &mut Vec<OptionEval>) {
    let (state, cfg, p, fr) = (ctx.state, ctx.cfg, ctx.p, ctx.fr);
    let ai = &cfg.ai;
    let tc = &cfg.tactics;
    let brain = &p.beliefs;
    for t in state.squad(p.side) {
        if t.id == p.id || t.sent_off {
            continue;
        }
        // LAW-01: not to a team-mate he judges to be in an offside position.
        if !ctx.sit.offside_exempt && judged_offside(ctx, t) {
            continue;
        }
        let d = (t.pos - ctx.from_w).length();
        if !(3.0..=ai.pass_max_distance.min(ctx.sit.max_pass)).contains(&d) {
            continue;
        }
        if !(ctx.sit.first_time || ctx.sit.restart || ctx.sit.gk_hands) && !in_vision(p, t.pos, ai) {
            continue;
        }
        let feet = clamp_to_pitch(t.pos + t.vel * (d / 16.0), 1.0);
        let arrival = (ai.pass_arrival_base + ai.pass_arrival_per_m * d).clamp(ai.pass_arrival_min, ai.pass_arrival_max);
        let mut specs = vec![(KickSpec::Ground { target: feet, arrival_speed: arrival }, feet, false, false)];
        // ATT-07: long passes may be lofted (BU-01: never a lofted back-pass to the own keeper); FT-02: a cross at any
        // length, flatter.
        let cross = |to: Vec2| is_cross(ctx.from, fr.to_team(to), true);
        if cross(feet) {
            specs.push((KickSpec::Lofted { target: feet, launch_angle: ai.cross_launch_angle, backspin: 6.0, sidespin: 0.0 }, feet, true, false));
        } else if d > tc.att07_long_min * 0.8 && !t.is_gk() {
            specs.push((KickSpec::Lofted { target: feet, launch_angle: ai.lofted_launch_angle, backspin: 6.0, sidespin: 0.0 }, feet, true, false));
        }
        // FT-02 / FT-03: a cross or a cut-back to the spot a box run offers, in the air or driven along the ground.
        if let (Movement::BoxRun, Some(o)) = super::reading::seen(state, ai, p.id, t)
            && t.plan.cue.in_possession
        {
            let spot = clamp_to_pitch(o, 1.5);
            if cross(spot) {
                specs.push((KickSpec::Lofted { target: spot, launch_angle: ai.cross_launch_angle, backspin: 6.0, sidespin: 0.0 }, spot, true, false));
            }
            specs.push((KickSpec::Ground { target: spot, arrival_speed: ai.pass_arrival_max }, spot, false, false));
        }
        // Through ball into the space ahead of a forward-moving teammate; to a runner in behind (ATT-18), into the
        // space his run offers, once the carrier has read the run (section 7).
        let tv = fr.to_team(t.vel);
        let run = match super::reading::seen(state, ai, p.id, t) {
            (Movement::RunInBehind, offer) if t.plan.cue.in_possession => offer,
            _ => None,
        };
        if !t.is_gk() && (run.is_some() || tv.x > 2.5 || (super::positioning::runs_in_behind(t.role) && brain.runs_on)) {
            let ahead_t = run.map_or(fr.to_team(t.pos) + Vec2::new(7.0, 0.0) + tv * 0.6, |o| fr.to_team(o));
            if ahead_t.x < HALF_LENGTH - 3.0 {
                let ahead = clamp_to_pitch(fr.to_world(ahead_t), 1.5);
                specs.push((KickSpec::Ground { target: ahead, arrival_speed: 4.0 }, ahead, false, true));
            }
        }
        // SP-06: a throw-in is thrown: every ball goes through the air, without spin.
        if ctx.sit.throw_in {
            specs = specs
                .into_iter()
                .filter(|s| !s.2)
                .map(|(_, target, _, through)| {
                    (KickSpec::Lofted { target, launch_angle: tc.sp06_angle, backspin: 0.0, sidespin: 0.0 }, target, true, through)
                })
                .collect();
        }
        for (spec, target, lofted, through) in specs {
            let Some(sol) = solve_kick(ctx.sit.ball, &spec, ctx.plan, ctx.max_speed) else {
                continue;
            };
            let Some((delay, foot)) = ctx.pass_delay(sol.vel) else {
                continue;
            };
            let spread = lofted.then(|| {
                let cross = is_cross(ctx.from, fr.to_team(target), true);
                ctx.landing_spread(&sol, foot, (target - ctx.sit.ball.truncate()).length(), cross)
            });
            let ev = evaluate_pass(state, cfg, ctx.plan, p.id, Some(t.id), ctx.sit.ball, &sol, target, delay, spread);
            let tt = fr.to_team(target);
            let mut rules = vec!["ATT-07"];
            // The ball is worth what it is where he gets it, and breaks the lines in front of that point (a ball into
            // space he meets on its way).
            let rt = fr.to_team(ev.recv_point);
            let mut v = value_at(rt, brain.opp_last, ai);
            let (n, last) = lines_broken(&brain.opp_units, ctx.from.x, rt.x);
            if n > 0 {
                v += ctx.scale * tc.att17_line_break_bonus * (n as f32 + if last { tc.att17_last_line_factor - 1.0 } else { 0.0 });
                rules.push("ATT-17");
            }
            // ATT-21 / FT-01: switch away from a crowded ball side once the carrier has drawn the pressure there.
            if (tt.y - ctx.from.y).abs() > 25.0 && tt.y.signum() != ctx.from.y.signum() && ctx.pressure >= tc.att21_min_pressure {
                let near = |c: Vec2| state.squad(p.side.opponent()).iter().filter(|o| (o.pos - c).length() < 20.0).count() as f32;
                if near(ctx.from_w) >= tc.att21_density_ratio * near(target).max(0.5) {
                    v += ctx.scale * ctx.att.switch_bonus;
                    rules.push("ATT-21");
                }
            }
            // ATT-03: the furthest comfortable pass.
            if ev.p_success >= tc.att03_comfortable_p {
                v += ctx.scale * tc.att03_forward_bonus_per_m * (tt.x - ctx.from.x).max(0.0);
                rules.push("ATT-03");
            }
            let forward = tt.x > ctx.from.x + 5.0;
            let backward = tt.x < ctx.from.x - 3.0;
            if forward {
                v += ctx.scale * tc.att01_forward_pass_bonus;
                rules.push("ATT-01");
            }
            if ev.pressed > 0.5 {
                rules.push("ATT-25");
            }
            // A knock-down after a header is a loose ball near the target, not the ball at it.
            let sec = ctx.secured(v, tt, ctx.att.receiver_base_loss + ctx.att.receiver_loss * ev.pressed);
            let mut u = (ev.p_success - ev.p_air * (1.0 - ai.aerial_value_factor)) * sec - (1.0 - ev.p_success) * ctx.cost_at(fr.to_team(ev.loss_point));
            if ctx.counter {
                if forward {
                    u += ctx.scale * tc.tr01_forward_bonus;
                } else if backward {
                    u -= ctx.scale * tc.tr01_backward_penalty;
                }
                rules.push("TR-01");
                // TR-02: the first pass goes away from the pressure into space: a pressed receiver keeps less of it
                // (ATT-25 in `secured`), the space is the possession value where he receives.
                if ev.pressed < 0.5 {
                    rules.push("TR-02");
                }
            }
            if t.is_gk() {
                rules.push("BU-01");
            }
            rules.push("BU-05");
            let kind = if lofted { KickKind::LoftedPass } else { KickKind::GroundPass };
            let label = format!(
                "{} → {} {}",
                if ctx.sit.throw_in {
                    "throw"
                } else if lofted {
                    "lofted pass"
                } else if through {
                    "through ball"
                } else {
                    "pass"
                },
                t.number,
                t.name
            );
            opts.push(OptionEval {
                label,
                kind: OptionKind::Pass { safe: !forward, through, lofted },
                action: Action::Kick(KickPlan {
                    spec,
                    kind,
                    receiver: Some(t.id),
                    target,
                    arrival_t: sol.time,
                    xg: 0.0,
                    speed: sol.vel.length(),
                    foot,
                }),
                p: ev.p_success,
                u_opt: u,
                instr: 0.0,
                pers: 0.0,
                prob: 0.0,
                pressed: ev.pressed,
                rules,
            });
        }
    }
}

fn shots(ctx: &Ctx, opts: &mut Vec<OptionEval>) {
    let (state, cfg, p, fr) = (ctx.state, ctx.cfg, ctx.p, ctx.fr);
    let ai = &cfg.ai;
    let from = ctx.from;
    if from.x < HALF_LENGTH - ai.max_shot_distance || goal_angle(from) < 0.12 {
        return;
    }
    let tc = &cfg.tactics;
    // SP-03: a direct free kick goes over the wall, whose blocking the xG ignores.
    let xg_of = |h: ShotHow| {
        let x = shot_xg_parts(state, cfg, p.id, from, h);
        if ctx.sit.direct { x.total * x.basic / x.after_blockers.max(1e-6) * tc.sp03_xg } else { x.total }
    };
    if xg_of(ShotHow::default()) < 0.005 {
        return;
    }
    let gk_y = state.squad(p.side.opponent()).iter().find(|o| o.is_gk()).map(|g| fr.to_team(g.pos).y).unwrap_or(0.0);
    // Far corner = the side away from the keeper.
    let far = if gk_y > 0.0 { -1.0 } else { 1.0 };
    let post = GOAL_HALF_WIDTH - 0.5;
    let flair = 0.5 * (norm(p.attrs.flair) + norm(p.attrs.long_shots));
    let mut variants = vec![
        ("shot: placed far corner", Vec3::new(HALF_LENGTH, far * post, 0.35), 0.8, false, 1.0),
        ("shot: power", Vec3::new(HALF_LENGTH, far * post * 0.6, 1.2), 0.97, false, 0.92),
    ];
    if flair > 0.55 && from.x < HALF_LENGTH - 12.0 {
        variants.push(("shot: curled far corner", Vec3::new(HALF_LENGTH, far * post, 1.6), 0.85, true, 1.0));
    }
    if ctx.sit.direct {
        variants = vec![("free kick: over the wall", Vec3::new(HALF_LENGTH, far * post, tc.sp03_height), tc.sp03_speed, true, 1.0)];
    }
    if ctx.sit.penalty {
        // SP-04: the kicker mixes his side [ACAD-PK02]: his natural side (a right-footer's left), the middle, the other.
        let u = SimRng::hashed_unit(p.decision_seed, b"penalty") as f32;
        let natural = if p.attrs.preferred_foot == Foot::Right { 1.0 } else { -1.0 };
        let (label, y, z) = if u < tc.sp04_natural {
            ("penalty: natural side", natural * tc.sp04_aim_m, tc.sp04_height)
        } else if u < tc.sp04_natural + tc.sp04_middle {
            ("penalty: middle", 0.0, 1.0)
        } else {
            ("penalty: other side", -natural * tc.sp04_aim_m, tc.sp04_height)
        };
        variants = vec![(label, Vec3::new(HALF_LENGTH, y, z), tc.sp04_speed, false, 1.0)];
    }
    let miss_cost = ctx.cost_at(crate::pitch::TeamFrame::opp_goal() - Vec2::new(6.0, 0.0));
    let strong = p.attrs.preferred_foot;
    let weak = if strong == Foot::Right { Foot::Left } else { Foot::Right };
    let ex = &cfg.execution;
    for (label, tgt, speed_f, curled, placement) in variants {
        let target_w = ctx.fr.to_team3(tgt);
        let side_curl = if curled {
            let to_t = Vec2::new(tgt.x, tgt.y) - from;
            let to_gk = Vec2::new(HALF_LENGTH, gk_y) - from;
            // Bend the ball away from the keeper (+z spin curls to the left of travel), sign in world frame.
            let left = to_gk.perp_dot(to_t) > 0.0;
            12.0 * if left { 1.0 } else { -1.0 } * ctx.fr.dir.signum().powi(0)
        } else {
            0.0
        };
        let speed = ctx.max_speed * speed_f;
        let spec = KickSpec::Through { target: target_w, speed, sidespin: side_curl, topspin: 0.0 };
        if solve_kick(ctx.sit.ball, &spec, ctx.plan, ctx.max_speed).is_none() {
            continue;
        }
        let dir = (target_w.truncate() - ctx.from_w).normalize_or_zero();
        // Strong foot, or the weak one when it strikes sooner: the ball need not be shifted onto the strong foot.
        let first_time = ctx.sit.first_time;
        let how = |foot| {
            let delay = ctx.kick_delay(dir, speed, Some(foot))?;
            let body_excess = if first_time { setup_turn(p.facing, dir, Some(foot), speed, ctx.max_speed, ex) } else { 0.0 };
            Some(ShotHow { foot: Some(foot), body_excess, first_time, lead: ctx.sit.arrives_in + delay })
        };
        let Some(strong_how) = how(strong) else {
            continue;
        };
        let mut feet = vec![strong_how];
        if let Some(w) = how(weak)
            && (w.lead < strong_how.lead - 0.02 || w.body_excess < strong_how.body_excess - 0.02)
        {
            feet.push(w);
        }
        for h in feet {
            let xg = xg_of(h);
            // Set-up time lets defenders close in; the time until a first-time ball arrives is priced in the xG.
            let p_goal = xg * placement * (-(h.lead - ctx.sit.arrives_in) / ex.setup_shot_decay_s).exp();
            // ATT-01 tie-break as a share of the shot's own value: a share of the local scale, which near the box is
            // two to three times a long shot's xG, made it 30 % of a 25 m shot's worth.
            let u = p_goal * (1.0 + cfg.tactics.att01_shot_bonus) - (1.0 - p_goal) * miss_cost;
            let foot_label = if h.foot == Some(strong) { "" } else { ", weak foot" };
            opts.push(OptionEval {
                label: format!("{label} (xG {p_goal:.2}{foot_label})"),
                kind: OptionKind::Shot { curled },
                action: Action::Kick(KickPlan {
                    spec,
                    kind: KickKind::Shot,
                    receiver: None,
                    target: target_w.truncate(),
                    arrival_t: (tgt.truncate() - from).length() / speed,
                    xg: p_goal,
                    speed,
                    foot: h.foot,
                }),
                p: p_goal,
                u_opt: u,
                instr: 0.0,
                pers: 0.0,
                prob: 0.0,
                pressed: 0.0,
                rules: vec!["ATT-01"],
            });
        }
    }
}

/// How far ahead (m) a dribbler pushes the ball: knock-ons in space, close control under pressure.
pub fn dribble_lead(pressure: f32) -> f32 {
    (2.5 - 2.0 * pressure).clamp(0.6, 2.5)
}

/// Probability that the carrier keeps the ball while dribbling `len` metres in world direction `dir`, whether an
/// opponent stands in the way (a take-on) and how pressed he is at the end (ATT-25).
fn dribble_keep(ctx: &Ctx, dir: Vec2, len: f32) -> (f32, bool, f32) {
    let (state, cfg, p) = (ctx.state, ctx.cfg, ctx.p);
    let v_carry = carry_speed(p);
    let mut keep = 1.0;
    let mut take_on = false;
    let mut opp_end = f32::MAX;
    for o in state.squad(p.side.opponent()) {
        let rel = o.pos - ctx.from_w;
        if rel.length() > len + o.kin.top_speed * (len / v_carry + 0.5) + 2.0 {
            continue;
        }
        if rel.length() < 4.0 && rel.normalize_or_zero().dot(dir) > 40f32.to_radians().cos() {
            take_on = true;
        }
        let mut best: f32 = 0.0;
        for k in 1..=3 {
            let s = len * k as f32 / 3.0;
            let pt = ctx.from_w + dir * s;
            let tc_ = s / v_carry + 0.15;
            let to = time_to_reach(o, pt, o.kin.tackle_reach, true);
            best = best.max(logistic((tc_ - to) / cfg.ai.race_sigma));
            if k == 3 {
                opp_end = opp_end.min(to);
            }
        }
        keep *= 1.0 - best * tackle_win_prob(o, p, false, &cfg.duel);
    }
    let t_end = len / v_carry + 0.15;
    (keep, take_on, pressed_at(t_end, opp_end, cfg))
}

fn dribbles(ctx: &Ctx, opts: &mut Vec<OptionEval>) {
    let cfg = ctx.cfg;
    let tc = &cfg.tactics;
    let len = ctx.carry().0;
    for deg in [0.0f32, 35.0, -35.0, 70.0, -70.0, 115.0, -115.0] {
        let dir_t = Vec2::from_angle(deg.to_radians());
        let end = ctx.from + dir_t * len;
        if end.x.abs() > HALF_LENGTH - 1.0 || end.y.abs() > HALF_WIDTH - 1.0 {
            continue;
        }
        // ATT-02: no risk in the own third; never carry the ball back towards the own goal inside the own box.
        if dir_t.x < 0.0 && end.x < -HALF_LENGTH + PENALTY_AREA_DEPTH && end.y.abs() < PENALTY_AREA_HALF_WIDTH {
            continue;
        }
        let dir = ctx.fr.to_world(dir_t);
        let (keep, take_on, pressed) = dribble_keep(ctx, dir, len);
        // ATT-06: the continuation (the best pass at the end of the carry) is only worth having while free.
        let mut v = value_at(end, ctx.p.beliefs.opp_last, &cfg.ai) + ctx.cont_carry * (1.0 - pressed);
        let mut rules = Vec::new();
        if dir_t.x > 0.7 {
            v += ctx.scale * tc.att01_forward_dribble_bonus;
            rules.push("ATT-01");
        }
        if pressed > 0.5 {
            rules.push("ATT-25");
        }
        let mut u = keep * ctx.secured(v, end, ctx.att.pressed_loss * pressed) - (1.0 - keep) * ctx.cost_at(ctx.from);
        if ctx.counter && dir_t.x > 0.7 {
            u += ctx.scale * tc.tr01_forward_bonus;
            rules.push("TR-01");
        }
        opts.push(OptionEval {
            label: format!("{} {:+.0}°", if take_on { "take on" } else { "dribble" }, deg),
            kind: OptionKind::Dribble { take_on },
            action: Action::Dribble { dir, lead: dribble_lead(ctx.pressure) },
            p: keep,
            u_opt: u,
            instr: 0.0,
            pers: 0.0,
            prob: 0.0,
            pressed,
            rules,
        });
    }
}

fn hold(ctx: &Ctx, opts: &mut Vec<OptionEval>) {
    let (cfg, p) = (ctx.cfg, ctx.p);
    let shield = 0.7 * challenge_strength(p, &cfg.duel) + 0.3 * norm(p.attrs.dribbling);
    let keep = 1.0 - ctx.pressure * cfg.ai.hold_loss * (1.0 - 0.6 * shield);
    // ATT-25: still pressed when the next decision comes.
    let opp_t = ctx
        .state
        .squad(p.side.opponent())
        .iter()
        .map(|o| time_to_reach(o, ctx.from_w, o.kin.tackle_reach, true))
        .fold(f32::MAX, f32::min);
    let pressed = pressed_at(cfg.ai.onball_interval, opp_t, cfg);
    // ATT-06: under pressure the ball is played now, so the continuation (the best pass kept for later) is only
    // worth having while free. Credited at every decision, it kept a pressed carrier deferring the pass step after
    // step (each one cheap on its own) until the tackle came (`hold_cal` example).
    let v = value_at(ctx.from, ctx.p.beliefs.opp_last, &cfg.ai) - 0.0005 + ctx.cont_hold * (1.0 - pressed);
    let mut u = keep * ctx.secured(v, ctx.from, ctx.att.pressed_loss * pressed) - (1.0 - keep) * ctx.cost_at(ctx.from);
    let mut rules = vec!["ATT-06"];
    if pressed > 0.5 {
        rules.push("ATT-25");
    }
    if ctx.counter {
        u -= ctx.scale * cfg.tactics.tr01_backward_penalty;
        rules.push("TR-01");
    }
    opts.push(OptionEval {
        label: if ctx.sit.first_time { "control".into() } else { "hold / shield".into() },
        kind: OptionKind::Hold,
        action: Action::Hold,
        p: keep,
        u_opt: u,
        instr: 0.0,
        pers: 0.0,
        prob: 0.0,
        pressed,
        rules,
    });
}

fn clearance(ctx: &Ctx, opts: &mut Vec<OptionEval>) {
    let (state, cfg, p, fr) = (ctx.state, ctx.cfg, ctx.p, ctx.fr);
    let deep = ctx.from.x < -HALF_LENGTH + PENALTY_AREA_DEPTH + 10.0;
    if !(ctx.sit.gk_hands || (deep && ctx.pressure > 0.5)) {
        return;
    }
    let side_y = if ctx.from.y >= 0.0 { 1.0 } else { -1.0 };
    for (label, tgt) in [("clear wide", Vec2::new(ctx.from.x + 45.0, side_y * 24.0)), ("clear long", Vec2::new(ctx.from.x + 55.0, 0.0))] {
        let tgt = Vec2::new(tgt.x.min(HALF_LENGTH - 10.0), tgt.y);
        let target = fr.to_world(tgt);
        let spec = KickSpec::Lofted { target, launch_angle: 0.6, backspin: 4.0, sidespin: 0.0 };
        let Some(sol) = solve_kick(ctx.sit.ball, &spec, ctx.plan, ctx.max_speed) else {
            continue;
        };
        let Some((delay, foot)) = ctx.pass_delay(sol.vel) else {
            continue;
        };
        let ev = evaluate_pass(state, cfg, ctx.plan, p.id, None, ctx.sit.ball, &sol, target, delay, None);
        let u = ev.p_success * possession_value(tgt, &cfg.ai) - (1.0 - ev.p_success) * ctx.cost_at(tgt);
        opts.push(OptionEval {
            label: label.into(),
            kind: OptionKind::Clear,
            action: Action::Kick(KickPlan {
                spec,
                kind: KickKind::Clearance,
                receiver: None,
                target,
                arrival_t: sol.time,
                xg: 0.0,
                speed: sol.vel.length(),
                foot,
            }),
            p: ev.p_success,
            u_opt: u,
            instr: 0.0,
            pers: 0.0,
            prob: 0.0,
            pressed: 0.0,
            rules: vec!["ATT-02"],
        });
    }
}

/// Layer 2 (instruction biases) and layer 3 (personality, softmax temperature).
fn layer2_and_3(ctx: &Ctx, opts: &mut [OptionEval]) {
    let (state, cfg, p) = (ctx.state, ctx.cfg, ctx.p);
    let team = state.team(p.side);
    let tac = team.tactics;
    let pi = resolve_player(&p.instructions);
    let ib = cfg.ai.instruction_bias * ctx.scale;
    let adh = adherence(&p.attrs, p.instruction_familiarity(true), false);
    let adh_drib = adherence(&p.attrs, p.instruction_familiarity(true), pi.dribble_freq < 0.0);
    let near_goal = (1.0 - ((crate::pitch::TeamFrame::opp_goal() - ctx.from).length() - 16.5) / 25.0).clamp(0.0, 1.0);
    let v_here = value_at(ctx.from, p.beliefs.opp_last, &cfg.ai);
    let best_self = opts.iter().filter(|o| o.kind.is_self()).map(|o| o.u_opt).fold(f32::MIN, f32::max);
    for o in opts.iter_mut() {
        // Layer 2.
        let mut instr = 0.0;
        match (o.kind, o.action) {
            (OptionKind::Pass { safe, .. }, Action::Kick(k)) => {
                let len = (k.target - ctx.from_w).length();
                if len > cfg.tactics.att07_long_min {
                    instr += ib * tac.directness * adh;
                } else if len < cfg.tactics.att07_short_max {
                    instr -= 0.5 * ib * tac.directness * adh;
                }
                if safe {
                    instr -= 0.5 * ib * tac.tempo * adh;
                }
            }
            (OptionKind::Dribble { .. }, Action::Dribble { dir, .. }) => {
                instr += ib * pi.dribble_freq * adh_drib;
                if ctx.fr.to_team(dir).x > 0.7 {
                    instr += 0.25 * ib * tac.tempo * adh;
                }
            }
            (OptionKind::Hold, _) => instr -= ib * tac.tempo * adh,
            _ => {}
        }
        o.instr = instr;
        // Layer 3: personality.
        let better_placed = match o.action {
            Action::Kick(k) if k.receiver.is_some() => {
                value_at(ctx.fr.to_team(k.target), p.beliefs.opp_last, &cfg.ai) > v_here && o.u_opt > best_self
            }
            _ => false,
        };
        let bctx = BiasCtx {
            near_goal,
            pressure: ctx.pressure,
            confidence: p.confidence,
            p: o.p,
            cost: ctx.cost_at(ctx.from),
            better_placed,
        };
        o.pers = bias(&p.attrs, o.kind, &bctx, ctx.scale, &cfg.personality);
        if norm(p.attrs.flair) > 0.6 && matches!(o.kind, OptionKind::Dribble { take_on: true } | OptionKind::Pass { through: true, .. }) {
            o.rules.push("ATT-23");
        }
    }
    softmax(opts, decision_temperature(p, cfg, ctx.scale, ctx.pressure));
}

/// Softmax temperature of carrier `p` at local value scale `scale` under `pressure`: instinct, scaled by confidence
/// and energy as tactical reliability is off the ball, on a concave curve (`temperature_exp`): a 100 at full
/// confidence and energy takes the optimum (`temperature_best` 0), and the quality of decisions rises slowly through
/// the middle of the scale and fast near the top. Pressure raises the temperature, less the more composed he is.
pub fn decision_temperature(p: &Player, cfg: &SimConfig, scale: f32, pressure: f32) -> f32 {
    let ai = &cfg.ai;
    let skill = norm(p.attrs.instinct)
        * state_factor(ai.reliability_confidence_floor, p.confidence)
        * state_factor(ai.reliability_energy_floor, p.energy);
    (ai.temperature_worst + (ai.temperature_best - ai.temperature_worst) * skill.powf(ai.temperature_exp))
        * scale
        * (1.0 + pressure * (1.0 - norm(p.attrs.composure)))
}

/// The shot variants (placed, power, curled) are one decision, to shoot, taken in different ways, so together they
/// get the weight of a single alternative. Otherwise listing more ways to shoot would make shooting more likely.
/// Likewise the ways to pass to one team-mate (to feet, lofted, into his path) are one decision, to pass to him:
/// together they get the weight of the best of them, shared in proportion to their own weights. Otherwise a far
/// team-mate, who can also be reached with a lofted ball, would count twice against a near one. So do the dribble
/// directions, the ways to carry it: otherwise the number of directions tried would make carrying more likely.
/// And the passes to the different team-mates are one decision too, to pass (ATT-01: the carrier's choice is between
/// shooting, passing, dribbling and keeping it [USS-D]; whom to pass to comes after): together they get the weight of
/// the best of them, shared among the team-mates in proportion to their own. Otherwise every team-mate on offer
/// would make a release more likely than keeping the ball, however much keeping it is worth.
pub fn softmax(opts: &mut [OptionEval], temp: f32) {
    let max = opts.iter().map(|o| o.utility()).fold(f32::MIN, f32::max);
    let shots = opts.iter().filter(|o| matches!(o.kind, OptionKind::Shot { .. })).count().max(1) as f32;
    let w: Vec<f32> = opts.iter().map(|o| ((o.utility() - max) / temp.max(1e-6)).exp()).collect();
    let group = |o: &OptionEval| match (o.kind, o.action) {
        (OptionKind::Pass { .. }, Action::Kick(k)) => k.receiver.map(|r| r as i64),
        (OptionKind::Dribble { .. }, _) => Some(-1),
        _ => None,
    };
    let mut sum = 0.0;
    let mut prob: Vec<f32> = (0..opts.len())
        .map(|i| {
            let mut prob = w[i];
            if matches!(opts[i].kind, OptionKind::Shot { .. }) {
                prob /= shots;
            }
            if let Some(g) = group(&opts[i]) {
                let members = (0..opts.len()).filter(|&j| group(&opts[j]) == Some(g));
                let (best, total) = members.fold((0.0f32, 0.0f32), |(b, s), j| (b.max(w[j]), s + w[j]));
                prob *= best / total.max(1e-12);
            }
            prob
        })
        .collect();
    let passes = (0..opts.len()).filter(|&i| matches!(group(&opts[i]), Some(r) if r >= 0));
    let (best, total) = passes.clone().fold((0.0f32, 0.0f32), |(b, s), i| (b.max(w[i]), s + prob[i]));
    for i in passes {
        prob[i] *= best / total.max(1e-12);
    }
    for (o, prob) in opts.iter_mut().zip(prob) {
        o.prob = prob;
        sum += prob;
    }
    for o in opts.iter_mut() {
        o.prob /= sum.max(1e-12);
    }
}

/// Decide an action. Returns the action and the evaluated options (for the viewer).
///
/// The softmax is sampled with the Gumbel-max trick, and each option's Gumbel draw is fixed by the carrier's
/// `decision_seed` and the option itself for `decision_noise_hold_s`. A misjudgement persists while he is on the ball
/// instead of being drawn afresh at every re-decision (every `onball_interval`), which would let any poor option, a
/// 30 m shot say, win sooner or later. It is drawn afresh when the pressure on him moves to another of
/// `decision_noise_pressure_bands`: a defender closing in (or dropping off) is a new situation, judged anew. A single
/// decision still follows the softmax probabilities.
pub fn decide(state: &MatchState, cfg: &SimConfig, plan: &BallParams, sit: Situation) -> (Action, Vec<OptionEval>, usize) {
    let opts = evaluate(state, cfg, plan, sit);
    if opts.is_empty() {
        return (Action::Hold, opts, 0);
    }
    if sit.corner
        && let Some(i) = super::set_piece::corner_delivery(state, &sit, &opts)
    {
        return (opts[i].action, opts, i);
    }
    let p = &state.players[sit.carrier];
    let spell = ((state.time - p.control_since).max(0.0) / cfg.ai.decision_noise_hold_s) as u64;
    let bands = cfg.ai.decision_noise_pressure_bands as f32;
    let band = if bands > 0.0 { (pressure_at(state, p.side, sit.ball.truncate()) * bands).min(bands - 1.0) as u64 } else { 0 };
    let seed = p.decision_seed ^ spell.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ band.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    let score = |o: &OptionEval| {
        // The option's identity: its label without the numbers that change from one decision to the next.
        let key = o.label.split(" (").next().unwrap_or(&o.label);
        let g = -(-SimRng::hashed_unit(seed, key.as_bytes()).ln()).ln();
        (o.prob as f64).ln() + g
    };
    let i = (0..opts.len()).max_by(|&a, &b| score(&opts[a]).total_cmp(&score(&opts[b]))).unwrap_or(0);
    (opts[i].action, opts, i)
}

/// ATT-06: is the receiver under pressure at the reception point, as he perceives the opponents (ATT-05 scanning)?
pub fn receiver_pressured(state: &MatchState, cfg: &SimConfig, receiver: PlayerId, point: Vec2, arrival_t: f32) -> bool {
    let p = &state.players[receiver];
    scan::opponents(state, p, &cfg.ai).any(|(pos, vel, o)| {
        time_to_reach_from(&o.kin, pos, vel, point, 1.0, pass_reaction(o, 0.0, &cfg.player)) < arrival_t + cfg.tactics.att06_pressure_time_s
    })
}

/// ATT-06 direction of the controlling touch: turn towards goal when free, away from pressure otherwise, from the
/// nearest opponent as he perceives him (ATT-05).
pub fn receive_dir(state: &MatchState, cfg: &SimConfig, receiver: PlayerId, point: Vec2, pressured: bool) -> Vec2 {
    let p = &state.players[receiver];
    let fr = state.frame(p.side);
    let forward = fr.to_world(Vec2::X);
    let nearest = scan::opponents(state, p, &cfg.ai)
        .min_by(|a, b| (a.0 - point).length().total_cmp(&(b.0 - point).length()).then(a.2.id.cmp(&b.2.id)));
    let away = nearest.map(|(pos, ..)| (point - pos).normalize_or_zero()).unwrap_or(forward);
    let mut dir = if pressured { away } else { (forward + 0.6 * away).normalize_or(forward) };
    // Don't touch it out of play.
    if (point.y + dir.y * 3.0).abs() > HALF_WIDTH - 1.0 {
        dir.y = -dir.y;
    }
    if (point.x + dir.x * 3.0).abs() > HALF_LENGTH - 1.0 {
        dir.x = -dir.x;
    }
    dir
}
