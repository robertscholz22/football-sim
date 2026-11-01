//! Pass pricing: the AI's success probability of every open-play pass against what happened (the passing team next
//! controls the ball, or restarts after it went out), by predicted p and, for ground passes under 25 m, by how close
//! the receiver's nearest opponent was at release. For completed forward passes, the lane at release: the nearest
//! opponent to the middle of the pass line (10–85 % of the way to the target), as `sb_lanes.py` measures it.
//! `cargo run -p match_engine --release --example pass_cal -- [matches]`

use match_engine::Simulation;
use match_engine::events::EventKind;
use match_engine::pitch::Side;
use match_engine::player::KickKind;

/// (predicted p, completed, lofted, length, receiver's nearest opponent at release, first event: 0 none, 1 own
/// header, 2 aerial duel, 3 opponent header, 4 opponent touch / interception, 5 miscontrol, lane (99 unless forward))
type Rec = (f32, bool, bool, f32, f32, u8, f32);

fn play(seed: u64) -> Vec<Rec> {
    let mut sim = Simulation::from_builtin("northbridge", "harbour", seed);
    for (k, f) in [("AWON", &mut sim.cfg.ai.aerial_won_keep), ("ALOST", &mut sim.cfg.ai.aerial_lost_keep), ("AVAL", &mut sim.cfg.ai.aerial_value_factor)] {
        if let Some(v) = std::env::var(k).ok().and_then(|s| s.parse().ok()) {
            *f = v;
        }
    }
    let mut out = Vec::new();
    let mut open: Option<(Side, f32, bool, f32, f32, f32)> = None;
    let mut how = 0u8;
    let mut seen = 0;
    while !sim.finished() {
        sim.step();
        let st = &sim.state;
        while seen < sim.events.len() {
            let e = &sim.events[seen];
            seen += 1;
            match e.kind {
                EventKind::Restart { side, .. } => {
                    if let Some((s, p, l, len, m, ln)) = open.take() {
                        out.push((p, s == side, l, len, m, how, ln));
                    }
                }
                EventKind::Kickoff { .. } => open = None,
                EventKind::Header { by } if how == 0 && open.is_some() => {
                    how = if open.is_some_and(|o| o.0 == st.players[by].side) { 1 } else { 3 }
                }
                EventKind::Aerial { won, .. } if how == 0 && open.is_some() => {
                    how = if open.is_some_and(|o| o.0 == st.players[won].side) { 2 } else { 6 }
                }
                EventKind::Interception { .. } if how == 0 && open.is_some() => how = 4,
                EventKind::Miscontrol { .. } if how == 0 && open.is_some() => how = 5,
                EventKind::Pass { by, length, to, headed: true, .. } if st.is_live() => {
                    // A headed pass: recorded with p = −1 − length / 100 (no AI price), closing the open record.
                    let side = st.players[by].side;
                    if let Some((s, p, l, len, m, ln)) = open.take() {
                        out.push((p, s == side, l, len, m, how, ln));
                    }
                    let marker = to.map_or(f32::NAN, |r| {
                        st.squad(side.opponent()).iter().map(|o| (o.pos - st.players[r].pos).length()).fold(f32::MAX, f32::min)
                    });
                    open = Some((side, -1.0 - length / 100.0, true, length, marker, 99.0));
                    how = 0;
                }
                EventKind::Pass { by, kind, length, to, headed: false, .. } if st.is_live() && matches!(kind, KickKind::GroundPass | KickKind::LoftedPass) => {
                    let side = st.players[by].side;
                    if let Some((s, p, l, len, m, ln)) = open.take() {
                        out.push((p, s == side, l, len, m, how, ln));
                    }
                    let marker = to.map_or(f32::NAN, |r| {
                        st.squad(side.opponent()).iter().map(|o| (o.pos - st.players[r].pos).length()).fold(f32::MAX, f32::min)
                    });
                    let fr = st.frame(side);
                    let a = st.players[by].pos;
                    let lane = st.flight.filter(|f| fr.to_team(f.target).x - fr.to_team(a).x > 5.0).map_or(99.0, |f| {
                        let ab = f.target - a;
                        st.squad(side.opponent())
                            .iter()
                            .map(|o| {
                                let u = (o.pos - a).dot(ab) / ab.length_squared();
                                if (0.1..=0.85).contains(&u) { (a + ab * u - o.pos).length() } else { 99.0 }
                            })
                            .fold(99.0, f32::min)
                    });
                    open = Some((side, st.players[by].debug.chosen_p, kind == KickKind::LoftedPass, length, marker, lane));
                    how = 0;
                }
                _ => {}
            }
        }
        if st.is_live()
            && st.flight.is_none()
            && let Some(c) = st.control
            && let Some((s, p, l, len, m, ln)) = open.take()
        {
            out.push((p, st.players[c].side == s, l, len, m, how, ln));
        }
    }
    out
}

fn main() {
    let n: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(10);
    let recs: Vec<Rec> = std::thread::scope(|s| {
        let hs: Vec<_> = (1..=n).map(|seed| s.spawn(move || play(seed))).collect();
        hs.into_iter().flat_map(|h| h.join().unwrap()).collect()
    });
    let show = |name: &str, r: Vec<&Rec>| {
        if r.is_empty() {
            return;
        }
        let k = r.len() as f32;
        let first = |h: u8, ok: bool| 100.0 * r.iter().filter(|x| x.5 == h && x.1 == ok).count() as f32 / k;
        println!(
            "  {name:<22} {:6.1}/match  predicted {:3.0}%  completed {:3.0}% | own header kept {:2.0}% lost {:2.0}%, aerial won: kept {:2.0}% lost {:2.0}%, aerial lost: kept {:2.0}% lost {:2.0}%, opp header {:2.0}%, opp touch kept {:2.0}% lost {:2.0}%, miscontrol kept {:2.0}% lost {:2.0}%, other lost {:2.0}%",
            k / n as f32,
            100.0 * r.iter().map(|x| x.0).sum::<f32>() / k,
            100.0 * r.iter().filter(|x| x.1).count() as f32 / k,
            first(1, true),
            first(1, false),
            first(2, true),
            first(2, false),
            first(6, true),
            first(6, false),
            first(3, false) + first(3, true),
            first(4, true),
            first(4, false),
            first(5, true),
            first(5, false),
            first(0, false),
        );
    };
    for (lofted, name) in [(false, "ground"), (true, "lofted")] {
        println!("{name} passes by predicted p:");
        for (lo, hi) in [(0.0, 0.5), (0.5, 0.7), (0.7, 0.8), (0.8, 0.9), (0.9, 0.95), (0.95, 1.01)] {
            show(&format!("p {lo:.2}–{hi:.2}"), recs.iter().filter(|r| r.2 == lofted && (lo..hi).contains(&r.0)).collect());
        }
    }
    println!("headed passes (p = −1 − length / 100):");
    for (lo, hi) in [(0.0, 10.0), (10.0, 20.0), (20.0, 35.0)] {
        show(&format!("{lo:.0}–{hi:.0} m"), recs.iter().filter(|r| r.0 < 0.0 && (lo..hi).contains(&r.3)).collect());
    }
    for (lo, hi) in [(0.0, 3.0), (3.0, 8.0), (8.0, 99.0)] {
        show(&format!("receiver's marker {lo:.0}–{hi:.0} m"), recs.iter().filter(|r| r.0 < 0.0 && (lo..hi).contains(&r.4)).collect());
    }
    println!("ground passes < 25 m by the receiver's nearest opponent at release:");
    for (lo, hi) in [(0.0, 2.0), (2.0, 3.0), (3.0, 4.0), (4.0, 6.0), (6.0, 10.0), (10.0, 99.0)] {
        show(&format!("{lo:.0}–{hi:.0} m"), recs.iter().filter(|r| !r.2 && r.3 < 25.0 && (lo..hi).contains(&r.4)).collect());
    }
    println!("completed forward passes, lane at release (nearest opponent to the middle of the pass line):");
    for (lofted, name) in [(false, "ground"), (true, "lofted")] {
        for (lo, hi) in [(10.0, 20.0), (20.0, 30.0), (30.0, 45.0)] {
            let mut r: Vec<f32> = recs.iter().filter(|r| r.1 && r.2 == lofted && r.6 < 98.0 && (lo..hi).contains(&r.3)).map(|r| r.6).collect();
            if r.len() < 10 {
                continue;
            }
            r.sort_by(f32::total_cmp);
            let k = r.len() as f32;
            let share = |a: f32, b: f32| 100.0 * r.iter().filter(|&&d| (a..b).contains(&d)).count() as f32 / k;
            println!(
                "  {name:<6} {lo:.0}-{hi:.0} m: {:6.1}/match  lane med {:4.1} m  < 2 m {:3.0}%  2-4 m {:3.0}%  4-7 m {:3.0}%  >= 7 m {:3.0}%",
                k / n as f32,
                r[r.len() / 2],
                share(0.0, 2.0),
                share(2.0, 4.0),
                share(4.0, 7.0),
                share(7.0, 999.0)
            );
        }
    }
    println!("ground passes ≥ 25 m by the receiver's nearest opponent at release:");
    for (lo, hi) in [(0.0, 3.0), (3.0, 6.0), (6.0, 10.0), (10.0, 99.0)] {
        show(&format!("{lo:.0}–{hi:.0} m"), recs.iter().filter(|r| !r.2 && r.3 >= 25.0 && (lo..hi).contains(&r.4)).collect());
    }
}
