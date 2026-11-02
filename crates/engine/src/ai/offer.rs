//! Offers (docs/player-brain.md section 8): what an off-ball movement is worth, in the on-ball value unit. An offer is
//! an option the carrier would have, the ball to a point: available as far as he can play that ball (`P_play`) and
//! worth what the pass there would be worth to him (as `on_ball::passes` prices it: possession value with the ATT-17
//! line breaks and the ATT-01 / ATT-03 forward terms, less the ATT-25 loss of a pressed receiver, against the ATT-02
//! cost of an interception). A movement is worth what its offer adds to the carrier's expected best option (marginal
//! value, 8.3) and, for a reader who looks a step ahead, what the response of the defender it occupies adds to his
//! team-mates' offers (space value, 8.2). A cross (FT-02: from the wide final third into the box) is priced as a ball in
//! the air: no opponent cuts it on the way, and a defender at the point by the time it arrives heads it with the
//! receiver (an aerial duel and a loose knock-down, as `analysis::evaluate_pass` prices a header).

use glam::Vec2;

use super::analysis::{in_vision_from, lines_broken, possession_value, pressed_at, pressure_at, value_at};
use super::brain::DefDuty;
use super::execution::setup_time;
use super::movement::lookahead;
use super::on_ball::{decision_temperature, receiver_pressured, risk_cost};
use super::positioning::marking_point;
use super::reach::{pass_reaction, time_to_reach, time_to_reach_from};
use crate::config::SimConfig;
use crate::duel::{aerial_win_prob, challenge_strength, control_skill, logistic, pass_touch_prob};
use crate::match_state::{MatchState, PLAYERS_PER_TEAM, side_of};
use crate::pitch::is_cross;
use crate::player::{Intent, KickKind, Player, PlayerId, norm};
use crate::team::{Movement, applied_attacking};

/// Speed (m/s) of the nominal pass an offer is priced with, and the time (s) the receiver of a pass in flight needs to
/// control it and set up the next one.
const PASS_SPEED: f32 = 15.0;
const SETUP_S: f32 = 0.25;
/// A team-mate this close (m) to a lane stands in it: the ball comes to him first (ATT-10).
const SCREEN_M: f32 = 1.5;
/// Mean horizontal speed (m/s) of a cross: the median of StatsBomb 360 open-play high crosses (25–35 m crosses hang
/// 1.62 s; `sb_cross.py` in the M4 notes).
const CROSS_SPEED: f32 = 17.0;

/// The player an offer is made to.
#[derive(Clone, Copy, Debug)]
pub struct Passer {
    pub id: PlayerId,
    /// Where he plays the ball from (team frame).
    pub at: Vec2,
    /// Seconds from now until he has the ball; 0 for the carrier, who sets up each pass by turning to it
    /// (`execution::setup_time`, as `on_ball` prices it).
    pub from_t: f32,
    /// Which way he faces when he plays it (world frame): what he can see (`analysis::in_vision`).
    pub facing: Vec2,
    /// Facing forward with time: an open ball [FA-FWD]. A closed carrier can hardly play a ball in behind.
    pub open: bool,
    /// Where a team-mate's cross still in flight to him was struck from (team frame): the box runs go on until it
    /// lands (FT-03).
    pub cross_from: Option<Vec2>,
}

/// An option of the carrier: the ball to `point` (team frame), available with probability `avail`, worth `value`.
#[derive(Clone, Copy, Debug)]
pub struct Opt {
    pub point: Vec2,
    pub avail: f32,
    pub value: f32,
}

/// An opponent as an offer sees him (world frame): now, or where he will be once he has responded to a movement.
#[derive(Clone, Copy, Debug)]
pub struct Opp {
    pub id: PlayerId,
    pub pos: Vec2,
    pub vel: Vec2,
}

/// The opponents of `side` as they are now.
pub fn opponents(state: &MatchState, side: crate::pitch::Side) -> Vec<Opp> {
    state.squad(side.opponent()).iter().map(|o| Opp { id: o.id, pos: o.pos, vel: o.vel }).collect()
}

/// Is the carrier `c` open: facing forward and not under pressure (ATT-21's threshold)?
pub fn is_open(state: &MatchState, cfg: &SimConfig, c: &Player) -> bool {
    state.frame(c.side).to_team(c.facing).x > 0.3 && pressure_at(state, c.side, c.pos) < cfg.tactics.att21_min_pressure
}

/// Whom `me` makes his offer to: the team-mate on the ball; or, once a team-mate's pass is due within `horizon` s (his
/// look-ahead, 8.4), its receiver where he takes it. That one is open if the ball comes to him from in front (he faces
/// it, so a forward pass leaves him facing his own goal) and nobody can press him there.
pub fn passer(state: &MatchState, cfg: &SimConfig, me: &Player, horizon: f32) -> Option<Passer> {
    let side = me.side;
    let fr = state.frame(side);
    if let Some(c) = state.control.filter(|&c| side_of(c) == side && c != me.id) {
        let cp = &state.players[c];
        return Some(Passer { id: c, at: fr.to_team(cp.pos), from_t: 0.0, facing: cp.facing, open: is_open(state, cfg, cp), cross_from: None });
    }
    let f = state.flight.filter(|f| side_of(f.kicker) == side)?;
    let r = f.receiver.filter(|&r| r != me.id)?;
    let Intent::Receive { point, t, .. } = state.players[r].intent else { return None };
    let left = t - state.time;
    if left > horizon {
        return None;
    }
    let at = fr.to_team(point);
    let open = fr.to_team(f.start_pos).x > at.x - 3.0 && !receiver_pressured(state, cfg, r, point, left);
    // He takes it facing the ball.
    let facing = (f.start_pos - point).normalize_or_zero();
    let start = fr.to_team(f.start_pos);
    let cross_from = is_cross(start, at, f.kind == KickKind::LoftedPass).then_some(start);
    Some(Passer { id: r, at, from_t: left + SETUP_S, facing, open, cross_from })
}

/// Look-ahead level 1 (section 8.4): the team-mate the carrier `ps` is most likely to play the ball to next, as he
/// chooses on the ball (`expected_best`: a softmax at temperature `temp` over keeping the ball, `keep`, and the options
/// `opts`, one per player in `mates`, each there with its availability), with the probability of that pass; that
/// team-mate as the next passer, where he takes the ball. Like the receiver of a pass in flight (`passer`), he is open
/// if the ball comes to him from in front and nobody can press him there.
pub fn next_passer(state: &MatchState, cfg: &SimConfig, ps: &Passer, mates: &[&Player], opts: &[Opt], keep: f32, temp: f32) -> Option<(Passer, f32)> {
    let t = temp.max(1e-6);
    let top = opts.iter().filter(|o| o.avail > 0.0).map(|o| o.value).fold(keep, f32::max);
    let w: Vec<f32> = opts.iter().map(|o| o.avail * ((o.value - top) / t).exp()).collect();
    let sum = w.iter().sum::<f32>() + ((keep - top) / t).exp();
    let (i, &wi) = w.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1))?;
    if wi <= 0.0 {
        return None;
    }
    let m = mates[i];
    let fr = state.frame(m.side);
    let at = opts[i].point;
    let from_t = SETUP_S + (at - ps.at).length() / PASS_SPEED;
    let (from_w, to_w) = (fr.to_world(ps.at), fr.to_world(at));
    let open = ps.at.x > at.x - 3.0 && !receiver_pressured(state, cfg, m.id, to_w, from_t);
    let next = Passer { id: m.id, at, from_t: from_t + SETUP_S, facing: (from_w - to_w).normalize_or_zero(), open, cross_from: None };
    Some((next, wi / sum))
}

/// The offer of `r` at team-frame `point` to `ps`, against the opponents `opps`; `through`: a ball in behind, for which
/// he times his run to cross the line in his lane at full speed as the pass is struck (section 6, "run in behind"),
/// starting up to his look-ahead before it (a run from deep gets there later); the defenders respond to the pass. The
/// carrier plays only what he sees, as on the ball (`on_ball::passes`).
pub fn offer(state: &MatchState, cfg: &SimConfig, ps: &Passer, r: &Player, point: Vec2, through: bool, opps: &[Opp]) -> Opt {
    let (ai, pc) = (&cfg.ai, &cfg.player);
    let fr = state.frame(r.side);
    let c = &state.players[ps.id];
    let len = (point - ps.at).length();
    if !(3.0..=ai.pass_max_distance).contains(&len) {
        return Opt { point, avail: 0.0, value: 0.0 };
    }
    // FT-02: a cross flies over the lane.
    let cross = !through && is_cross(ps.at, point, true);
    // P_play: any carrier can play a ball to feet; a ball in behind only an open one, as well as his passing allows;
    // a cross as well as his crossing allows.
    let avail = if cross {
        0.5 + 0.5 * norm(c.attrs.crossing)
    } else if !through {
        1.0
    } else if ps.open {
        0.5 + 0.5 * norm(c.attrs.long_passing.max(c.attrs.short_passing))
    } else {
        ai.offer_closed_play
    };
    let (from_w, to_w) = (fr.to_world(ps.at), fr.to_world(point));
    if ps.facing != Vec2::ZERO && !in_vision_from(c, from_w, ps.facing, to_w, ai) {
        return Opt { point, avail: 0.0, value: 0.0 };
    }
    let lane = to_w - from_w;
    // ATT-10: a ball past a team-mate standing in its lane is his ball, not this receiver's [FIFA-TCP; USS-D].
    let screened = !cross && state.squad(r.side).iter().any(|m| {
        let u = (m.pos - from_w).dot(lane) / (len * len);
        m.id != r.id && m.id != ps.id && (0.0..1.0).contains(&u) && (m.pos - (from_w + lane * u)).length() < SCREEN_M
    });
    if screened {
        return Opt { point, avail: 0.0, value: 0.0 };
    }
    let t0 = if ps.from_t > 0.0 { ps.from_t + SETUP_S } else { setup_time(c, lane / len, PASS_SPEED, None, &cfg.execution) };
    let t_r = if through {
        let line = fr.to_world(Vec2::new(r.beliefs.offside_line, point.y));
        let dir = fr.to_world(Vec2::X) - fr.to_world(Vec2::ZERO);
        let at_line = time_to_reach(r, line, 0.0, false) - lookahead(r) * ai.lookahead_horizon_s;
        t0.max(at_line) + time_to_reach_from(&r.kin, line, dir * r.kin.top_speed, to_w, pc.control_radius, 0.0)
    } else {
        time_to_reach(r, to_w, pc.control_radius, false)
    };
    let t_arr = (t0 + len / if cross { CROSS_SPEED } else { PASS_SPEED }).max(t_r);
    // p_arrive, as the on-ball race along the ball's path (`analysis::evaluate_pass`): no opponent cuts the lane before
    // the point (one beyond it does not), and he is at the point before any opponent.
    let (mut open, mut opp_t) = (1.0f32, f32::MAX);
    // A cross: the chance no outfield opponent is at the point when it arrives (they track his run), his chance in the
    // header against those who are, and the chance the keeper takes it (a catch, not a knock-down), who comes for it
    // only once it is played (GK-04) and so has its flight time.
    let flight = len / CROSS_SPEED;
    let (mut clean, mut pw_sum, mut f_sum, mut keeper) = (1.0f32, 0.0f32, 0.0f32, 0.0f32);
    for o in opps {
        let op = &state.players[o.id];
        let react = pass_reaction(op, 0.0, pc);
        let u = ((o.pos - from_w).dot(lane) / (len * len)).max(0.0);
        let q = from_w + lane * u;
        let tb = t0 + u * len / PASS_SPEED;
        if !cross && u < 1.0 && (o.pos - q).length() <= op.kin.top_speed * (tb + 0.3) + pc.control_radius {
            let to = time_to_reach_from(&op.kin, o.pos, o.vel, q, pc.control_radius, react);
            open *= 1.0 - logistic((tb - to) / ai.race_sigma) * pass_touch_prob(PASS_SPEED, op.attrs.instinct, pc);
        }
        let to = time_to_reach_from(&op.kin, o.pos, o.vel, to_w, pc.control_radius, react);
        opp_t = opp_t.min(to);
        if cross {
            if op.is_gk() {
                keeper = keeper.max(logistic((flight - to) / ai.race_sigma));
            } else {
                let f = logistic((t_arr - to) / ai.race_sigma);
                clean *= 1.0 - f;
                pw_sum += f * aerial_win_prob(r, op, &cfg.duel);
                f_sum += f;
            }
        }
    }
    // A contested header is worth `aerial_value_factor` of the ball at the point, kept `aerial_won_keep` of the time
    // after a won duel and `aerial_lost_keep` after a lost one (`analysis::evaluate_pass`, `on_ball::passes`).
    let (p_arrive, knock) = if cross {
        let pw = if f_sum > 0.0 { pw_sum / f_sum } else { 0.5 };
        let keep = pw * ai.aerial_won_keep + (1.0 - pw) * ai.aerial_lost_keep;
        let p_air = (1.0 - keeper) * (1.0 - clean) * keep;
        ((1.0 - keeper) * clean + p_air, p_air * (1.0 - ai.aerial_value_factor))
    } else {
        (open * logistic((opp_t - t_r) / ai.race_sigma), 0.0)
    };
    let att = applied_attacking(&state.team(r.side).tactics, &c.attrs, c.instruction_familiarity(true), &cfg.tactics);
    let scale = possession_value(ps.at, ai) + ai.value_base;
    let tc = &cfg.tactics;
    let mut v = value_at(point, r.beliefs.opp_last, ai);
    let (n, last) = lines_broken(&r.beliefs.opp_units, ps.at.x, point.x);
    if n > 0 {
        v += scale * tc.att17_line_break_bonus * (n as f32 + if last { tc.att17_last_line_factor - 1.0 } else { 0.0 });
    }
    if p_arrive >= tc.att03_comfortable_p {
        v += scale * tc.att03_forward_bonus_per_m * (point.x - ps.at.x).max(0.0);
    }
    if point.x > ps.at.x + 5.0 {
        v += scale * tc.att01_forward_pass_bonus;
    }
    let loss = att.receiver_base_loss + att.receiver_loss * pressed_at(t_arr, opp_t, cfg) * receiver_factor(r, cfg);
    let secured = v - loss * (v + risk_cost(point, cfg));
    let lost_at = ps.at + (point - ps.at) * 0.7;
    Opt { point, avail, value: (p_arrive - knock) * secured - (1.0 - p_arrive) * risk_cost(lost_at, cfg) }
}

/// ATT-25 for this receiver (section 8.1): a pressed receiver loses the ball as far as he cannot control and keep
/// it; his first touch (`duel::control_skill`) and strength (`duel::challenge_strength`) enter as the shield does
/// for a carrier who holds the ball (1 − 0.6 × skill, `on_ball` hold / shield). A receiver at the duel pivot loses the
/// configured share.
fn receiver_factor(r: &Player, cfg: &SimConfig) -> f32 {
    let skill = 0.5 * challenge_strength(r, &cfg.duel) + 0.5 * control_skill(&r.attrs, r.confidence, r.energy);
    (1.0 - 0.6 * skill) / (1.0 - 0.6 * cfg.duel.skill_pivot)
}

/// What the carrier has without any pass: the ball where he is, less the ATT-25 loss under the pressure on him once
/// the opponents' first defender (DEF-01) has closed him down for `ahead` s (look-ahead level 1: the opponents' first
/// response, section 8.4). A carrier with time keeps the ball only until he is closed down; support is for then.
pub fn keep_value(state: &MatchState, cfg: &SimConfig, ps: &Passer, ahead: f32) -> f32 {
    let c = &state.players[ps.id];
    let v = value_at(ps.at, c.beliefs.opp_last, &cfg.ai);
    let pressure = if ps.from_t > 0.0 { 0.0 } else { pressure_ahead(state, cfg, c, ahead) };
    let att = applied_attacking(&state.team(c.side).tactics, &c.attrs, c.instruction_familiarity(true), &cfg.tactics);
    v - att.pressed_loss * pressure * (v + risk_cost(ps.at, cfg))
}

/// The pressure on carrier `c` (`analysis::pressure_at`) once the opponents' first defender has run at him for `ahead`
/// s, up to the IND-01 contain distance.
fn pressure_ahead(state: &MatchState, cfg: &SimConfig, c: &Player, ahead: f32) -> f32 {
    let now = pressure_at(state, c.side, c.pos);
    let opp = c.side.opponent();
    let Some(slot) = state.team_view(opp).def_duties.iter().position(|d| *d == DefDuty::First) else { return now };
    let o = &state.players[opp.index() * PLAYERS_PER_TEAM + slot];
    let d = ((o.pos - c.pos).length() - o.kin.top_speed * ahead).max(cfg.tactics.ind01_contain_distance);
    now.max((1.0 - (d - 1.0) / 5.0).clamp(0.0, 1.0))
}

/// The temperature of the carrier's choice as an off-ball player sees it: his own decision noise
/// (`on_ball::decision_temperature`) and, independent of it, how much the options' values change before he plays the
/// ball (`offer_uncertainty`; a lane closes, a receiver is pressed). Together they are the scale of the Gumbel noise
/// behind the softmax, so they add in quadrature; the second is what makes a second and a third option worth offering
/// (ATT-09 [FFA-GTC]).
pub fn temperature(state: &MatchState, cfg: &SimConfig, ps: &Passer) -> f32 {
    let c = &state.players[ps.id];
    let pressure = if ps.from_t > 0.0 { 0.0 } else { pressure_at(state, c.side, c.pos) };
    let scale = possession_value(ps.at, &cfg.ai) + cfg.ai.value_base;
    decision_temperature(c, cfg, scale, pressure).hypot(cfg.ai.offer_uncertainty * scale)
}

/// At most this many uncertain options (a ball in behind) are enumerated; the rest count as unavailable.
const MAX_UNCERTAIN: usize = 4;

/// The carrier's expected best option: what his choice among keeping the ball and the options is worth to him, as he
/// chooses on the ball (a softmax at temperature `temp`: its inclusive value, the plain best at 0), over whether each
/// uncertain option (`avail` < 1) is there. An option within `overlap` m of a better one is the same option and adds
/// nothing.
pub fn expected_best(keep: f32, opts: &[Opt], overlap: f32, temp: f32) -> f32 {
    let mut v: Vec<&Opt> = opts.iter().filter(|o| o.avail > 0.0).collect();
    v.sort_by(|a, b| b.value.total_cmp(&a.value));
    let mut kept: Vec<&Opt> = Vec::new();
    for o in v {
        if !kept.iter().any(|q| (q.point - o.point).length() < overlap) {
            kept.push(o);
        }
    }
    let sure: Vec<f32> = kept.iter().filter(|o| o.avail >= 1.0).map(|o| o.value).chain([keep]).collect();
    let unsure: Vec<&Opt> = kept.iter().filter(|o| o.avail < 1.0).take(MAX_UNCERTAIN).copied().collect();
    let mut e = 0.0;
    let mut vals = Vec::with_capacity(sure.len() + unsure.len());
    for mask in 0..1u32 << unsure.len() {
        let mut p = 1.0;
        vals.clear();
        vals.extend_from_slice(&sure);
        for (i, o) in unsure.iter().enumerate() {
            if mask & (1 << i) != 0 {
                p *= o.avail;
                vals.push(o.value);
            } else {
                p *= 1.0 - o.avail;
            }
        }
        e += p * inclusive(&vals, temp);
    }
    e
}

/// Inclusive value of a softmax choice among `vals` at temperature `temp`: `temp` · ln Σ exp(v / `temp`), the best
/// value at 0. Adding an option never lowers it.
fn inclusive(vals: &[f32], temp: f32) -> f32 {
    let m = vals.iter().copied().fold(f32::MIN, f32::max);
    if temp <= 1e-6 {
        return m;
    }
    m + temp * vals.iter().map(|v| ((v - m) / temp).exp()).sum::<f32>().ln()
}

/// The opponent who responds to `me` moving (section 8.2): the one who marks or tracks him (DEF-04), as his team's
/// beliefs hold it; in a zonal block, the nearest outfield opponent goal-side of him within the DEF-04 zone radius,
/// who follows him within his zone.
pub fn marker_of(state: &MatchState, cfg: &SimConfig, me: &Player) -> Option<PlayerId> {
    let side = me.side.opponent();
    let duties = &state.team_view(side).def_duties;
    if let Some(slot) = duties.iter().position(|d| matches!(*d, DefDuty::Mark(o) | DefDuty::Track(o) if o == me.id)) {
        return Some(side.index() * PLAYERS_PER_TEAM + slot);
    }
    let fr = state.frame(me.side);
    let at = fr.to_team(me.pos);
    state
        .squad(side)
        .iter()
        .filter(|o| !o.is_gk() && fr.to_team(o.pos).x > at.x - 1.0)
        .map(|o| (o.id, (o.pos - me.pos).length()))
        .filter(|&(_, d)| d < cfg.tactics.def04_zone_radius)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| id)
}

/// Look-ahead level 1 (section 8.4): the opponents as `me` expects them once they have responded to his team-mates'
/// movements (the man marking each follows him to his offer, DEF-04 / IND-04), as far as he looks ahead (`look`).
pub fn responded(state: &MatchState, cfg: &SimConfig, opps: &[Opp], me: &Player, look: f32) -> Vec<Opp> {
    let mut out = opps.to_vec();
    if look <= 0.0 {
        return out;
    }
    let fr = state.frame(me.side);
    for m in state.squad(me.side) {
        if m.id == me.id || !m.plan.cue.in_possession {
            continue;
        }
        // His team-mate's movement as he has read it (section 7).
        let (kind, offer) = super::reading::seen(state, &cfg.ai, me.id, m);
        if matches!(kind, Movement::Hold | Movement::Pin) {
            continue;
        }
        let (Some(o), Some(k)) = (offer, marker_of(state, cfg, m)) else { continue };
        let to = after_response(state, cfg, &out, m, k, fr.to_team(o));
        for (a, b) in out.iter_mut().zip(to) {
            if a.id == k {
                a.pos = a.pos.lerp(b.pos, look);
                a.vel *= 1.0 - look;
            }
        }
    }
    out
}

/// The opponents once `marker` has followed `me` to team-frame point `to` (DEF-04 / IND-04: he takes up the marking
/// point there), the others as they are. A zonal marker (one without a mark or track duty on him) lets a player who
/// comes short or across out of his zone go (DEF-04).
pub fn after_response(state: &MatchState, cfg: &SimConfig, opps: &[Opp], me: &Player, marker: PlayerId, to: Vec2) -> Vec<Opp> {
    let ofr = state.frame(me.side.opponent());
    let fr = state.frame(me.side);
    let to_w = fr.to_world(to);
    let man = matches!(state.team_view(me.side.opponent()).def_duties[marker % PLAYERS_PER_TEAM], DefDuty::Mark(o) | DefDuty::Track(o) if o == me.id);
    let out_of_zone = opps.iter().any(|o| o.id == marker && (o.pos - to_w).length() > cfg.tactics.def04_zone_radius);
    if !man && out_of_zone && to.x <= fr.to_team(me.pos).x {
        return opps.to_vec();
    }
    let ball = ofr.to_team(state.ball.ground_pos());
    let at = ofr.to_world(marking_point(ofr.to_team(to_w), ball, &cfg.tactics));
    opps.iter().map(|&o| if o.id == marker { Opp { id: o.id, pos: at, vel: Vec2::ZERO } } else { o }).collect()
}
