//! Open-play passes by kind and length band: release speed, time from release to the receiver's control (completed
//! passes, as StatsBomb's pass `duration`), ball speed when he controls it, and the share intercepted (an opponent
//! controls the ball first). Completed passes are banded by the distance from release to where the receiver controls
//! the ball (StatsBomb's end location). Then the AI's predicted success of the chosen pass against the outcome;
//! `LOW=file` writes the ground passes it rated below 0.5 (best option, the chosen one's utility and probability).
//! `cargo run -p match_engine --release --example pass_speed -- [matches]`

use match_engine::Simulation;
use match_engine::events::EventKind;
use match_engine::match_state::side_of;
use match_engine::player::KickKind;

const BANDS: [(f32, f32); 5] = [(0.0, 10.0), (10.0, 20.0), (20.0, 30.0), (30.0, 45.0), (14.0, 18.0)];

/// Kind (0 ground, 1 lofted), length, release speed, duration (None: not completed), arrival speed, intercepted,
/// success probability the kicker's AI gave it.
type Rec = (usize, f32, f32, Option<f32>, f32, bool, f32);

/// Pass in flight: kind, length, release speed, release time, receiver, kicker, predicted p, release point.
type Open = (usize, f32, f32, f32, usize, usize, f32, match_engine::Vec2);

fn play(seed: u64) -> (Vec<Rec>, Vec<String>) {
    let mut sim = Simulation::from_builtin("northbridge", "harbour", seed);
    let mut out = Vec::new();
    let mut low = Vec::new();
    let mut open: Option<Open> = None;
    let mut seen = 0;
    let mut prev_ball_speed = 0.0;
    while !sim.finished() {
        sim.step();
        let st = &sim.state;
        while seen < sim.events.len() {
            let e = &sim.events[seen];
            seen += 1;
            if let EventKind::Pass { by, to: Some(to), kind, length, first_time, headed: false } = e.kind
                && st.is_live()
                && matches!(kind, KickKind::GroundPass | KickKind::LoftedPass)
                && let Some(f) = st.flight
            {
                if let Some(o) = open.take() {
                    out.push((o.0, o.1, o.2, None, 0.0, false, o.6));
                }
                let d = &st.players[by].debug;
                open = Some((usize::from(kind == KickKind::LoftedPass), length, f.speed, e.t, to, by, d.chosen_p, f.start_pos));
                if d.chosen_p < 0.5 && kind == KickKind::GroundPass {
                    // Low-p ground pass: first time?, best option by optimum utility, the chosen one's optimum and
                    // probability.
                    let label = d.label.trim_start_matches("receive → ").trim_start_matches("set up → ");
                    let best = d.options.iter().max_by(|a, b| a.1.total_cmp(&b.1));
                    let chosen = d.options.iter().find(|o| o.0 == label);
                    let kind_of = |l: &str| l.split_whitespace().next().unwrap_or("?").to_string();
                    low.push(format!(
                        "{} best={} chosen_is_best={} u_ch={:.3} u_best={:.3} prob_ch={:.2} n_opts={} role={:?} x={:.0}",
                        if first_time { "FT" } else { "--" },
                        best.map_or("?".into(), |b| kind_of(&b.0)),
                        best.is_some_and(|b| b.0 == label),
                        chosen.map_or(f32::NAN, |c| c.1),
                        best.map_or(f32::NAN, |b| b.1),
                        chosen.map_or(f32::NAN, |c| c.4),
                        d.options.len(),
                        st.players[by].role,
                        match_engine::pitch::HALF_LENGTH + st.frame(st.players[by].side).to_team(st.players[by].pos).x,
                    ));
                }
            }
            if matches!(e.kind, EventKind::Restart { .. } | EventKind::Kickoff { .. })
                && let Some(o) = open.take()
            {
                out.push((o.0, o.1, o.2, None, 0.0, false, o.6));
            }
        }
        if let Some(o) = open
            && let Some(c) = st.control
            && c != o.5
        {
            let done = c == o.4;
            let intercepted = side_of(c) != side_of(o.5);
            let len = if done { (st.ball.ground_pos() - o.7).length() } else { o.1 };
            out.push((o.0, len, o.2, done.then_some(st.time - o.3), prev_ball_speed, intercepted, o.6));
            open = None;
        }
        prev_ball_speed = st.ball.vel.length();
    }
    (out, low)
}

fn main() {
    let n: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(10);
    let res: Vec<(Vec<Rec>, Vec<String>)> = std::thread::scope(|s| {
        let hs: Vec<_> = (1..=n).map(|seed| s.spawn(move || play(seed))).collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let recs: Vec<Rec> = res.iter().flat_map(|r| r.0.iter().copied()).collect();
    if let Ok(path) = std::env::var("LOW") {
        let lines: Vec<String> = res.iter().flat_map(|r| r.1.iter().cloned()).collect();
        std::fs::write(path, lines.join("\n")).unwrap();
    }
    let med = |mut v: Vec<f32>| {
        v.sort_by(f32::total_cmp);
        if v.is_empty() { f32::NAN } else { v[v.len() / 2] }
    };
    println!("{n} matches");
    println!("kind    band      /match  release v  duration med  arrival v  completed  intercepted");
    for k in 0..2 {
        for (lo, hi) in BANDS {
            let r: Vec<&Rec> = recs.iter().filter(|r| r.0 == k && (lo..hi).contains(&r.1)).collect();
            if r.is_empty() {
                continue;
            }
            let done: Vec<&&Rec> = r.iter().filter(|r| r.3.is_some()).collect();
            println!(
                "{:<7} {:>2}–{:<2} m  {:7.1}  {:9.1}  {:12.2}  {:9.1}  {:8.0}%  {:10.0}%",
                if k == 0 { "ground" } else { "lofted" },
                lo,
                hi,
                r.len() as f32 / n as f32,
                med(r.iter().map(|r| r.2).collect()),
                med(done.iter().map(|r| r.3.unwrap()).collect()),
                med(done.iter().map(|r| r.4).collect()),
                100.0 * done.len() as f32 / r.len() as f32,
                100.0 * r.iter().filter(|r| r.5).count() as f32 / r.len() as f32,
            );
        }
    }
    println!("\npredicted p of the chosen pass → actual: receiver controls / opponent controls first");
    for (lo, hi) in [(0.0, 0.5), (0.5, 0.7), (0.7, 0.8), (0.8, 0.9), (0.9, 0.95), (0.95, 1.01)] {
        for k in 0..2 {
            let r: Vec<&Rec> = recs.iter().filter(|r| r.0 == k && (lo..hi).contains(&r.6)).collect();
            if r.is_empty() {
                continue;
            }
            let done = r.iter().filter(|r| r.3.is_some()).count() as f32 / r.len() as f32;
            let int = r.iter().filter(|r| r.5).count() as f32 / r.len() as f32;
            println!("  {} p {lo:.2}–{hi:.2}: {:6.1}/match  completed {:3.0}%  intercepted {:3.0}%", if k == 0 { "ground" } else { "lofted" }, r.len() as f32 / n as f32, 100.0 * done, 100.0 * int);
        }
    }
}
