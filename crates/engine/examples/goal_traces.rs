//! Set K: the last 4.5 s before every goal that was not the scorer's own clean shot or header (own goals, deflections,
//! rebounds), every 0.1 s in the defending team's frame: the ball (position, velocity, control, whether its path goes
//! in), the keeper (position, velocity, body, movement label, the chaser he believes in) and the nearest outfield
//! defender to the ball. Usage: goal_traces [matches] (all matches run in parallel).

use std::collections::VecDeque;

use match_engine::Simulation;
use match_engine::events::EventKind;

fn main() {
    let matches: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(32);
    let out: Vec<String> = std::thread::scope(|scope| {
        let hs: Vec<_> = (1..=matches)
            .map(|seed| {
                scope.spawn(move || {
                    let mut sim = Simulation::from_builtin("northbridge", "harbour", seed);
                    let mut rows: VecDeque<(f32, [String; 2])> = VecDeque::new();
                    let mut seen = 0;
                    let mut text = String::new();
                    let mut tick = 0u64;
                    while !sim.finished() {
                        sim.step();
                        tick += 1;
                        if tick.is_multiple_of(5) {
                            let mut r: [String; 2] = Default::default();
                            for (k, side) in [match_engine::pitch::Side::Home, match_engine::pitch::Side::Away].into_iter().enumerate() {
                                let fr = sim.state.frame(side);
                                let Some(g) = sim.state.squad(side).iter().find(|p| p.is_gk()) else { continue };
                                let b = fr.to_team3(sim.state.ball.pos);
                                let bv = fr.to_team3(sim.state.ball.vel);
                                let gp = fr.to_team(g.pos);
                                let gv = fr.to_team(g.vel);
                                let near = sim
                                    .state
                                    .squad(side)
                                    .iter()
                                    .filter(|p| !p.is_gk())
                                    .map(|p| (p.pos - sim.state.ball.ground_pos()).length())
                                    .fold(f32::MAX, f32::min);
                                r[k] = format!(
                                    "t {:6.1} ball ({:6.1},{:5.1},{:4.1}) v ({:5.1},{:5.1}) |{:4.1}| ctl {:?} on_goal {} | gk ({:6.1},{:5.1}) v ({:4.1},{:4.1}) {:?} '{}' chaser {:?} | nearest def {:.1} m",
                                    sim.state.time,
                                    b.x,
                                    b.y,
                                    b.z,
                                    bv.x,
                                    bv.y,
                                    bv.truncate().length(),
                                    sim.state.control,
                                    sim.traj.on_goal,
                                    gp.x,
                                    gp.y,
                                    gv.x,
                                    gv.y,
                                    g.body,
                                    g.debug.label,
                                    g.beliefs.chaser,
                                    near
                                );
                            }
                            rows.push_back((sim.state.time, r));
                            while rows.len() > 45 {
                                rows.pop_front();
                            }
                        }
                        while seen < sim.events.len() {
                            let e = sim.events[seen].clone();
                            seen += 1;
                            let EventKind::Goal { side, scorer, own_goal, .. } = e.kind else { continue };
                            let prev = sim.events[..seen - 1]
                                .iter()
                                .rev()
                                .find(|p| !matches!(p.kind, EventKind::Restart { .. } | EventKind::Kickoff { .. }));
                            let shot = matches!(prev.map(|p| &p.kind), Some(EventKind::Shot { by, .. } | EventKind::Header { by }) if Some(*by) == scorer);
                            if shot && !own_goal {
                                continue;
                            }
                            let k = usize::from(side == match_engine::pitch::Side::Home);
                            text += &format!("== seed {seed} t {:.1}: goal for {side:?}, own goal {own_goal}, before: {:?}\n", e.t, prev.map(|p| &p.kind));
                            for p in sim.events[..seen].iter().rev().take(6).collect::<Vec<_>>().into_iter().rev() {
                                text += &format!("   event t {:.2}: {:?}\n", p.t, p.kind);
                            }
                            for (_, r) in &rows {
                                text += &format!("   {}\n", r[k]);
                            }
                        }
                    }
                    text
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    for t in out {
        print!("{t}");
    }
}
