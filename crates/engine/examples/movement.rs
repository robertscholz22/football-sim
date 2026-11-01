//! Off-ball movement quality: speed profile, sharp reversals at speed, and how far / how often movement targets
//! jump between decisions (per positioning label). Usage: movement [matches] [minutes]

use std::collections::BTreeMap;

use match_engine::Simulation;
use match_engine::Vec2;
use match_engine::match_state::PlayPhase;

const BANDS: [f32; 4] = [2.0, 4.0, 5.5, 7.0];

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let matches: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(2);
    let minutes: f32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(90.0);
    let mut band_time = [0.0f64; 5];
    let mut total_time = 0.0f64;
    let mut dist = 0.0f64;
    let mut reversals = 0u32;
    // label → (decisions, jumps > 2 m, sum of jump length, heading changes > 60° while > 2 m from target)
    let mut by_label: BTreeMap<String, (u32, u32, f64, u32)> = BTreeMap::new();
    let mut jump_hist = [0u32; 6];
    let (mut anchor_jump, mut offset_jump, mut n_dec) = (0.0f64, 0.0f64, 0u32);
    // Turning: summed |angular speed| of running direction (speed > 1 m/s) and of facing, and the time sampled.
    let (mut run_turn, mut run_t, mut face_turn, mut fast_turn_t) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    // Turning by distance to the target: < 1, 1–2, 2–4, 4+ m → (turn degrees, time).
    let mut turn_by_dist = [(0.0f64, 0.0f64); 4];
    let mut turn_by_speed = [(0.0f64, 0.0f64); 4];
    let mut turn_by_label: BTreeMap<String, (f64, f64, f64)> = BTreeMap::new();
    let mut label_changes = 0u32;
    let mut transitions: BTreeMap<String, u32> = BTreeMap::new();
    let first: u64 = std::env::var("SEED").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
    let mut fine = [0.0f64; 16];
    for seed in first..first + matches {
        let (t0, d0, w0) = (total_time, dist, band_time[0]);
        let mut sim = Simulation::from_builtin("northbridge", "harbour", seed);
        sim.cfg.timing.half_length_s = minutes * 30.0;
        sim.state.half_length = sim.cfg.timing.half_length_s;
        // Experiments: DRIFT=<σ max>, NOISE=<score noise max>, COMMIT=<cost per metre>, LASTDITCH=<s>, TWORST=<temperature>.
        let env = |k: &str| std::env::var(k).ok().and_then(|v| v.parse::<f32>().ok());
        if let Some(v) = env("DRIFT") {
            sim.cfg.ai.drift_sigma_max = v;
        }
        if let Some(v) = env("NOISE") {
            sim.cfg.ai.position_noise_max = v;
        }
        if let Some(v) = env("COMMIT") {
            sim.cfg.ai.commit_cost = v;
        }
        if let Some(v) = env("FULLDIST") {
            sim.cfg.ai.position_full_speed_dist = v;
        }
        if let Some(v) = env("MINURG") {
            sim.cfg.ai.position_min_urgency = v;
        }
        if let Some(v) = env("SHIFT") {
            sim.cfg.tactics.def05_shift_speed = v;
        }
        if let Some(v) = env("SETTLE") {
            sim.cfg.ai.settle_radius = v;
        }
        if let Some(v) = env("LASTDITCH") {
            sim.cfg.tactics.ind03_last_ditch_s = v;
        }
        if let Some(v) = env("TWORST") {
            sim.cfg.ai.temperature_worst = v;
        }
        if let Some(v) = env("TEXP") {
            sim.cfg.ai.temperature_exp = v;
        }
        if let Some(v) = env("TACKLE") {
            sim.cfg.duel.tackle_slope = v;
            sim.cfg.duel.aerial_slope = v;
        }
        if let Some(v) = env("SKILLSLOPE") {
            sim.cfg.duel.first_touch_skill_slope = v;
            sim.cfg.duel.save_skill_slope = 0.8 * v;
        }
        if let Some(v) = env("ERRWORST") {
            sim.cfg.execution.error_scale_worst = v;
            sim.cfg.execution.error_scale_best = env("ERRBEST").unwrap_or(0.4);
        }
        if env("OLDPHYS").is_some() {
            let pc = &mut sim.cfg.player;
            (pc.top_speed_min, pc.top_speed_max) = (7.0, 10.0);
            (pc.accel_min, pc.accel_max) = (6.0, 10.5);
            (pc.decel_min, pc.decel_max) = (5.0, 8.0);
            (pc.lateral_accel_min, pc.lateral_accel_max) = (4.0, 7.5);
        }
        let dt = sim.cfg.timing.dt as f64;
        let lag = (0.5 / dt) as usize;
        let n = sim.state.players.len();
        let mut hist: Vec<Vec<Vec2>> = vec![Vec::new(); n];
        let mut prev_target: Vec<Vec2> = sim.state.players.iter().map(|p| p.target).collect();
        let mut prev_anchor: Vec<Vec2> = sim.state.players.iter().map(|p| p.debug.anchor).collect();
        let mut prev_decision: Vec<f32> = sim.state.players.iter().map(|p| p.next_decision).collect();
        let mut refractory = vec![0.0f32; n];
        let mut prev_vel: Vec<Vec2> = vec![Vec2::ZERO; n];
        let mut prev_label: Vec<String> = vec![String::new(); n];
        let mut prev_facing: Vec<Vec2> = sim.state.players.iter().map(|p| p.facing).collect();
        while !sim.finished() {
            sim.step();
            if sim.state.phase != PlayPhase::Live {
                for (i, p) in sim.state.players.iter().enumerate() {
                    prev_target[i] = p.target;
                    prev_decision[i] = p.next_decision;
                    hist[i].clear();
                }
                continue;
            }
            let now = sim.state.time;
            for (i, p) in sim.state.players.iter().enumerate() {
                if p.is_gk() || sim.state.control == Some(i) {
                    hist[i].clear();
                    prev_target[i] = p.target;
                    prev_decision[i] = p.next_decision;
                    continue;
                }
                let s = p.vel.length() as f64;
                total_time += dt;
                face_turn += prev_facing[i].angle_to(p.facing).abs().to_degrees() as f64;
                if s > 1.0 && prev_vel[i].length() > 1.0 {
                    let w = prev_vel[i].angle_to(p.vel).abs().to_degrees() as f64 / dt;
                    run_turn += w * dt;
                    let dd = (p.target - p.pos).length();
                    let k = if dd < 1.0 { 0 } else if dd < 2.0 { 1 } else if dd < 4.0 { 2 } else { 3 };
                    turn_by_dist[k].0 += w * dt;
                    turn_by_dist[k].1 += dt;
                    let k = if s < 2.5 { 0 } else if s < 4.0 { 1 } else if s < 5.5 { 2 } else { 3 };
                    turn_by_speed[k].0 += w * dt;
                    turn_by_speed[k].1 += dt;
                    let e = turn_by_label.entry(p.debug.label.clone()).or_default();
                    e.0 += w * dt;
                    e.1 += dt;
                    e.2 += s * dt;
                    run_t += dt;
                    if w > 90.0 {
                        fast_turn_t += dt;
                    }
                }
                prev_vel[i] = p.vel;
                prev_facing[i] = p.facing;
                dist += s * dt;
                fine[((s / 0.5) as usize).min(15)] += dt;
                band_time[BANDS.iter().position(|&b| s < b as f64).unwrap_or(4)] += dt;
                hist[i].push(p.vel);
                if hist[i].len() > lag {
                    let old = hist[i][hist[i].len() - 1 - lag];
                    if s > 2.5 && old.length() > 2.5 && old.angle_to(p.vel).abs() > 2.1 && now > refractory[i] {
                        reversals += 1;
                        refractory[i] = now + 1.0;
                    }
                }
                // A new off-ball decision was taken this tick.
                if p.next_decision != prev_decision[i] {
                    let jump = (p.target - prev_target[i]).length();
                    if Some(&p.debug.label) != prev_label.get(i) {
                        label_changes += 1;
                        *transitions.entry(format!("{} → {}", prev_label[i], p.debug.label)).or_default() += 1;
                    }
                    let e = by_label.entry(p.debug.label.clone()).or_default();
                    e.0 += 1;
                    e.2 += jump as f64;
                    if jump > 2.0 {
                        e.1 += 1;
                    }
                    let (a, b) = (prev_target[i] - p.pos, p.target - p.pos);
                    if a.length() > 2.0 && b.length() > 2.0 && a.angle_to(b).abs() > 1.05 {
                        e.3 += 1;
                    }
                    jump_hist[((jump / 1.0) as usize).min(5)] += 1;
                    anchor_jump += (p.debug.anchor - prev_anchor[i]).length() as f64;
                    offset_jump += ((p.target - p.debug.anchor) - (prev_target[i] - prev_anchor[i])).length() as f64;
                    n_dec += 1;
                }
                prev_target[i] = p.target;
                prev_anchor[i] = p.debug.anchor;
                prev_label[i] = p.debug.label.clone();
                prev_decision[i] = p.next_decision;
            }
        }
        eprintln!("seed {seed}: mean speed {:.2} m/s, walking {:.0} %", (dist - d0) / (total_time - t0), 100.0 * (band_time[0] - w0) / (total_time - t0));
    }
    if std::env::var("HIST").is_ok() {
        for (i, f) in fine.iter().enumerate() {
            eprintln!("  {:>4.1}–{:<4.1} m/s {:>5.1} %", i as f64 * 0.5, i as f64 * 0.5 + 0.5, 100.0 * f / total_time);
        }
    }
    let player_minutes = total_time / 60.0;
    println!("outfield players off the ball, live play only");
    println!("  mean speed {:.2} m/s  (≈ {:.1} km per 90 min)", dist / total_time, dist / total_time * 5.4);
    let names = ["< 2 m/s (walk)", "2–4 (jog)", "4–5.5 (run)", "5.5–7 (high speed)", "> 7 (sprint)"];
    for (nm, t) in names.iter().zip(band_time) {
        println!("  {nm:<20} {:>5.1} % of time", 100.0 * t / total_time);
    }
    println!("  running direction turns {:.0}°/s on average while moving; turning faster than 90°/s {:.0} % of moving time", run_turn / run_t, 100.0 * fast_turn_t / run_t);
    let total_turn: f64 = turn_by_dist.iter().map(|t| t.0).sum();
    for (nm, t) in ["< 1 m", "1–2 m", "2–4 m", "4+ m"].iter().zip(turn_by_dist) {
        println!("    target {nm:<6}: {:>4.0} % of moving time, {:>4.0} % of all turning, {:>4.0}°/s", 100.0 * t.1 / run_t, 100.0 * t.0 / total_turn, t.0 / t.1);
    }
    for (nm, t) in ["1–2.5 m/s", "2.5–4", "4–5.5", "5.5+"].iter().zip(turn_by_speed) {
        println!("    speed {nm:<9}: {:>4.0} % of moving time, {:>4.0} % of all turning, {:>4.0}°/s", 100.0 * t.1 / run_t, 100.0 * t.0 / total_turn, t.0 / t.1);
    }
    let mut tl: Vec<_> = turn_by_label.into_iter().collect();
    tl.sort_by(|a, b| b.1.0.total_cmp(&a.1.0));
    for (l, (deg, t, sp)) in tl.iter().take(10) {
        println!("    {l:<16} {:>4.0} % of moving time, {:>4.0} % of all turning, {:>4.0}°/s, mean speed {:.1} m/s", 100.0 * t / run_t, 100.0 * deg / total_turn, deg / t, sp / t);
    }
    println!("  positioning kind changes: {:.1} per player-minute", label_changes as f64 / player_minutes);
    let mut tv: Vec<_> = transitions.into_iter().collect();
    tv.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for (t, n) in tv.iter().take(12) {
        println!("      {t:<32} {:>5.2} per player-minute", *n as f64 / player_minutes);
    }
    println!("  facing turns {:.0}°/s on average", face_turn / total_time);
    println!("  reversals (> 120° within 0.5 s, both above 2.5 m/s): {:.2} per player-minute", reversals as f64 / player_minutes);
    let decisions: u32 = by_label.values().map(|e| e.0).sum();
    println!("\noff-ball decisions: {:.1} per player-minute", decisions as f64 / player_minutes);
    println!("target jump per decision: 0–1 m {:.0} %, 1–2 {:.0} %, 2–3 {:.0} %, 3–4 {:.0} %, 4–5 {:.0} %, 5+ {:.0} %",
        100.0 * jump_hist[0] as f64 / decisions as f64, 100.0 * jump_hist[1] as f64 / decisions as f64,
        100.0 * jump_hist[2] as f64 / decisions as f64, 100.0 * jump_hist[3] as f64 / decisions as f64,
        100.0 * jump_hist[4] as f64 / decisions as f64, 100.0 * jump_hist[5] as f64 / decisions as f64);
    println!("mean per decision: anchor moves {:.2} m, point relative to anchor moves {:.2} m", anchor_jump / n_dec as f64, offset_jump / n_dec as f64);
    println!("\n  label                 share   mean jump   jumps > 2 m   direction flips > 60°");
    let mut v: Vec<_> = by_label.into_iter().collect();
    v.sort_by_key(|(_, e)| std::cmp::Reverse(e.0));
    for (l, e) in v.iter().take(14) {
        println!(
            "  {l:<20} {:>5.1} %   {:>6.2} m   {:>8.0} %   {:>10.0} %",
            100.0 * e.0 as f64 / decisions as f64,
            e.2 / e.0 as f64,
            100.0 * e.1 as f64 / e.0 as f64,
            100.0 * e.3 as f64 / e.0 as f64
        );
    }
}
