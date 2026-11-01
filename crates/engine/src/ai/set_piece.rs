//! Set pieces (M7): where the players go while a corner, a free kick in the attacking third or a penalty kick is
//! being prepared. SP-01 defending corners, SP-02 attacking corners, SP-03 free kicks near goal (the wall), SP-04
//! penalty kicks. Once the kick is taken open play takes over (CRS-01 / FT-03 decide the delivery); the delivery is
//! the taker's on-ball choice like any pass or shot. Goal kicks (SP-05) need no positions of their own: the build-up
//! (BU-01) and the press on the keeper (PRS-01) handle them.

use glam::Vec2;

use super::analysis::goal_angle;
use crate::config::TacticsConfig;
use crate::match_state::{MatchState, RestartKind};
use crate::pitch::{CENTRE_CIRCLE_RADIUS, HALF_LENGTH, HALF_WIDTH, PENALTY_AREA_DEPTH, PENALTY_AREA_HALF_WIDTH, Side, TeamFrame};
use crate::player::{Player, PlayerId};

/// A set-piece position: where (world), what it is, and the rule behind it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SetSpot {
    pub pos: Vec2,
    pub label: &'static str,
    pub rule: &'static str,
}

/// Does this restart wait for the teams to organise (SP-01..04)? Corners, penalty kicks and free kicks in the attacking
/// third (within 35 m of the goal line).
pub fn organised(state: &MatchState, kind: RestartKind, side: Side, spot: Vec2) -> bool {
    match kind {
        RestartKind::Corner | RestartKind::Penalty => true,
        RestartKind::FreeKick { .. } => state.frame(side).to_team(spot).x > HALF_LENGTH / 3.0,
        _ => false,
    }
}

/// The set-piece position of `id` for this restart (`open`: his open-play target), or `None` to keep that.
#[allow(clippy::too_many_arguments)]
pub fn target(state: &MatchState, tc: &TacticsConfig, id: PlayerId, kind: RestartKind, side: Side, spot: Vec2, taker: PlayerId, open: Vec2) -> Option<SetSpot> {
    if id == taker || !organised(state, kind, side, spot) {
        return None;
    }
    match kind {
        RestartKind::Penalty => penalty(state, tc, id, side, open),
        RestartKind::Corner => plan(state, tc, side, spot, taker, Kind::Corner)[id],
        _ => {
            let fr = state.frame(side);
            let t = fr.to_team(spot);
            let dist = (TeamFrame::opp_goal() - t).length();
            let kind = if kind == (RestartKind::FreeKick { direct: true }) && dist < tc.sp03_range_m { Kind::DirectFk } else { Kind::CrossFk };
            plan(state, tc, side, spot, taker, kind)[id]
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Corner,
    /// A direct free kick within shooting range: a wall.
    DirectFk,
    /// A free kick to be crossed: a line.
    CrossFk,
}

/// Strength in the air: heading, jumping and height.
fn aerial(p: &Player) -> f32 {
    p.attrs.heading + p.attrs.jumping + 0.5 * (p.attrs.height_cm - 180.0)
}

/// SP-03: wall size (FIFA-SP06, ACAD-FK01: more players the more central and the closer the kick; LOW-FK03: "the further
/// away, the smaller the wall"): 1 + (`max` − 1) × closeness × centrality, where closeness falls from 1 at 18 m to 0
/// at `sp03_range_m` and centrality is the goal angle relative to a central kick from the same distance.
pub fn wall_size(t: Vec2, max: u8, tc: &TacticsConfig) -> usize {
    let goal = TeamFrame::opp_goal();
    let dist = (goal - t).length();
    let closeness = (1.0 - (dist - 18.0) / (tc.sp03_range_m - 18.0)).clamp(0.0, 1.0);
    let central = goal_angle(Vec2::new(HALF_LENGTH - dist, 0.0));
    let centrality = (goal_angle(t) / central.max(1e-3)).clamp(0.0, 1.0);
    1 + ((max as f32 - 1.0) * closeness * centrality).round() as usize
}

/// Every player's position for a corner or free kick by `side` from `spot` (indexed by id).
fn plan(state: &MatchState, tc: &TacticsConfig, side: Side, spot: Vec2, taker: PlayerId, kind: Kind) -> Vec<Option<SetSpot>> {
    let mut out = vec![None; state.players.len()];
    let afr = state.frame(side);
    let dfr = state.frame(side.opponent());
    let ball_t = afr.to_team(spot);
    // Near side of the ball (attacking frame); a central kick counts as the left.
    let s = if ball_t.y < 0.0 { -1.0 } else { 1.0 };
    let set = |out: &mut Vec<Option<SetSpot>>, id: PlayerId, p: Vec2, label: &'static str, rule: &'static str| {
        out[id] = Some(SetSpot { pos: p, label, rule });
    };

    let (sa, sd_t) = (state.team(side).tactics.set_pieces, state.team(side.opponent()).tactics.set_pieces);
    // --- attack (SP-02 / SP-03)
    let mut att: Vec<&Player> = state.squad(side).iter().filter(|p| !p.is_gk() && !p.sent_off && p.id != taker).collect();
    att.sort_by(|a, b| aerial(b).total_cmp(&aerial(a)).then(a.id.cmp(&b.id)));
    let n_box = (sa.attackers as usize).min(att.len());
    let boxed: Vec<&Player> = att.drain(..n_box).collect();
    // The line the defence holds at a free kick (SP-03); attackers stay level with it, onside.
    let line_x = HALF_LENGTH - sd_t.line_m.min(HALF_LENGTH - ball_t.x - 2.0).max(PENALTY_AREA_DEPTH * 0.5);
    for (i, p) in boxed.iter().enumerate() {
        let (pt, label) = match kind {
            // SP-02: 5–6 attackers, front post overloaded (FIFA-SP04, FIFA-SP05); deliveries go to the first post
            // 42 %, the centre 24 %, the second post 21 % (ACAD-C01).
            Kind::Corner => [
                (Vec2::new(HALF_LENGTH - 6.0, -s * 0.5), "attack the centre"),
                (Vec2::new(HALF_LENGTH - 5.0, s * 2.5), "attack the front post"),
                (Vec2::new(HALF_LENGTH - 5.5, -s * 3.5), "attack the far post"),
                (Vec2::new(HALF_LENGTH - 8.5, s * 4.0), "front post, second wave"),
                (Vec2::new(HALF_LENGTH - 11.0, -s * 1.5), "penalty spot"),
                (Vec2::new(HALF_LENGTH - 1.5, s * 0.5), "screen the keeper"),
            ][i.min(5)],
            // SP-03: level with the defensive line, across the goal, more to the far side of a wide kick.
            _ => {
                let span = 16.0;
                let y = -s * 2.0 + span * (i as f32 / (n_box.max(2) - 1) as f32 - 0.5);
                (Vec2::new(line_x - 0.5, y), "on the line")
            }
        };
        set(&mut out, p.id, afr.to_world(pt), label, "SP-02");
    }
    // Edge of the box for the second ball (FIFA-SP02), a short option at a corner, the rest stay back (rest defence,
    // TR-05).
    att.sort_by(|a, b| b.attrs.long_shots.total_cmp(&a.attrs.long_shots).then(a.id.cmp(&b.id)));
    if !att.is_empty() {
        let p = att.remove(0);
        set(&mut out, p.id, afr.to_world(Vec2::new(HALF_LENGTH - PENALTY_AREA_DEPTH - 2.5, -s * 3.0)), "edge of the box", "SP-02");
    }
    if kind == Kind::Corner && sa.short && !att.is_empty() {
        att.sort_by(|a, b| b.attrs.short_passing.total_cmp(&a.attrs.short_passing).then(a.id.cmp(&b.id)));
        let p = att.remove(0);
        set(&mut out, p.id, afr.to_world(Vec2::new(HALF_LENGTH - 8.0, s * (HALF_WIDTH - 5.0))), "short option", "SP-02");
    }
    let rest = att.len();
    for (i, p) in att.iter().enumerate() {
        let y = if rest == 1 { 0.0 } else { 20.0 * (i as f32 / (rest - 1) as f32 - 0.5) };
        set(&mut out, p.id, afr.to_world(Vec2::new(0.0, y)), "rest defence", "TR-05");
    }

    // --- defence (SP-01 / SP-03), in the defending team's frame
    let d_side = side.opponent();
    let goal_d = TeamFrame::own_goal();
    let ball_d = dfr.to_team(spot);
    let sd = if ball_d.y < 0.0 { -1.0 } else { 1.0 };
    let mut def: Vec<&Player> = state.squad(d_side).iter().filter(|p| !p.sent_off).collect();
    if let Some(i) = def.iter().position(|p| p.is_gk()) {
        let gk = def.remove(i);
        let pt = match kind {
            // FIFA-GK01: the keeper's starting position at a corner is near his line.
            Kind::Corner => Vec2::new(-HALF_LENGTH + tc.sp01_gk_m, -sd * 0.5),
            // FIFA-SP06: with the wall protecting the near side, the keeper shifts towards the far side.
            Kind::DirectFk => Vec2::new(-HALF_LENGTH + 1.0, -sd * tc.sp03_gk_shift_m),
            // FIFA-GK02: off his line to defend the space behind the line.
            Kind::CrossFk => Vec2::new(-HALF_LENGTH + tc.sp03_gk_m, -sd * 0.5),
        };
        set(&mut out, gk.id, dfr.to_world(pt), "set-piece keeper", if kind == Kind::Corner { "SP-01" } else { "SP-03" });
    }
    // Counter-attack outlets (FIFA-SP01: most teams bring all ten outfield players back): the fastest.
    def.sort_by(|a, b| b.attrs.speed.total_cmp(&a.attrs.speed).then(a.id.cmp(&b.id)));
    for _ in 0..(sd_t.counter as usize).min(def.len()) {
        let p = def.remove(0);
        set(&mut out, p.id, dfr.to_world(Vec2::new(-2.0, -sd * 8.0)), "counter outlet", "SP-01");
    }
    // SP-03 wall: the weakest in the air (the strong headers mark), the end man a yard outside the near post on the
    // ball's line to it (LOW-FK03), shoulder to shoulder towards the middle, 9.15 m from the ball.
    let n_wall = match kind {
        Kind::DirectFk => wall_size(ball_t, sd_t.wall_max, tc),
        Kind::CrossFk if ball_d.x < -HALF_LENGTH + PENALTY_AREA_DEPTH + 15.0 && ball_d.y.abs() > PENALTY_AREA_HALF_WIDTH => 1,
        _ => 0,
    }
    .min(def.len());
    if n_wall > 0 {
        def.sort_by(|a, b| aerial(a).total_cmp(&aerial(b)).then(a.id.cmp(&b.id)));
        let near_post = Vec2::new(-HALF_LENGTH, sd * (crate::pitch::GOAL_HALF_WIDTH + 0.9));
        let dir = (near_post - ball_d).normalize_or(Vec2::NEG_X);
        let base = ball_d + dir * (CENTRE_CIRCLE_RADIUS + 0.5);
        // Towards the middle of the goal: perpendicular to the ball's line, on the far side of it.
        let mut across = dir.perp();
        if (base + across).y.abs() > base.y.abs() {
            across = -across;
        }
        for k in 0..n_wall {
            let p = def.remove(0);
            set(&mut out, p.id, dfr.to_world(base + across * (0.75 * k as f32)), "wall", "SP-03");
        }
    }
    // The edge of the box (FIFA-SP02: an edge player cuts second phases from 57 % to 40 %).
    def.sort_by(|a, b| aerial(b).total_cmp(&aerial(a)).then(a.id.cmp(&b.id)));
    let n_zonal = if kind == Kind::Corner { (sd_t.zonal as usize).min(def.len()) } else { 0 };
    // SP-01 hybrid: the best in the air defend zones in the goal area (FIFA-SP01: two or three zonal players most
    // common at FWC22; no one on the posts) …
    let zones = [
        (Vec2::new(-HALF_LENGTH + 5.0, sd * 2.0), "zone: front of the goal area"),
        (Vec2::new(-HALF_LENGTH + 5.5, -sd * 1.0), "zone: goal-area centre"),
        (Vec2::new(-HALF_LENGTH + 5.0, -sd * 4.0), "zone: back of the goal area"),
        (Vec2::new(-HALF_LENGTH + 4.0, sd * 4.5), "zone: front post"),
        (Vec2::new(-HALF_LENGTH + 9.0, 0.0), "zone: penalty spot"),
        (Vec2::new(-HALF_LENGTH + 8.0, -sd * 6.0), "zone: far side"),
    ];
    for &(pt, label) in zones.iter().take(n_zonal) {
        let p = def.remove(0);
        set(&mut out, p.id, dfr.to_world(pt), label, "SP-01");
    }
    if !def.is_empty() && sd_t.edge {
        let p = def.remove(def.len() - 1);
        set(&mut out, p.id, dfr.to_world(Vec2::new(-HALF_LENGTH + PENALTY_AREA_DEPTH + 1.0, sd * 2.0)), "edge of the box", "SP-01");
    }
    // … the rest mark the attackers in the box player to player, goal-side within a metre, the best headers on the
    // best headers; anyone left over holds a zone (corner) or the line (free kick).
    let rule = if kind == Kind::Corner { "SP-01" } else { "SP-03" };
    for a in &boxed {
        if def.is_empty() {
            break;
        }
        let p = def.remove(0);
        let ap = dfr.to_team(a.pos);
        let mut pt = ap + (goal_d - ap).normalize_or(Vec2::NEG_X) * 1.0;
        if kind != Kind::Corner {
            pt.x = pt.x.max(-line_x);
        }
        set(&mut out, p.id, dfr.to_world(pt), "mark", rule);
    }
    let left = def.len();
    for (i, p) in def.iter().enumerate() {
        let y = if left == 1 { 0.0 } else { 14.0 * (i as f32 / (left - 1) as f32 - 0.5) };
        let x = if kind == Kind::Corner { -HALF_LENGTH + 11.0 } else { -line_x };
        set(&mut out, p.id, dfr.to_world(Vec2::new(x, y)), "zone", rule);
    }
    out
}

/// SP-04 / Law 14: the keeper on his goal line between the posts; everybody else outside the penalty area, behind
/// the penalty mark and 9.15 m from it (the keep-out radius), here at their open-play spot pushed out of the area.
/// The kicking team's keeper stays where he is.
fn penalty(state: &MatchState, tc: &TacticsConfig, id: PlayerId, side: Side, open: Vec2) -> Option<SetSpot> {
    let p = &state.players[id];
    let dfr = state.frame(side.opponent());
    if p.is_gk() {
        return (p.side != side).then(|| SetSpot { pos: dfr.to_world(Vec2::new(-HALF_LENGTH + 0.1, 0.0)), label: "on the goal line", rule: "SP-04" });
    }
    let mut t = dfr.to_team(open);
    // Just outside the area, ready for a rebound; spread across its width.
    let edge = -HALF_LENGTH + PENALTY_AREA_DEPTH + tc.sp04_edge_m;
    t.x = t.x.clamp(edge, edge + 8.0);
    t.y = t.y.clamp(-PENALTY_AREA_HALF_WIDTH, PENALTY_AREA_HALF_WIDTH);
    Some(SetSpot { pos: dfr.to_world(t), label: "outside the area", rule: "SP-04" })
}
