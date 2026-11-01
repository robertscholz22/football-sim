//! Calibration of the on-ball AI's keep probability for carrying options: for every open-play decision to dribble,
//! take on or hold, the probability the AI gave it against whether the opponents gained possession within
//! `WINDOW_S`. Also how many such decisions a carrier makes in a row before he passes, shoots or loses the ball.
//! `cargo run -p match_engine --release --example carry_cal -- [matches]`

use match_engine::Simulation;
use match_engine::match_state::side_of;

const WINDOW_S: f32 = 1.2;

/// Kind (0 dribble, 1 take on, 2 hold), predicted p, lost within the window, nearest opponent at the decision (m).
type Rec = (usize, f32, bool, f32, f32);

/// ATT-25 chain: pressed at the end of the chosen carrying option (`OptionEval::pressed`), predicted keep, lost before
/// the carrier's next kick (the rest of his spell).
type Chain = (f32, f32, bool);

/// Chosen dribble / take-on: nearest opponent (m), tagged ATT-04, tagged ATT-01, the layer-1 best option is a pass,
/// the best pass beats it without personality (layer 1 + 2), it is a take-on.
type Why = (f32, bool, bool, bool, bool, bool);

/// One match's records: decisions, runs, spells, chains, why carried.
type Played = (Vec<Rec>, Vec<u32>, Vec<Spell>, Vec<Chain>, Vec<Why>);

/// Per carrying decision in a spell: pressured (pressure_at > 0.5), direction (0 forward ≤ 35°, 1 lateral, 2 back,
/// 3 hold), team-frame x.
type Dec = (bool, usize, f32);

/// A finished spell: its carrying decisions, how it ended (0 pass / shot, 1 lost, 2 other), carrier role.
type Spell = (Vec<Dec>, usize, String);

fn play(seed: u64) -> Played {
    let mut sim = Simulation::from_builtin("northbridge", "harbour", seed);
    sim.decision_log = Some(Vec::new());
    let mut chains: Vec<Chain> = Vec::new();
    let mut whys: Vec<Why> = Vec::new();
    // This spell's carrying decisions awaiting its end: pressed, keep.
    let mut open_chain: Vec<(f32, f32)> = Vec::new();
    let mut last = vec![f32::MIN; sim.state.players.len()];
    // Pending decisions: kind, p, decision time, side, nearest opponent.
    let mut pending: Vec<(usize, f32, f32, match_engine::pitch::Side, f32, f32)> = Vec::new();
    let mut out = Vec::new();
    // Carrying decisions in a row by the current carrier, by spell.
    let mut runs = Vec::new();
    let mut run = (usize::MAX, 0u32);
    let mut spells: Vec<Spell> = Vec::new();
    let mut cur: Vec<Dec> = Vec::new();
    let mut seen = 0;
    let mut kicked = false;
    while !sim.finished() {
        let vel_before: Vec<glam::Vec2> = sim.state.players.iter().map(|p| p.vel).collect();
        sim.step();
        if let Some(l) = &mut sim.decision_log
            && l.len() > 400
        {
            l.drain(..300);
        }
        let st = &sim.state;
        let now = st.time;
        pending.retain(|&(k, p, t, side, near, turn)| {
            if st.possession != side {
                out.push((k, p, true, near, turn));
                return false;
            }
            if now - t >= WINDOW_S {
                out.push((k, p, false, near, turn));
                return false;
            }
            true
        });
        while seen < sim.events.len() {
            let e = &sim.events[seen];
            seen += 1;
            if let match_engine::events::EventKind::Pass { by, .. } | match_engine::events::EventKind::Shot { by, .. } = e.kind
                && by == run.0
            {
                kicked = true;
            }
        }
        if st.control != Some(run.0) {
            if run.1 > 0 {
                runs.push(run.1);
                let end = if kicked {
                    0
                } else if st.control.is_some_and(|c| side_of(c) != side_of(run.0)) || st.possession != side_of(run.0) {
                    1
                } else {
                    2
                };
                spells.push((std::mem::take(&mut cur), end, format!("{:?}", st.players[run.0].role)));
                for (pr, k) in open_chain.drain(..) {
                    chains.push((pr, k, end == 1));
                }
            }
            open_chain.clear();
            cur.clear();
            kicked = false;
            run = (st.control.unwrap_or(usize::MAX), 0);
        }
        for p in &st.players {
            if p.last_decision == last[p.id] {
                continue;
            }
            last[p.id] = p.last_decision;
            if !st.is_live() || st.control != Some(p.id) {
                continue;
            }
            let label = p.debug.label.as_str();
            let kind = if label.starts_with("dribble") {
                0
            } else if label.starts_with("take on") {
                1
            } else if label.starts_with("hold") {
                2
            } else {
                continue;
            };
            let near = st.squad(p.side.opponent()).iter().map(|o| (o.pos - p.pos).length()).fold(f32::MAX, f32::min);
            // Turn the dribble asks of him at the decision (s of braking the velocity not along it, as `time_to_reach`).
            let mut turn = 0.0;
            if let Some((_, _, opts, chosen)) = sim.decision_log.as_ref().and_then(|l| l.iter().rev().find(|d| d.1.carrier == p.id)) {
                open_chain.push((opts[*chosen].pressed, opts[*chosen].p));
                if let match_engine::ai::on_ball::Action::Dribble { dir, .. } = opts[*chosen].action {
                    let c = &opts[*chosen];
                    let is_pass = |o: &match_engine::ai::on_ball::OptionEval| matches!(o.action, match_engine::ai::on_ball::Action::Kick(k) if k.receiver.is_some());
                    let l1 = opts.iter().max_by(|a, b| a.u_opt.total_cmp(&b.u_opt)).is_some_and(is_pass);
                    let bp = opts.iter().filter(|o| is_pass(o)).map(|o| o.u_opt + o.instr).fold(f32::MIN, f32::max);
                    whys.push((near, c.rules.contains(&"ATT-04"), c.rules.contains(&"ATT-01"), l1, bp > c.u_opt + c.instr, kind == 1));
                    let v = vel_before[p.id];
                    let along = v.dot(dir);
                    turn = (v - along * dir).length() / p.kin.decel.max(p.kin.lateral_accel).max(0.1) + (-along).max(0.0) / p.kin.decel.max(0.1);
                }
            }
            pending.push((kind, p.debug.chosen_p, p.last_decision, side_of(p.id), near, turn));
            run.1 += 1;
            let deg: f32 = label.split_whitespace().last().and_then(|w| w.trim_end_matches('°').parse().ok()).unwrap_or(0.0);
            let dir = if kind == 2 {
                3
            } else if deg.abs() <= 35.0 {
                0
            } else if deg.abs() <= 70.0 {
                1
            } else {
                2
            };
            let pressured = match_engine::ai::analysis::pressure_at(st, p.side, st.ball.ground_pos()) > 0.5;
            let x = match_engine::pitch::HALF_LENGTH + st.frame(p.side).to_team(p.pos).x;
            cur.push((pressured, dir, x));
        }
    }
    (out, runs, spells, chains, whys)
}

fn main() {
    let n: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(10);
    let res: Vec<Played> = std::thread::scope(|s| {
        let hs: Vec<_> = (1..=n).map(|seed| s.spawn(move || play(seed))).collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let recs: Vec<Rec> = res.iter().flat_map(|r| r.0.iter().copied()).collect();
    let mut runs: Vec<u32> = res.iter().flat_map(|r| r.1.iter().copied()).collect();
    runs.sort();
    println!("{n} matches; lost = opponents gain possession within {WINDOW_S} s of the decision");
    for (k, name) in ["dribble", "take on", "hold"].iter().enumerate() {
        let r: Vec<&Rec> = recs.iter().filter(|r| r.0 == k).collect();
        if r.is_empty() {
            continue;
        }
        let mean_p = r.iter().map(|r| r.1).sum::<f32>() / r.len() as f32;
        let kept = r.iter().filter(|r| !r.2).count() as f32 / r.len() as f32;
        println!("{name:<8} {:7.1}/match  predicted keep {:.2}  actual {:.2}", r.len() as f32 / n as f32, mean_p, kept);
        for (lo, hi) in [(0.0, 0.6), (0.6, 0.8), (0.8, 0.9), (0.9, 0.95), (0.95, 1.01)] {
            let b: Vec<&&Rec> = r.iter().filter(|r| (lo..hi).contains(&r.1)).collect();
            if !b.is_empty() {
                let kept = b.iter().filter(|r| !r.2).count() as f32 / b.len() as f32;
                println!("    p {lo:.2}–{hi:.2}: {:6.1}/match  kept {:.2}", b.len() as f32 / n as f32, kept);
            }
        }
        for (lo, hi) in [(0.0, 1.5), (1.5, 3.0), (3.0, 5.0), (5.0, 999.0)] {
            let b: Vec<&&Rec> = r.iter().filter(|r| (lo..hi).contains(&r.3)).collect();
            if !b.is_empty() {
                let kept = b.iter().filter(|r| !r.2).count() as f32 / b.len() as f32;
                let mean_p = b.iter().map(|r| r.1).sum::<f32>() / b.len() as f32;
                println!("    nearest opponent {lo}–{hi} m: {:6.1}/match  predicted {mean_p:.2}  kept {kept:.2}", b.len() as f32 / n as f32);
            }
        }
        for (lo, hi) in [(0.0, 0.1), (0.1, 0.3), (0.3, 0.6), (0.6, 99.0)] {
            let b: Vec<&&Rec> = r.iter().filter(|r| r.0 < 2 && (lo..hi).contains(&r.4)).collect();
            if !b.is_empty() {
                let kept = b.iter().filter(|r| !r.2).count() as f32 / b.len() as f32;
                let mean_p = b.iter().map(|r| r.1).sum::<f32>() / b.len() as f32;
                let close = b.iter().filter(|r| r.3 < 3.0).count() as f32 / b.len() as f32;
                println!("    turn {lo}–{hi} s: {:6.1}/match  predicted {mean_p:.2}  kept {kept:.2}  (opponent within 3 m {:.0}%)", b.len() as f32 / n as f32, 100.0 * close);
            }
        }
    }
    let whys: Vec<&Why> = res.iter().flat_map(|r| r.4.iter()).collect();
    println!("chosen dribbles / take-ons by nearest opponent: per match, take-on share, tagged ATT-04 / ATT-01, layer-1 best a pass, best pass better without personality:");
    for (lo, hi) in [(0.0, 3.0), (3.0, 8.0), (8.0, 999.0)] {
        let b: Vec<&&Why> = whys.iter().filter(|w| (lo..hi).contains(&w.0)).collect();
        let f = |g: &dyn Fn(&Why) -> bool| 100.0 * b.iter().filter(|w| g(w)).count() as f32 / b.len().max(1) as f32;
        println!(
            "    {lo}–{hi} m: {:6.1}/match  take on {:.0}%  ATT-04 {:.0}%  ATT-01 {:.0}%  layer-1 pass {:.0}%  pass without personality {:.0}%",
            b.len() as f32 / n as f32,
            f(&|w| w.5),
            f(&|w| w.1),
            f(&|w| w.2),
            f(&|w| w.3),
            f(&|w| w.4)
        );
    }
    let chains: Vec<&Chain> = res.iter().flat_map(|r| r.3.iter()).collect();
    let pl = sim_cfg_pressed_loss();
    println!("ATT-25 chain: carrying decisions by pressed at their end; the AI's implied loss (1 − keep) + keep × {pl} × pressed against the carrier losing the ball before his next kick:");
    for (lo, hi) in [(0.0, 0.1), (0.1, 0.3), (0.3, 0.5), (0.5, 0.7), (0.7, 0.9), (0.9, 1.01)] {
        let b: Vec<&&Chain> = chains.iter().filter(|c| (lo..hi).contains(&c.0)).collect();
        if b.is_empty() {
            continue;
        }
        let implied = b.iter().map(|c| (1.0 - c.1) + c.1 * pl * c.0).sum::<f32>() / b.len() as f32;
        let lost = b.iter().filter(|c| c.2).count() as f32 / b.len() as f32;
        println!("    pressed {lo:.1}–{hi:.1}: {:6.1}/match  implied {implied:.2}  lost before next kick {lost:.2}", b.len() as f32 / n as f32);
    }
    let spells: Vec<&Spell> = res.iter().flat_map(|r| r.2.iter()).collect();
    for (lo, hi) in [(1usize, 2usize), (2, 5), (5, 10), (10, 1000)] {
        let sp: Vec<&&Spell> = spells.iter().filter(|s| (lo..hi).contains(&s.0.len())).collect();
        if sp.is_empty() {
            continue;
        }
        let decs: Vec<&Dec> = sp.iter().flat_map(|s| s.0.iter()).collect();
        let nd = decs.len() as f32;
        let share = |f: &dyn Fn(&Dec) -> bool| 100.0 * decs.iter().filter(|d| f(d)).count() as f32 / nd;
        let ends = |k: usize| 100.0 * sp.iter().filter(|s| s.1 == k).count() as f32 / sp.len() as f32;
        let mut roles = std::collections::BTreeMap::new();
        for s in &sp {
            *roles.entry(s.2.clone()).or_insert(0u32) += 1;
        }
        let mut roles: Vec<_> = roles.into_iter().collect();
        roles.sort_by_key(|a| std::cmp::Reverse(a.1));
        println!(
            "spells with {lo}–{} carrying decisions: {:.0}/match; decisions pressured {:.0}%, fwd {:.0}% lat {:.0}% back {:.0}% hold {:.0}%, mean x {:.0}; end pass/shot {:.0}% lost {:.0}% other {:.0}%; roles {}",
            hi - 1,
            sp.len() as f32 / n as f32,
            share(&|d| d.0),
            share(&|d| d.1 == 0),
            share(&|d| d.1 == 1),
            share(&|d| d.1 == 2),
            share(&|d| d.1 == 3),
            decs.iter().map(|d| d.2).sum::<f32>() / nd,
            ends(0),
            ends(1),
            ends(2),
            roles.iter().take(4).map(|(r, c)| format!("{r} {:.0}%", 100.0 * *c as f32 / sp.len() as f32)).collect::<Vec<_>>().join(" ")
        );
    }
    let q = |f: f32| runs[((runs.len() - 1) as f32 * f) as usize];
    println!(
        "carrying decisions in a row per spell on the ball: {:.0} spells/match, median {}, p75 {}, p90 {}, ≥ 5: {:.0}%",
        runs.len() as f32 / n as f32,
        q(0.5),
        q(0.75),
        q(0.9),
        100.0 * runs.iter().filter(|&&r| r >= 5).count() as f32 / runs.len() as f32
    );
}

fn sim_cfg_pressed_loss() -> f32 {
    match_engine::config::SimConfig::default().tactics.att25_pressed_loss
}
