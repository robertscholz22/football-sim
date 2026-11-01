//! Finishing bench: placed shots (far corner, 0.8 of the kicker's maximum speed) with execution error, from fixed
//! spots against a keeper set on his GK-01 spot and nobody else near, by shooting skill and foot. Prints the goal rate
//! next to the xG the shooter would be given (`shot_xg_parts`), so the finishing factor can be checked against what
//! the physics converts. `cargo run -p match_engine --release --example finishing -- [trials]`
//! Env overrides: `WEAK` (weak_foot_max_factor), `EXP` (xg_finish_exp), `REF` (xg_ref_skill), `WF` (weak-foot
//! attribute, default 9), `ANGLE` (angle_shot), `SPEED` (fraction of maximum kick speed, default 0.8).

use glam::Vec2;
use match_engine::ai::analysis::{ShotHow, gk01_spot, shot_xg_parts};
use match_engine::ball::KickSpec;
use match_engine::events::EventKind;
use match_engine::pitch::{GOAL_HALF_WIDTH, HALF_LENGTH, Side};
use match_engine::player::{Foot, KickKind, KickPlan};
use match_engine::scenario::Scenario;

fn env(name: &str) -> Option<f32> {
    std::env::var(name).ok().and_then(|s| s.parse().ok())
}

#[derive(Default, Clone, Copy)]
struct Out {
    goals: u32,
    on_target: u32,
    xg: f64,
    basic: f64,
}

/// Goals, shots on target and mean xG over `trials` shots from team-frame `spot` by a shooter with shooting
/// attributes `skill`, struck with his weak foot or not, at `speed_f` of his maximum kick speed.
fn bench(spot: Vec2, skill: f32, weak: bool, speed_f: f32, trials: u64) -> Out {
    let shooter = 9; // home striker
    let mut out = Out::default();
    for seed in 0..trials {
        let mut sc = Scenario::new(7000 + seed);
        let cfg = &mut sc.sim.cfg;
        if let Some(v) = env("WEAK") {
            cfg.execution.weak_foot_max_factor = v;
        }
        if let Some(v) = env("ANGLE") {
            cfg.execution.angle_shot = v;
        }
        if let Some(v) = env("EXP") {
            cfg.ai.xg_finish_exp = v;
        }
        if let Some(v) = env("REF") {
            cfg.ai.xg_ref_skill = v;
        }
        let fr = sc.sim.state.frame(Side::Home);
        for id in 0..22 {
            if id != shooter && !sc.sim.state.players[id].is_gk() {
                let y = if id < 11 { -30.0 } else { 30.0 };
                sc.place(id, fr.to_world(Vec2::new(-20.0 + id as f32, y)));
            }
        }
        let gk = (11..22).find(|&i| sc.sim.state.players[i].is_gk()).unwrap();
        sc.place(gk, fr.to_world(gk01_spot(spot, &sc.sim.cfg)));
        sc.place(shooter, fr.to_world(spot));
        {
            let p = &mut sc.sim.state.players[shooter];
            (p.attrs.finishing, p.attrs.long_shots) = (skill, skill);
            p.attrs.weak_foot = env("WF").unwrap_or(42.0);
            p.confidence = 0.5;
            p.energy = 1.0;
        }
        let far = if spot.y > 0.0 { -1.0 } else { 1.0 };
        let target_t = Vec2::new(HALF_LENGTH, far * (GOAL_HALF_WIDTH - 0.5));
        let target = fr.to_world(target_t);
        sc.sim.state.players[shooter].facing = (target - fr.to_world(spot)).normalize();
        sc.give_ball(shooter);
        let pref = sc.sim.state.players[shooter].attrs.preferred_foot;
        let foot = match (pref, weak) {
            (f, false) => f,
            (Foot::Right, true) => Foot::Left,
            (Foot::Left, true) => Foot::Right,
        };
        let parts = shot_xg_parts(&sc.sim.state, &sc.sim.cfg, shooter, spot, ShotHow { foot: Some(foot), ..Default::default() });
        out.xg += parts.total as f64 / trials as f64;
        out.basic += parts.basic as f64 / trials as f64;
        let speed = speed_f * sc.sim.state.players[shooter].kin.max_kick_speed;
        let plan = KickPlan {
            spec: KickSpec::Through { target: target.extend(0.35), speed, sidespin: 0.0, topspin: 0.0 },
            kind: KickKind::Shot,
            receiver: None,
            target,
            arrival_t: (target_t - spot).length() / speed,
            xg: parts.total,
            speed,
            foot: Some(foot),
        };
        let n0 = sc.sim.events.len();
        let now = sc.sim.state.time;
        sc.sim.execute_kick(shooter, plan, false, now);
        out.on_target += sc.sim.state.flight.is_some_and(|f| f.on_goal) as u32;
        for _ in 0..200 {
            sc.sim.step();
            if sc.sim.state.flight.is_none() || !sc.sim.state.is_live() {
                break;
            }
        }
        out.goals += sc.sim.events[n0..].iter().any(|e| matches!(e.kind, EventKind::Goal { .. })) as u32;
    }
    out
}

fn main() {
    let trials: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(300);
    let spots = [(8.0f32, 0.0f32), (12.0, 0.0), (16.0, 0.0), (20.0, 0.0), (25.0, 0.0), (30.0, 0.0), (12.0, 9.0), (18.0, 12.0), (25.0, 14.0)];
    let skills = [6.0f32, 11.0, 16.0];
    let mut jobs = Vec::new();
    for &(d, y) in &spots {
        for &s in &skills {
            for weak in [false, true] {
                jobs.push((d, y, s, weak));
            }
        }
    }
    let speed_f = env("SPEED").unwrap_or(0.8);
    let results: Vec<Out> = std::thread::scope(|scope| {
        let handles: Vec<_> = jobs
            .iter()
            .map(|&(d, y, s, weak)| scope.spawn(move || bench(Vec2::new(HALF_LENGTH - d, y), s, weak, speed_f, trials)))
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let n = trials as f64;
    println!("{trials} shots per row at {speed_f} of max speed; keeper set, nobody else near.");
    println!("conv = goal rate, on = on target, xG = the shooter's xG, basic = xg_basic (reference finisher, strong foot)");
    println!(
        "{:<12}{:>6}{:>8}  {:>26}  {:>26}  {:>12}",
        "spot", "skill", "basic", "strong: conv on% / xG", "weak: conv on% / xG", "weak/strong"
    );
    let mut sum = [0.0f64; 4];
    for (i, &(d, y, s, weak)) in jobs.iter().enumerate() {
        if weak {
            continue;
        }
        let (a, b) = (results[i], results[i + 1]);
        let (cs, cw) = (a.goals as f64 / n, b.goals as f64 / n);
        sum = [sum[0] + cs, sum[1] + a.xg, sum[2] + cw, sum[3] + b.xg];
        println!(
            "{:<12}{:>6}{:>8.3}  {:>9.3} {:>4.0}% / {:.3}      {:>9.3} {:>4.0}% / {:.3}      {:>5.2} / {:.2}",
            format!("{d} m, {y} w"),
            s,
            a.basic,
            cs,
            100.0 * a.on_target as f64 / n,
            a.xg,
            cw,
            100.0 * b.on_target as f64 / n,
            b.xg,
            cw / cs.max(1e-9),
            b.xg / a.xg.max(1e-9)
        );
    }
    println!("total: strong conv {:.3} xG {:.3}; weak conv {:.3} xG {:.3}", sum[0], sum[1], sum[2], sum[3]);
}
