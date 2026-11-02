//! How does the ball go out of play? For every throw-in, corner and goal kick: the last on-ball action before it
//! (dribbled out while in control, or the last event: pass by kind, clearance, shot, save, block, tackle,
//! interception, heavy touch, header), whether the awarded team or its opponent played it, and the third of the
//! restart in the awarded team's frame. StatsBomb counterpart: `an/sb_outs.py` (scratchpad).
//! Usage: outs [matches] [minutes]

use std::collections::BTreeMap;

use match_engine::Simulation;
use match_engine::events::EventKind;
use match_engine::match_state::{PlayPhase, RestartKind};
use match_engine::pitch::{HALF_LENGTH, Side};
use match_engine::player::state::KickKind;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let matches: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(4);
    let minutes: f32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(90.0);

    // (restart, last action, played by the awarded team?) → count
    let mut rows: BTreeMap<(&str, String, bool), u32> = BTreeMap::new();
    let mut thirds: BTreeMap<(&str, usize), u32> = BTreeMap::new();
    let mut since: Vec<f32> = Vec::new();
    for seed in 1..=matches {
        let mut sim = Simulation::from_builtin("northbridge", "harbour", seed);
        sim.cfg.timing.half_length_s = minutes * 30.0;
        sim.state.half_length = sim.cfg.timing.half_length_s;
        let mut seen = 0usize;
        let mut last: Option<(String, Side, f32)> = None;
        let mut prev_control = sim.state.control;
        while !sim.finished() {
            sim.step();
            let now = sim.state.time;
            for e in &sim.events[seen..] {
                let side = |p: usize| sim.state.players[p].side;
                let ev = match &e.kind {
                    EventKind::Pass { by, kind, headed, to, .. } => {
                        let k = match (kind, headed) {
                            (_, true) => "headed pass",
                            (KickKind::GroundPass, _) if to.is_some() => "ground pass",
                            (KickKind::GroundPass, _) => "ground pass into space",
                            (KickKind::LoftedPass, _) if to.is_some() => "lofted pass",
                            (KickKind::LoftedPass, _) => "lofted pass into space",
                            (KickKind::Clearance, _) => "clearance",
                            (KickKind::Shot, _) => "shot",
                        };
                        Some((k.to_string(), side(*by)))
                    }
                    EventKind::Shot { by, .. } => Some(("shot".into(), side(*by))),
                    EventKind::Save { gk, .. } => Some(("keeper save".into(), side(*gk))),
                    EventKind::Block { by } => Some(("block".into(), side(*by))),
                    EventKind::Tackle { by, outcome, .. } => Some((format!("tackle {outcome:?}"), side(*by))),
                    EventKind::Interception { by } => Some(("interception".into(), side(*by))),
                    EventKind::Miscontrol { by } => Some(("heavy touch".into(), side(*by))),
                    EventKind::Header { by } => Some(("header".into(), side(*by))),
                    EventKind::Aerial { won, .. } => Some(("aerial".into(), side(*won))),
                    EventKind::Restart { kind, side } => {
                        let k = match kind {
                            RestartKind::ThrowIn => Some("throw-in"),
                            RestartKind::Corner => Some("corner"),
                            RestartKind::GoalKick => Some("goal kick"),
                            _ => None,
                        };
                        if let Some(k) = k {
                            let (label, by, t) = match prev_control {
                                Some(c) => ("dribbled out (in control)".to_string(), sim.state.players[c].side, now),
                                None => last.clone().unwrap_or((String::from("none"), side.opponent(), now)),
                            };
                            since.push(now - t);
                            *rows.entry((k, label, by == *side)).or_default() += 1;
                            if let PlayPhase::Restart { spot, .. } = sim.state.phase {
                                let x = sim.state.frame(*side).to_team(spot).x + HALF_LENGTH;
                                *thirds.entry((k, ((x / 35.0) as usize).min(2))).or_default() += 1;
                            }
                        }
                        last = None;
                        None
                    }
                    _ => None,
                };
                if let Some((l, s)) = ev {
                    last = Some((l, s, e.t));
                }
            }
            seen = sim.events.len();
            prev_control = sim.state.control;
        }
    }
    let per = |n: u32| n as f32 / matches as f32;
    let mut by_kind: BTreeMap<&str, u32> = BTreeMap::new();
    for ((k, _, _), n) in &rows {
        *by_kind.entry(k).or_default() += n;
    }
    println!("{matches} matches; per match: {}", by_kind.iter().map(|(k, n)| format!("{k} {:.1}", per(*n))).collect::<Vec<_>>().join(", "));
    println!("restart     last action                   by        per match");
    let mut v: Vec<_> = rows.into_iter().collect();
    v.sort_by(|a, b| a.0.0.cmp(b.0.0).then(b.1.cmp(&a.1)));
    for ((k, l, own), n) in v {
        if per(n) >= 0.1 {
            println!("{k:<11} {l:<29} {:<9} {:>5.2}", if own { "awarded" } else { "opponent" }, per(n));
        }
    }
    for k in ["throw-in", "corner", "goal kick"] {
        let t: Vec<String> = (0..3).map(|i| format!("{:.1}", per(*thirds.get(&(k, i)).unwrap_or(&0)))).collect();
        println!("{k} by the awarded team's third own / middle / final: {}", t.join(" / "));
    }
    since.sort_by(f32::total_cmp);
    let q = |f: f32| since.get(((since.len() as f32 - 1.0) * f) as usize).copied().unwrap_or(0.0);
    println!("time from the last action to the restart call: median {:.2} s, p90 {:.2} s", q(0.5), q(0.9));
}
