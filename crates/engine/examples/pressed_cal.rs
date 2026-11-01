//! Calibration of ATT-25 for passes: for every completed open-play pass, how pressed the AI expected the receiver to
//! be (`OptionEval::pressed` of the chosen option) against whether he lost the ball before his next pass or shot
//! (an opponent gained control, or the opponents got the restart). The AI prices that loss at
//! `att25_receiver_loss` × pressed. By pressed band and by pass length.
//! `cargo run -p match_engine --release --example pressed_cal -- [matches]`

use match_engine::Simulation;
use match_engine::ai::on_ball::Action;
use match_engine::ai::personality::OptionKind;
use match_engine::events::EventKind;
use match_engine::match_state::side_of;
use match_engine::player::Attributes;

/// (pressed at the decision, pass length, lost before his next pass or shot, away side, team lost the ball within 6 s,
/// free outlets at the reception: team-mates within 30 m with no opponent within 4 m, what happened on the ball first
/// (0 nothing, 1 heavy touch, 2 tackled), seconds on the ball)
type Rec = (f32, f32, bool, bool, bool, usize, u8, f32);

fn play(seed: u64) -> Vec<Rec> {
    let mut sim = Simulation::from_builtin("northbridge", "harbour", seed);
    // `ATTRS=68 AWAY=47`: every attribute uniform per side (as `layers` does), confidence / energy / role familiarity 1.
    if let Some(home) = std::env::var("ATTRS").ok().and_then(|s| s.parse::<f32>().ok()) {
        let away: f32 = std::env::var("AWAY").ok().and_then(|s| s.parse().ok()).unwrap_or(home);
        for p in &mut sim.state.players {
            let v = if p.side == match_engine::pitch::Side::Away { away } else { home };
            p.attrs = Attributes { preferred_foot: p.attrs.preferred_foot, height_cm: p.attrs.height_cm, weight_kg: p.attrs.weight_kg, ..Attributes::uniform(v) };
            p.confidence = 1.0;
            p.energy = 1.0;
            p.familiarity = [1.0; 2];
        }
        let c = sim.state.conditions;
        sim.set_conditions(c);
    }
    sim.decision_log = Some(Vec::new());
    let mut out = Vec::new();
    // Receptions whose 6 s chain is still open: (index into `out`, side, deadline).
    let mut chains: Vec<(usize, match_engine::pitch::Side, f32)> = Vec::new();
    let mut seen = 0;
    // Pass in flight: receiver, pressed, length.
    let mut flying: Option<(usize, f32, f32)> = None;
    // Receiver on the ball: receiver, pressed, length, free outlets, what happened first, since when.
    let mut holding: Option<(usize, f32, f32, usize, u8, f32)> = None;
    while !sim.finished() {
        sim.step();
        let st = &sim.state;
        while seen < sim.events.len() {
            let e = &sim.events[seen];
            seen += 1;
            if let Some(h) = holding.as_mut() {
                match e.kind {
                    EventKind::Miscontrol { by } if by == h.0 && h.4 == 0 => h.4 = 1,
                    EventKind::Tackle { on, .. } if on == h.0 && h.4 == 0 => h.4 = 2,
                    _ => {}
                }
            }
            if let Some(h) = holding {
                let side = side_of(h.0);
                let end = match e.kind {
                    EventKind::Pass { by, .. } | EventKind::Shot { by, .. } if by == h.0 => Some(false),
                    EventKind::Restart { side: s, .. } => Some(s != side),
                    _ => None,
                };
                if let Some(lost) = end {
                    chains.push((out.len(), side, st.time + 6.0));
                    out.push((h.1, h.2, lost, side == match_engine::pitch::Side::Away, false, h.3, h.4, st.time - h.5));
                    holding = None;
                }
            }
            if let EventKind::Restart { side: s, .. } = e.kind {
                chains.retain(|&(i, side, _)| {
                    if s != side {
                        out[i].4 = true;
                    }
                    s == side
                });
            }
            match e.kind {
                EventKind::Pass { by, to: Some(to), length, headed: false, .. } if st.is_live() => {
                    // The decision behind this pass: the carrier's latest logged one.
                    let log = sim.decision_log.as_ref().unwrap();
                    let pressed = log.iter().rev().find(|d| d.1.carrier == by).and_then(|(_, _, opts, i)| {
                        let o = opts.get(*i)?;
                        match (o.kind, o.action) {
                            (OptionKind::Pass { .. }, Action::Kick(k)) if k.receiver == Some(to) => Some(o.pressed),
                            _ => None,
                        }
                    });
                    flying = pressed.map(|p| (to, p, length));
                }
                EventKind::Pass { .. } | EventKind::Restart { .. } | EventKind::Kickoff { .. } => flying = None,
                _ => {}
            }
        }
        // Keep the log short: only the latest decision per carrier is needed.
        if let Some(log) = sim.decision_log.as_mut()
            && log.len() > 400
        {
            log.drain(..200);
        }
        if let Some(h) = holding
            && let Some(c) = st.control
            && side_of(c) != side_of(h.0)
        {
            let side = side_of(h.0);
            chains.push((out.len(), side, st.time + 6.0));
            out.push((h.1, h.2, true, side == match_engine::pitch::Side::Away, false, h.3, h.4, st.time - h.5));
            holding = None;
        }
        if let Some(c) = st.control {
            let s = side_of(c);
            chains.retain(|&(i, side, _)| {
                if s != side {
                    out[i].4 = true;
                }
                s == side
            });
        }
        chains.retain(|&(_, _, until)| st.time < until);
        if let Some(f) = flying
            && st.flight.is_none()
            && let Some(c) = st.control
        {
            if c == f.0 {
                let r = &st.players[c];
                let side = side_of(c);
                let outlets = st
                    .squad(side)
                    .iter()
                    .filter(|t| t.id != c && !t.is_gk() && (t.pos - r.pos).length() < 30.0)
                    .filter(|t| st.squad(side.opponent()).iter().all(|o| (o.pos - t.pos).length() > 4.0))
                    .count();
                holding = Some((f.0, f.1, f.2, outlets, 0, st.time));
            }
            flying = None;
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
    let tc = match_engine::config::SimConfig::default().tactics;
    let (k, base) = (tc.att25_receiver_loss, tc.att25_receiver_base_loss);
    println!("completed passes received: {:.0}/match; AI prices the loss at {base} + {k} × pressed", res.len() as f32 / n as f32);
    let row = |name: String, r: Vec<&Rec>| {
        let c = r.len().max(1) as f32;
        let pr: f32 = r.iter().map(|x| x.0).sum::<f32>() / c;
        let lost = r.iter().filter(|x| x.2).count() as f32 / c;
        let chain = r.iter().filter(|x| x.4).count() as f32 / c;
        let cause = |w: u8| r.iter().filter(|x| x.2 && x.6 == w).count() as f32 / c;
        let median = |mut v: Vec<f32>| {
            v.sort_by(f32::total_cmp);
            v.get(v.len() / 2).copied().unwrap_or(0.0)
        };
        let held_lost = median(r.iter().filter(|x| x.2).map(|x| x.7).collect());
        let held_kept = median(r.iter().filter(|x| !x.2).map(|x| x.7).collect());
        println!(
            "  {name:<20} {:6.1}/match  mean pressed {pr:.2}  lost before next pass {:4.1} % (heavy touch {:4.1}, tackled {:4.1}, other {:4.1})  AI {:4.1} %  team lost within 6 s {:4.1} %  on the ball median lost {held_lost:.1} s kept {held_kept:.1} s",
            r.len() as f32 / n as f32,
            100.0 * lost,
            100.0 * cause(1),
            100.0 * cause(2),
            100.0 * cause(0),
            100.0 * (base + k * pr),
            100.0 * chain
        );
    };
    let sides: &[(&str, Option<bool>)] =
        if std::env::var("AWAY").is_ok() { &[("home", Some(false)), ("away", Some(true))] } else { &[("", None)] };
    for (sname, away) in sides {
        if !sname.is_empty() {
            println!("--- {sname}");
        }
        let res: Vec<&Rec> = res.iter().filter(|x| away.is_none_or(|a| x.3 == a)).collect();
        for (lo, hi) in [(0.0, 0.1), (0.1, 0.5), (0.5, 0.9), (0.9, 1.01)] {
            row(format!("pressed {lo:.1}-{hi:.1}"), res.iter().filter(|x| (lo..hi).contains(&x.0)).copied().collect());
        }
        for (lo, hi) in [(0.0, 20.0), (20.0, 30.0), (30.0, 200.0)] {
            for (name, p) in [("free", false), ("pressed", true)] {
                row(format!("{lo:.0}-{hi:.0} m {name}"), res.iter().filter(|x| (lo..hi).contains(&x.1) && (x.0 > 0.5) == p).copied().collect());
            }
        }
        for (name, p) in [("free", false), ("pressed", true)] {
            for (lo, hi) in [(0, 1), (1, 2), (2, 3), (3, 99)] {
                row(format!("{name} outlets {lo}-{}", hi - 1), res.iter().filter(|x| (lo..hi).contains(&x.5) && (x.0 > 0.5) == p).copied().collect());
            }
        }
    }
}
