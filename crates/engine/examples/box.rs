//! Box defending at the moment of a shot: how the shooter got the ball, how close and where the defenders are, how many
//! of them are in the box and in the shooting cone, how deep the line is, what the nearest defender was doing, and how
//! the shot ended (goal, saved, blocked by an outfield player, off target).
//! Usage: box [matches] [minutes]

use std::collections::BTreeMap;

use glam::Vec2;
use match_engine::Simulation;
use match_engine::events::EventKind;
use match_engine::match_state::side_of;
use match_engine::pitch::{GOAL_HALF_WIDTH, HALF_LENGTH, PENALTY_AREA_DEPTH, PENALTY_AREA_HALF_WIDTH};

#[derive(Default)]
struct Acc {
    shots: u32,
    goals: u32,
    on_ball: f32,
    first_time: u32,
    after_pass: u32,
    within2: u32,
    within4: f32,
    nearest: f32,
    nearest_goal_side: u32,
    def_in_box: f32,
    att_in_box: f32,
    cone: f32,
    line_depth: f32,
    ball_5s_ago: f32,
    /// Outcome: goal, saved, blocked by an outfield defender, off target / other.
    outcome: [u32; 4],
    labels: BTreeMap<String, u32>,
}

fn in_triangle(q: Vec2, a: Vec2, b: Vec2, c: Vec2) -> bool {
    let s = |p: Vec2, q: Vec2, r: Vec2| (q - p).perp_dot(r - p);
    let (d1, d2, d3) = (s(a, b, q), s(b, c, q), s(c, a, q));
    !((d1 < 0.0 || d2 < 0.0 || d3 < 0.0) && (d1 > 0.0 || d2 > 0.0 || d3 > 0.0))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let matches: u64 = args.first().and_then(|s| s.parse().ok()).unwrap_or(4);
    let mins: f32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(90.0);
    let bands = [(0.0, 11.0, "0–11 m"), (11.0, 16.5, "11–16.5 m"), (16.5, 25.0, "16.5–25 m")];
    let mut acc: Vec<Acc> = bands.iter().map(|_| Acc::default()).collect();
    for seed in 1..=matches {
        let mut sim = Simulation::from_builtin("northbridge", "harbour", seed);
        sim.state.half_length = mins * 30.0;
        let mut seen = 0;
        // Last completed pass to each player (time), and the ball's track (time, world position).
        let mut passed_to: Vec<f32> = vec![f32::MIN; 22];
        let mut track: Vec<(f32, Vec2)> = Vec::new();
        let mut open: Option<usize> = None;
        // Shot in flight: band, shooter, time.
        let mut flying: Option<(usize, usize, f32)> = None;
        while !sim.finished() {
            sim.step();
            let st = &sim.state;
            track.push((st.time, st.ball.ground_pos()));
            if let Some((b, by, t0)) = flying {
                let k = match st.last_touch {
                    Some(x) if x != by && side_of(x) != side_of(by) => Some(if st.players[x].is_gk() { 1 } else { 2 }),
                    Some(x) if x != by => Some(3),
                    _ if st.time - t0 > 3.0 || st.control.is_some_and(|c| c != by) => Some(3),
                    _ => None,
                };
                if let Some(k) = k {
                    acc[b].outcome[k] += 1;
                    flying = None;
                }
            }
            while seen < sim.events.len() {
                let e = sim.events[seen].clone();
                seen += 1;
                match e.kind {
                    EventKind::Pass { to: Some(to), .. } => passed_to[to] = e.t,
                    EventKind::Goal { .. } => {
                        if let Some(b) = open.take() {
                            acc[b].goals += 1;
                        }
                        if let Some((b, _, _)) = flying.take() {
                            acc[b].outcome[0] += 1;
                        }
                    }
                    EventKind::Restart { .. } => {
                        if let Some((b, _, _)) = flying.take() {
                            acc[b].outcome[3] += 1;
                        }
                    }
                    EventKind::Shot { by, first_time, .. } => {
                        open = None;
                        let side = side_of(by);
                        let fr = st.frame(side);
                        let sh = &st.players[by];
                        let p = fr.to_team(sh.pos);
                        let goal = Vec2::new(HALF_LENGTH, 0.0);
                        let d = (goal - p).length();
                        let Some(b) = bands.iter().position(|&(lo, hi, _)| d >= lo && d < hi) else { continue };
                        open = Some(b);
                        flying = Some((b, by, e.t));
                        if std::env::var("TRACE").is_ok() && b == 0 {
                            println!("--- {:.1} shot by {} ({}) from {:.1} m, control_since {:.1}", e.t, sh.name, by, d, sh.control_since);
                            let from = sim.events[..seen - 1].iter().rposition(|x| x.t < e.t - 8.0).unwrap_or(0);
                            for x in &sim.events[from..seen - 1] {
                                println!("    {}", x.describe(|i| format!("{}({i})", st.players[i].name)));
                            }
                        }
                        let a = &mut acc[b];
                        a.shots += 1;
                        a.first_time += first_time as u32;
                        a.on_ball += if first_time { 0.0 } else { (st.time - sh.control_since).max(0.0) };
                        a.after_pass += (e.t - passed_to[by] < 3.0) as u32;
                        let in_box = |q: Vec2| q.x > HALF_LENGTH - PENALTY_AREA_DEPTH && q.y.abs() < PENALTY_AREA_HALF_WIDTH;
                        let post_l = Vec2::new(HALF_LENGTH, GOAL_HALF_WIDTH);
                        let post_r = Vec2::new(HALF_LENGTH, -GOAL_HALF_WIDTH);
                        let mut nearest: Option<(f32, usize)> = None;
                        for o in st.squad(side.opponent()) {
                            if o.is_gk() {
                                continue;
                            }
                            let q = fr.to_team(o.pos);
                            let dd = (q - p).length();
                            a.within2 += (dd < 2.0) as u32;
                            a.within4 += (dd < 4.0) as u32 as f32;
                            a.def_in_box += in_box(q) as u32 as f32;
                            a.cone += in_triangle(q, p, post_l, post_r) as u32 as f32;
                            if nearest.is_none_or(|(n, _)| dd < n) {
                                nearest = Some((dd, o.id));
                            }
                        }
                        for m in st.squad(side) {
                            a.att_in_box += in_box(fr.to_team(m.pos)) as u32 as f32;
                        }
                        if let Some((dd, id)) = nearest {
                            a.nearest += dd;
                            let q = fr.to_team(st.players[id].pos);
                            a.nearest_goal_side += (q.x > p.x) as u32;
                            let label = st.players[id].debug.label.split(" (").next().unwrap_or("").to_string();
                            *a.labels.entry(label).or_default() += 1;
                        }
                        a.line_depth += st.team_view(side.opponent()).def_block.line_x + HALF_LENGTH;
                        let back = track.iter().rev().find(|(t, _)| *t <= e.t - 5.0).map(|(_, w)| fr.to_team(*w).x);
                        a.ball_5s_ago += HALF_LENGTH - back.unwrap_or(p.x);
                    }
                    _ => {}
                }
            }
        }
    }
    println!("{} matches, shots by distance band (shooting team's frame)", matches);
    for ((_, _, name), a) in bands.iter().zip(&acc) {
        let n = a.shots.max(1) as f32;
        println!("\n{name}: {} shots ({:.1} per match), {} goals ({:.0} %)", a.shots, a.shots as f32 / matches as f32, a.goals, 100.0 * a.goals as f32 / n);
        println!("  shooter: first time {:.0} %, after a pass within 3 s {:.0} %, mean time on the ball {:.2} s",
            100.0 * a.first_time as f32 / n, 100.0 * a.after_pass as f32 / n, a.on_ball / n);
        println!("  defenders: nearest {:.1} m (goal-side {:.0} %), any within 2 m {:.0} %, mean within 4 m {:.2}, in the cone {:.2}",
            a.nearest / n, 100.0 * a.nearest_goal_side as f32 / n, 100.0 * a.within2 as f32 / n, a.within4 / n, a.cone / n);
        println!("  in the box: {:.1} defenders (outfield), {:.1} attackers; defensive line {:.1} m from goal; ball 5 s earlier {:.1} m from goal",
            a.def_in_box / n, a.att_in_box / n, a.line_depth / n, a.ball_5s_ago / n);
        let o = a.outcome;
        let m = o.iter().sum::<u32>().max(1) as f32;
        println!("  outcome: goal {:.0} %, saved {:.0} %, blocked {:.0} %, off target / other {:.0} %",
            100.0 * o[0] as f32 / m, 100.0 * o[1] as f32 / m, 100.0 * o[2] as f32 / m, 100.0 * o[3] as f32 / m);
        let mut l: Vec<_> = a.labels.iter().collect();
        l.sort_by(|x, y| y.1.cmp(x.1));
        let top: Vec<String> = l.iter().take(6).map(|(k, v)| format!("{k} {:.0} %", 100.0 * **v as f32 / n)).collect();
        println!("  nearest defender doing: {}", top.join(", "));
    }
}
