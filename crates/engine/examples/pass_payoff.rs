//! What a pass is worth afterwards, as `sb_pass_payoff.py` measures it in StatsBomb: for every open-play pass (not
//! headed), the xG its team still gets before the opponents gain control or restart, by origin band (metres from the
//! own goal line) and pass class (length × direction: forward > 5 m, backward < -3 m, else lateral). Completed: the
//! next player to control the ball (or to restart it) is a team-mate.
//! `cargo run -p match_engine --release --example pass_payoff -- [matches]`

use match_engine::Simulation;
use match_engine::player::KickKind;
use match_engine::events::EventKind;
use match_engine::match_state::side_of;
use match_engine::pitch::HALF_LENGTH;
use std::collections::BTreeMap;

const BANDS: [(f32, f32); 6] = [(0.0, 25.0), (25.0, 40.0), (40.0, 52.5), (52.5, 65.0), (65.0, 80.0), (80.0, 106.0)];

/// One pass: origin band, class, completed, xG after, a shot after.
type Rec = (usize, String, bool, f32, bool);

fn class(length: f32, dx: f32) -> String {
    let d = if dx > 5.0 {
        "fwd"
    } else if dx < -3.0 {
        "back"
    } else {
        "lat"
    };
    let l = if length < 20.0 {
        "<20"
    } else if length < 30.0 {
        "20-30"
    } else if length < 45.0 {
        "30-45"
    } else {
        "45+"
    };
    format!("{l} {d}")
}

fn play(seed: u64) -> Vec<Rec> {
    let mut sim = Simulation::from_builtin("northbridge", "harbour", seed);
    let mut out: Vec<Rec> = Vec::new();
    // Passes whose possession is still running, per side: indices into `out`.
    let mut open: [Vec<usize>; 2] = [Vec::new(), Vec::new()];
    // Passes waiting for their next controller: index, side.
    let mut pending: Vec<(usize, usize)> = Vec::new();
    let mut seen = 0;
    while !sim.finished() {
        sim.step();
        let st = &sim.state;
        while seen < sim.events.len() {
            let e = &sim.events[seen];
            seen += 1;
            match e.kind {
                EventKind::Pass { by, length, kind, headed: false, .. }
                    if st.is_live() && matches!(kind, KickKind::GroundPass | KickKind::LoftedPass) =>
                {
                    let p = &st.players[by];
                    let fr = st.frame(p.side);
                    let x = HALF_LENGTH + fr.to_team(p.pos).x;
                    let dx = st.flight.map_or(0.0, |f| fr.to_team(f.target).x - fr.to_team(p.pos).x);
                    let band = BANDS.iter().position(|&(lo, hi)| (lo..hi).contains(&x)).unwrap_or(BANDS.len() - 1);
                    let s = p.side.index();
                    open[s].push(out.len());
                    pending.push((out.len(), s));
                    out.push((band, class(length, dx), false, 0.0, false));
                }
                EventKind::Shot { by, xg, .. } => {
                    for &i in &open[side_of(by).index()] {
                        out[i].3 += xg;
                        out[i].4 = true;
                    }
                }
                EventKind::Restart { side, .. } => {
                    let s = side.index();
                    pending.retain(|&(i, ps)| {
                        out[i].2 = ps == s;
                        false
                    });
                    open[1 - s].clear();
                }
                EventKind::Goal { .. } | EventKind::Kickoff { .. } | EventKind::HalfTime | EventKind::FullTime => {
                    pending.clear();
                    open[0].clear();
                    open[1].clear();
                }
                _ => {}
            }
        }
        if let Some(c) = st.control
            && st.flight.is_none()
        {
            let s = side_of(c).index();
            pending.retain(|&(i, ps)| {
                out[i].2 = ps == s;
                false
            });
            open[1 - s].clear();
        }
    }
    out
}

fn main() {
    let n: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(6);
    let res: Vec<Rec> = std::thread::scope(|scope| {
        let hs: Vec<_> = (1..=n).map(|seed| scope.spawn(move || play(seed))).collect();
        hs.into_iter().flat_map(|h| h.join().unwrap()).collect()
    });
    let mut acc: BTreeMap<(usize, String), (u32, u32, f32, u32)> = BTreeMap::new();
    for r in &res {
        for c in [r.1.clone(), "all".to_string()] {
            let a = acc.entry((r.0, c)).or_default();
            a.0 += 1;
            a.1 += r.2 as u32;
            a.2 += r.3;
            a.3 += r.4 as u32;
        }
    }
    println!("{n} matches, open-play passes (not headed) per match: {:.0}", res.len() as f32 / n as f32);
    let order: Vec<String> = std::iter::once("all".to_string())
        .chain(["<20", "20-30", "30-45", "45+"].iter().flat_map(|l| ["fwd", "lat", "back"].iter().map(move |d| format!("{l} {d}"))))
        .collect();
    for (k, (lo, hi)) in BANDS.iter().enumerate() {
        println!("origin {lo}-{hi} m");
        for c in &order {
            if let Some(a) = acc.get(&(k, c.clone()))
                && a.0 >= 10
            {
                println!(
                    "  {c:<11} {:6.1}/match  compl {:3.0} %  xG after {:.4}  shot after {:4.1} %",
                    a.0 as f32 / n as f32,
                    100.0 * a.1 as f32 / a.0 as f32,
                    a.2 / a.0 as f32,
                    100.0 * a.3 as f32 / a.0 as f32
                );
            }
        }
    }
}
