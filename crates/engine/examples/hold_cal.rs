//! Calibration of the hold option ("control" / "hold / shield"): for every decision in which the carrier chose to keep
//! the ball, the keep probability the AI gave it (`OptionEval::p`), the pressure it expected at the next decision
//! (`pressed`) and the continuation premium it credited (best pass utility now minus the value of standing still),
//! against what happened: did he still have the ball at his next decision, did he keep it until his next kick, and
//! how good was his best pass then compared with now. By the pressure expected at the next decision.
//! Env `ATTRS=68 AWAY=47`: uniform attributes per side (as `layers`).
//! `cargo run -p match_engine --release --example hold_cal -- [matches]`

use match_engine::Simulation;
use match_engine::ai::analysis::{possession_value, pressure_at};
use match_engine::ai::personality::OptionKind;
use match_engine::events::EventKind;
use match_engine::match_state::side_of;
use match_engine::pitch::Side;
use match_engine::player::Attributes;

/// One hold decision: pressure now (distance based), pressed at the next decision, keep given, premium credited,
/// best pass utility now, away side; then: kept to the next decision, kept to the next kick, best pass utility at the
/// next decision (if any), holds in a row from here, tackled.
#[derive(Clone, Copy, Debug)]
struct Rec {
    pressure: f32,
    pressed: f32,
    keep: f32,
    premium: f32,
    best_now: Option<f32>,
    away: bool,
    /// Decided before the ball arrived ("control"); counted only once he had it.
    first_time: bool,
    got: bool,
    kept_next: bool,
    kept_kick: bool,
    best_next: Option<f32>,
    tackled: bool,
}

fn play(seed: u64) -> Vec<Rec> {
    let mut sim = Simulation::from_builtin("northbridge", "harbour", seed);
    if let Some(home) = std::env::var("ATTRS").ok().and_then(|s| s.parse::<f32>().ok()) {
        let away: f32 = std::env::var("AWAY").ok().and_then(|s| s.parse().ok()).unwrap_or(home);
        for p in &mut sim.state.players {
            let v = if p.side == Side::Away { away } else { home };
            p.attrs = Attributes { preferred_foot: p.attrs.preferred_foot, height_cm: p.attrs.height_cm, weight_kg: p.attrs.weight_kg, ..Attributes::uniform(v) };
            p.confidence = 1.0;
            p.energy = 1.0;
            p.familiarity = [1.0; 2];
        }
        let c = sim.state.conditions;
        sim.set_conditions(c);
    }
    sim.decision_log = Some(Vec::new());
    let mut out: Vec<Rec> = Vec::new();
    // Open hold per carrier: index into `out`.
    let mut open: Vec<Option<usize>> = vec![None; sim.state.players.len()];
    let mut seen_log = 0;
    let mut seen_ev = 0;
    while !sim.finished() {
        sim.step();
        let st = &sim.state;
        let log = sim.decision_log.as_ref().unwrap();
        while seen_log < log.len() {
            let (_, sit, opts, chosen) = &log[seen_log];
            seen_log += 1;
            let c = sit.carrier;
            let best = opts.iter().filter(|o| matches!(o.kind, OptionKind::Pass { .. })).map(|o| o.u_opt).fold(f32::MIN, f32::max);
            // This decision closes the carrier's open hold: he still had the ball.
            if let Some(i) = open[c].take() {
                out[i].kept_next = true;
                if best > f32::MIN {
                    out[i].best_next = Some(best);
                }
            }
            if !st.is_live() {
                continue;
            }
            let Some(o) = opts.get(*chosen) else { continue };
            if !matches!(o.kind, OptionKind::Hold) {
                continue;
            }
            let p = &st.players[c];
            let fr = st.frame(p.side);
            let v = possession_value(fr.to_team(p.pos), &sim.cfg.ai);
            // `DUMP=n`: print the first n pressed hold decisions of seed 1 with every option.
            if seed == 1
                && o.pressed > 0.9
                && let Some(n) = std::env::var("DUMP").ok().and_then(|s| s.parse::<usize>().ok())
                && out.iter().filter(|r| r.pressed > 0.9).count() < n
            {
                let mut sorted: Vec<&match_engine::ai::on_ball::OptionEval> = opts.iter().collect();
                sorted.sort_by(|a, b| b.u_opt.total_cmp(&a.u_opt));
                println!("  t {:.0} player {c} ({:?}) team-x {:.0} pressure {:.2} V {:.4} first_time {}", st.time, p.side, fr.to_team(p.pos).x, pressure_at(st, p.side, p.pos), v, sit.first_time);
                for o in sorted.iter().take(7) {
                    println!("      u {:+.4} p {:.2} pressed {:.2} prob {:.2}  {}  [{}]", o.u_opt, o.p, o.pressed, o.prob, o.label, o.rules.join(","));
                }
            }
            open[c] = Some(out.len());
            out.push(Rec {
                pressure: pressure_at(st, p.side, p.pos),
                pressed: o.pressed,
                keep: o.p,
                premium: if best > f32::MIN { (best - v).max(0.0) } else { 0.0 },
                best_now: (best > f32::MIN).then_some(best),
                away: p.side == Side::Away,
                first_time: sit.first_time,
                got: !sit.first_time || st.control == Some(c),
                kept_next: false,
                kept_kick: false,
                best_next: None,
                tackled: false,
            });
        }
        while seen_ev < sim.events.len() {
            let e = &sim.events[seen_ev];
            seen_ev += 1;
            match e.kind {
                EventKind::Pass { by, .. } | EventKind::Shot { by, .. } => {
                    // His kick: every hold of his still open (none, after the decision closed it) and the chain of
                    // holds before it kept the ball to a kick — walk back over his consecutive holds.
                    let mut i = out.len();
                    while i > 0 {
                        i -= 1;
                        let r = &mut out[i];
                        if r.away != (side_of(by) == Side::Away) {
                            continue;
                        }
                        if r.kept_kick || !r.kept_next {
                            break;
                        }
                        r.kept_kick = true;
                    }
                    if let Some(i) = open[by].take() {
                        out[i].kept_next = true;
                        out[i].kept_kick = true;
                    }
                }
                EventKind::Tackle { on, .. } => {
                    if let Some(i) = open[on] {
                        out[i].tackled = true;
                    }
                }
                EventKind::Restart { side, .. } => {
                    for (c, slot) in open.iter_mut().enumerate() {
                        if let Some(i) = slot.take()
                            && side_of(c) != side
                        {
                            out[i].kept_next = false;
                        }
                    }
                }
                _ => {}
            }
        }
        if let Some(c) = st.control {
            let s = side_of(c);
            if let Some(j) = open[c] {
                out[j].got = true;
            }
            for (i, slot) in open.iter_mut().enumerate() {
                if side_of(i) != s
                    && let Some(j) = slot.take()
                {
                    out[j].kept_next = false;
                }
            }
        }
        if let Some(log) = sim.decision_log.as_mut()
            && log.len() > 400
        {
            log.drain(..200);
            seen_log -= 200;
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
    // A "control" whose ball never arrived is the pass's failure, not the hold's.
    let res: Vec<Rec> = res.into_iter().filter(|r| r.got).collect();
    println!("hold decisions: {:.0}/match", res.len() as f32 / n as f32);
    let row = |name: String, r: Vec<&Rec>| {
        let c = r.len().max(1) as f32;
        let mean = |f: &dyn Fn(&Rec) -> f32| r.iter().map(|x| f(x)).sum::<f32>() / c;
        let with_next: Vec<(f32, f32)> = r.iter().filter_map(|x| Some((x.best_now?, x.best_next?))).collect();
        let decay = if with_next.is_empty() { f32::NAN } else { with_next.iter().map(|(a, b)| b - a).sum::<f32>() / with_next.len() as f32 };
        println!(
            "  {name:<18} {:6.1}/match  pressure {:.2}  keep given {:.2}  kept to next decision {:4.1} %  to next kick {:4.1} %  tackled {:4.1} %  premium {:+.4}  best pass next − now {:+.4}",
            r.len() as f32 / n as f32,
            mean(&|x| x.pressure),
            mean(&|x| x.keep),
            100.0 * mean(&|x| x.kept_next as u8 as f32),
            100.0 * mean(&|x| x.kept_kick as u8 as f32),
            100.0 * mean(&|x| x.tackled as u8 as f32),
            mean(&|x| x.premium),
            decay
        );
    };
    let sides: &[(&str, Option<bool>)] = if std::env::var("AWAY").is_ok() { &[("home", Some(false)), ("away", Some(true))] } else { &[("", None)] };
    for (sname, away) in sides {
        if !sname.is_empty() {
            println!("--- {sname}");
        }
        let res: Vec<&Rec> = res.iter().filter(|x| away.is_none_or(|a| x.away == a)).collect();
        for (name, ft) in [("control", true), ("hold", false)] {
            for (lo, hi) in [(0.0, 0.1), (0.1, 0.5), (0.5, 0.9), (0.9, 1.01)] {
                row(format!("{name} pressed {lo:.1}-{hi:.1}"), res.iter().filter(|x| x.first_time == ft && (lo..hi).contains(&x.pressed)).copied().collect());
            }
        }
        for (lo, hi) in [(0.0, 0.2), (0.2, 0.5), (0.5, 0.8), (0.8, 1.01)] {
            row(format!("pressure {lo:.1}-{hi:.1}"), res.iter().filter(|x| (lo..hi).contains(&x.pressure)).copied().collect());
        }
    }
}
