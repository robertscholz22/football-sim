//! Kicks against the body: angle between a kick's direction and the kicker's facing / running direction just
//! before the kick, by kick kind, length and first-time. Usage: kicks [matches] [minutes]

use std::collections::BTreeMap;

use match_engine::Simulation;
use match_engine::Vec2;
use match_engine::events::EventKind;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let matches: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(2);
    let minutes: f32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(90.0);
    // group → [count, facing > 60°, facing > 90°, facing > 120°, running away from the kick (> 2 m/s), moving count]
    let mut groups: BTreeMap<String, [u32; 6]> = BTreeMap::new();
    for seed in 1..=matches {
        let mut sim = Simulation::from_builtin("northbridge", "harbour", seed);
        sim.cfg.timing.half_length_s = minutes * 30.0;
        sim.state.half_length = sim.cfg.timing.half_length_s;
        let mut prev: Vec<(Vec2, Vec2)> = sim.state.players.iter().map(|p| (p.facing, p.vel)).collect();
        let mut seen = 0usize;
        while !sim.finished() {
            sim.step();
            for e in &sim.events[seen..] {
                let (by, kind, len, ft) = match &e.kind {
                    EventKind::Pass { by, kind, length, first_time, .. } => (*by, format!("{kind:?}"), *length, *first_time),
                    EventKind::Shot { by, first_time, .. } => (*by, "Shot".to_string(), 0.0, *first_time),
                    _ => continue,
                };
                let dir = sim.state.ball.ground_vel().normalize_or_zero();
                if dir == Vec2::ZERO {
                    continue;
                }
                let (facing, vel) = prev[by];
                let band = if kind == "Shot" {
                    ""
                } else if len < 15.0 {
                    " <15 m"
                } else if len < 30.0 {
                    " 15–30 m"
                } else {
                    " 30+ m"
                };
                let key = format!("{kind}{band}{}", if ft { " (first time)" } else { "" });
                let g = groups.entry(key).or_default();
                let fa = facing.angle_to(dir).abs().to_degrees();
                g[0] += 1;
                g[1] += (fa > 60.0) as u32;
                g[2] += (fa > 90.0) as u32;
                g[3] += (fa > 120.0) as u32;
                if vel.length() > 2.0 {
                    g[5] += 1;
                    g[4] += (vel.dot(dir) < -2.0) as u32;
                }
            }
            seen = sim.events.len();
            for (i, p) in sim.state.players.iter().enumerate() {
                prev[i] = (p.facing, p.vel);
            }
        }
    }
    println!("kick vs body (facing / running direction just before the kick)");
    println!("  kind                              per match   facing >60°  >90°  >120°   running away (> 2 m/s against the kick)");
    for (k, g) in &groups {
        let pct = |n: u32, d: u32| 100.0 * n as f32 / d.max(1) as f32;
        println!(
            "  {k:<34} {:>7.1}     {:>5.0} %  {:>4.0} %  {:>4.0} %        {:>4.0} % of {:.0}",
            g[0] as f32 / matches as f32,
            pct(g[1], g[0]),
            pct(g[2], g[0]),
            pct(g[3], g[0]),
            pct(g[4], g[5]),
            g[5] as f32 / matches as f32
        );
    }
}
