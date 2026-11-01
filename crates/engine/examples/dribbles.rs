//! Dribble touches: for every touch of a carrier, the angle between his running direction and the touch, the
//! speed surplus of the ball over the carrier, and what followed (next touch, ball lost without a defender, other).
//! Usage: dribbles [matches] [minutes]

use std::collections::BTreeMap;

use match_engine::Simulation;
use match_engine::player::Intent;

struct Touch {
    carrier: usize,
    angle_deg: f32,
    surplus: f32,
    max_sep: f32,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let matches: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(3);
    let minutes: f32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(90.0);
    // angle bin → (touches, kept, lost silently, other, sum of max separation of lost)
    let mut by_angle: BTreeMap<u32, [f32; 5]> = BTreeMap::new();
    let mut by_surplus: BTreeMap<u32, [f32; 5]> = BTreeMap::new();
    let (mut lost_speed_ratio, mut n_lost) = (0.0f32, 0u32);
    for seed in 1..=matches {
        let mut sim = Simulation::from_builtin("northbridge", "harbour", seed);
        sim.cfg.timing.half_length_s = minutes * 30.0;
        sim.state.half_length = sim.cfg.timing.half_length_s;
        let mut open: Option<Touch> = None;
        let mut prev_vel = sim.state.ball.ground_vel();
        let mut seen = 0usize;
        while !sim.finished() {
            sim.step();
            let quiet = seen == sim.events.len();
            seen = sim.events.len();
            let ball = sim.state.ball;
            let v = ball.ground_vel();
            let close = |c: usize| (ball.ground_pos() - sim.state.players[c].pos).length();
            let mut finish = |t: Touch, outcome: usize| {
                let ab = (t.angle_deg / 30.0) as u32 * 30;
                let sb = ((t.surplus.max(0.0) / 1.0) as u32).min(6);
                for e in [by_angle.entry(ab).or_default(), by_surplus.entry(sb).or_default()] {
                    e[0] += 1.0;
                    e[outcome] += 1.0;
                    if outcome == 2 {
                        e[4] += t.max_sep;
                    }
                }
            };
            if let Some(t) = open.as_mut() {
                t.max_sep = t.max_sep.max(close(t.carrier));
            }
            match sim.state.control {
                Some(c) if matches!(sim.state.players[c].intent, Intent::Carry { .. }) => {
                    // A new touch: the ball was kicked from the carrier's feet this tick.
                    if (v - prev_vel).length() > 0.8 && close(c) < 1.2 && quiet {
                        if let Some(t) = open.take() {
                            finish(t, 1);
                        }
                        let p = &sim.state.players[c];
                        let dir = v.normalize_or_zero();
                        let pv = p.vel;
                        let angle = if pv.length() < 0.5 { 0.0 } else { pv.angle_to(dir).abs().to_degrees() };
                        open = Some(Touch {
                            carrier: c,
                            angle_deg: angle,
                            surplus: v.length() - pv.dot(dir),
                            max_sep: 0.0,
                        });
                    }
                }
                Some(c) if open.as_ref().is_some_and(|t| t.carrier == c) => {}
                None if quiet && open.is_some() => {
                    let t = open.take().unwrap();
                    lost_speed_ratio += sim.state.players[t.carrier].vel.length() / v.length().max(0.1);
                    n_lost += 1;
                    finish(t, 2);
                }
                _ => {
                    if let Some(t) = open.take() {
                        finish(t, 3);
                    }
                }
            }
            prev_vel = v;
        }
    }
    let per = |n: f32| n / matches as f32;
    let print = |title: &str, m: &BTreeMap<u32, [f32; 5]>, label: &dyn Fn(u32) -> String| {
        println!("{title}");
        println!("  bin          touches/match   kept    lost (no defender)   other   mean max gap when lost");
        for (k, e) in m {
            println!(
                "  {:<12} {:>10.1}   {:>5.0} %   {:>8.1} %          {:>5.0} %   {:>6.1} m",
                label(*k),
                per(e[0]),
                100.0 * e[1] / e[0],
                100.0 * e[2] / e[0],
                100.0 * e[3] / e[0],
                e[4] / e[2].max(1.0)
            );
        }
    };
    print("by angle between carrier's running direction and the touch:", &by_angle, &|k| format!("{k:>3}–{:<3}°", k + 30));
    print("\nby ball speed surplus over the carrier's speed along the touch:", &by_surplus, &|k| {
        if k == 6 { "6+ m/s".into() } else { format!("{k}–{} m/s", k + 1) }
    });
    println!(
        "\nlost touches: carrier speed / ball speed at the moment of loss = {:.2}",
        lost_speed_ratio / n_lost.max(1) as f32
    );
}
