//! Tackle diagnostics: what the carrier was doing when tackled, how long he had the ball, outcome shares.
//! `cargo run -p match_engine --release --example tackles -- [matches] [minutes]`

use std::collections::BTreeMap;

use match_engine::Simulation;
use match_engine::events::EventKind;

fn main() {
    let mut args = std::env::args().skip(1);
    let matches: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(2);
    let mins: f32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(90.0);
    let mut by_label: BTreeMap<String, (u32, u32)> = BTreeMap::new();
    let mut held = [0u32; 6];
    let mut total = 0;
    let mut controls = 0u32;
    let mut reasons = [0u32; 6];
    // Loose-ball / nearer-ball tackles (ball nearer the defender than the carrier before the step): where the
    // defender stood relative to the carrier's carry direction (front < 60°, side, behind > 120°), ball-carrier
    // distance, and whether the defender was nearer to the ball than the carrier because the touch ran towards him.
    let mut geo = [[0u32; 3]; 2];
    let mut d_car_sum = [0.0f32; 2];
    // Ball nearer the defender: ball speed (m/s), carrier speed along the ball's direction, time since his last touch,
    // ball behind the carrier's motion (he ran past it / turned away from it).
    let mut far: Vec<(f32, f32, f32, bool)> = Vec::new();
    // ... settling (no decision since control) and the angle between his velocity and his carry direction (°).
    let mut far2: Vec<(bool, f32)> = Vec::new();
    for seed in 1..=matches {
        let mut sim = Simulation::from_builtin("northbridge", "harbour", seed);
        let mut seen = 0;
        let mut last_ctrl = None;
        while sim.state.time < mins * 60.0 && !sim.finished() {
            // Snapshot before the step: the tackle resolves inside it.
            let before: Vec<(String, f32, String)> = sim
                .state
                .players
                .iter()
                .map(|p| (p.debug.label.clone(), sim.state.time - p.control_since, format!("{:?}", p.intent).split([' ', '{', '(']).next().unwrap_or("").to_string()))
                .collect();
            let pos: Vec<(glam::Vec2, glam::Vec2)> = sim.state.players.iter().map(|p| (p.pos, p.vel)).collect();
            let carry: Vec<Option<glam::Vec2>> = sim
                .state
                .players
                .iter()
                .map(|p| if let match_engine::player::Intent::Carry { dir, .. } = p.intent { Some(dir) } else { None })
                .collect();
            let bp = sim.state.ball.ground_pos();
            let bv = sim.state.ball.vel.truncate();
            let last_touch: Vec<f32> = sim.state.players.iter().map(|p| p.touch_cooldown_until - 0.2).collect();
            let t_now = sim.state.time;
            let settling: Vec<bool> = sim.state.players.iter().map(|p| p.last_decision < p.control_since).collect();
            sim.step();
            if sim.state.control != last_ctrl {
                if sim.state.control.is_some() {
                    controls += 1;
                }
                last_ctrl = sim.state.control;
            }
            while seen < sim.events.len() {
                let e = sim.events[seen].clone();
                seen += 1;
                if let EventKind::Tackle { by, on, outcome, .. } = e.kind {
                    total += 1;
                    let (d_def, d_car) = ((bp - pos[by].0).length(), (bp - pos[on].0).length());
                    if let Some(dir) = carry[on] {
                        let k = usize::from(d_def >= d_car);
                        let rel = (pos[by].0 - pos[on].0).normalize_or_zero().dot(dir);
                        geo[k][if rel > 0.5 { 0 } else if rel < -0.5 { 2 } else { 1 }] += 1;
                        d_car_sum[k] += d_car;
                        if k == 0 {
                            let (cp, cv) = pos[on];
                            far.push((bv.length(), cv.dot(bv.normalize_or_zero()), t_now - last_touch[on], (bp - cp).dot(cv) < 0.0));
                            far2.push((settling[on], cv.normalize_or_zero().dot(dir).clamp(-1.0, 1.0).acos().to_degrees()));
                        }
                    }
                    let (label, held_s, intent) = &before[on];
                    let key = format!("{intent:<8} {}", label.split(" →").next().unwrap_or(label).split(" (").next().unwrap_or(label));
                    let ent = by_label.entry(key).or_default();
                    ent.0 += 1;
                    if !matches!(outcome, match_engine::duel::TackleOutcome::Beaten) {
                        ent.1 += 1;
                    }
                    held[((held_s / 0.5) as usize).min(5)] += 1;
                }
            }
        }
        for t in &sim.stats.teams {
            for (r, c) in reasons.iter_mut().zip(t.tackle_reasons) {
                *r += c;
            }
        }
    }
    let n = matches as f32;
    println!("tackles per match {:.1}, controls per match {:.1}", total as f32 / n, controls as f32 / n);
    println!("time on ball when tackled (0.5 s bins): {:?}", held);
    let names = ["BOX-01 shot", "IND-03 last ditch", "loose touch", "odds, ball nearer him", "odds, controlled", "aggression"];
    println!(
        "why committed, per match: {}",
        names.iter().zip(reasons).map(|(k, c)| format!("{k} {:.1}", c as f32 / n)).collect::<Vec<_>>().join(", ")
    );
    for (k, name) in ["ball nearer the defender", "ball nearer the carrier"].iter().enumerate() {
        let t: u32 = geo[k].iter().sum();
        println!(
            "carry tackles, {name}: {:.1}/match, defender in front / side / behind the carry {:?}, ball from carrier mean {:.2} m",
            t as f32 / n,
            geo[k].iter().map(|&c| (100.0 * c as f32 / t.max(1) as f32).round()).collect::<Vec<_>>(),
            d_car_sum[k] / t.max(1) as f32
        );
    }
    let q = |mut v: Vec<f32>| {
        v.sort_by(f32::total_cmp);
        if v.is_empty() { [f32::NAN; 3] } else { [v[v.len() / 4], v[v.len() / 2], v[3 * v.len() / 4]] }
    };
    println!(
        "  ball nearer the defender: ball speed p25/50/75 {:?} m/s, carrier speed along it {:?}, since his last touch {:?} s, ball behind his motion {:.0}%",
        q(far.iter().map(|f| f.0).collect()),
        q(far.iter().map(|f| f.1).collect()),
        q(far.iter().map(|f| f.2).collect()),
        100.0 * far.iter().filter(|f| f.3).count() as f32 / far.len().max(1) as f32
    );
    let nf = far2.len().max(1) as f32;
    println!(
        "  settling {:.0}%; velocity vs carry direction <30° {:.0}%, 30–90° {:.0}%, >90° {:.0}%",
        100.0 * far2.iter().filter(|f| f.0).count() as f32 / nf,
        100.0 * far2.iter().filter(|f| f.1 < 30.0).count() as f32 / nf,
        100.0 * far2.iter().filter(|f| (30.0..90.0).contains(&f.1)).count() as f32 / nf,
        100.0 * far2.iter().filter(|f| f.1 >= 90.0).count() as f32 / nf
    );
    let mut v: Vec<_> = by_label.into_iter().collect();
    v.sort_by_key(|e| std::cmp::Reverse(e.1.0));
    for (k, (c, w)) in v.iter().take(15) {
        println!("{:>6.1} per match ({:>3.0}% won)  {k}", *c as f32 / n, 100.0 * *w as f32 / *c as f32);
    }
}
