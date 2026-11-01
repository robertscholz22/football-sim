//! Shot selection as a conditional rate: of the open-play passes and shots played from each distance band to the goal
//! centre, the share that are shots (StatsBomb, 40 matches, `sb_shot_share.py`: 0–11 m 86 %, 11–16.5 m 46 %,
//! 16.5–22 m 24 %, 22–30 m 7.9 %, 30–40 m 0.2 %; inside the box 44 %, outside within 40 m 4.4 %; per team within a few
//! points, so it holds across styles). Restarts (the first kick after a restart) are left out. Also split by
//! pressure (an opponent within 3 m of the kicker), and the first-time share of the shots.
//! Env `ATTRS=68 AWAY=47`: uniform attributes per side (as `layers`); `EGO` / `FLAIR`: that egotism / flair for
//! every player; `TRACE=n`: print the first n decisions at 22–30 m whose layer-1 best is a shot.
//! `cargo run -p match_engine --release --example shot_share -- [matches]`

use match_engine::Simulation;
use match_engine::events::EventKind;
use match_engine::match_state::side_of;
use match_engine::pitch::HALF_LENGTH;
use match_engine::player::Attributes;

const EDGES: [f32; 5] = [11.0, 16.5, 22.0, 30.0, 40.0];
const REAL: [f32; 5] = [85.6, 45.7, 23.9, 7.9, 0.2];

/// Open-play passes from 16.5–22 / 22–30 m, [into the box, elsewhere]: [passes, completed] (StatsBomb 40 matches,
/// `sb_box_entry.py`: into the box 61 / 30 % of them, completed 38 / 42 %; elsewhere completed 82 / 83 %), with the
/// AI's predicted completion of the chosen pass: [passes, completed, summed predicted p].
type Entry = [[[f32; 3]; 2]; 5];
/// Support around the kicker per band: nearest outfield team-mate < 5 / 5–10 / 10–15 / 15+ m, then team-mates
/// within 15 m 0 / 1 / 2 / 3+; [passes, shots].
type Support = [[[u32; 2]; 8]; 5];
/// One match: tally, shots [inside, outside the box], decisions, passes near the box, support, and per player at
/// 16.5–30 m (egotism, actions, shots).
type MatchResult = (Tally, [u32; 2], [Diag; 5], Entry, Support, Vec<(f32, u32, u32)>);

/// Per band and pressure (free, pressed): [passes, shots, first-time shots].
type Tally = [[[u32; 3]; 2]; 5];

/// On-ball decisions per band (all of them, re-decisions included): [decisions, a shot chosen, the layer-1 best is a
/// shot, the best after layers 2 and 3 is a shot, of the chosen shots the layer-1 best, of the chosen shots the best
/// after layers 2 and 3]; summed shot probability; summed (best shot − best other option) in layer-1 utility, in
/// layer 2 + 3 bias, over the chosen shots, as fractions of the local value scale.
#[derive(Clone, Copy, Default)]
struct Diag {
    n: [u32; 6],
    /// Chosen shots that were first-time choices (ATT-06 reception decisions).
    first: u32,
    prob: f32,
    gap_l1: f32,
    gap_bias: f32,
}

static TRACED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// `TRACE=n`: print the first n decisions at 22–30 m whose layer-1 best is a shot.
fn trace_n() -> usize {
    std::env::var("TRACE").ok().and_then(|s| s.parse().ok()).unwrap_or(0)
}

fn kind(o: &match_engine::ai::on_ball::OptionEval) -> usize {
    use match_engine::ai::personality::OptionKind as K;
    match o.kind {
        K::Pass { .. } => 0,
        K::Shot { .. } => 1,
        K::Dribble { .. } => 2,
        _ => 3,
    }
}

fn diagnose(sim: &Simulation, log: &[match_engine::sim::LoggedDecision], diag: &mut [Diag; 5], p_pred: &mut [f32; 22]) {
    use match_engine::ai::analysis::possession_value;
    for (_, sit, opts, chosen) in log {
        if let Some(o) = opts.get(*chosen).filter(|o| kind(o) == 0) {
            p_pred[sit.carrier] = o.p;
        }
        let st = &sim.state;
        let fr = st.frame(st.players[sit.carrier].side);
        let p = fr.to_team(sit.ball.truncate());
        let d = (glam::Vec2::new(HALF_LENGTH, 0.0) - p).length();
        let Some(b) = EDGES.iter().position(|&x| d < x) else { continue };
        if opts.is_empty() || sit.restart {
            continue;
        }
        let arg = |f: &dyn Fn(&match_engine::ai::on_ball::OptionEval) -> f32| {
            (0..opts.len()).max_by(|&a, &c| f(&opts[a]).total_cmp(&f(&opts[c]))).unwrap()
        };
        let l1 = kind(&opts[arg(&|o| o.u_opt)]) == 1;
        let tot = kind(&opts[arg(&|o| o.utility())]) == 1;
        let shot = kind(&opts[*chosen]) == 1;
        let g = &mut diag[b];
        g.n[0] += 1;
        g.n[1] += shot as u32;
        g.n[2] += l1 as u32;
        g.n[3] += tot as u32;
        g.prob += opts.iter().filter(|o| kind(o) == 1).map(|o| o.prob).sum::<f32>();
        if l1 && b == 3 && TRACED.fetch_add(1, std::sync::atomic::Ordering::Relaxed) < trace_n() {
            let near = st.squad(st.players[sit.carrier].side.opponent()).iter().map(|o| (o.pos - sit.ball.truncate()).length()).fold(f32::MAX, f32::min);
            let mut out = format!(
                "t {:.1} carrier {} at ({:.1}, {:.1}) d {d:.1} m, nearest opp {near:.1} m, first time {}, V here {:.4}, scale {:.4}\n",
                st.time,
                sit.carrier,
                p.x,
                p.y,
                sit.first_time,
                possession_value(p, &sim.cfg.ai),
                possession_value(p, &sim.cfg.ai) + sim.cfg.ai.value_base
            );
            let mut idx: Vec<usize> = (0..opts.len()).collect();
            idx.sort_by(|&a, &c| opts[c].u_opt.total_cmp(&opts[a].u_opt));
            let npass = opts.iter().filter(|o| kind(o) == 0).count();
            out += &format!("   ({} options, {npass} passes; top 5, then the 3 best passes)\n", opts.len());
            let passes: Vec<usize> = idx.iter().copied().filter(|&i| kind(&opts[i]) == 0).take(3).collect();
            for &i in idx.iter().take(5).chain(passes.iter()) {
                let o = &opts[i];
                out += &format!(
                    "   {}{:<44} p {:.2} pressed {:.2}  u {:.4} instr {:+.4} pers {:+.4} prob {:.3}\n",
                    if i == *chosen { "*" } else { " " },
                    o.label,
                    o.p,
                    o.pressed,
                    o.u_opt,
                    o.instr,
                    o.pers,
                    o.prob
                );
            }
            eprint!("{out}");
        }
        if shot {
            g.first += sit.first_time as u32;
            g.n[4] += l1 as u32;
            g.n[5] += tot as u32;
            let scale = possession_value(p, &sim.cfg.ai) + sim.cfg.ai.value_base;
            let c = &opts[*chosen];
            let other = &opts[(0..opts.len()).filter(|&i| kind(&opts[i]) != 1).max_by(|&a, &c| opts[a].u_opt.total_cmp(&opts[c].u_opt)).unwrap_or(*chosen)];
            g.gap_l1 += (c.u_opt - other.u_opt) / scale;
            g.gap_bias += (c.instr + c.pers - other.instr - other.pers) / scale;
        }
    }
}

fn uniform(sim: &mut Simulation) {
    let get = |k: &str| std::env::var(k).ok().and_then(|s| s.parse::<f32>().ok());
    // `EGO` / `FLAIR`: that egotism / flair for every player, the squads otherwise as they are.
    for p in sim.state.players.iter_mut() {
        if let Some(e) = get("EGO") {
            p.attrs.egotism = e;
        }
        if let Some(f) = get("FLAIR") {
            p.attrs.flair = f;
        }
    }
    for (side, v) in [(0usize, get("ATTRS")), (1, get("AWAY").or(get("ATTRS")))] {
        let Some(v) = v else { continue };
        for p in sim.state.players.iter_mut().filter(|p| p.side.index() == side) {
            let a = &p.attrs;
            p.attrs = Attributes { preferred_foot: a.preferred_foot, height_cm: a.height_cm, weight_kg: a.weight_kg, ..Attributes::uniform(v) };
            p.confidence = 1.0;
            p.energy = 1.0;
            p.familiarity = [1.0; 2];
        }
    }
}

fn main() {
    let matches: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(12);
    let res: Vec<MatchResult> = std::thread::scope(|scope| {
        let handles: Vec<_> = (1..=matches)
            .map(|seed| {
                scope.spawn(move || {
                    let mut sim = Simulation::from_builtin("northbridge", "harbour", seed);
                    uniform(&mut sim);
                    sim.decision_log = Some(Vec::new());
                    let mut diag = [Diag::default(); 5];
                    let mut t: Tally = Default::default();
                    // Open-play shots [inside, outside the box], any distance.
                    let mut boxes = [0u32; 2];
                    let mut entry: Entry = Default::default();
                    let mut sup: Support = Default::default();
                    // Per player at 16.5–30 m: (egotism, actions, shots).
                    let mut per: Vec<(f32, u32, u32)> = sim.state.players.iter().map(|p| (p.attrs.egotism, 0, 0)).collect();
                    // Pass awaiting its outcome: (kicker, band, into the box).
                    let mut pending: Option<(usize, usize, bool, f32)> = None;
                    let mut p_pred = [0.0f32; 22];
                    let mut seen = 0;
                    let mut restart = true;
                    while !sim.finished() {
                        sim.step();
                        let log = sim.decision_log.take().unwrap_or_default();
                        diagnose(&sim, &log, &mut diag, &mut p_pred);
                        sim.decision_log = Some(Vec::new());
                        if let Some((k, b, inbox, pp)) = pending {
                            let st = &sim.state;
                            let done = match st.control {
                                Some(c) if c != k || st.flight.is_none() => Some(side_of(c) == side_of(k)),
                                _ if !st.is_live() => Some(false),
                                _ => None,
                            };
                            if let Some(ok) = done {
                                let e = &mut entry[b][!inbox as usize];
                                e[0] += 1.0;
                                e[1] += ok as u32 as f32;
                                e[2] += pp;
                                pending = None;
                            }
                        }
                        while seen < sim.events.len() {
                            let e = sim.events[seen].clone();
                            seen += 1;
                            let (by, shot, first) = match e.kind {
                                EventKind::Restart { .. } | EventKind::Kickoff { .. } => {
                                    restart = true;
                                    continue;
                                }
                                EventKind::Pass { by, first_time, .. } => (by, false, first_time),
                                EventKind::Shot { by, first_time, .. } => (by, true, first_time),
                                _ => continue,
                            };
                            if std::mem::take(&mut restart) {
                                continue;
                            }
                            let st = &sim.state;
                            let side = side_of(by);
                            let fr = st.frame(side);
                            let p = fr.to_team(st.players[by].pos);
                            if shot {
                                boxes[!(p.x > HALF_LENGTH - 16.5 && p.y.abs() < 20.16) as usize] += 1;
                            }
                            let d = (glam::Vec2::new(HALF_LENGTH, 0.0) - p).length();
                            let Some(b) = EDGES.iter().position(|&x| d < x) else { continue };
                            if !shot && pending.is_none()
                                && let Some(f) = st.flight
                            {
                                let tt = fr.to_team(f.target);
                                pending = Some((by, b, tt.x > HALF_LENGTH - 16.5 && tt.y.abs() < 20.16, p_pred[by]));
                            }
                            let near = st.squad(side.opponent()).iter().map(|o| (o.pos - st.players[by].pos).length()).fold(f32::MAX, f32::min);
                            let c = &mut t[b][(near < 3.0) as usize];
                            c[shot as usize] += 1;
                            c[2] += (shot && first) as u32;
                            let mates: Vec<f32> = st
                                .squad(side)
                                .iter()
                                .filter(|m| m.id != by && !m.is_gk())
                                .map(|m| (m.pos - st.players[by].pos).length())
                                .collect();
                            let nearest = mates.iter().copied().fold(f32::MAX, f32::min);
                            let nb = [5.0, 10.0, 15.0, f32::MAX].iter().position(|&e| nearest < e).unwrap_or(3);
                            let k = mates.iter().filter(|&&d| d < 15.0).count().min(3);
                            sup[b][nb][shot as usize] += 1;
                            if b == 2 || b == 3 {
                                per[by].1 += 1;
                                per[by].2 += shot as u32;
                            }
                            sup[b][4 + k][shot as usize] += 1;
                        }
                    }
                    (t, boxes, diag, entry, sup, per)
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut t: Tally = Default::default();
    let mut boxes = [0u32; 2];
    let mut diag = [Diag::default(); 5];
    let mut entry: Entry = Default::default();
    let mut sup: Support = Default::default();
    for (_, _, _, _, s, _) in &res {
        for b in 0..5 {
            for i in 0..8 {
                for k in 0..2 {
                    sup[b][i][k] += s[b][i][k];
                }
            }
        }
    }
    for (_, _, _, e, _, _) in &res {
        for b in 0..5 {
            for i in 0..2 {
                for k in 0..3 {
                    entry[b][i][k] += e[b][i][k];
                }
            }
        }
    }
    for (_, _, d, _, _, _) in &res {
        for (g, x) in diag.iter_mut().zip(d) {
            for k in 0..6 {
                g.n[k] += x.n[k];
            }
            g.prob += x.prob;
            g.first += x.first;
            g.gap_l1 += x.gap_l1;
            g.gap_bias += x.gap_bias;
        }
    }
    for (m, b, _, _, _, _) in &res {
        for (i, band) in m.iter().enumerate() {
            for (j, c) in band.iter().enumerate() {
                for k in 0..3 {
                    t[i][j][k] += c[k];
                }
            }
        }
        boxes[0] += b[0];
        boxes[1] += b[1];
    }
    let n = matches as f32;
    let shots = (boxes[0] + boxes[1]) as f32;
    println!("{matches} matches: open-play shots {:.1} per match, inside the box {:.1} % (real ≈ 62 %)", shots / n, 100.0 * boxes[0] as f32 / shots.max(1.0));
    println!("band (m)    free: passes shots share   pressed (< 3 m): passes shots share   all share  real   shots/match  first time");
    let names = ["0-11", "11-16.5", "16.5-22", "22-30", "30-40"];
    let pct = |c: &[u32; 3]| 100.0 * c[1] as f32 / (c[0] + c[1]).max(1) as f32;
    for (b, nm) in names.iter().enumerate() {
        let (f, p) = (t[b][0], t[b][1]);
        let a = [f[0] + p[0], f[1] + p[1], f[2] + p[2]];
        println!(
            "{nm:<9} {:7.1} {:5} {:5.1}%   {:7.1} {:5} {:5.1}%            {:5.1}%  {:5.1}%  {:6.2}      {:4.0}%",
            f[0] as f32 / n,
            f[1],
            pct(&f),
            p[0] as f32 / n,
            p[1],
            pct(&p),
            pct(&a),
            REAL[b],
            a[1] as f32 / n,
            100.0 * a[2] as f32 / a[1].max(1) as f32
        );
    }
    for (b, nm) in names.iter().enumerate() {
        let [i, o] = entry[b];
        println!(
            "passes from {nm} m: {:.1} per match, into the box {:.1} % completed {:.1} % (AI {:.1} %), elsewhere completed {:.1} % (AI {:.1} %)",
            (i[0] + o[0]) / n,
            100.0 * i[0] / (i[0] + o[0]).max(1.0),
            100.0 * i[1] / i[0].max(1.0),
            100.0 * i[2] / i[0].max(1.0),
            100.0 * o[1] / o[0].max(1.0),
            100.0 * o[2] / o[0].max(1.0)
        );
    }
    // StatsBomb (`sb_support.py`, 360 frames for passes, freeze frames for shots): share of actions by bin, shot share.
    let real: [[f32; 8]; 2] = [[17.7, 60.0, 17.4, 4.9, 4.9, 16.6, 24.3, 54.2], [7.3, 10.0, 7.4, 2.8, 2.8, 3.2, 7.9, 11.3]];
    for (b, nm) in names.iter().enumerate().skip(2) {
        let tot: u32 = (0..4).map(|i| sup[b][i][0] + sup[b][i][1]).sum();
        let row = |r: std::ops::Range<usize>| {
            r.map(|i| {
                let a = sup[b][i][0] + sup[b][i][1];
                format!("{:4.1}/{:4.1}", 100.0 * a as f32 / tot.max(1) as f32, 100.0 * sup[b][i][1] as f32 / a.max(1) as f32)
            })
            .collect::<Vec<_>>()
            .join(" ")
        };
        println!("support {nm:<8} nearest mate <5 / 5-10 / 10-15 / 15+ m (% of actions / shot share): {}", row(0..4));
        println!("                 mates within 15 m 0 / 1 / 2 / 3+:                         {}", row(4..8));
    }
    let r = |r: std::ops::Range<usize>| r.map(|i| format!("{:4.1}/{:4.1}", real[0][i], real[1][i])).collect::<Vec<_>>().join(" ");
    println!("real 22-30      nearest mate (% of actions / shot share):                       {}", r(0..4));
    println!("                mates within 15 m:                                              {}", r(4..8));
    // Per player over all matches (same squads, so a player's tallies add up across seeds): the spread of the shot
    // share at 16.5–30 m against StatsBomb (`sb_player_shot_share.py`, players with ≥ 25 actions there: p10 2 %,
    // p50 14 %, p90 19 %, max 24 %), and by egotism.
    let mut per: Vec<(f32, u32, u32)> = res[0].5.iter().map(|&(e, _, _)| (e, 0, 0)).collect();
    for r in &res {
        for (q, x) in per.iter_mut().zip(&r.5) {
            q.1 += x.1;
            q.2 += x.2;
        }
    }
    let mut rows: Vec<(f32, f32, u32)> = per.iter().filter(|q| q.1 >= 25).map(|q| (q.2 as f32 / q.1 as f32, q.0, q.1)).collect();
    rows.sort_by(|a, b| a.0.total_cmp(&b.0));
    if !rows.is_empty() {
        let q = |f: f32| 100.0 * rows[((f * rows.len() as f32) as usize).min(rows.len() - 1)].0;
        println!(
            "per player at 16.5-30 m ({} with >= 25 actions): shot share p10 {:.1} p50 {:.1} p90 {:.1} max {:.1} % (real 2 / 14 / 19 / 24 %); by egotism: {}",
            rows.len(),
            q(0.1),
            q(0.5),
            q(0.9),
            100.0 * rows[rows.len() - 1].0,
            rows.iter().rev().map(|r| format!("{:.0}:{:.0}%", r.1, 100.0 * r.0)).collect::<Vec<_>>().join(" ")
        );
    }
    println!("decisions (re-decisions included): shot chosen / layer-1 best / best after layers 2+3 / mean shot prob; of the chosen shots: layer-1 best, best after 2+3, mean gap to the best other option in layer 1 and layers 2+3 (× local scale)");
    for (b, nm) in names.iter().enumerate() {
        let g = &diag[b];
        let n = g.n[0].max(1) as f32;
        let s = g.n[1].max(1) as f32;
        println!(
            "{nm:<9} n {:6}  {:5.1}% / {:5.1}% / {:5.1}% / {:5.3}   shots {:5} ({:3} first time): {:5.1}% / {:5.1}%   gap {:+.2} / {:+.2}",
            g.n[0],
            100.0 * g.n[1] as f32 / n,
            100.0 * g.n[2] as f32 / n,
            100.0 * g.n[3] as f32 / n,
            g.prob / n,
            g.n[1],
            g.first,
            100.0 * g.n[4] as f32 / s,
            100.0 * g.n[5] as f32 / s,
            g.gap_l1 / s,
            g.gap_bias / s
        );
    }
}
