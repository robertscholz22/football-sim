//! Forward open-play ground passes of 20 m or more to a team-mate, by the lane at release (the nearest outfield
//! opponent to the middle of the pass line, 10–85 % of the way to the target, as `sb_lanes.py` measures it): what that
//! lane defender does during the flight. Whether he could intercept at release (`intercept` before the receiver),
//! whether his brain sends him (chaser), how close he gets to the ball, whether he misses a touch, the outcome.
//! `cargo run -p match_engine --release --example lane_cal -- [matches]`

use match_engine::Simulation;
use match_engine::ai::reach::{intercept, pass_reaction};
use match_engine::events::EventKind;
use match_engine::match_state::side_of;
use match_engine::player::KickKind;

/// (lane, fraction along the line, could intercept, ever chaser, closest to the ball, missed touch, outcome: 0
/// completed, 1 lane defender, 2 other opponent, label at +0.3 s, length)
type Rec = (f32, f32, bool, bool, f32, bool, u8, String, f32);

fn play(seed: u64) -> Vec<Rec> {
    let mut sim = Simulation::from_builtin("northbridge", "harbour", seed);
    let mut out = Vec::new();
    let mut seen = 0;
    // Kicker, lane defender, release time, record so far.
    let mut open: Option<(usize, usize, f32, Rec)> = None;
    while !sim.finished() {
        sim.step();
        let st = &sim.state;
        while seen < sim.events.len() {
            let e = &sim.events[seen];
            seen += 1;
            if let EventKind::Pass { by, kind: KickKind::GroundPass, length, to: Some(_), .. } = e.kind
                && st.is_live()
                && length >= 20.0
                && let Some(f) = st.flight
            {
                let a = st.players[by].pos;
                let fr = st.frame(st.players[by].side);
                if fr.to_team(f.target).x - fr.to_team(a).x <= 5.0 {
                    open = None;
                    continue;
                }
                let ab = f.target - a;
                let (lane, u, l) = st
                    .squad(st.players[by].side.opponent())
                    .iter()
                    .filter(|o| !o.is_gk())
                    .map(|o| {
                        let u = (o.pos - a).dot(ab) / ab.length_squared();
                        let d = if (0.1..=0.85).contains(&u) { (a + ab * u - o.pos).length() } else { 99.0 };
                        (d, u, o.id)
                    })
                    .min_by(|x, y| x.0.total_cmp(&y.0))
                    .unwrap();
                let pc = &sim.cfg.player;
                let could = intercept(&st.players[l], &sim.traj, pc.control_height, pc.control_radius, pass_reaction(&st.players[l], 0.0, pc))
                    .is_some_and(|(t, _)| t <= f.arrival_t);
                open = Some((by, l, st.time, (lane, u, could, false, f32::MAX, false, 0, String::new(), length)));
            } else if matches!(e.kind, EventKind::Pass { .. } | EventKind::Shot { .. } | EventKind::Restart { .. } | EventKind::Kickoff { .. }) {
                open = None;
            }
        }
        let Some(o) = open.as_mut() else { continue };
        let side = side_of(o.0);
        let l = &st.players[o.1];
        let b = st.team_view(side.opponent());
        o.3.3 |= b.chaser.is_some_and(|c| st.squad(side.opponent())[c].id == o.1);
        o.3.4 = o.3.4.min((l.pos - st.ball.ground_pos()).length());
        o.3.5 |= l.touch_cooldown_until > st.time + 0.3 && st.last_touch != Some(o.1);
        if o.3.7.is_empty() && st.time - o.2 >= 0.3 {
            o.3.7 = l.debug.label.clone();
        }
        if st.flight.is_none() {
            let lt = st.last_touch;
            o.3.6 = match lt {
                Some(t) if side_of(t) == side => 0,
                Some(t) if t == o.1 => 1,
                _ => 2,
            };
            out.push(o.3.clone());
            open = None;
        }
    }
    out
}

fn main() {
    let n: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(10);
    let recs: Vec<Rec> = std::thread::scope(|s| {
        let hs: Vec<_> = (0..n).map(|seed| s.spawn(move || play(seed))).collect();
        hs.into_iter().flat_map(|h| h.join().unwrap()).collect()
    });
    let m = n as f32;
    println!("{n} matches, forward ground passes ≥ 20 m to a team-mate, by the lane at release");
    for (lo, hi) in [(0.0, 2.0), (2.0, 4.0), (4.0, 7.0), (7.0, 99.0), (99.0, 999.0)] {
        let r: Vec<&Rec> = recs.iter().filter(|r| (lo..hi).contains(&r.0)).collect();
        if r.is_empty() {
            continue;
        }
        let k = r.len() as f32;
        let pct = |f: &dyn Fn(&&Rec) -> bool| 100.0 * r.iter().filter(|x| f(x)).count() as f32 / k;
        let mut close: Vec<f32> = r.iter().map(|x| x.4).collect();
        close.sort_by(f32::total_cmp);
        let mut u: Vec<f32> = r.iter().map(|x| x.1).collect();
        u.sort_by(f32::total_cmp);
        println!(
            "  lane {lo:2.0}-{hi:3.0} m: {:6.1}/match  completed {:3.0}%  by lane defender {:3.0}%  other opp {:3.0}% | lane def: along med {:.2}, could intercept {:3.0}%, chaser {:3.0}%, closest med {:4.1} m, within 0.75 m {:3.0}%, missed touch {:3.0}%",
            k / m,
            pct(&|x| x.6 == 0),
            pct(&|x| x.6 == 1),
            pct(&|x| x.6 == 2),
            u[u.len() / 2],
            pct(&|x| x.2),
            pct(&|x| x.3),
            close[close.len() / 2],
            pct(&|x| x.4 <= 0.75),
            pct(&|x| x.5)
        );
        if lo < 4.0 {
            for (could, name) in [(true, "could"), (false, "could not")] {
                let rr: Vec<&&Rec> = r.iter().filter(|x| x.2 == could).collect();
                if rr.is_empty() {
                    continue;
                }
                let kk = rr.len() as f32;
                println!(
                    "      {name:<9} intercept: {:5.1}/match completed {:3.0}%, chaser {:3.0}%, within 0.75 m {:3.0}%, missed touch {:3.0}%",
                    kk / m,
                    100.0 * rr.iter().filter(|x| x.6 == 0).count() as f32 / kk,
                    100.0 * rr.iter().filter(|x| x.3).count() as f32 / kk,
                    100.0 * rr.iter().filter(|x| x.4 <= 0.75).count() as f32 / kk,
                    100.0 * rr.iter().filter(|x| x.5).count() as f32 / kk
                );
            }
            let mut labels: Vec<(String, usize)> = Vec::new();
            for x in &r {
                match labels.iter_mut().find(|(l, _)| *l == x.7) {
                    Some(e) => e.1 += 1,
                    None => labels.push((x.7.clone(), 1)),
                }
            }
            labels.sort_by_key(|l| std::cmp::Reverse(l.1));
            let s: Vec<String> = labels.iter().take(6).map(|(l, c)| format!("'{l}' {:.0}%", 100.0 * *c as f32 / k)).collect();
            println!("      lane defender at +0.3 s: {}", s.join(", "));
        }
    }
    println!("completed 20-45 m: where the lane defender is along the line (share of all such completed passes)");
    let rs: Vec<&Rec> = recs.iter().filter(|r| r.6 == 0 && (20.0..45.0).contains(&r.8)).collect();
    let k = rs.len() as f32;
    for (lo, hi) in [(0.1, 0.33), (0.33, 0.6), (0.6, 0.86)] {
        let share = |a: f32, b: f32| 100.0 * rs.iter().filter(|r| (a..b).contains(&r.0) && (lo..hi).contains(&r.1)).count() as f32 / k;
        println!("  along {lo:.2}-{hi:.2}: lane < 2 m {:4.1}%  2-4 m {:4.1}%", share(0.0, 2.0), share(2.0, 4.0));
    }
}
