//! Support around the passer at every open-play pass, as `sb_support.py` measures it in StatsBomb 360 frames:
//! team-mates within 5 / 10 / 15 m, the nearest team-mate, the nearest free team-mate (no opponent within 3 m of him),
//! the nearest opponent, and the pass length; by third (metres from the passer's own goal line). Then the pass length
//! by the free team-mates within 15 m and whether an opponent is within 3 m of the passer (what was on offer vs what
//! was chosen).
//! `cargo run -p match_engine --release --example support -- [matches]`

use match_engine::Simulation;
use match_engine::events::EventKind;
use match_engine::pitch::HALF_LENGTH;
use match_engine::player::KickKind;

/// From own goal (m), mates < 5 / 10 / 15 m, nearest mate, nearest free mate, nearest opponent, pass length, free
/// mates < 15 m.
type Rec = (f32, usize, usize, usize, f32, f32, f32, f32, usize);

fn play(seed: u64) -> Vec<Rec> {
    let mut sim = Simulation::from_builtin("northbridge", "harbour", seed);
    let mut out = Vec::new();
    let mut seen = 0;
    while !sim.finished() {
        sim.step();
        let st = &sim.state;
        while seen < sim.events.len() {
            let e = &sim.events[seen];
            seen += 1;
            let EventKind::Pass { by, kind, length, .. } = e.kind else {
                continue;
            };
            if !st.is_live() || !matches!(kind, KickKind::GroundPass | KickKind::LoftedPass) {
                continue;
            }
            let p = &st.players[by];
            let opps = st.squad(p.side.opponent());
            let mut mates: Vec<f32> = st.squad(p.side).iter().filter(|m| m.id != by).map(|m| (m.pos - p.pos).length()).collect();
            mates.sort_by(f32::total_cmp);
            let free: Vec<f32> = st
                .squad(p.side)
                .iter()
                .filter(|m| m.id != by && opps.iter().all(|o| (o.pos - m.pos).length() > 3.0))
                .map(|m| (m.pos - p.pos).length())
                .collect();
            let opp = opps.iter().map(|o| (o.pos - p.pos).length()).fold(99.0, f32::min);
            let within = |r: f32| mates.iter().filter(|&&d| d < r).count();
            let x = HALF_LENGTH + st.frame(p.side).to_team(p.pos).x;
            let nearest_free = free.iter().copied().fold(99.0, f32::min);
            out.push((x, within(5.0), within(10.0), within(15.0), mates[0], nearest_free, opp, length, free.iter().filter(|&&d| d < 15.0).count()));
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
    let med = |mut v: Vec<f32>| {
        v.sort_by(f32::total_cmp);
        if v.is_empty() { f32::NAN } else { v[v.len() / 2] }
    };
    println!("{} open-play passes, {n} matches", recs.len());
    for (lo, hi, name) in [(0.0, 35.0, "own third"), (35.0, 70.0, "middle third"), (70.0, 106.0, "final third"), (0.0, 106.0, "all")] {
        let r: Vec<&Rec> = recs.iter().filter(|r| (lo..hi).contains(&r.0)).collect();
        let k = r.len().max(1) as f32;
        let mean = |f: fn(&Rec) -> usize| r.iter().map(|x| f(x) as f32).sum::<f32>() / k;
        println!(
            "{name:13} n {:6}  mates <5 m {:.2}  <10 m {:.2}  <15 m {:.2}  nearest mate med {:.1} m  nearest free mate med {:.1} m  nearest opp med {:.1} m  pass len med {:.1} m",
            r.len(),
            mean(|x| x.1),
            mean(|x| x.2),
            mean(|x| x.3),
            med(r.iter().map(|x| x.4).collect()),
            med(r.iter().map(|x| x.5).collect()),
            med(r.iter().map(|x| x.6).collect()),
            med(r.iter().map(|x| x.7).collect()),
        );
    }
    println!("pass length by free team-mates < 15 m and pressure (opponent < 3 m of the passer):");
    for pressed in [false, true] {
        for k in 0..4 {
            let r: Vec<&Rec> = recs.iter().filter(|r| (r.6 < 3.0) == pressed && r.8.min(3) == k).collect();
            let c = r.len().max(1) as f32;
            println!(
                "  {} free <15 m {}{}: {:4.1} % of passes, len med {:4.1} m, 30-45 m {:4.1} %, ≥ 30 m {:4.1} %",
                if pressed { "pressed" } else { "free   " },
                k,
                if k == 3 { "+" } else { " " },
                100.0 * r.len() as f32 / recs.len() as f32,
                med(r.iter().map(|x| x.7).collect()),
                100.0 * r.iter().filter(|x| (30.0..45.0).contains(&x.7)).count() as f32 / c,
                100.0 * r.iter().filter(|x| x.7 >= 30.0).count() as f32 / c,
            );
        }
    }
}
