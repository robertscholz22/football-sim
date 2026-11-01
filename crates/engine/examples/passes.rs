//! Pass risk: for every open-play pass, where it was played from (metres from the passer's own goal line), its length,
//! the success probability the on-ball AI gave it, and whether the passer's team had the ball next. Prints completion
//! against predicted p by zone and length, and the share of each length per zone. For lofted 30–45 m passes: flight
//! time, landing error, nearest opponent at the first touch and aerial duels. `DUMP=file.jsonl` writes one record per
//! pass, `GEO=file.jsonl` its geometry at release (forward metres, nearest opponent to the target, target y, length).
//! `cargo run -p match_engine --release --example passes -- [matches]`

use std::fmt::Write as _;

use match_engine::Simulation;
use match_engine::events::EventKind;
use match_engine::match_state::side_of;
use match_engine::pitch::HALF_LENGTH;
use match_engine::player::KickKind;

/// (from own goal m, length m, lofted, predicted p, completed, under pressure, how it failed, forward m)
type Rec = (f32, f32, bool, f32, bool, bool, &'static str, f32);
/// Per pass at release: forward metres, nearest opponent to the target (m), target y (team frame), length (m), passer
/// x from the own goal line (m), passer role, intended receiver's role, passer y (team frame).
type Geo = (f32, f32, f32, f32, f32, String, String, f32);
/// Lofted 30–45 m pass: flight time to the first landing (s), landing error from the target (m), nearest opponent
/// to the ball at the first touch (m), aerial duel at the end.
type Aer = (f32, f32, f32, bool, f32);
/// Lofted pass being tracked: target, release time, landing (time, error), aerial duel.
type Lob = (glam::Vec2, f32, Option<(f32, f32)>, bool, Option<f32>);

fn play(seed: u64) -> (Vec<Rec>, Vec<Geo>, Vec<Aer>) {
    let mut sim = Simulation::from_builtin("northbridge", "harbour", seed);
    let mut out = Vec::new();
    let mut geo = Vec::new();
    let mut aer = Vec::new();
    let mut lob: Option<Lob> = None;
    let mut seen = 0;
    // Pass waiting for its outcome.
    let mut open: Option<Rec> = None;
    let mut open_side = None;
    let mut open_to = None;
    // First thing that happened to the pass: an opponent's interception / header, the receiver's heavy touch.
    let mut how: &'static str = "";
    while !sim.finished() {
        sim.step();
        let st = &sim.state;
        while seen < sim.events.len() {
            let e = &sim.events[seen];
            seen += 1;
            match e.kind {
                // Out of play: kept if the passer's team restarts.
                EventKind::Restart { side, .. } => {
                    if let Some(r) = open.take() {
                        out.push((r.0, r.1, r.2, r.3, open_side == Some(side), r.5, if how.is_empty() { "out of play" } else { how }, r.7));
                    }
                }
                EventKind::Interception { by } if how.is_empty() && open.is_some() && Some(side_of(by)) != open_side => how = "intercepted",
                EventKind::Header { by } if how.is_empty() && open.is_some() => {
                    how = if Some(side_of(by)) != open_side { "opponent header" } else { "own header" }
                }
                EventKind::Miscontrol { by } if how.is_empty() && open.is_some() => {
                    how = if Some(by) == open_to { "receiver heavy touch" } else if Some(side_of(by)) == open_side { "mate heavy touch" } else { "opponent touch" }
                }
                EventKind::Aerial { won, .. } if how.is_empty() && open.is_some() => {
                    how = if Some(side_of(won)) != open_side { "opponent aerial" } else { "own aerial" };
                    if let Some(l) = lob.as_mut() {
                        l.3 = true;
                        l.4.get_or_insert(st.time - l.1);
                    }
                }
                EventKind::Kickoff { .. } => open = None,
                _ => {}
            }
            // Headed before it came down: the landing is where it was headed.
            if let EventKind::Header { .. } = e.kind
                && let Some(l) = lob.as_mut()
            {
                l.4.get_or_insert(st.time - l.1);
                if l.2.is_none() {
                    l.2 = Some((st.time - l.1, (st.ball.ground_pos() - l.0).length()));
                }
            }
            if let EventKind::Pass { by, kind, length, headed: false, .. } = e.kind
                && st.is_live()
                && matches!(kind, KickKind::GroundPass | KickKind::LoftedPass)
            {
                if let Some(r) = open.take() {
                    // A pass played before the previous one was settled: the team kept the ball if it is theirs.
                    out.push((r.0, r.1, r.2, r.3, open_side == Some(side_of(by)), r.5, if how.is_empty() { "kicked on" } else { how }, r.7));
                }
                let p = &st.players[by];
                let x = HALF_LENGTH + st.frame(p.side).to_team(p.pos).x;
                let pressed = match_engine::ai::analysis::pressure_at(st, p.side, p.pos) > 0.5;
                let fwd = st.flight.map_or(0.0, |f| st.frame(p.side).to_team(f.target).x - st.frame(p.side).to_team(p.pos).x);
                open = Some((x, length, kind == KickKind::LoftedPass, p.debug.chosen_p, false, pressed, "", fwd));
                if let Some(f) = st.flight {
                    let fr = st.frame(p.side);
                    let tt = fr.to_team(f.target);
                    let near = st.squad(p.side.opponent()).iter().map(|o| (o.pos - f.target).length()).fold(f32::MAX, f32::min);
                    let to = match e.kind {
                        EventKind::Pass { to: Some(t), .. } => format!("{:?}", st.players[t].role),
                        _ => "-".to_string(),
                    };
                    geo.push((tt.x - fr.to_team(p.pos).x, near, tt.y, length, x, format!("{:?}", p.role), to, fr.to_team(p.pos).y));
                } else {
                    geo.push((0.0, -1.0, 0.0, length, x, String::new(), String::new(), 0.0));
                }
                lob = None;
                if kind == KickKind::LoftedPass
                    && (30.0..45.0).contains(&length)
                    && let Some(f) = st.flight
                {
                    lob = Some((f.target, e.t, None, false, None));
                }
                open_side = Some(p.side);
                open_to = match e.kind {
                    EventKind::Pass { to, .. } => to,
                    _ => None,
                };
                how = "";
            }
        }
        if let Some(l) = lob.as_mut()
            && l.2.is_none()
            && st.ball.pos.z < 0.2
            && st.time - l.1 > 0.3
        {
            l.2 = Some((st.time - l.1, (st.ball.ground_pos() - l.0).length()));
        }
        if let Some(l) = lob
            && (st.flight.is_none() || st.control.is_some())
        {
            let b = st.ball.ground_pos();
            let near = open_side.map_or(-1.0, |s| st.squad(s.opponent()).iter().map(|o| (o.pos - b).length()).fold(f32::MAX, f32::min));
            let (ft, err) = l.2.unwrap_or((-1.0, -1.0));
            aer.push((ft, err, near, l.3, l.4.unwrap_or(st.time - l.1)));
            lob = None;
        }
        if let Some(r) = open
            && st.is_live()
            && st.flight.is_none()
            && let Some(c) = st.control
        {
            let ok = Some(side_of(c)) == open_side;
            let h = if !how.is_empty() {
                how
            } else if ok {
                if Some(c) == open_to { "receiver" } else { "other mate" }
            } else {
                "opponent first to the ball"
            };
            out.push((r.0, r.1, r.2, r.3, ok, r.5, h, r.7));
            open = None;
        }
    }
    (out, geo, aer)
}

fn main() {
    let matches: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(20);
    let res: Vec<(Vec<Rec>, Vec<Geo>, Vec<Aer>)> = std::thread::scope(|scope| {
        let hs: Vec<_> = (1..=matches).map(|seed| scope.spawn(move || play(seed))).collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let all: Vec<Rec> = res.iter().flat_map(|r| r.0.iter().copied()).collect();
    let geo: Vec<Geo> = res.iter().flat_map(|r| r.1.iter().cloned()).collect();
    let aer: Vec<Aer> = res.iter().flat_map(|r| r.2.iter().copied()).collect();
    {
        let med = |mut v: Vec<f32>, q: f32| {
            v.sort_by(f32::total_cmp);
            if v.is_empty() { 0.0 } else { v[((v.len() as f32 * q) as usize).min(v.len() - 1)] }
        };
        let ft: Vec<f32> = aer.iter().filter(|a| a.0 >= 0.0).map(|a| a.0).collect();
        let err: Vec<f32> = aer.iter().filter(|a| a.1 >= 0.0).map(|a| a.1).collect();
        let near: Vec<f32> = aer.iter().filter(|a| a.2 >= 0.0).map(|a| a.2).collect();
        let contact: Vec<f32> = aer.iter().map(|a| a.4).collect();
        let n = aer.len().max(1) as f32;
        println!(
            "lofted 30–45 m: {:.0} per match; flight to landing median {:.2} s; landing error median {:.1} m, p75 {:.1} m, p90 {:.1} m; nearest opponent at first touch median {:.1} m, < 2 m {:.0}%; aerial duel {:.0}%; kick to first contact (StatsBomb `duration`, 2.45–2.56 s) median {:.2} s, p25 {:.2}, p75 {:.2}",
            aer.len() as f32 / matches as f32,
            med(ft, 0.5),
            med(err.clone(), 0.5),
            med(err.clone(), 0.75),
            med(err, 0.9),
            med(near.clone(), 0.5),
            100.0 * near.iter().filter(|&&d| d < 2.0).count() as f32 / n,
            100.0 * aer.iter().filter(|a| a.3).count() as f32 / n,
            med(contact.clone(), 0.5),
            med(contact.clone(), 0.25),
            med(contact, 0.75)
        );
    }
    if let Ok(path) = std::env::var("GEO") {
        let mut s = String::new();
        for g in &geo {
            writeln!(
                s,
                "{{\"fwd\": {:.1}, \"near\": {:.1}, \"ty\": {:.1}, \"len\": {:.1}, \"x\": {:.1}, \"by\": \"{}\", \"to\": \"{}\", \"fy\": {:.1}}}",
                g.0, g.1, g.2, g.3, g.4, g.5, g.6, g.7
            )
            .unwrap();
        }
        std::fs::write(&path, s).expect("write geo");
    }
    if let Ok(path) = std::env::var("DUMP") {
        let mut s = String::new();
        for r in &all {
            writeln!(s, "{{\"x\": {:.1}, \"len\": {:.1}, \"lofted\": {}, \"p\": {:.3}, \"ok\": {}, \"pressed\": {}}}", r.0, r.1, r.2, r.3, r.4, r.5).unwrap();
        }
        std::fs::write(&path, s).expect("write dump");
    }
    let zones = [(0.0, 35.0, "own third"), (35.0, 70.0, "middle third"), (70.0, 120.0, "final third")];
    let lens = [(0.0, 10.0), (10.0, 20.0), (20.0, 30.0), (30.0, 45.0), (45.0, 200.0)];
    println!("{} open-play passes per match", all.len() as f32 / matches as f32);
    // By length and direction (forward > 5 m, backward < -3 m, else lateral), as `sb_dir.py` counts StatsBomb.
    for (l0, l1) in lens {
        let lr: Vec<&Rec> = all.iter().filter(|r| r.1 >= l0 && r.1 < l1).collect();
        let parts: Vec<String> = [("fwd", 5.0, f32::MAX), ("lat", -3.0, 5.0), ("back", f32::MIN, -3.0)]
            .iter()
            .map(|&(name, a, b)| {
                let d: Vec<&&Rec> = lr.iter().filter(|r| r.7 > a && r.7 <= b).collect();
                let k = d.len().max(1) as f32;
                format!(
                    "{name} {:5.1}/m {:3.0}% compl {:3.0}% lofted",
                    d.len() as f32 / matches as f32,
                    100.0 * d.iter().filter(|r| r.4).count() as f32 / k,
                    100.0 * d.iter().filter(|r| r.2).count() as f32 / k
                )
            })
            .collect();
        println!("{l0:>3.0}–{l1:<3.0} m: {:6.1}/match  {}", lr.len() as f32 / matches as f32, parts.join(" | "));
    }
    for (z0, z1, zl) in zones {
        let zr: Vec<&Rec> = all.iter().filter(|r| r.0 >= z0 && r.0 < z1).collect();
        let n = zr.len().max(1) as f32;
        let mut lens_sorted: Vec<f32> = zr.iter().map(|r| r.1).collect();
        lens_sorted.sort_by(f32::total_cmp);
        println!(
            "{zl}: {:.0} per match, length median {:.1} m, completed {:.0}%, predicted {:.0}%, pressed {:.0}%",
            zr.len() as f32 / matches as f32,
            lens_sorted.get(lens_sorted.len() / 2).copied().unwrap_or(0.0),
            100.0 * zr.iter().filter(|r| r.4).count() as f32 / n,
            100.0 * zr.iter().map(|r| r.3).sum::<f32>() / n,
            100.0 * zr.iter().filter(|r| r.5).count() as f32 / n
        );
        for (l0, l1) in lens {
            let lr: Vec<&&Rec> = zr.iter().filter(|r| r.1 >= l0 && r.1 < l1).collect();
            let m = lr.len().max(1) as f32;
            println!(
                "    {l0:>3.0}–{l1:<3.0} m  share {:3.0}%  completed {:3.0}%  predicted {:3.0}%  lofted {:3.0}%",
                100.0 * lr.len() as f32 / n,
                100.0 * lr.iter().filter(|r| r.4).count() as f32 / m,
                100.0 * lr.iter().map(|r| r.3).sum::<f32>() / m,
                100.0 * lr.iter().filter(|r| r.2).count() as f32 / m
            );
            let mut hows: Vec<(String, usize)> = Vec::new();
            for r in lr.iter().filter(|r| !r.4) {
                let key = format!("{}{}", r.6, if r.2 { " (lofted)" } else { "" });
                match hows.iter_mut().find(|h| h.0 == key) {
                    Some(h) => h.1 += 1,
                    None => hows.push((key, 1)),
                }
            }
            hows.sort_by_key(|h| std::cmp::Reverse(h.1));
            let hs: Vec<String> = hows.iter().map(|(h, k)| format!("{h} {:.0}%", 100.0 * *k as f32 / m)).collect();
            println!("              failed: {}", hs.join(", "));
        }
    }
}
