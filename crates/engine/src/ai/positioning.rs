//! Off-ball positioning (layers 1 + 2, with layer-3 noise). Out of possession the duty in his beliefs decides
//! the target directly (pressure, cover, marking, recovery) or by scoring candidate points around the anchor;
//! in possession every player scores candidate points. **Every score term is a catalog principle.**

use glam::Vec2;

use super::analysis::{gk01_depth, possession_value};
use super::deviation::{noise_sigma, reliability};
use super::reading;
use super::on_ball::receiver_pressured;
use super::reach::time_to_reach;
use super::shape::{anchor, slot_point};
use super::brain::{AttDuty, Beliefs, DefDuty, InPhase, OutPhase, TeamPhase, box04_weight};
use super::{movement, offer};
use crate::config::{AiConfig, SimConfig, TacticsConfig};
use crate::ball::BallMode;
use crate::match_state::{MatchState, side_of};
use crate::pitch::{GOAL_HALF_WIDTH, HALF_LENGTH, HALF_WIDTH, PENALTY_AREA_HALF_WIDTH, Side, TeamFrame, Third, clamp_to_pitch, is_cross, third_of};
use crate::player::{Intent, KickKind, Player, PlayerId, norm};
use crate::rng::SimRng;
use crate::team::{Channel, DefendingTactics, Movement, Role, adherence, applied_attacking, applied_defending, resolve_player};

/// Result of an off-ball decision (world frame).
#[derive(Clone, Debug)]
pub struct Move {
    pub target: Vec2,
    pub urgency: f32,
    /// Pacing (M6, `Player::pace_s`): the time (s) in which he closes the gap to his target, before fatigue and work
    /// rate (`pace_time`); 0 = as fast as `urgency` allows. A paced movement goes at most at `AiConfig::pace_run_speed`.
    pub pace_s: f32,
    pub brake: bool,
    /// Velocity of a moving target (`Player::target_vel`).
    pub target_vel: Vec2,
    pub look_at: Vec2,
    pub intent: Intent,
    pub rules: Vec<&'static str>,
    pub label: &'static str,
    pub anchor: Vec2,
    /// Apply layer-3 positional drift to this target.
    pub drifts: bool,
    /// Committed positioning point for the next decision (see `Player::commit`).
    pub commit: Option<(Vec2, &'static str)>,
    /// Positional play (shape, balance, support): small corrections within `AiConfig::settle_radius` are ignored.
    pub settles: bool,
    /// IND-01: an engaged first defender mirrors the carrier, re-reading him at his reaction time instead of the
    /// off-ball decision interval.
    pub tracks: bool,
    /// Movement model: the kind of movement, and whether it was chosen afresh (a trigger or the periodic check) rather
    /// than followed.
    pub kind: Movement,
    pub chose: bool,
    /// The decision belongs to the movement model (its plan is kept, and he re-reads at his reaction time).
    pub moving: bool,
    /// In possession: the point he offers the ball at (world frame).
    pub offer: Option<Vec2>,
    /// Chosen for the receiver of the pass in flight (look-ahead).
    pub projected: bool,
    /// Layer 3: the seed of his misjudgement (`movement::Plan::noise`).
    pub noise: u64,
    /// In possession: the movement the concept prescribes, when it is not his (`movement::Plan::concept`).
    pub concept: Option<(Movement, Vec2)>,
}

/// Layer 3: the committed point of the same kind of positioning, carried along with the anchor.
fn committed(p: &Player, kind: &str, anchor_t: Vec2) -> Option<Vec2> {
    p.commit.filter(|c| c.1 == kind).map(|c| anchor_t + c.0)
}

/// Layer 3: players stick to a chosen point unless another is better by `commit_cost` per metre of change.
fn switch_cost(kept: Option<Vec2>, c: Vec2, ai: &AiConfig) -> f32 {
    kept.map_or(0.0, |k| ai.commit_cost * (c - k).length())
}

/// DEF-02: cover point behind and inside the first defender (team frame).
pub fn cover_point(first: Vec2, ball: Vec2, tc: &TacticsConfig) -> Vec2 {
    let goal = TeamFrame::own_goal();
    let to_goal = (goal - ball).normalize_or(Vec2::NEG_X);
    // Behind the pressure: the first defender while he is goal-side; once he is beaten, the point he was containing
    // from, so the cover stays where he can support (DEF-02).
    let first = if (first - goal).length() < (ball - goal).length() { first } else { ball + to_goal * tc.ind01_contain_distance };
    let angle = tc.def02_cover_angle_deg.to_radians();
    // Rotate towards the centre line (inside).
    let inside = if ball.y > 0.0 { -1.0 } else { 1.0 };
    let a = Vec2::from_angle(angle).rotate(to_goal);
    let b = Vec2::from_angle(-angle).rotate(to_goal);
    let dir = if a.y * inside > b.y * inside { a } else { b };
    clamp_to_pitch(first + dir * tc.def02_cover_distance, 0.5)
}

/// IND-01/IND-02: contain point goal-side of the carrier at `dist`, shifted to block the side named by DEF-07.
pub fn contain_point(carrier: Vec2, dist: f32, channel: Channel, tc: &TacticsConfig) -> Vec2 {
    let to_goal = (TeamFrame::own_goal() - carrier).normalize_or(Vec2::NEG_X);
    let inside = Vec2::new(0.0, if carrier.y > 0.0 { -1.0 } else { 1.0 });
    // Channel Wide: block inside (show outside); Inside: block outside.
    let block = if channel == Channel::Wide { inside } else { -inside };
    carrier + to_goal * dist + block * tc.ind02_show_offset
}

/// BOX-01/02: is a carrier at team-frame `ball` within shooting range of the own goal?
pub fn in_shooting_range(ball: Vec2, tc: &TacticsConfig) -> bool {
    (ball - TeamFrame::own_goal()).length() < tc.box01_range
}

/// BOX-02: the cover's point on the shooting line, `block_distance` from the ball towards the goal centre.
pub fn shot_block_point(ball: Vec2, d: &DefendingTactics) -> Vec2 {
    let to_goal = TeamFrame::own_goal() - ball;
    ball + to_goal.normalize_or(Vec2::NEG_X) * d.block_distance.min(to_goal.length() - 0.5).max(0.5)
}

/// IND-04: marking point goal-side of the opponent, closer near goal, slightly towards the ball.
pub fn marking_point(opp: Vec2, ball: Vec2, tc: &TacticsConfig) -> Vec2 {
    let goal = TeamFrame::own_goal();
    let d = (tc.ind04_distance_per_m * (opp - goal).length()).clamp(tc.ind04_min, tc.ind04_max);
    opp + (goal - opp).normalize_or(Vec2::NEG_X) * d + (ball - opp).normalize_or_zero() * 0.5
}

/// IND-06: point on the ball–own-goal line at depth `x`; centre-backs within the goal width.
pub fn recovery_point(ball: Vec2, x: f32, centre_back: bool, tc: &TacticsConfig) -> Vec2 {
    let goal = TeamFrame::own_goal();
    let x = x.min(ball.x - 1.0).max(goal.x + 1.0);
    let u = ((ball.x - x) / (ball.x - goal.x).max(0.1)).clamp(0.0, 1.0);
    let mut p = ball + (goal - ball) * u;
    if centre_back {
        p.y = p.y.clamp(-tc.ind06_goal_half_width, tc.ind06_goal_half_width);
    }
    p
}

/// IND-06 against an opponent's pass in flight (not a shot): its reception point (world frame) and the time until it
/// gets there (s). Like the first defender's (PRS-02), a recovery run is judged from where the ball will be played
/// on, not from the ball on its way there: a pass at 15 m/s is not an attack at 15 m/s.
pub fn opponent_pass_in_flight(state: &MatchState, side: Side) -> Option<(Vec2, f32)> {
    state
        .flight
        .filter(|f| side_of(f.kicker) != side && f.kind != KickKind::Shot)
        .map(|f| (f.target, (f.arrival_t - (state.time - f.start_t)).max(0.0)))
}

/// IND-06 with pacing (M6): the first point on the ball–goal line, from his recovery point `rp` towards goal, that he
/// reaches before the ball, at `ball` in `t0` s and coming towards goal at `ball_speed` from there, does; and the speed
/// (m/s) that gets him there just in time. A player far ahead of the ball runs back to a deeper point at the pace it
/// allows instead of sprinting at a point the ball has passed before he can get there; against a slow build-up he
/// jogs, against a counter he sprints.
#[allow(clippy::too_many_arguments)]
fn recovery_run(p: &Player, pos: Vec2, ball: Vec2, t0: f32, rp: Vec2, ball_speed: f32, centre_back: bool, tc: &TacticsConfig) -> (Vec2, f32) {
    let line = TeamFrame::own_goal() - ball;
    let u0 = ((rp.x - ball.x) / line.x.min(-0.1)).clamp(0.0, 1.0);
    let mut out = (rp, 0.0);
    for k in 0..=10 {
        let u = u0 + (1.0 - u0) * k as f32 / 10.0;
        let mut q = ball + line * u;
        if centre_back {
            q.y = q.y.clamp(-tc.ind06_goal_half_width, tc.ind06_goal_half_width);
        }
        let t_ball = (t0 + (q - ball).length() / ball_speed).max(0.5);
        let d = (q - pos).length();
        out = (q, d / t_ball);
        if d <= 0.9 * p.kin.top_speed * t_ball {
            break;
        }
    }
    out
}

/// ATT-18: a role that runs in behind as one of its main movements (prior at least 0.5).
pub fn runs_in_behind(role: Role) -> bool {
    role.prior(Movement::RunInBehind) >= 0.5
}

/// Pacing (M6): the time in which `p` closes the gap to a target of base pace `base` (`Move::pace_s`): longer as he
/// tires (`pace_fatigue` · (1 − energy)), shorter with work rate (± `pace_work_rate` / 2 around the middle).
pub fn pace_time(p: &Player, ai: &AiConfig, base: f32) -> f32 {
    base * (1.0 + ai.pace_fatigue * (1.0 - p.energy)) * (1.0 + ai.pace_work_rate * (0.5 - norm(p.attrs.work_rate)))
}

/// Positional play: walk or jog back into shape when close, run when far off (scaled by work rate).
fn work_urgency(dist: f32, work_rate: f32, ai: &AiConfig) -> f32 {
    ((dist / ai.position_full_speed_dist).clamp(ai.position_min_urgency, 0.85) * (0.8 + 0.2 * norm(work_rate))).clamp(0.05, 1.0)
}

/// ATT-24: how the intended receiver of a pass in flight moves to take it at `point` at time `t`: (target, target
/// velocity, brake). A ball along the ground he takes on its line: he steers onto the line and stops across it, and
/// moves along it at the speed that brings him to the point with the ball (towards the passer when he meets it).
/// Steering at the point itself, a receiver coming from the side ran through the line before the ball got there and
/// missed it. A ball in the air, or one already past him, he runs to the point: through it only when he needs all
/// his speed to be there in time, otherwise he arrives and stops there.
pub fn receive_steering(state: &MatchState, p: &Player, point: Vec2, t: f32) -> (Vec2, Vec2, bool) {
    let left = t - state.time;
    let along_ground = state.ball.mode == BallMode::Rolling || state.flight.is_some_and(|f| f.kind == KickKind::GroundPass);
    let ball = state.ball.ground_pos();
    let vel = state.ball.vel.truncate();
    if along_ground && vel.length() > 1.0 {
        let dir = vel / vel.length();
        let s = (p.pos - ball).dot(dir);
        if s > 0.0 {
            let along = ((point - ball).dot(dir) - s) / left.max(0.2);
            return (ball + dir * s, dir * along.clamp(-p.kin.top_speed, p.kin.top_speed), true);
        }
    }
    (point, Vec2::ZERO, time_to_reach(p, point, 0.0, false) + 0.2 < left)
}

/// Decide where player `id` should go now.
pub fn decide(state: &MatchState, cfg: &SimConfig, id: PlayerId, rng: &mut SimRng) -> Move {
    let p = &state.players[id];
    let side = p.side;
    let fr = state.frame(side);
    let b = &p.beliefs;
    let (anchor_t, _) = anchor(state, cfg, id);
    let anchor_w = fr.to_world(anchor_t);
    let ball_ref_w = fr.to_world(b.ball_ref);
    let mut mv = Move {
        target: anchor_w,
        urgency: work_urgency((anchor_w - p.pos).length(), p.attrs.work_rate, &cfg.ai),
        pace_s: 0.0,
        brake: true,
        target_vel: Vec2::ZERO,
        look_at: ball_ref_w,
        intent: Intent::Position,
        rules: Vec::new(),
        label: "shape",
        anchor: anchor_w,
        drifts: true,
        commit: None,
        settles: false,
        tracks: false,
        kind: Movement::Hold,
        chose: true,
        moving: false,
        offer: None,
        projected: false,
        noise: p.plan.noise,
        concept: None,
    };

    // Intended receiver of a pass in flight: keep going to the reception point.
    if let Intent::Receive { point, t, meet } = p.intent
        && state.flight.is_some_and(|f| f.receiver == Some(id))
    {
        (mv.target, mv.target_vel, mv.brake) = receive_steering(state, p, point, t);
        mv.urgency = 1.0;
        mv.intent = p.intent;
        mv.label = if meet { "meet the ball" } else { "receive" };
        mv.rules.extend(["ATT-06", "ATT-24"]);
        mv.drifts = false;
        // He follows the ball he is watching: re-planned at his reaction time, not at the positional rhythm.
        mv.tracks = true;
        return mv;
    }
    // ATT-24: a team-mate is setting up a pass to him: he moves to receive it before it is struck, "judging when the
    // pass is ready to be played", once he has read the passer (section 7). Under pressure there he comes towards the
    // passer to meet it; free, he goes to where it is aimed and sets himself.
    if let Some(c) = state.control.filter(|&c| side_of(c) == side)
        && let Intent::SetUp { plan, until, since } = state.players[c].intent
        && plan.receiver == Some(id)
        && matches!(plan.kind, KickKind::GroundPass | KickKind::LoftedPass)
        && state.time - since >= reading::receive_delay(state, &cfg.ai, id, c)
    {
        // The ball arrives after the rest of the set-up and the planned flight.
        let arrive = (until - cfg.execution.setup_timeout_s - state.time).max(0.0) + plan.arrival_t;
        let back = state.players[c].pos - plan.target;
        // A lofted pass is met where it drops: coming short would let it fly over his head.
        if plan.kind == KickKind::GroundPass && receiver_pressured(state, cfg, id, plan.target, arrive) {
            let meet = applied_attacking(&state.team(p.side).tactics, &p.attrs, p.instruction_familiarity(true), &cfg.tactics).meet_m;
            mv.target = plan.target + back.normalize_or_zero() * meet.min(0.3 * back.length());
            mv.brake = false;
            mv.label = "come to the ball";
        } else {
            mv.target = plan.target;
            mv.brake = time_to_reach(p, plan.target, 0.0, false) + 0.2 < arrive;
            mv.label = "show for the ball";
        }
        mv.urgency = 1.0;
        mv.rules.push("ATT-24");
        mv.drifts = false;
        return mv;
    }
    if b.chaser == Some(p.slot) {
        mv.target = b.chase_point;
        mv.urgency = 1.0;
        // As for a receiver: run through only when he needs all his speed to be there in time.
        mv.brake = time_to_reach(p, b.chase_point, 0.0, false) + 0.2 < b.chase_t - state.time;
        mv.intent = Intent::Chase;
        mv.label = if b.challenge { "attack the ball" } else { "loose ball" };
        mv.rules.push(if b.in_possession { "ATT-22" } else { "PRS-02" });
        if b.challenge {
            mv.rules.push("IND-07");
        }
        mv.drifts = false;
        return mv;
    }
    if p.is_gk() {
        goalkeeper(state, cfg, id, &mut mv);
        return mv;
    }
    if b.in_possession {
        possession_movement(state, cfg, id, anchor_t, rng, &mut mv);
    } else {
        mv.moving = movement::applies(p);
        mv.chose = !mv.moving || movement::rechoose(state, cfg, p);
        defend(state, cfg, id, anchor_t, rng, &mut mv);
    }
    // Pacing (M6): on the counter the runners go forward at speed (TR-01, TR-03); the team that lost the ball
    // counter-presses or recovers at once (TRN-01..03), but only "players far from or behind the ball run back"
    // [FIFA-TD]: those make recovery runs (IND-06), and the block gets into shape at a run, as the team in possession
    // does (watch round).
    let shape = !b.in_possession && b.def_duties[p.slot] == DefDuty::Shape;
    if matches!(b.phase, TeamPhase::In(InPhase::CounterAttack) | TeamPhase::Out(OutPhase::CounterPress | OutPhase::Recovery)) && !shape {
        mv.pace_s = 0.0;
    }
    // ATT-11 in the attacking transition (PH-01), the mirror of TRN-03: "Quickly make playing area as big as possible –
    // width and depth"; "When regaining possession: … players away from ball make the field big" [USS-D §6.3]. Paced,
    // the wide players were still 8–14 m behind and 3–7 m inside their points 3 s after a regain (set W).
    if b.in_possession && state.time - state.possession_since < cfg.tactics.ph01_attacking_transition_s {
        mv.pace_s = 0.0;
    }
    // TR-01: "intense forward runs" on the counter, by the players going ahead of the ball, through the space up to
    // the last line; the approach to the line itself keeps the timing of a run in behind (ATT-24).
    let (to, at) = (fr.to_team(mv.target), fr.to_team(p.pos));
    if b.phase == TeamPhase::In(InPhase::CounterAttack)
        && mv.intent == Intent::Position
        && to.x > b.ball_ref.x
        && to.x > at.x + 2.0
        && at.x < b.offside_line - cfg.tactics.tr01_line_margin
    {
        mv.urgency = mv.urgency.max(cfg.tactics.tr01_run_urgency * (0.8 + 0.2 * norm(p.attrs.work_rate)));
    }
    mv.target = clamp_to_pitch(mv.target, 0.3);
    mv
}

fn defend(state: &MatchState, cfg: &SimConfig, id: PlayerId, anchor_t: Vec2, rng: &mut SimRng, mv: &mut Move) {
    let tc = &cfg.tactics;
    let p = &state.players[id];
    let side = p.side;
    let fr = state.frame(side);
    let team = state.team(side);
    let b = &p.beliefs;
    let pos = fr.to_team(p.pos);
    let carrier = state.control.filter(|&c| side_of(c) != side);
    // Point the defence orients to: carrier or ball reference.
    let ball_t = carrier.map(|c| fr.to_team(state.players[c].pos)).unwrap_or(b.ball_ref);
    // PRS-02: against an opponent's pass the first defender, and the cover joining the press, move towards the
    // receiver as soon as the ball leaves the passer's foot: their reference is the reception point, not the ball on
    // its way there.
    let press_t = state
        .flight
        .filter(|f| side_of(f.kicker) != side && f.receiver.is_some() && f.kind != KickKind::Shot)
        .map_or(ball_t, |f| fr.to_team(f.target));
    let defending = applied_defending(&team.tactics, &p.attrs, p.instruction_familiarity(false), tc);
    let anchor_t = shuffled(state, cfg, id, anchor_t, defending.shuffle);
    mv.pace_s = cfg.ai.pace_position_s;
    match b.def_duties[p.slot] {
        DefDuty::First => {
            mv.drifts = false;
            mv.kind = Movement::Press;
            mv.label = if b.press { "press" } else { "delay" };
            mv.rules.extend(["DEF-01", "DEF-02", "DEF-03"]);
            mv.rules.extend(b.press_rules.iter().copied());
            let mut dist = if b.press { tc.ind01_contain_distance } else { tc.prs07_delay_distance };
            let mut channel = team.tactics.channel;
            if carrier.is_some() && in_shooting_range(ball_t, tc) {
                // BOX-01: in shooting range delaying is no longer enough: get tight and force him wide, away from goal.
                dist = defending.contain_distance;
                channel = Channel::Wide;
                mv.rules.push("BOX-01");
            }
            let mut contain = contain_point(press_t, dist, channel, tc);
            // IND-05: alone against two, he angles his approach and retreats diagonally between the carrier, the
            // goal and the free attacker, cutting the pass between them, instead of engaging [FIFA-1V2].
            // At the touchline in a press the trap (PRS-05) is the answer instead.
            let trap = b.press && defending.trap > 0.0 && ball_t.y.abs() > tc.prs01_touchline_zone;
            if let Some(a) = carrier.filter(|_| !trap && !in_shooting_range(ball_t, tc)).and_then(|c| outnumbered(state, cfg, id, c)) {
                let goal = TeamFrame::own_goal();
                let dir = ((goal - press_t).normalize_or_zero() + (a - press_t).normalize_or_zero()).normalize_or(Vec2::NEG_X);
                contain = press_t + dir * dist.max(tc.prs07_delay_distance);
                mv.label = "delay, outnumbered";
                mv.rules.push("IND-05");
            }
            if !b.press && b.phase != TeamPhase::Out(OutPhase::Recovery) {
                // PRS-07: no press → "stop moving forward": delay from the front of the block, don't leave it.
                contain.x = contain.x.min(b.def_block.front_x + tc.prs07_front_step);
            }
            mv.rules.extend(["IND-01", "IND-02", "DEF-07"]);
            if let Some(c) = carrier {
                // IND-01: match the carrier's movement: the contain point moves with him, and once engaged the
                // defender mirrors him at his reaction time.
                mv.target_vel = state.players[c].vel;
                mv.tracks = (state.players[c].pos - p.pos).length() < tc.ind01_track_distance;
            }
            let to = contain - pos;
            if to.length() > 2.0 * tc.prs03_decel_distance {
                // PRS-03: curved approach from the blocked side.
                let inside = Vec2::new(0.0, if press_t.y > 0.0 { -1.0 } else { 1.0 });
                let side_dir = if channel == Channel::Wide { inside } else { -inside };
                contain += side_dir * tc.prs03_curve_offset;
                mv.rules.push("PRS-03");
            }
            if pos.x > press_t.x + 0.5 {
                // Beaten / not goal-side: recovery run (PRS-07, IND-06).
                mv.rules.extend(["PRS-07", "IND-06"]);
                mv.label = "recover";
            }
            mv.target = fr.to_world(contain);
            mv.urgency = if b.press || to.length() > tc.prs03_decel_distance { 1.0 } else { 0.7 };
            mv.pace_s = 0.0;
            mv.brake = true;
            mv.look_at = fr.to_world(ball_t);
        }
        DefDuty::Second => {
            let first_pos = b.first.map(|f| fr.to_team(state.squad(side)[f].pos)).unwrap_or(press_t);
            mv.target = fr.to_world(cover_point(first_pos, press_t, tc));
            mv.urgency = 0.9;
            mv.pace_s = cfg.ai.pace_cover_s;
            mv.kind = Movement::Cover;
            mv.label = "cover";
            mv.rules.extend(["DEF-01", "DEF-02"]);
            if let Some(c) = carrier {
                // DEF-02: keep the supporting distance as the carrier moves: move with him, at reaction time.
                mv.target_vel = state.players[c].vel;
                mv.tracks = true;
            }
            if let Some(c) = carrier.filter(|_| b.press && defending.trap > 0.0 && ball_t.y.abs() > tc.prs01_touchline_zone) {
                // PRS-05: a press at the touchline is a trap: the first defender shows the carrier one way (DEF-07)
                // and he closes that way, down the line or inside, so the touchline, the first defender and he leave
                // him no way out [FIFA-PR].
                let inward = -ball_t.y.signum();
                let way = if team.tactics.channel == Channel::Wide { Vec2::new(-1.0, 0.15 * inward) } else { Vec2::new(-0.35, inward) };
                let trap_pt = clamp_to_pitch(ball_t + way.normalize() * tc.prs05_trap_distance, 0.5);
                let cover = fr.to_team(mv.target);
                mv.target = fr.to_world(cover + (trap_pt - cover) * defending.trap);
                mv.target_vel = state.players[c].vel;
                mv.urgency = 1.0;
                mv.pace_s = 0.0;
                mv.kind = Movement::Press;
                mv.label = "trap";
                mv.rules.push("PRS-05");
            }
            if carrier.is_some() && in_shooting_range(ball_t, tc) {
                // BOX-02: one stays tight, the other covers him and blocks the shot: on the shooting line.
                mv.target = fr.to_world(shot_block_point(ball_t, &defending));
                mv.urgency = 1.0;
                mv.pace_s = 0.0;
                mv.label = "block the shot";
                mv.rules.push("BOX-02");
            }
            mv.drifts = false;
        }
        DefDuty::Double => {
            // PRS-01: from the side the carrier faces, close in on him and go for the ball, while the first defender
            // keeps him from turning [BDFL-PETER].
            mv.drifts = false;
            mv.kind = Movement::Press;
            mv.label = "double";
            mv.rules.push("PRS-01");
            match carrier {
                Some(c) => {
                    let cp = &state.players[c];
                    mv.target = clamp_to_pitch(cp.pos + cp.facing * tc.prs01_double_distance, 0.5);
                    mv.target_vel = cp.vel;
                    mv.tracks = (cp.pos - p.pos).length() < tc.ind01_track_distance;
                    mv.look_at = cp.pos;
                }
                None => mv.target = fr.to_world(anchor_t),
            }
            mv.urgency = 1.0;
            mv.pace_s = 0.0;
            mv.brake = true;
        }
        DefDuty::Third => {
            mv.target = fr.to_world(anchor_t);
            mv.settles = true;
            mv.kind = Movement::Cover;
            mv.label = "balance";
            mv.rules.extend(["DEF-02", "DEF-06"]);
        }
        DefDuty::Mark(o) => {
            let opp = fr.to_team(state.players[o].pos);
            let pass_to_him = state.flight.is_some_and(|f| f.receiver == Some(o));
            mv.target = fr.to_world(marking_point(opp, ball_t, tc));
            mv.urgency = if pass_to_him { 1.0 } else { 0.8 };
            mv.pace_s = if pass_to_him { 0.0 } else { cfg.ai.pace_mark_s };
            mv.kind = Movement::Mark;
            mv.label = "mark";
            mv.rules.extend(["DEF-04", "IND-04"]);
            mv.drifts = false;
            if movement::applies(p) {
                // Movement model: the marker moves with his man between re-reads instead of chasing where he was.
                mv.target_vel = state.players[o].vel;
            }
        }
        DefDuty::Track(o) => {
            // IND-04: goal-side of the runner at the marking distance, running with him until a team-mate takes him
            // over (DEF-04).
            let r = &state.players[o];
            mv.target = fr.to_world(marking_point(fr.to_team(r.pos), ball_t, tc));
            mv.target_vel = r.vel;
            mv.urgency = 1.0;
            mv.pace_s = 0.0;
            mv.kind = Movement::Track;
            mv.label = "track the runner";
            mv.rules.extend(["DEF-04", "IND-04"]);
            mv.drifts = false;
            mv.tracks = true;
        }
        DefDuty::Recover => {
            // The ball comes towards goal at its own pace, at least `pace_ball_speed`; a pass in flight from its
            // reception point once it gets there, at `pace_ball_speed` until the receiver shows his own.
            let (ref_t, t0, ball_speed) = match opponent_pass_in_flight(state, side) {
                Some((to, t0)) => (fr.to_team(to), t0, cfg.ai.pace_ball_speed),
                None => (ball_t, 0.0, (-fr.to_team(state.ball.vel.truncate()).x).max(cfg.ai.pace_ball_speed)),
            };
            let rp = recovery_point(ref_t, anchor_t.x, p.out_role.is_centre_back(), tc);
            let (q, need) = recovery_run(p, pos, ref_t, t0, rp, ball_speed, p.out_role.is_centre_back(), tc);
            mv.target = fr.to_world(q);
            // Against a ball no faster than `pace_ball_speed` he runs back, he does not sprint; against a pass in
            // flight he goes as fast as its arrival needs.
            let cap = if ball_speed > cfg.ai.pace_ball_speed || t0 > 0.0 { f32::MAX } else { cfg.ai.pace_run_speed };
            mv.urgency = (need.min(cap) / p.kin.top_speed).clamp(cfg.ai.position_min_urgency, 1.0);
            mv.pace_s = 0.0;
            mv.kind = Movement::Recover;
            mv.label = "recovery run";
            mv.rules.push("IND-06");
            mv.drifts = false;
        }
        DefDuty::Shape | DefDuty::Keeper => {
            let back_line = p.out_role.is_back_line();
            let sigma = noise_sigma(p, &cfg.ai);
            let kept = committed(p, "block", anchor_t);
            let mut best = (f32::MIN, anchor_t);
            match kept.filter(|_| !mv.chose) {
                // Movement model: between triggers he holds the point he chose, carried along with his anchor.
                Some(k) => best.1 = k,
                None => {
                    for c in candidates(anchor_t, pos, cfg).into_iter().chain(kept) {
                        if back_line && (c.x - anchor_t.x).abs() > 1.5 {
                            continue;
                        }
                        let s = score_defend(state, cfg, side, anchor_t, c, ball_t, carrier, back_line, b) + rng.gauss(0.0, sigma)
                            - switch_cost(kept, c, &cfg.ai);
                        if s > best.0 {
                            best = (s, c);
                        }
                    }
                }
            }
            mv.target = fr.to_world(best.1);
            mv.commit = Some((best.1 - anchor_t, "block"));
            mv.settles = true;
            mv.label = "block";
            mv.rules.extend(["DEF-05", "DEF-06", "BLK-02", "BLK-05"]);
            // BOX-04: near goal the back line drops with the ball, not a jog behind it.
            if back_line && box04_weight(ball_t, &defending, tc) > 0.0 && best.1.x < pos.x - 0.5 {
                mv.settles = false;
                mv.urgency = mv.urgency.max(defending.box_drop_urgency);
                mv.pace_s = 0.0;
                mv.rules.push("BOX-04");
            }
            if !back_line {
                mv.rules.push("PRS-04");
            }
            if b.ball_ref != state.ball_in(side) {
                mv.rules.push("BLK-04");
            }
        }
    }
}

/// BLK-03: `id`'s anchor (team frame) while a back-line team-mate has stepped out to a man between the lines: a
/// back-line player on the side of the gap away from the ball shifts into it by `shuffle` of the gap between the
/// vacated slot and the nearest line slot on his side ("shuffle across" [FIFA-MB]).
fn shuffled(state: &MatchState, cfg: &SimConfig, id: PlayerId, anchor_t: Vec2, shuffle: f32) -> Vec2 {
    let p = &state.players[id];
    if !p.out_role.is_back_line() || shuffle <= 0.0 {
        return anchor_t;
    }
    let b = &p.beliefs;
    let (fr, team, squad) = (state.frame(p.side), state.team(p.side), state.squad(p.side));
    let back = |j: usize| j != p.slot && squad[j].out_role.is_back_line();
    let Some(k) = (0..squad.len()).find(|&k| {
        back(k) && matches!(b.def_duties[k], DefDuty::Mark(o) if fr.to_team(state.players[o].pos).x - b.def_block.line_x > cfg.tactics.blk03_front_m)
    }) else {
        return anchor_t;
    };
    let slot_y = |j: usize| slot_point(&team.formation.slots[j], false, &b.def_block).y;
    let (gap, mine) = (slot_y(k), slot_y(p.slot));
    let side = (mine - gap).signum();
    if side == (b.ball_ref.y - gap).signum() {
        return anchor_t;
    }
    let nearest = (0..squad.len())
        .filter(|&j| j != k && squad[j].out_role.is_back_line() && (slot_y(j) - gap).signum() == side)
        .map(|j| (slot_y(j) - gap).abs())
        .min_by(f32::total_cmp)
        .unwrap_or(0.0);
    Vec2::new(anchor_t.x, anchor_t.y - side * shuffle * nearest)
}

/// IND-05: the free attacker (team frame) first defender `me` is outnumbered by against carrier `c`: no team-mate of
/// his near the carrier, an unmarked opponent near him, the one nearest the own goal.
fn outnumbered(state: &MatchState, cfg: &SimConfig, me: PlayerId, c: PlayerId) -> Option<Vec2> {
    let tc = &cfg.tactics;
    let side = state.players[me].side;
    let fr = state.frame(side);
    let cp = state.players[c].pos;
    let mine = state.squad(side);
    if mine.iter().any(|d| d.id != me && !d.is_gk() && (d.pos - cp).length() < tc.ind05_cover_m) {
        return None;
    }
    state
        .squad(side.opponent())
        .iter()
        .filter(|a| a.id != c && !a.is_gk() && (a.pos - cp).length() < tc.ind05_support_m)
        .filter(|a| !mine.iter().any(|d| d.id != me && (d.pos - a.pos).length() < 3.0))
        .map(|a| fr.to_team(a.pos))
        .min_by(|a, b| (*a - TeamFrame::own_goal()).length().total_cmp(&(*b - TeamFrame::own_goal()).length()))
}

#[allow(clippy::too_many_arguments)]
fn score_defend(
    state: &MatchState,
    cfg: &SimConfig,
    side: crate::pitch::Side,
    anchor_t: Vec2,
    c: Vec2,
    ball: Vec2,
    carrier: Option<PlayerId>,
    back_line: bool,
    b: &Beliefs,
) -> f32 {
    let tc = &cfg.tactics;
    let ai = &cfg.ai;
    let fr = state.frame(side);
    let mut s = -ai.shape_weight * ((c - anchor_t).length() / ai.shape_scale).powi(2);
    // DEF-05: stay inside the compact block.
    let blk = &b.def_block;
    let out_x = (blk.line_x - c.x).max(0.0) + (c.x - blk.front_x).max(0.0);
    let out_y = ((c.y - blk.centre_y).abs() - blk.half_width).max(0.0);
    s -= 0.5 * (out_x + out_y);
    if back_line {
        return s;
    }
    // PRS-04: screen passing lanes from the ball to dangerous nearby opponents.
    for o in state.squad(side.opponent()) {
        if Some(o.id) == carrier || o.is_gk() {
            continue;
        }
        let q = fr.to_team(o.pos);
        let d = (q - ball).length();
        if !(3.0..30.0).contains(&d) {
            continue;
        }
        let danger = (1.0 - d / 30.0) * (0.5 + 0.5 * ((HALF_LENGTH - q.x) / (2.0 * HALF_LENGTH)).clamp(0.0, 1.0));
        let lane = q - ball;
        let u = (c - ball).dot(lane) / lane.length_squared();
        if !(0.15..=0.85).contains(&u) {
            continue;
        }
        let dist = (ball + lane * u - c).length();
        s += tc.prs04_lane_weight * danger * (1.0 - dist / tc.prs04_lane_radius).max(0.0);
    }
    s
}

/// Candidate points: anchor, current position and rings around the anchor.
fn candidates(anchor_t: Vec2, pos: Vec2, cfg: &SimConfig) -> Vec<Vec2> {
    let mut v = vec![anchor_t, pos];
    let n = cfg.ai.candidate_directions.max(1);
    for &r in &cfg.ai.candidate_radii {
        for k in 0..n {
            let a = k as f32 / n as f32 * std::f32::consts::TAU;
            v.push(anchor_t + Vec2::from_angle(a) * r);
        }
    }
    v.retain(|c| c.x.abs() < HALF_LENGTH - 0.5 && c.y.abs() < HALF_WIDTH - 0.5);
    v
}

/// The movements of a player in possession.
const IN_POSSESSION: [Movement; 14] = [
    Movement::Hold,
    Movement::Pin,
    Movement::Check,
    Movement::Drop,
    Movement::RunInBehind,
    Movement::Show,
    Movement::InToOut,
    Movement::OutToIn,
    Movement::Overlap,
    Movement::Underlap,
    Movement::Invert,
    Movement::DropIntoBackLine,
    Movement::StepIntoMidfield,
    Movement::BoxRun,
];

/// TR-05: the movements a rest defender keeps, all behind the ball.
const REST: [Movement; 5] = [Movement::Hold, Movement::Show, Movement::Invert, Movement::DropIntoBackLine, Movement::StepIntoMidfield];

/// A shift of the offside line beyond this (m) since he chose his movement is a trigger (section 9).
const LINE_BAND: f32 = 4.0;

/// Phases D and E (docs/player-brain.md sections 6 and 8–10): an outfield player of the team in possession chooses
/// how to offer from his role's repertoire: hold his position (ATT-11), pin the line (ATT-11), check to feet (ATT-26),
/// drop between the lines (ATT-15), run in behind (ATT-18), show for the carrier (ATT-08), move in to out or out to in
/// (ATT-12), overlap or underlap the carrier (ATT-20), invert (BU-06), drop into the back line (BU-04) or step into
/// midfield (ROLE-02), or run into the box while the ball is wide in the final third (FT-03); a rest defender (TR-05) only those that keep him behind the ball. He re-chooses at a trigger,
/// the periodic check, the end of a check or a drop or a shift of the line; otherwise he follows the movement he
/// holds, whose references (the line, his anchor, the carrier) he re-reads at his reaction time. A run in behind waits
/// onside on the line and goes when the carrier sets the pass up (ATT-24), which aims at the run's offer
/// (`on_ball::passes`).
fn possession_movement(state: &MatchState, cfg: &SimConfig, id: PlayerId, anchor_t: Vec2, rng: &mut SimRng, mv: &mut Move) {
    let (tc, ai) = (&cfg.tactics, &cfg.ai);
    let p = &state.players[id];
    let side = p.side;
    let fr = state.frame(side);
    let b = &p.beliefs;
    let pos = fr.to_team(p.pos);
    let onside = b.offside_line - tc.att11_onside_margin;
    let look = movement::lookahead(p);
    let ps = offer::passer(state, cfg, p, look * ai.lookahead_horizon_s);
    let sp = spots(state, cfg, p, anchor_t);
    let points = |k: Movement| offer_points(cfg, p, &sp, ps.as_ref(), k);
    let held = (sp.repertoire().contains(&p.plan.kind) && p.plan.cue.in_possession).then_some(p.plan.kind);
    let held_offer = held.and(p.plan.offer).map(|o| fr.to_team(o));
    let held_point = held_offer.filter(|_| held.is_some_and(|k| matches!(k, Movement::Check | Movement::Drop)));
    let rechoose = held.is_none()
        || movement::rechoose(state, cfg, p)
        || held_point.is_some_and(|q| (q - pos).length() < 1.5)
        || held.is_some_and(|k| points(k).is_empty() && held_offer.is_none())
        || (held.is_some_and(|k| matches!(k, Movement::Pin | Movement::Check | Movement::Drop | Movement::RunInBehind))
            && (p.plan.line - b.offside_line).abs() > LINE_BAND)
        || (ps.is_some_and(|s| s.from_t > 0.0) && !p.plan.projected);
    mv.chose = false;
    let mut choice = None;
    if let Some(ps) = ps.filter(|_| rechoose) {
        let scores = score_moves(state, cfg, p, &ps, &sp);
        if held.is_none() || (ai.movement_noise_period_s <= 0.0 && movement::cue(state, cfg, p) != p.plan.cue) {
            mv.noise = rng.next_seed();
        }
        choice = (!scores.is_empty()).then(|| choose_move(cfg, p, held, look, &scores, mv.noise, state.time));
        mv.concept = choice.and_then(|c| c.2).map(|(k, q)| (k, fr.to_world(q)));
        mv.chose = true;
        mv.projected = ps.from_t > 0.0;
    }
    // Following a movement: a check or a drop keeps its point; the others follow their references, nearest to the
    // point he offered at and away from a team-mate's offer (two offers within `offer_overlap_m` are one option,
    // ATT-10); one he took up for the next carrier (8.4), which the carrier on the ball has no point of, keeps its
    // point.
    if choice.is_none() {
        mv.concept = p.plan.concept;
    }
    let (kind, pt) = choice.map(|c| (c.0, c.1)).unwrap_or_else(|| {
        let k = held.unwrap_or(Movement::Hold);
        let taken = |q: &Vec2| {
            state.squad(side).iter().any(|m| {
                m.id != id
                    && m.plan.cue.in_possession
                    && reading::seen(state, ai, id, m).1.is_some_and(|o| (fr.to_team(o) - *q).length() < ai.offer_overlap_m)
            })
        };
        let near = |q: &Vec2| held_offer.map_or(0.0, |o| (*q - o).length()) + if taken(q) { 1e3 } else { 0.0 };
        match held_point.or_else(|| points(k).into_iter().min_by(|a, c| near(a).total_cmp(&near(c)))).or(held_offer) {
            Some(q) => (k, q),
            None => (Movement::Hold, sp.hold),
        }
    });
    // The last line's speed: a pin moves with it (section 10).
    let opps = state.squad(side.opponent());
    let line: Vec<f32> = opps.iter().filter(|o| (fr.to_team(o.pos).x - b.offside_line).abs() < 2.0).map(|o| fr.to_team(o.vel).x).collect();
    let line_vel = fr.to_world(Vec2::new(if line.is_empty() { 0.0 } else { line.iter().sum::<f32>() / line.len() as f32 }, 0.0));
    mv.target = fr.to_world(pt);
    mv.drifts = false;
    mv.brake = true;
    // Speed by intention (section 10): hold, pin and the wait before a run walk or jog (a pin and a wait at the line's
    // speed on top); a show arrives by the carrier's next decision; a check and a run beside the carrier burst. The
    // sprint of a run comes with the set-up pass (ATT-24 "show for the ball").
    let jog = |to: Vec2| work_urgency((to - p.pos).length(), p.attrs.work_rate, ai);
    mv.urgency = jog(mv.target);
    // Pacing (M6): positional movements close the gap in `pace_position_s`; a check, a show, an overlap and a box run
    // go at their own speed.
    mv.pace_s = if matches!(kind, Movement::Check | Movement::Show | Movement::Overlap | Movement::Underlap | Movement::BoxRun) {
        0.0
    } else {
        ai.pace_position_s
    };
    match kind {
        Movement::Hold => {
            mv.settles = true;
            mv.drifts = true;
            if sp.rest.is_some() {
                mv.label = "rest defence";
                mv.rules.push("TR-05");
            } else {
                mv.label = "support";
            }
            mv.rules.push("ATT-11");
        }
        Movement::Pin => {
            mv.target_vel = line_vel;
            mv.urgency += line_vel.length() / p.kin.top_speed;
            mv.label = "pin the line";
            mv.rules.extend(["ATT-11", "ATT-26"]);
        }
        Movement::Check => {
            mv.urgency = 1.0;
            mv.label = "check short";
            mv.rules.push("ATT-26");
        }
        Movement::Drop => {
            mv.urgency = mv.urgency.max(0.5);
            mv.label = "drop between the lines";
            mv.rules.extend(["ATT-15", "ATT-26"]);
        }
        Movement::RunInBehind => {
            // Waiting onside on the line in his lane until the pass is set up.
            mv.target = fr.to_world(Vec2::new(onside - 0.5, pt.y));
            mv.target_vel = line_vel;
            mv.urgency = jog(mv.target) + line_vel.length() / p.kin.top_speed;
            mv.label = "run in behind";
            mv.rules.extend(["ATT-18", "ATT-26"]);
        }
        Movement::Show => {
            let need = (mv.target - p.pos).length() / ai.show_reach_s;
            mv.urgency = mv.urgency.max(need / p.kin.top_speed).min(1.0);
            mv.settles = true;
            mv.label = "show";
            mv.rules.extend(["ATT-08", "ATT-09"]);
        }
        Movement::InToOut | Movement::OutToIn => {
            mv.urgency = mv.urgency.max(0.5);
            mv.label = if kind == Movement::InToOut { "in to out" } else { "out to in" };
            mv.rules.push("ATT-12");
        }
        Movement::Overlap | Movement::Underlap => {
            mv.urgency = 1.0;
            mv.brake = false;
            mv.label = if kind == Movement::Overlap { "overlap" } else { "underlap" };
            mv.rules.push("ATT-20");
        }
        Movement::Invert => {
            mv.settles = true;
            mv.label = "invert";
            mv.rules.push("BU-06");
        }
        Movement::DropIntoBackLine => {
            mv.settles = true;
            mv.label = "drop into the back line";
            mv.rules.extend(["BU-03", "BU-04"]);
        }
        Movement::BoxRun => {
            // Onside until a team-mate's cross is in the air, then onto the spot (the cross aims at it).
            let crossed = state.flight.is_some_and(|f| {
                side_of(f.kicker) == side && is_cross(fr.to_team(f.start_pos), fr.to_team(f.target), f.kind == KickKind::LoftedPass)
            });
            if !crossed {
                mv.target = fr.to_world(Vec2::new(pt.x.min(onside), pt.y));
            }
            mv.urgency = jog(mv.target).max(0.7);
            mv.label = "box run";
            mv.rules.push("FT-03");
        }
        _ => {
            mv.settles = true;
            mv.label = "step into midfield";
            mv.rules.extend(["ROLE-02", "ATT-19"]);
        }
    }
    if sp.rest.is_some() && kind != Movement::Hold {
        mv.rules.push("TR-05");
    }
    if state.flight.is_some_and(|f| side_of(f.kicker) == side) {
        mv.rules.push("ATT-22");
    }
    mv.kind = kind;
    mv.moving = true;
    mv.offer = Some(fr.to_world(pt));
}

/// What the candidate points of player `p`'s movements depend on (team frame).
struct Spots {
    pos: Vec2,
    /// His layer-1 reference, onside (and behind the ball for a rest defender).
    hold: Vec2,
    /// On the line in his lane.
    pin: Vec2,
    onside: f32,
    /// ATT-26: a front-line player (anchor near the front of the shape) checks from the line, others from their spot.
    front: bool,
    /// TR-05: as a rest defender, no point beyond this.
    rest: Option<f32>,
    /// The touchline of his lane: +1 or −1.
    side: f32,
    /// The deepest pivot (DM, CMP) among his team-mates.
    pivot_x: Option<f32>,
    /// Between the two centre-backs of a back four.
    back_gap: Option<Vec2>,
    /// His team's midfield line.
    mid_x: Option<f32>,
    role: Role,
}

impl Spots {
    fn repertoire(&self) -> &'static [Movement] {
        if self.rest.is_some() { &REST } else { &IN_POSSESSION }
    }
}

fn spots(state: &MatchState, cfg: &SimConfig, p: &Player, anchor_t: Vec2) -> Spots {
    let tc = &cfg.tactics;
    let b = &p.beliefs;
    let fr = state.frame(p.side);
    let pos = fr.to_team(p.pos);
    let onside = b.offside_line - tc.att11_onside_margin;
    // TR-05: a rest defender goes no further forward than `tr05_gap` behind the ball, or than his own spot in the shape
    // while the ball is behind that (in the build-up the shape puts the full-backs high and wide, BU-02).
    let rest = (b.att_duties[p.slot] == AttDuty::RestDefence).then(|| (b.ball_ref.x - tc.tr05_gap).max(anchor_t.x));
    let side = if anchor_t.y.abs() > 1.0 { anchor_t.y.signum() } else if pos.y >= 0.0 { 1.0 } else { -1.0 };
    let mates: Vec<(Role, Vec2)> =
        state.squad(p.side).iter().filter(|m| m.id != p.id && !m.is_gk()).map(|m| (m.role, fr.to_team(m.pos))).collect();
    let pivot_x = mates.iter().filter(|(r, _)| matches!(r, Role::DM | Role::CMP)).map(|(_, q)| q.x).reduce(f32::min);
    let cbs: Vec<Vec2> = mates.iter().filter(|(r, _)| r.is_centre_back()).map(|(_, q)| *q).collect();
    let back_gap = (cbs.len() == 2 && !p.role.is_back_line()).then(|| 0.5 * (cbs[0] + cbs[1]));
    let mids: Vec<f32> = mates.iter().filter(|(r, _)| r.unit() == crate::team::Unit::Midfield).map(|(_, q)| q.x).collect();
    let mid_x = (!mids.is_empty()).then(|| mids.iter().sum::<f32>() / mids.len() as f32);
    let mut hold = Vec2::new(anchor_t.x.min(onside), anchor_t.y);
    if let Some(r) = rest {
        hold.x = hold.x.min(r);
    }
    Spots {
        pos,
        hold,
        pin: Vec2::new(onside, anchor_t.y),
        onside,
        front: anchor_t.x >= b.att_block.front_x - tc.att26_front_m,
        rest,
        side,
        pivot_x,
        back_gap,
        mid_x,
        role: p.role,
    }
}

/// Candidate points (team frame) of movement `k` for player `p`, offering to `ps`. A check, a drop and a show look for
/// the freest pocket: the candidate whose offer is worth most when he chooses.
fn offer_points(cfg: &SimConfig, p: &Player, s: &Spots, ps: Option<&offer::Passer>, k: Movement) -> Vec<Vec2> {
    let tc = &cfg.tactics;
    let b = &p.beliefs;
    let on_pitch = |q: &Vec2| q.y.abs() < HALF_WIDTH - 2.0 && q.x.abs() < HALF_LENGTH - 2.0;
    let onside = |q: Vec2| Vec2::new(q.x.min(s.onside), q.y);
    let mut v: Vec<Vec2> = match k {
        Movement::Hold => vec![s.hold],
        Movement::Pin => vec![s.pin],
        Movement::Check => {
            // From the line (or his spot) towards the ball, 3–8 m [FA-FWD], straight or angled off, ending short of
            // the carrier.
            let base = if s.front { s.pin } else { s.hold };
            let from = ps.map_or(b.ball_ref, |c| c.at);
            if from.x >= base.x - 4.0 {
                return Vec::new();
            }
            let dir = (from - base).normalize();
            let far = tc.att26_check_depth_m.min(0.5 * (base - from).length());
            [far, 0.5 * far].into_iter().flat_map(|d| [-0.6f32, 0.0, 0.6].map(|a| base + Vec2::from_angle(a).rotate(dir) * d)).collect()
        }
        // Between the opponents' midfield and back units [BDFL-HALE], in his lane or beside it.
        Movement::Drop => match b.opp_units[..] {
            [.., mid, def] if def - mid >= 2.0 * tc.att15_min_space => {
                let x = (0.5 * (mid + def)).min(s.onside - 4.0);
                [0.0, -6.0, 6.0, -12.0, 12.0].map(|dy| Vec2::new(x, s.hold.y + dy)).to_vec()
            }
            _ => Vec::new(),
        },
        // Into the space behind the line, as deep as `att18_run_depth`, in his lane, the channel beside it or diagonally
        // inside ("the wide players should run into the space left behind" [FIFA-JOHNSON]): the ball goes where he
        // gets first, short of a sweeping keeper.
        Movement::RunInBehind => [0.5, 0.75, 1.0]
            .into_iter()
            .flat_map(|f| {
                let x = (b.offside_line + f * tc.att18_run_depth).min(HALF_LENGTH - 6.0);
                let y = 0.8 * s.pin.y;
                [y, y - 8.0, y + 8.0, 0.3 * s.pin.y].map(|y| Vec2::new(x, y))
            })
            .filter(|q| q.x > s.onside + 2.0)
            .collect(),
        // At support distance from the carrier (ATT-08: 6–25 m) on his side of him, or at his own distance a few
        // metres sideways out of a cover shadow (ATT-14), where he arrives by the carrier's next decision
        // (`show_reach_s`).
        Movement::Show => {
            let Some(c) = ps else { return Vec::new() };
            let to = s.hold - c.at;
            let dir = if to.length() > 1.0 { to.normalize() } else { Vec2::NEG_X };
            let reach = p.kin.top_speed * cfg.ai.show_reach_s;
            [10.0, 16.0, to.length().clamp(tc.att08_min_distance, tc.att08_max_distance)]
                .into_iter()
                .flat_map(|d| [-0.7f32, -0.35, 0.0, 0.35, 0.7].map(|a| onside(c.at + Vec2::from_angle(a).rotate(dir) * d)))
                .filter(|q| (*q - s.pos).length() <= reach)
                .collect()
        }
        // Across from his lane to the wide channel or inside [FIFA-FL].
        Movement::InToOut => [8.0, 14.0].map(|d| Vec2::new(s.hold.x, s.hold.y + s.side * d)).to_vec(),
        Movement::OutToIn => [8.0, 14.0].map(|d| Vec2::new(s.hold.x, s.hold.y - s.side * d)).to_vec(),
        // Round the team-mate on the ball when he is wide on this side, level or ahead: outside or inside him and
        // beyond.
        Movement::Overlap | Movement::Underlap => {
            let Some(c) = ps.map(|c| c.at) else { return Vec::new() };
            if c.y * s.side < 8.0 || c.x < s.pos.x - 5.0 {
                return Vec::new();
            }
            let y = if k == Movement::Overlap { (c.y.abs() + 6.0).min(HALF_WIDTH - 2.5) } else { (c.y.abs() - 8.0).max(4.0) };
            [6.0, 12.0].map(|d| onside(Vec2::new(c.x + d, s.side * y))).to_vec()
        }
        // Into the inside channel beside or ahead of the pivot [FIFA-IFB-CWC25].
        Movement::Invert => {
            let x = s.pivot_x.unwrap_or(s.hold.x + 5.0);
            [(x, 10.0), (x + 8.0, 12.0)].map(|(x, y)| onside(Vec2::new(x, s.side * y))).to_vec()
        }
        // Between the two centre-backs of a back four, who then split [FIFA-WC26-B3].
        Movement::DropIntoBackLine => s.back_gap.into_iter().collect(),
        // FT-03: into the box while the carrier is wide in the final third, and on until his cross lands: near post,
        // far post, penalty spot, edge of the box ("attacks the front post", "the back post", "the edge of the
        // penalty area for any second balls" [FIFA-ES]).
        Movement::BoxRun => {
            let Some(c) = ps.map(|c| c.cross_from.unwrap_or(c.at)) else { return Vec::new() };
            if third_of(c.x) != Third::Attacking || c.y.abs() <= PENALTY_AREA_HALF_WIDTH {
                return Vec::new();
            }
            tc.ft03_spots.iter().map(|&[d, y]| Vec2::new(HALF_LENGTH - d, c.y.signum() * y)).collect()
        }
        // Up out of the back line into the midfield line [UEFA-UCL24]: the extra man in midfield (ATT-19).
        Movement::StepIntoMidfield => match s.mid_x {
            Some(m) if m > s.hold.x + 6.0 => [m, 0.5 * (m + s.hold.x)].map(|x| onside(Vec2::new(x, 0.6 * s.hold.y))).to_vec(),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    };
    if let Some(r) = s.rest {
        for q in &mut v {
            q.x = q.x.min(r);
        }
    }
    v.retain(on_pitch);
    v
}

/// What a movement is worth (section 8), in the on-ball value unit.
#[derive(Clone, Copy, Debug)]
pub struct MoveScore {
    pub kind: Movement,
    /// Its point (team frame): for a run, where the ball in behind goes.
    pub point: Vec2,
    /// The offer itself (`offer::offer`).
    pub own: f32,
    /// What it adds to the carrier's expected best option (8.1 / 8.3).
    pub marginal: f32,
    /// What the response of the man marking him adds to his team-mates' offers (8.2), before his look-ahead weight.
    pub space: f32,
    /// What it adds to the expected best option of the team-mate the carrier most likely plays to next, times the
    /// probability of that pass (8.4, level 1), before his look-ahead weight.
    pub next: f32,
    pub effort: f32,
    /// Role prior (layer 1) and instructions (layer 2).
    pub bias: f32,
    /// The local value scale the biases, the switch margin and the noise are shares of.
    pub scale: f32,
}

impl MoveScore {
    /// Its value to a player who looks `look` ahead (`movement::lookahead`).
    pub fn value(&self, look: f32) -> f32 {
        self.marginal + look * (self.space + self.next) - self.effort + self.bias
    }
}

/// The movements of outfield player `id` of the team in possession and what each is worth now, if he has someone to
/// offer to (for the viewer and the scenario tests).
pub fn move_scores(state: &MatchState, cfg: &SimConfig, id: PlayerId) -> Option<Vec<MoveScore>> {
    let p = &state.players[id];
    let (anchor_t, _) = anchor(state, cfg, id);
    let ps = offer::passer(state, cfg, p, movement::lookahead(p) * cfg.ai.lookahead_horizon_s)?;
    Some(score_moves(state, cfg, p, &ps, &spots(state, cfg, p, anchor_t)))
}

/// Values (section 8) of the movements in `p`'s repertoire (his role's prior > 0, and hold): each is worth what its
/// offer adds to the carrier's expected best option (marginal value, 8.1 / 8.3), plus — weighted by his look-ahead —
/// what the response of the man marking him (he follows a check or a drop and tracks a run, DEF-04 / IND-04) adds to
/// his team-mates' offers (space value, 8.2) and what his offer adds to the options of the team-mate the carrier most
/// likely plays to next (8.4: "values offers to the next carrier too"; a box run as the ball goes wide), less its
/// effort (8.5). The role's movement prior (layer 1) is a share of the local value scale; an instruction (layer 2) a
/// share of the movement's own worth (or that scale, if larger), so a full one at full adherence doubles or cancels
/// it. He prices the offers against the opponents as he
/// expects them once they have responded to his team-mates' movements (look-ahead level 1, 8.4). A run into the area
/// a team-mate's run already attacks adds little to the carrier's best option, so runners take different lanes
/// (TR-03, FT-03), and a player goes where his offer adds most, which is where his side can outnumber the
/// opponents near the ball (ATT-19).
fn score_moves(state: &MatchState, cfg: &SimConfig, p: &Player, ps: &offer::Passer, sp: &Spots) -> Vec<MoveScore> {
    let ai = &cfg.ai;
    let fr = state.frame(p.side);
    let look = movement::lookahead(p);
    let opps = offer::responded(state, cfg, &offer::opponents(state, p.side), p, look);
    let mates_of = |ps: &offer::Passer| -> Vec<&Player> {
        state.squad(p.side).iter().filter(|m| m.id != p.id && m.id != ps.id && !m.is_gk()).collect()
    };
    let mates = mates_of(ps);
    // Team-mates offer where their own movement offers, as far as he has read it (section 7), or else where they
    // stand (the carrier, once he has played the ball, where he played it from).
    let seen = |m: &Player| -> (Vec2, bool) {
        let (kind, offer) = reading::seen(state, ai, p.id, m);
        match offer.filter(|_| m.plan.cue.in_possession && m.id != ps.id) {
            Some(o) => (fr.to_team(o), kind == Movement::RunInBehind),
            None => (fr.to_team(m.pos), false),
        }
    };
    let offers_of = |to: &offer::Passer, mates: &[&Player], opps: &[offer::Opp]| -> Vec<offer::Opt> {
        mates
            .iter()
            .map(|m| {
                let (pt, through) = seen(m);
                offer::offer(state, cfg, to, m, pt, through, opps)
            })
            .collect()
    };
    let base = offers_of(ps, &mates, &opps);
    let keep = offer::keep_value(state, cfg, ps, look * ai.lookahead_horizon_s);
    let temp = offer::temperature(state, cfg, ps);
    let e0 = offer::expected_best(keep, &base, ai.offer_overlap_m, temp);
    // Look-ahead level 1 (8.4): the carrier's likely next pass, and that receiver's options without him. Not for the
    // receiver of a pass in flight, who is already the next carrier.
    let next = (look > 0.0 && ps.from_t <= 0.0)
        .then(|| offer::next_passer(state, cfg, ps, &mates, &base, keep, temp))
        .flatten()
        .map(|(nx, prob)| {
            let mates = mates_of(&nx);
            let base = offers_of(&nx, &mates, &opps);
            let keep = offer::keep_value(state, cfg, &nx, look * ai.lookahead_horizon_s);
            let temp = offer::temperature(state, cfg, &nx);
            let e0 = offer::expected_best(keep, &base, ai.offer_overlap_m, temp);
            (nx, prob, base, keep, temp, e0)
        });
    let marker = offer::marker_of(state, cfg, p);
    let scale = possession_value(ps.at, ai) + ai.value_base;
    let instr = resolve_player(&p.instructions);
    let adh = adherence(&p.attrs, p.instruction_familiarity(true), false);
    // The carrier's options with his offer last (and the next carrier's), refilled at every candidate point.
    let none = offer::Opt { point: Vec2::ZERO, avail: 0.0, value: 0.0 };
    let mut with: Vec<offer::Opt> = base.iter().copied().chain([none]).collect();
    let mut with_next: Vec<offer::Opt> = next.as_ref().map_or(Vec::new(), |(_, _, base, ..)| base.iter().copied().chain([none]).collect());
    let mut out = Vec::new();
    for &k in sp.repertoire().iter().filter(|&&k| k == Movement::Hold || sp.role.prior(k) > 0.0) {
        let through = k == Movement::RunInBehind;
        // Its point: the one whose offer adds most to the carrier's options and, as far as he looks ahead, to the
        // next carrier's (a box run to the spot no team-mate takes, FT-03); an offer neither can play (out of sight
        // or range, behind a team-mate) ranks last.
        let mut points = offer_points(cfg, p, sp, Some(ps), k);
        if let Some((nx, ..)) = &next {
            for q in offer_points(cfg, p, sp, Some(nx), k) {
                if !points.contains(&q) {
                    points.push(q);
                }
            }
        }
        let Some((own, marginal, ahead)) = points
            .into_iter()
            .map(|q| {
                let own = offer::offer(state, cfg, ps, p, q, through, &opps);
                *with.last_mut().unwrap() = own;
                let marginal = offer::expected_best(keep, &with, ai.offer_overlap_m, temp) - e0;
                let ahead = next.as_ref().map(|(nx, prob, _, keep, temp, e0)| {
                    let o = offer::offer(state, cfg, nx, p, q, through, &opps);
                    *with_next.last_mut().unwrap() = o;
                    (o.avail, prob * (offer::expected_best(*keep, &with_next, ai.offer_overlap_m, *temp) - e0))
                });
                (own, marginal, ahead)
            })
            .max_by(|a, c| {
                let rank = |(o, m, n): &(offer::Opt, f32, Option<(f32, f32)>)| {
                    let (na, nv) = n.unwrap_or((0.0, 0.0));
                    if o.avail > 0.0 || na > 0.0 { (*m + look * nv, o.value) } else { (f32::MIN, f32::MIN) }
                };
                let (ra, rc) = (rank(a), rank(c));
                ra.0.total_cmp(&rc.0).then(ra.1.total_cmp(&rc.1))
            })
        else {
            continue;
        };
        let ahead = ahead.map_or(0.0, |(_, v)| v);
        let space = match marker {
            Some(o) if !matches!(k, Movement::Hold | Movement::Pin) => {
                let moved = offer::after_response(state, cfg, &opps, p, o, own.point);
                offer::expected_best(keep, &offers_of(ps, &mates, &moved), ai.offer_overlap_m, temp) - e0
            }
            _ => 0.0,
        };
        // Speed class (section 8.5): walk, jog, run, sprint.
        let class = match k {
            Movement::Hold => 0.25,
            Movement::Pin => 0.5,
            Movement::RunInBehind | Movement::Overlap | Movement::Underlap | Movement::BoxRun => 2.0,
            _ => 1.0,
        };
        let effort = scale * ai.effort_per_m * class * (own.point - sp.pos).length() / (0.5 + norm(p.attrs.work_rate))
            * (1.0 + ai.effort_fatigue * (1.0 - p.energy));
        // Layer 2: `forward_runs` towards runs in behind, round the carrier and out of the back line (−1: stay back);
        // `come_short` from the line towards feet; `overlap` and `invert` (full-backs, wing-backs); followed as far as
        // he adheres to instructions (discipline, familiarity), as on the ball.
        let instruction = adh * match k {
            Movement::RunInBehind => instr.forward_runs - instr.come_short,
            Movement::BoxRun => instr.forward_runs,
            Movement::Check | Movement::Drop => instr.come_short,
            Movement::Pin => -instr.come_short,
            Movement::Overlap | Movement::Underlap => instr.overlap + instr.forward_runs,
            Movement::StepIntoMidfield => instr.forward_runs,
            Movement::DropIntoBackLine => -instr.forward_runs,
            Movement::Invert => instr.invert,
            _ => 0.0,
        };
        let worth = (marginal + look * (space + ahead) - effort).abs().max(scale);
        let bias = scale * ai.role_prior_weight * sp.role.prior(k) + worth * ai.movement_instruction_weight * instruction;
        out.push(MoveScore { kind: k, point: own.point, own: own.value, marginal, space, next: ahead, effort, bias, scale });
    }
    out
}

/// Movement points closer than this are the same place (`choose_move`).
const SAME_PLACE_M: f32 = 1.0;

/// The best of `scores` to a player who looks `look` ahead, with the switch margin for the movement he holds and his
/// noise (layer 3), both shares of the local value scale. The noise of each movement comes from `noise`, which he
/// keeps until the next trigger; movements to the same place (within `SAME_PLACE_M`, e.g. holding where he pins) are
/// one option and share a draw, so a place does not win more often for having two names. Also the concept's choice
/// (a perfect reader's: full look-ahead, no noise) with its point, when it is another movement (section 7: what his
/// team-mates expect of him).
fn choose_move(
    cfg: &SimConfig,
    p: &Player,
    held: Option<Movement>,
    look: f32,
    scores: &[MoveScore],
    noise: u64,
    now: f32,
) -> (Movement, Vec2, Option<(Movement, Vec2)>) {
    let ai = &cfg.ai;
    let mut best = (f32::MIN, Movement::Hold, Vec2::ZERO);
    let mut concept = (f32::MIN, Movement::Hold, Vec2::ZERO);
    let sigma = ai.movement_noise * (1.0 - reliability(p, ai));
    for s in scores {
        let same = scores.iter().find(|q| (q.point - s.point).length() < SAME_PLACE_M).map_or(s.kind, |q| q.kind);
        let draw = |k: u64| SimRng::new(noise ^ k ^ (same as u64 + 1).wrapping_mul(0x9e37_79b9_7f4a_7c15)).normal();
        // Drifting: draws for successive periods, blended linearly and rescaled to unit variance; each player's
        // periods start at his own phase.
        let e = if ai.movement_noise_period_s > 0.0 {
            let u = now / ai.movement_noise_period_s + (noise >> 40) as f32 / (1u64 << 24) as f32;
            let (k, w) = (u.floor(), u.fract());
            let k = (k as i64 as u64).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            (draw(k) * (1.0 - w) + draw(k.wrapping_add(0xbf58_476d_1ce4_e5b9)) * w) / ((1.0 - w).powi(2) + w * w).sqrt()
        } else {
            draw(0)
        };
        let margin = if held == Some(s.kind) { s.scale * ai.movement_switch_margin } else { 0.0 };
        let u = s.value(look) + e * s.scale * sigma + margin;
        if u > best.0 {
            best = (u, s.kind, s.point);
        }
        let c = s.value(1.0) + margin;
        if c > concept.0 {
            concept = (c, s.kind, s.point);
        }
    }
    (best.1, best.2, (concept.1 != best.1).then_some((concept.1, concept.2)))
}

/// GK-01 / GK-02: with the ball within shooting range the keeper stays in goal and narrows the angle (`gk01_depth`)
/// instead of standing at `depth` (sweeper or support depth); blended over the 10 m beyond the range, so he is set by
/// the time the ball is in range.
fn in_range_depth(depth: f32, dist: f32, tc: &TacticsConfig) -> f32 {
    let w = ((dist - tc.gk03_shot_range) / 10.0).clamp(0.0, 1.0);
    let narrow = gk01_depth(dist, tc);
    narrow + (depth - narrow) * w
}

fn goalkeeper(state: &MatchState, cfg: &SimConfig, id: PlayerId, mv: &mut Move) {
    let tc = &cfg.tactics;
    let p = &state.players[id];
    let side = p.side;
    let fr = state.frame(side);
    let b = &p.beliefs;
    let goal = TeamFrame::own_goal();
    mv.drifts = false;
    mv.urgency = 0.8;
    let ball = b.ball_ref;
    let dist = (ball - goal).length();
    // GK-01: a ball going into his goal (whichever side has the ball): he goes to its path, set there if he has time.
    if let Some((pt, t)) = b.gk_path {
        mv.target = pt;
        mv.urgency = 1.0;
        mv.brake = time_to_reach(p, pt, 0.0, false) + 0.2 < t - state.time;
        mv.tracks = true;
        mv.label = "to the ball's path";
        mv.rules.push("GK-01");
        return;
    }
    if b.in_possession {
        // BU-01: support option behind the centre-backs; GK-02 still holds with the ball in shooting range, so he
        // is in goal if it is lost there.
        let x = (b.att_block.line_x - tc.bu01_gk_support_depth).clamp(goal.x + 1.0, goal.x + tc.gk03_max_depth);
        let x = goal.x + in_range_depth(x - goal.x, dist, tc);
        mv.target = fr.to_world(Vec2::new(x, b.ball_ref.y * 0.15));
        mv.label = "support (GK)";
        mv.rules.push("BU-01");
        return;
    }
    // GK-02: rush out to smother a carrier who is through on goal.
    if let Some(c) = state.control.filter(|&c| side_of(c) != side) {
        let cp = &state.players[c];
        let ct = fr.to_team(cp.pos);
        let through = ct.x < goal.x + 30.0
            && !state.squad(side).iter().any(|d| {
                if d.is_gk() {
                    return false;
                }
                let q = fr.to_team(d.pos);
                let u = (q - ct).dot(goal - ct) / (goal - ct).length_squared();
                (0.0..1.0).contains(&u) && (ct + (goal - ct) * u - q).length() < 3.0
            });
        if through {
            let t_gk = time_to_reach(p, cp.pos, tc.gk02_smother_distance, false);
            // He can strike as soon as he is on the ball: one touch away, plus catching up with a ball pushed ahead.
            let ahead = (state.ball.ground_pos() - cp.pos).length() - cfg.player.control_radius;
            let t_shoot = 0.3 + ahead.max(0.0) / cp.speed().max(3.0);
            if t_gk <= t_shoot {
                mv.target = cp.pos;
                mv.urgency = 1.0;
                mv.brake = false;
                mv.label = "smother";
                mv.rules.push("GK-02");
                return;
            }
        }
    }
    // GK-01 / GK-03: on the ball–goal line at a depth set by the defensive line (sweeper depth).
    let depth = (b.def_block.line_x - tc.gk03_line_gap - goal.x).clamp(tc.gk03_min_depth, tc.gk03_max_depth);
    let depth = in_range_depth(depth, dist, tc);
    let range = tc.gk03_shot_range;
    let dir = (ball - goal).normalize_or(Vec2::X);
    let along = (depth / dir.x.max(0.3)).min(dist - 1.0).max(0.5);
    let mut t = goal + dir * along;
    if dist < range + 10.0 {
        t.y = t.y.clamp(-GOAL_HALF_WIDTH - 0.5, GOAL_HALF_WIDTH + 0.5);
    }
    mv.target = fr.to_world(t);
    mv.look_at = fr.to_world(ball);
    // Caught high (the ball came into range quickly): sprint back into position.
    if t.x < fr.to_team(p.pos).x - 1.0 {
        mv.urgency = 1.0;
    }
    mv.label = "angle";
    mv.rules.extend(["GK-01", "GK-03"]);
}
