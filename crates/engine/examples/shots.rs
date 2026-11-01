//! Shot diagnostics: outcome (goal / save / off target / other) by distance band, keeper depth at the shot and xG.
//! `cargo run -p match_engine --release --example shots -- [matches] [minutes] [--trace]`
//! Env: `SOFT`, `HARD`, `SLACK` override `foot_outside_soft_deg`, `foot_outside_hard_deg`, `strong_foot_slack_s`.

use match_engine::Simulation;
use match_engine::events::EventKind;
use match_engine::match_state::side_of;
use match_engine::pitch::HALF_LENGTH;

#[derive(Default, Clone, Copy)]
struct Band {
    shots: u32,
    goals: u32,
    saves: u32,
    xg: f32,
    gk_depth: f32,
    first_time: u32,
    cone: u32,
    nearest: f32,
    /// Sums of the xG factors at the moment of the shot: basic, after blockers, after keeper, after pressure, total.
    parts: [f32; 5],
    /// Shots where the keeper term added more than 0.05.
    keeper_boost: u32,
    /// Sum of the finishing factor (skill and foot against the reference finisher) at the shot.
    finish: f32,
    /// Shots where the keeper term added more than 0.05: goals and summed decision xG.
    boost_goals: u32,
    boost_xg: f32,
}

/// Shots, goals and summed decision xG of one group of shots.
#[derive(Default, Clone, Copy)]
struct Group {
    shots: u32,
    goals: u32,
    xg: f64,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let matches: u64 = args.first().and_then(|s| s.parse().ok()).unwrap_or(4);
    let mins: f32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(90.0);
    let trace = args.iter().any(|a| a == "--trace");
    let edges = [6.0, 11.0, 16.5, 22.0, 30.0, 40.0, 200.0];
    let mut bands = [Band::default(); 7];
    let mut headers = 0u32;
    // [strong foot, weak foot, header], [central (|y| < 7 m), wide] footed shots, [inside, outside the box].
    let mut feet = [Group::default(); 3];
    let mut lateral = [Group::default(); 2];
    let mut boxes = [Group::default(); 2];
    // Passes: [all, with the weaker foot].
    let mut passes = [0u32; 2];
    // Carriers in shooting range (every 0.2 s), by band: (samples, opponents in the shooter–posts triangle).
    let mut carry_tri = [(0u32, 0u32); 7];
    let mut phases: std::collections::BTreeMap<String, u32> = Default::default();
    // Shots from 16.5 m or more: [won the ball < 5 s before][keeper > 3 m deeper than his GK-01 depth].
    let mut long_gk = [[0u32; 2]; 2];
    for seed in 1..=matches {
        let mut sim = Simulation::from_builtin("northbridge", "harbour", seed);
        let env = |k: &str| std::env::var(k).ok().and_then(|s| s.parse::<f32>().ok());
        if let Some(v) = env("SOFT") {
            sim.cfg.execution.foot_outside_soft_deg = v;
        }
        if let Some(v) = env("HARD") {
            sim.cfg.execution.foot_outside_hard_deg = v;
        }
        if let Some(v) = env("SLACK") {
            sim.cfg.execution.strong_foot_slack_s = v;
        }
        let mut seen = 0;
        let mut open: Option<(usize, f32, [usize; 4])> = None;
        let mut poss = (sim.state.possession, 0.0f32);
        while sim.state.time < mins * 60.0 && !sim.finished() {
            sim.step();
            let st = &sim.state;
            if st.possession != poss.0 {
                poss = (st.possession, st.time);
            }
            if let Some(c) = st.control.filter(|_| st.is_live() && (st.time / 0.2).fract() < 0.1) {
                let side = side_of(c);
                let fr = st.frame(side);
                let p = fr.to_team(st.players[c].pos);
                let d = (glam::Vec2::new(HALF_LENGTH, 0.0) - p).length();
                if let Some(b) = edges.iter().position(|&e| d < e).filter(|&b| b < 6) {
                    carry_tri[b].0 += 1;
                    carry_tri[b].1 += in_triangle(st, side, p) as u32;
                }
            }
            while seen < sim.events.len() {
                let e = sim.events[seen].clone();
                seen += 1;
                let st = &sim.state;
                match e.kind {
                    EventKind::Shot { by, xg, first_time, weak_foot } => {
                        let side = side_of(by);
                        let fr = st.frame(side);
                        let p = fr.to_team(st.players[by].pos);
                        let d = (glam::Vec2::new(HALF_LENGTH, 0.0) - p).length();
                        let b = edges.iter().position(|&e| d < e).unwrap();
                        let gk = st.squad(side.opponent()).iter().find(|q| q.is_gk()).unwrap();
                        let depth = HALF_LENGTH - fr.to_team(gk.pos).x;
                        bands[b].shots += 1;
                        bands[b].xg += xg;
                        bands[b].gk_depth += depth;
                        bands[b].first_time += first_time as u32;
                        if d >= 16.5 {
                            let high = depth > match_engine::ai::analysis::gk01_depth(d, &sim.cfg.tactics) + 3.0;
                            long_gk[(st.time - poss.1 < 5.0) as usize][high as usize] += 1;
                        }
                        // Defenders in the shooter–posts triangle and the nearest defender.
                        let (pl, pr) = (glam::Vec2::new(HALF_LENGTH, 3.66), glam::Vec2::new(HALF_LENGTH, -3.66));
                        let mut cone = 0;
                        let mut nearest = f32::MAX;
                        for q in st.squad(side.opponent()).iter().filter(|q| !q.is_gk()) {
                            let qt = fr.to_team(q.pos);
                            nearest = nearest.min((qt - p).length());
                            let d1 = (qt - p).perp_dot(pl - p);
                            let d2 = (qt - pl).perp_dot(pr - pl);
                            let d3 = (qt - pr).perp_dot(p - pr);
                            if (d1 < 0.0) == (d2 < 0.0) && (d2 < 0.0) == (d3 < 0.0) {
                                cone += 1;
                            }
                        }
                        bands[b].cone += cone;
                        let xp = match_engine::ai::analysis::shot_xg_parts(st, &sim.cfg, by, p, Default::default());
                        for (k, v) in [xp.basic, xp.after_blockers, xp.after_keeper, xp.after_pressure, xp.total].into_iter().enumerate() {
                            bands[b].parts[k] += v;
                        }
                        let boosted = xp.after_keeper > xp.after_blockers + 0.05;
                        bands[b].keeper_boost += boosted as u32;
                        if boosted {
                            bands[b].boost_xg += xg;
                        }
                        bands[b].finish += xp.finish_factor;
                        bands[b].nearest += nearest.min(30.0);
                        let ph = format!("{:?}", st.team_view(side.opponent()).phase);
                        *phases.entry(ph).or_insert(0u32) += 1;
                        let header = st.players[by].debug.label.contains("header")
                            || sim.events[seen.saturating_sub(2)..seen].iter().any(|e| matches!(e.kind, EventKind::Header { .. }));
                        headers += header as u32;
                        let in_box = p.x > HALF_LENGTH - 16.5 && p.y.abs() < 20.16;
                        let groups =
                            [if header { 2 } else { weak_foot as usize }, (p.y.abs() >= 7.0) as usize, (!in_box) as usize, boosted as usize];
                        feet[groups[0]].shots += 1;
                        feet[groups[0]].xg += xg as f64;
                        if !header {
                            lateral[groups[1]].shots += 1;
                            lateral[groups[1]].xg += xg as f64;
                        }
                        boxes[groups[2]].shots += 1;
                        boxes[groups[2]].xg += xg as f64;
                        open = Some((b, e.t, groups));
                        if trace {
                            let gt = fr.to_team(gk.pos);
                            let tt = fr.to_team(gk.target);
                            let line = (glam::Vec2::new(HALF_LENGTH, 0.0) - p).normalize();
                            let off = (gt - p).perp_dot(line);
                            println!(
                                "{:6.1} SHOT {:<16} d {:4.1} at ({:5.1},{:5.1}) xG {:.2} | gk ({:5.1},{:5.1}) target ({:5.1},{:5.1}) off-line {:4.1} speed {:3.1} {:?} '{}' | on goal {}",
                                e.t, st.players[by].name, d, p.x, p.y, xg, gt.x, gt.y, tt.x, tt.y, off, gk.speed(), gk.body, gk.debug.label,
                                st.flight.is_some_and(|f| f.on_goal)
                            );
                            if d >= 22.0 {
                                let mut o = st.players[by].debug.options.clone();
                                o.sort_by(|a, b| b.4.total_cmp(&a.4));
                                for (l, u, i, pe, pr) in o.iter().take(5) {
                                    println!("         {pr:4.2}  U {u:6.3} {i:+6.3} {pe:+6.3}  {l}");
                                }
                            }
                            let spot = match_engine::ai::analysis::gk01_spot(p, &sim.cfg);
                            println!(
                                "       keeper open {:.2} vs set {:.2}; keeper {:.1} m from the GK-01 spot ({:5.1},{:5.1}), basic {:.3} → keeper {:.3}",
                                xp.open_share, xp.open_share_set, (gt - spot).length(), spot.x, spot.y, xp.after_blockers, xp.after_keeper
                            );
                        }
                    }
                    EventKind::Pass { .. } => {
                        passes[0] += 1;
                        passes[1] += st.flight.is_some_and(|f| f.weak_foot) as u32;
                    }
                    EventKind::Goal { own_goal, .. } => {
                        if let Some((b, _, g)) = open.take() {
                            bands[b].goals += 1;
                            bands[b].boost_goals += g[3] as u32;
                            feet[g[0]].goals += 1;
                            if g[0] != 2 {
                                lateral[g[1]].goals += 1;
                            }
                            boxes[g[2]].goals += 1;
                        }
                        if trace {
                            println!("{:6.1}   GOAL own {own_goal}", e.t);
                        }
                    }
                    EventKind::Save { held, .. } => {
                        if let Some((b, _, _)) = open.take() {
                            bands[b].saves += 1;
                        }
                        if trace {
                            println!("{:6.1}   save held {held}", e.t);
                        }
                    }
                    _ => {
                        if open.is_some_and(|(_, t, _)| e.t - t > 3.0) {
                            open = None;
                        }
                    }
                }
            }
        }
    }
    println!("band (m)      shots  goals  saves  conv   mean xG  gk depth  first-time  in cone  nearest def");
    let mut lo = 0.0;
    for (i, b) in bands.iter().enumerate() {
        let n = b.shots.max(1) as f32;
        println!(
            "{:>4.1}-{:<6.1} {:>6} {:>6} {:>6} {:>5.0}%  {:>7.3}  {:>7.1}  {:>5.0}%      {:>5.2}  {:>6.1}",
            lo, edges[i], b.shots, b.goals, b.saves, 100.0 * b.goals as f32 / n, b.xg / n, b.gk_depth / n, 100.0 * b.first_time as f32 / n,
            b.cone as f32 / n, b.nearest / n
        );
        lo = edges[i];
    }
    println!("\nxG factors at the shot (mean): basic → blockers → keeper → pressure → finishing (finishing factor)");
    let mut lo = 0.0;
    for (i, b) in bands.iter().enumerate() {
        let n = b.shots.max(1) as f32;
        let x = |k: usize| b.parts[k] / n;
        println!(
            "{:>4.1}-{:<6.1} {:.3} → {:.3} → {:.3} → {:.3} → {:.3} ({:.2})   keeper term adds > 0.05 in {:.0} %: {} goals / {:.1} xG",
            lo,
            edges[i],
            x(0),
            x(1),
            x(2),
            x(3),
            x(4),
            b.finish / n,
            100.0 * b.keeper_boost as f32 / n,
            b.boost_goals,
            b.boost_xg
        );
        lo = edges[i];
    }
    println!("header shots ≈ {headers}");
    let n_shots: u32 = bands.iter().map(|b| b.shots).sum();
    let n_goals: u32 = bands.iter().map(|b| b.goals).sum();
    let xg_sum: f64 = feet.iter().map(|g| g.xg).sum();
    println!(
        "per match: {:.1} shots, {:.2} goals, {:.2} xG (goals per xG {:.2})",
        n_shots as f64 / matches as f64,
        n_goals as f64 / matches as f64,
        xg_sum / matches as f64,
        n_goals as f64 / xg_sum.max(1e-9)
    );
    let show = |name: &str, g: &Group, all: u32| {
        println!(
            "  {name:<22} {:>5} shots ({:>4.1} %)  conv {:>5.1} %  mean xG {:.3}  goals/xG {:.2}",
            g.shots,
            100.0 * g.shots as f64 / all.max(1) as f64,
            100.0 * g.goals as f64 / g.shots.max(1) as f64,
            g.xg / g.shots.max(1) as f64,
            g.goals as f64 / g.xg.max(1e-9)
        );
    };
    let footed = feet[0].shots + feet[1].shots;
    println!("  passes with the weaker foot: {:.1} %", 100.0 * passes[1] as f64 / passes[0].max(1) as f64);
    show("strong foot", &feet[0], footed);
    show("weak foot", &feet[1], footed);
    show("header", &feet[2], n_shots);
    show("central (|y| < 7 m)", &lateral[0], footed);
    show("wide", &lateral[1], footed);
    show("inside the box", &boxes[0], n_shots);
    show("outside the box", &boxes[1], n_shots);
    println!("defending team phase at shot: {phases:?}");
    let pct = |a: u32, b: u32| 100.0 * a as f64 / (a + b).max(1) as f64;
    println!(
        "shots from 16.5 m+: keeper > 3 m above his GK-01 depth in {:.0} % of those < 5 s after winning the ball (n {}), {:.0} % of the rest (n {})",
        pct(long_gk[1][1], long_gk[1][0]),
        long_gk[1][0] + long_gk[1][1],
        pct(long_gk[0][1], long_gk[0][0]),
        long_gk[0][0] + long_gk[0][1]
    );
    let tri: Vec<String> = carry_tri.iter().take(6).map(|(n, t)| format!("{:.2}", *t as f64 / (*n).max(1) as f64)).collect();
    println!("opponents in the shooting triangle of a carrier in range, per band (every 0.2 s): {}", tri.join(" / "));
}

/// Outfield opponents of `side` in the triangle from team-frame `p` to the posts.
fn in_triangle(st: &match_engine::match_state::MatchState, side: match_engine::pitch::Side, p: glam::Vec2) -> usize {
    let fr = st.frame(side);
    let (pl, pr) = (glam::Vec2::new(HALF_LENGTH, 3.66), glam::Vec2::new(HALF_LENGTH, -3.66));
    st.squad(side.opponent())
        .iter()
        .filter(|q| !q.is_gk())
        .filter(|q| {
            let qt = fr.to_team(q.pos);
            let (d1, d2, d3) = ((qt - p).perp_dot(pl - p), (qt - pl).perp_dot(pr - pl), (qt - pr).perp_dot(p - pr));
            (d1 < 0.0) == (d2 < 0.0) && (d2 < 0.0) == (d3 < 0.0)
        })
        .count()
}
