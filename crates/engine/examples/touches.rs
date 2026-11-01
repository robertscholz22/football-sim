//! First touches: control rate by ball speed relative to the receiver, and what a heavy touch does to the ball
//! (how far it gets from the player, who gets it next). Usage: touches [matches] [minutes]

use match_engine::Simulation;
use match_engine::Vec2;
use match_engine::events::EventKind;

const BINS: [f32; 5] = [5.0, 10.0, 15.0, 20.0, 25.0];

struct Heavy {
    by: usize,
    t0: f32,
    max_sep: f32,
    out_speed: f32,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let matches: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(2);
    let minutes: f32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(90.0);
    // relative-speed bin → (controlled, heavy)
    let mut bins = [(0u32, 0u32); 6];
    let mut sep_hist = [0u32; 6];
    // next possession after a heavy touch: same player, teammate, opponent, none within 3 s
    let mut next = [0u32; 4];
    let (mut out_speed, mut n_heavy) = (0.0f32, 0u32);
    for seed in 1..=matches {
        let mut sim = Simulation::from_builtin("northbridge", "harbour", seed);
        sim.cfg.timing.half_length_s = minutes * 30.0;
        sim.state.half_length = sim.cfg.timing.half_length_s;
        let mut seen = 0usize;
        let mut prev_ball = sim.state.ball;
        let mut prev_vel: Vec<Vec2> = sim.state.players.iter().map(|p| p.vel).collect();
        let mut prev_control = sim.state.control;
        let mut open: Option<Heavy> = None;
        while !sim.finished() {
            sim.step();
            let now = sim.state.time;
            let bin = |id: usize| {
                let rel = (prev_ball.ground_vel() - prev_vel[id]).length();
                BINS.iter().position(|&b| rel < b).unwrap_or(5)
            };
            let tackled = sim.events[seen..].iter().any(|e| matches!(e.kind, EventKind::Tackle { .. }));
            for e in &sim.events[seen..] {
                if let EventKind::Miscontrol { by } = e.kind {
                    bins[bin(by)].1 += 1;
                    if let Some(h) = open.take() {
                        sep_hist[((h.max_sep / 1.0) as usize).min(5)] += 1;
                        next[3] += 1;
                    }
                    let v = (sim.state.ball.ground_vel() - sim.state.players[by].vel).length();
                    open = Some(Heavy { by, t0: now, max_sep: 0.0, out_speed: v });
                }
            }
            if let Some(c) = sim.state.control
                && prev_control != Some(c)
                && !tackled
                && prev_ball.speed() > 1.0
            {
                bins[bin(c)].0 += 1;
            }
            if let Some(h) = open.as_mut() {
                let sep = (sim.state.ball.ground_pos() - sim.state.players[h.by].pos).length();
                h.max_sep = h.max_sep.max(sep);
                let done = match sim.state.control {
                    Some(c) if c == h.by => Some(0),
                    Some(c) if (c < 11) == (h.by < 11) => Some(1),
                    Some(_) => Some(2),
                    None if now - h.t0 > 3.0 => Some(3),
                    None => None,
                };
                if let Some(k) = done {
                    let h = open.take().unwrap();
                    next[k] += 1;
                    sep_hist[((h.max_sep / 1.0) as usize).min(5)] += 1;
                    out_speed += h.out_speed;
                    n_heavy += 1;
                }
            }
            seen = sim.events.len();
            prev_ball = sim.state.ball;
            prev_control = sim.state.control;
            for (i, p) in sim.state.players.iter().enumerate() {
                prev_vel[i] = p.vel;
            }
        }
    }
    let per = |n: u32| n as f32 / matches as f32;
    let total_heavy: u32 = bins.iter().map(|b| b.1).sum();
    let total_ctrl: u32 = bins.iter().map(|b| b.0).sum();
    println!("first touches per match: {:.0} controlled, {:.1} heavy ({:.1} %)", per(total_ctrl), per(total_heavy),
        100.0 * total_heavy as f32 / (total_ctrl + total_heavy).max(1) as f32);
    println!("  ball speed relative to the player   attempts/match   heavy");
    let names = ["< 5 m/s", "5–10", "10–15", "15–20", "20–25", "25+"];
    for (nm, (c, h)) in names.iter().zip(bins) {
        println!("  {nm:<34} {:>10.1}   {:>5.1} %", per(c + h), 100.0 * h as f32 / (c + h).max(1) as f32);
    }
    let n: u32 = sep_hist.iter().sum();
    println!("heavy touch: ball leaves the foot at {:.1} m/s relative to the player (mean)", out_speed / n_heavy.max(1) as f32);
    println!("  furthest the ball gets from him before someone has it: 0–1 m {:.0} %, 1–2 {:.0} %, 2–3 {:.0} %, 3–4 {:.0} %, 4–5 {:.0} %, 5+ {:.0} %",
        100.0 * sep_hist[0] as f32 / n as f32, 100.0 * sep_hist[1] as f32 / n as f32, 100.0 * sep_hist[2] as f32 / n as f32,
        100.0 * sep_hist[3] as f32 / n as f32, 100.0 * sep_hist[4] as f32 / n as f32, 100.0 * sep_hist[5] as f32 / n as f32);
    let m: u32 = next.iter().sum();
    println!("  next on the ball: same player {:.0} %, teammate {:.0} %, opponent {:.0} %, nobody within 3 s {:.0} %",
        100.0 * next[0] as f32 / m as f32, 100.0 * next[1] as f32 / m as f32, 100.0 * next[2] as f32 / m as f32, 100.0 * next[3] as f32 / m as f32);
}
