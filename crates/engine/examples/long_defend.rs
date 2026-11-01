//! What the defence does against an opponent's 25–45 m open-play pass to a team-mate: half a second after release,
//! the label of the defender nearest to the target (press, delay, mark, shape …) and whether his team presses; at the
//! receiver's first touch, the nearest opponent's distance. Split by pass direction (forward > 5 m, backward < -3 m).
//! `cargo run -p match_engine --release --example long_defend -- [matches]`

use match_engine::Simulation;
use match_engine::ai::reach::time_to_reach;
use match_engine::events::EventKind;
use match_engine::match_state::side_of;
use match_engine::player::KickKind;

/// (direction, label of the nearest defender at +0.5 s, his team presses, distance at release, at the first touch,
/// completed, mean speed of the defender nearest at release until the first touch, flight time, his distance to the
/// receiver at the first touch, closest any defender could be to the target at arrival running straight there, lofted)
type Rec = (&'static str, String, bool, f32, f32, bool, f32, f32, f32, f32, bool);

fn play(seed: u64) -> Vec<Rec> {
    let mut sim = Simulation::from_builtin("northbridge", "harbour", seed);
    let mut out = Vec::new();
    let mut seen = 0;
    let trace = std::env::var("TRACE").is_ok() && seed == 1;
    let mut traced = 0;
    // Pass being followed: kicker, receiver, target, release time, direction, release distance, label, press.
    #[allow(clippy::type_complexity)]
    let mut open: Option<(usize, usize, glam::Vec2, f32, &'static str, f32, Option<(String, bool)>, usize, f32, f32, bool)> = None;
    while !sim.finished() {
        sim.step();
        let st = &sim.state;
        while seen < sim.events.len() {
            let e = &sim.events[seen];
            seen += 1;
            if let EventKind::Pass { by, kind, length, to: Some(to), headed: false, .. } = e.kind
                && st.is_live()
                && matches!(kind, KickKind::GroundPass | KickKind::LoftedPass)
                && (25.0..45.0).contains(&length)
                && let Some(f) = st.flight
            {
                let p = &st.players[by];
                let fr = st.frame(p.side);
                let fwd = fr.to_team(f.target).x - fr.to_team(p.pos).x;
                let dir = if fwd > 5.0 {
                    "fwd"
                } else if fwd < -3.0 {
                    "back"
                } else {
                    "lat"
                };
                let d = st
                    .squad(p.side.opponent())
                    .iter()
                    .filter(|q| !q.is_gk())
                    .min_by(|a, b| (a.pos - f.target).length().total_cmp(&(b.pos - f.target).length()))
                    .unwrap();
                // Closest any outfield opponent could be to the target when the ball arrives, sprinting straight there.
                let ideal = st
                    .squad(p.side.opponent())
                    .iter()
                    .filter(|q| !q.is_gk())
                    .map(|q| {
                        (0..80).map(|k| k as f32 * 0.5).find(|&r| time_to_reach(q, f.target, r, true) <= f.arrival_t).unwrap_or(40.0)
                    })
                    .fold(f32::MAX, f32::min);
                open = Some((by, to, f.target, st.time, dir, (d.pos - f.target).length(), None, d.id, 0.0, ideal, kind == KickKind::LoftedPass));
                // `TRACE=band`: only forward lofted passes with the nearest defender 5–15 m from the target at release.
                if trace && std::env::var("TRACE").is_ok_and(|v| v == "band") && !(kind == KickKind::LoftedPass && dir == "fwd" && (5.0..15.0).contains(&(d.pos - f.target).length())) {
                    open = None;
                    continue;
                }
                if trace && dir == "fwd" && traced < 12 {
                    println!(
                        "pass {:?} → {:?}, {length:.1} m, {:?}, flight {:.2} s",
                        p.role,
                        st.players[to].role,
                        kind,
                        f.arrival_t
                    );
                }
            } else if matches!(e.kind, EventKind::Pass { .. } | EventKind::Shot { .. } | EventKind::Restart { .. } | EventKind::Kickoff { .. }) {
                open = None;
            }
        }
        let Some(o) = open.as_mut() else { continue };
        let side = side_of(o.0);
        o.8 += st.players[o.7].vel.length() * sim.cfg.timing.dt;
        let since = st.time - o.3;
        if trace && o.4 == "fwd" && traced < 12 && ((since / 0.4).fract() < 0.02 / 0.4) {
            let b = st.team_view(side.opponent());
            let r = &st.players[o.1];
            let n = &st.players[o.7];
            let first = b.first.map(|f| &st.squad(side.opponent())[f]);
            let fs = first.map_or(String::new(), |f| {
                format!(
                    "first {:?} #{} {:.1} m from rcv, {:.1} m/s '{}'",
                    f.role,
                    f.number,
                    (f.pos - r.pos).length(),
                    f.vel.length(),
                    f.debug.label
                )
            });
            println!(
                "  t+{since:.1} rcv {:.1} m from target | nearest@release {:?} #{} {:.1} m from rcv, {:.1} m/s '{}' | {fs} | press {}",
                (r.pos - o.2).length(),
                n.role,
                n.number,
                (n.pos - r.pos).length(),
                n.vel.length(),
                n.debug.label,
                b.press
            );
        }
        if o.6.is_none() && st.time - o.3 >= 0.5 {
            let d = st
                .squad(side.opponent())
                .iter()
                .filter(|q| !q.is_gk())
                .min_by(|a, b| (a.pos - o.2).length().total_cmp(&(b.pos - o.2).length()))
                .unwrap();
            o.6 = Some((d.debug.label.clone(), st.team_view(side.opponent()).press));
        }
        if st.flight.is_none()
            && let Some(c) = st.control
        {
            let near = st.squad(side.opponent()).iter().map(|q| (q.pos - st.players[c].pos).length()).fold(f32::MAX, f32::min);
            let (label, press) = o.6.clone().unwrap_or_default();
            let t = st.time - o.3;
            if trace && o.4 == "fwd" && traced < 12 {
                traced += 1;
                println!("  -> first touch by {}, nearest opponent {near:.1} m\n", if c == o.1 { "receiver" } else { "other" });
            }
            out.push((o.4, label, press, o.5, near, c == o.1, o.8 / t.max(0.02), t, (st.players[o.7].pos - st.players[c].pos).length(), o.9, o.10));
            open = None;
        }
    }
    out
}

fn median(mut v: Vec<f32>) -> f32 {
    v.sort_by(f32::total_cmp);
    v.get(v.len() / 2).copied().unwrap_or(f32::NAN)
}

fn main() {
    let n: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(6);
    let res: Vec<Rec> = std::thread::scope(|scope| {
        let hs: Vec<_> = (1..=n).map(|seed| scope.spawn(move || play(seed))).collect();
        hs.into_iter().flat_map(|h| h.join().unwrap()).collect()
    });
    for dir in ["fwd", "lat", "back"] {
        let r: Vec<&Rec> = res.iter().filter(|r| r.0 == dir).collect();
        let k = r.len().max(1) as f32;
        println!(
            "{dir}: {:.0}/match, completed {:.0} %, team pressing {:.0} %, nearest opponent at release {:.1} m → first touch {:.1} m",
            r.len() as f32 / n as f32,
            100.0 * r.iter().filter(|x| x.5).count() as f32 / k,
            100.0 * r.iter().filter(|x| x.2).count() as f32 / k,
            median(r.iter().map(|x| x.3).collect()),
            median(r.iter().map(|x| x.4).collect()),
        );
        for (name, lofted) in [("ground", false), ("lofted", true)] {
            let q: Vec<&&Rec> = r.iter().filter(|x| x.10 == lofted).collect();
            println!(
                "    {name}: {:.0}/match, completed {:.0} %, nearest opponent at release {:.1} m, possible at arrival {:.1} m, actual at first touch {:.1} m",
                q.len() as f32 / n as f32,
                100.0 * q.iter().filter(|x| x.5).count() as f32 / q.len().max(1) as f32,
                median(q.iter().map(|x| x.3).collect()),
                median(q.iter().map(|x| x.9).collect()),
                median(q.iter().map(|x| x.4).collect()),
            );
        }
        println!(
            "    defender nearest at release: mean speed {:.1} m/s over {:.2} s, ends {:.1} m from the receiver",
            median(r.iter().map(|x| x.6).collect()),
            median(r.iter().map(|x| x.7).collect()),
            median(r.iter().map(|x| x.8).collect()),
        );
        let mut labels: Vec<(String, usize)> = Vec::new();
        for x in &r {
            match labels.iter_mut().find(|l| l.0 == x.1) {
                Some(l) => l.1 += 1,
                None => labels.push((x.1.clone(), 1)),
            }
        }
        labels.sort_by_key(|l| std::cmp::Reverse(l.1));
        let s: Vec<String> = labels.iter().take(8).map(|(l, c)| format!("{l} {:.0} %", 100.0 * *c as f32 / k)).collect();
        println!("    nearest defender at +0.5 s: {}", s.join(", "));
    }
}
