//! How goals are scored: for every goal, the event just before it (the scorer's shot, a save, a pass, a header, a
//! tackle …), own goals, and the scorer's intent label at the goal. Goals that are not a shot crossing the line
//! (deflections, rebounds, a ball dribbled or passed into the net) explain a lab goal count above shots on target
//! minus saves. `cargo run -p match_engine --release --example goals -- [matches]`

use std::collections::BTreeMap;

use match_engine::Simulation;
use match_engine::events::EventKind;
use match_engine::match_state::side_of;

/// Per match: goals, counts by the event before the goal, counts by the scorer's intent label.
type Tally = (u32, BTreeMap<String, u32>, BTreeMap<String, u32>);

fn main() {
    let matches: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(20);
    let per_match: Vec<Tally> = std::thread::scope(|scope| {
        let handles: Vec<_> = (1..=matches)
            .map(|seed| {
                scope.spawn(move || {
                    let mut sim = Simulation::from_builtin("northbridge", "harbour", seed);
                    // `CFG=file.ron`: `SimConfig` overrides, as `match_lab --config`.
                    if let Ok(path) = std::env::var("CFG") {
                        sim.cfg = match_engine::config::SimConfig::from_ron(&std::fs::read_to_string(path).unwrap()).unwrap();
                    }
                    let mut seen = 0;
                    let mut goals = 0u32;
                    let mut before: BTreeMap<String, u32> = BTreeMap::new();
                    let mut labels: BTreeMap<String, u32> = BTreeMap::new();
                    while !sim.finished() {
                        sim.step();
                        while seen < sim.events.len() {
                            let e = sim.events[seen].clone();
                            seen += 1;
                            let EventKind::Goal { side, scorer, own_goal, .. } = e.kind else { continue };
                            goals += 1;
                            let prev = sim.events[..seen - 1]
                                .iter()
                                .rev()
                                .find(|p| !matches!(p.kind, EventKind::Restart { .. } | EventKind::Kickoff { .. }));
                            let dt = prev.map_or(f32::MAX, |p| e.t - p.t);
                            let class = match prev.map(|p| &p.kind) {
                                _ if own_goal => "own goal".to_string(),
                                Some(EventKind::Shot { by, .. }) if Some(*by) == scorer => "scorer's shot".to_string(),
                                Some(EventKind::Shot { by, .. }) if side_of(*by) == side => "teammate's shot, then a touch".to_string(),
                                Some(EventKind::Save { .. }) => "after a save (rebound / parry)".to_string(),
                                Some(EventKind::Pass { by, kind, .. }) if Some(*by) == scorer => format!("scorer's {kind:?} into the goal"),
                                Some(EventKind::Pass { .. }) => "pass, then a touch".to_string(),
                                Some(EventKind::Header { by }) if Some(*by) == scorer => "scorer's header".to_string(),
                                Some(k) => format!("after {}", format!("{k:?}").split([' ', '{', '(']).next().unwrap_or("?")),
                                None => "none".to_string(),
                            };
                            let class = if dt > 2.0 && !class.starts_with("own") { format!("{class} (> 2 s before)") } else { class };
                            *before.entry(class).or_default() += 1;
                            if let Some(s) = scorer {
                                let l = sim.state.players[s].debug.label.split(" →").next().unwrap_or("").to_string();
                                *labels.entry(l).or_default() += 1;
                            }
                        }
                    }
                    (goals, before, labels)
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut goals = 0;
    let mut before: BTreeMap<String, u32> = BTreeMap::new();
    let mut labels: BTreeMap<String, u32> = BTreeMap::new();
    for (g, b, l) in per_match {
        goals += g;
        for (k, v) in b {
            *before.entry(k).or_default() += v;
        }
        for (k, v) in l {
            *labels.entry(k).or_default() += v;
        }
    }
    let n = matches as f64;
    println!("{matches} matches: {:.2} goals per match", goals as f64 / n);
    let mut rows: Vec<_> = before.into_iter().collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.1));
    println!("event before the goal:");
    for (k, v) in rows {
        println!("  {:<44} {:>5.2} per match  {:>4.0} %", k, v as f64 / n, 100.0 * v as f64 / goals.max(1) as f64);
    }
    let mut rows: Vec<_> = labels.into_iter().collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.1));
    println!("scorer's intent label at the goal:");
    for (k, v) in rows.into_iter().take(12) {
        println!("  {:<44} {:>5.2} per match", k, v as f64 / n);
    }
}
