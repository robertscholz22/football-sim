//! Shape around the passer in and near the final third: at every open-play pass from 16.5–22 / 22–30 / 30–40 m to
//! the goal centre, the outfield team-mates within 15 m ahead of the ball (nearer the goal line) and level or behind,
//! the team-mates in the box, the most advanced team-mate's and the deepest outfield opponent's distance to the goal
//! line, the opponents within 15 m and in the box, and where the team-mates within 25 m stand (dx toward goal × |dy|).
//! Real (StatsBomb 360, visible players, `sb_ft_shape.py`), 22–30 m: ahead / behind 1.40 / 1.20, in the box 1.70,
//! most advanced 13.4 m, opponents within 15 m 5.10, in the box 3.30, deepest opponent 12.1 m. Restarts (the first
//! kick after a restart) are left out. Also by role: where each role's team-mates stand relative to the ball and to
//! their anchor.
//! Env `ATTRS=68 AWAY=47`: uniform attributes per side (as `layers`).
//! `cargo run -p match_engine --release --example final_third -- [matches]`

use glam::Vec2;
use match_engine::Simulation;
use match_engine::ai::shape::anchor;
use match_engine::events::EventKind;
use match_engine::match_state::side_of;
use match_engine::pitch::HALF_LENGTH;
use match_engine::player::Attributes;
use match_engine::team::Role;

const EDGES: [f32; 5] = [11.0, 16.5, 22.0, 30.0, 40.0];
const DX: [f32; 7] = [-25.0, -10.0, -5.0, 0.0, 5.0, 10.0, 25.0];
const DY: [f32; 4] = [0.0, 5.0, 10.0, 25.0];
/// Role groups: centre-backs, wide defenders, single pivots, central midfielders (CM8, CMP), attacking midfielders,
/// wide players (W, WM), strikers (CF, ST2).
const ROLES: [&str; 7] = ["CB", "FB", "DM", "CM", "AM", "W", "ST"];

fn group(r: Role) -> Option<usize> {
    match r {
        Role::CB | Role::CBC | Role::CBW => Some(0),
        Role::FB | Role::WB => Some(1),
        Role::DM => Some(2),
        Role::CM8 | Role::CMP => Some(3),
        Role::AM => Some(4),
        Role::W | Role::WM => Some(5),
        Role::CF | Role::ST2 => Some(6),
        Role::GK => None,
    }
}

/// Per band: passes, ahead15, behind15, box, front, opp15, oppbox, oppline, mean of the 4 deepest opponents, mean of
/// the 2 most advanced team-mates.
type Acc = [[f64; 10]; 5];
/// Per band, dx bin, |dy| bin: team-mates within 25 m.
type Grid = [[[f64; 3]; 6]; 5];
/// Per role at 22–30 m: count, dx to the ball, within 15 m, distance to the goal line, anchor dx to the ball,
/// distance to the anchor, target x − anchor x, position x − target x.
type PerRole = [[f64; 8]; 7];
/// Defending back line (CB, FB) at 22–30 m: count, distance to the goal line, the block line's, target's, then (per
/// pass) the attacking block's front, the brain's offside line, the offside line now; and the defending players'
/// label and the defending team's phase counts.
type Def = ([f64; 8], Vec<(String, u32)>, Vec<(String, u32)>);

/// Line depth against the ball (as `sb_line_depth.py`): per central / wide (|y| < 20 m) × 4 m bin of the ball's
/// distance to the goal line from 10 m × free / pressed (an opponent within 3 m): passes, deepest and second-deepest
/// outfield opponent, ball distance, most advanced team-mate, team-mates in the box.
type Depth = [[[f64; 6]; 2]; 20];
/// The most advanced team-mate for passes with the ball 22–46 m from the goal line: per (role, label): count, his
/// anchor x − offside line, target x − anchor x, position x − target x, deepest outfield opponent x − offside line.
type Front = Vec<(String, [f64; 5])>;
/// One match's tallies.
type MatchResult = (Acc, Grid, PerRole, Vec<(String, u32)>, Def, Vec<(String, f64, f64, f64)>, Depth, Front);

fn bump(v: &mut Vec<(String, u32)>, k: String) {
    match v.iter_mut().find(|(l, _)| *l == k) {
        Some(e) => e.1 += 1,
        None => v.push((k, 1)),
    }
}

fn show(name: &str, mut v: Vec<(String, u32)>) {
    v.sort_by_key(|x| std::cmp::Reverse(x.1));
    let tot: u32 = v.iter().map(|x| x.1).sum();
    println!(
        "{name}: {}",
        v.iter().take(12).map(|(l, n)| format!("{l} {:.0}%", 100.0 * *n as f32 / tot.max(1) as f32)).collect::<Vec<_>>().join(", ")
    );
}

fn inbox(p: Vec2) -> bool {
    p.x > HALF_LENGTH - 16.5 && p.y.abs() < 20.16
}

fn uniform(sim: &mut Simulation) {
    let get = |k: &str| std::env::var(k).ok().and_then(|s| s.parse::<f32>().ok());
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
                    // `CFG=file.ron`: `SimConfig` overrides, as `match_lab --config`.
                    if let Ok(path) = std::env::var("CFG") {
                        sim.cfg = match_engine::config::SimConfig::from_ron(&std::fs::read_to_string(path).unwrap()).unwrap();
                    }
                    uniform(&mut sim);
                    let mut acc: Acc = Default::default();
                    let mut grid: Grid = Default::default();
                    let mut roles: PerRole = Default::default();
                    // What the strikers are doing at 22–30 m (positioning label).
                    let mut labels: Vec<(String, u32)> = Vec::new();
                    let mut def: Def = Default::default();
                    let mut def_lag: Vec<(String, f64, f64, f64)> = Vec::new();
                    let mut depth: Depth = [[[0.0; 6]; 2]; 20];
                    let mut fronts: Front = Vec::new();
                    let mut seen = 0;
                    let mut restart = true;
                    while !sim.finished() {
                        sim.step();
                        while seen < sim.events.len() {
                            let e = sim.events[seen].clone();
                            seen += 1;
                            let by = match e.kind {
                                EventKind::Restart { .. } | EventKind::Kickoff { .. } => {
                                    restart = true;
                                    continue;
                                }
                                EventKind::Pass { by, .. } => by,
                                EventKind::Shot { .. } => {
                                    restart = false;
                                    continue;
                                }
                                _ => continue,
                            };
                            if std::mem::take(&mut restart) {
                                continue;
                            }
                            let st = &sim.state;
                            let side = side_of(by);
                            let fr = st.frame(side);
                            let ball = fr.to_team(st.players[by].pos);
                            let d = (Vec2::new(HALF_LENGTH, 0.0) - ball).length();
                            let bx = HALF_LENGTH - ball.x;
                            if (10.0..50.0).contains(&bx) {
                                let opp: Vec<f32> = {
                                    let mut v: Vec<f32> = st.squad(side.opponent()).iter().filter(|o| !o.is_gk()).map(|o| HALF_LENGTH - fr.to_team(o.pos).x).collect();
                                    v.sort_by(|a, b| a.total_cmp(b));
                                    v
                                };
                                let pressed = st.squad(side.opponent()).iter().any(|o| (fr.to_team(o.pos) - ball).length() < 3.0);
                                let mates: Vec<Vec2> = st.squad(side).iter().filter(|m| m.id != by && !m.is_gk()).map(|m| fr.to_team(m.pos)).collect();
                                let e = &mut depth[(ball.y.abs() >= 20.0) as usize * 10 + ((bx - 10.0) / 4.0) as usize][pressed as usize];
                                e[0] += 1.0;
                                e[1] += opp[0] as f64;
                                e[2] += opp[1] as f64;
                                e[3] += bx as f64;
                                e[4] += mates.iter().map(|q| HALF_LENGTH - q.x).fold(f32::MAX, f32::min) as f64;
                                e[5] += mates.iter().filter(|q| inbox(**q)).count() as f64;
                                if (22.0..46.0).contains(&bx)
                                    && let Some(m) = st.squad(side).iter().filter(|m| m.id != by && !m.is_gk()).max_by(|a, b| fr.to_team(a.pos).x.total_cmp(&fr.to_team(b.pos).x))
                                {
                                    let off = st.team_view(side).offside_line;
                                    let (anc, _) = anchor(st, &sim.cfg, m.id);
                                    let tx = fr.to_team(m.target).x;
                                    let k = format!("{:?} {}", st.team(side).formation.slots[m.slot].role, m.debug.label);
                                    let v = [1.0, (anc.x - off) as f64, (tx - anc.x) as f64, (fr.to_team(m.pos).x - tx) as f64, (HALF_LENGTH - opp[0] - off) as f64];
                                    match fronts.iter_mut().find(|(l, _)| *l == k) {
                                        Some(e) => (0..5).for_each(|i| e.1[i] += v[i]),
                                        None => fronts.push((k, v)),
                                    }
                                }
                            }
                            let Some(b) = EDGES.iter().position(|&x| d < x) else { continue };
                            if b < 2 {
                                continue;
                            }
                            let a = &mut acc[b];
                            a[0] += 1.0;
                            let team = st.team(side);
                            let mut front = f32::MIN;
                            let mut xs: Vec<f32> = Vec::new();
                            for m in st.squad(side).iter().filter(|m| m.id != by && !m.is_gk()) {
                                let q = fr.to_team(m.pos);
                                let (dx, dy, dist) = (q.x - ball.x, (q.y - ball.y).abs(), (q - ball).length());
                                if dist < 15.0 {
                                    a[if dx > 0.0 { 1 } else { 2 }] += 1.0;
                                }
                                a[3] += inbox(q) as u32 as f64;
                                front = front.max(q.x);
                                xs.push(q.x);
                                if dist < 25.0
                                    && let Some(i) = (0..DX.len() - 1).find(|&k| dx >= DX[k] && dx < DX[k + 1])
                                    && let Some(j) = (0..DY.len() - 1).find(|&k| dy >= DY[k] && dy < DY[k + 1])
                                {
                                    grid[b][i][j] += 1.0;
                                }
                                if b == 3
                                    && let Some(r) = group(team.formation.slots[m.slot].role)
                                {
                                    let (anc, _) = anchor(st, &sim.cfg, m.id);
                                    let rr = &mut roles[r];
                                    rr[0] += 1.0;
                                    rr[1] += dx as f64;
                                    rr[2] += (dist < 15.0) as u32 as f64;
                                    rr[3] += (HALF_LENGTH - q.x) as f64;
                                    rr[4] += (anc.x - ball.x) as f64;
                                    rr[5] += (anc - q).length() as f64;
                                    let tg = fr.to_team(m.target);
                                    rr[6] += (tg.x - anc.x) as f64;
                                    rr[7] += (q.x - tg.x) as f64;
                                    if r == 6 {
                                        match labels.iter_mut().find(|(l, _)| *l == m.debug.label) {
                                            Some(e) => e.1 += 1,
                                            None => labels.push((m.debug.label.clone(), 1)),
                                        }
                                    }
                                }
                            }
                            a[4] += (HALF_LENGTH - front) as f64;
                            xs.sort_by(|a, b| b.total_cmp(a));
                            a[9] += xs.iter().take(2).map(|&x| (HALF_LENGTH - x) as f64).sum::<f64>() / 2.0;
                            let mut line = f32::MIN;
                            let mut os: Vec<f32> = Vec::new();
                            let dteam = st.team(side.opponent());
                            if b == 3 {
                                bump(&mut def.2, format!("{:?}", st.team_view(side.opponent()).phase));
                                def.0[4] += 1.0;
                                def.0[5] += (HALF_LENGTH - st.team_view(side).att_block.front_x) as f64;
                                def.0[6] += (HALF_LENGTH - st.team_view(side).offside_line) as f64;
                                def.0[7] += (HALF_LENGTH - match_engine::ai::analysis::offside_line(st, side)) as f64;
                                for o in st.squad(side.opponent()).iter().filter(|o| dteam.formation.slots[o.slot].role(false).is_back_line()) {
                                    let q = fr.to_team(o.pos);
                                    def.0[0] += 1.0;
                                    def.0[1] += (HALF_LENGTH - q.x) as f64;
                                    // The defending team's own frame: its line_x is measured from its own goal.
                                    def.0[2] += (HALF_LENGTH + st.team_view(side.opponent()).def_block.line_x) as f64;
                                    def.0[3] += (HALF_LENGTH - fr.to_team(o.target).x) as f64;
                                    bump(&mut def.1, o.debug.label.clone());
                                    // How far he trails his target towards his own goal, by what he is doing.
                                    let lag = (fr.to_team(o.target).x - q.x) as f64;
                                    // How far above the block line his target is (towards the ball).
                                    let above = (HALF_LENGTH - fr.to_team(o.target).x - (HALF_LENGTH + st.team_view(side.opponent()).def_block.line_x)) as f64;
                                    match def_lag.iter_mut().find(|(l, _, _, _)| *l == o.debug.label) {
                                        Some(e) => {
                                            e.1 += 1.0;
                                            e.2 += lag;
                                            e.3 += above;
                                        }
                                        None => def_lag.push((o.debug.label.clone(), 1.0, lag, above)),
                                    }
                                }
                            }
                            for o in st.squad(side.opponent()).iter().filter(|o| !o.is_gk()) {
                                let q = fr.to_team(o.pos);
                                a[5] += ((q - ball).length() < 15.0) as u32 as f64;
                                a[6] += inbox(q) as u32 as f64;
                                line = line.max(q.x);
                                os.push(q.x);
                            }
                            a[7] += (HALF_LENGTH - line) as f64;
                            os.sort_by(|a, b| b.total_cmp(a));
                            a[8] += os.iter().take(4).map(|&x| (HALF_LENGTH - x) as f64).sum::<f64>() / 4.0;
                        }
                    }
                    (acc, grid, roles, labels, def, def_lag, depth, fronts)
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut acc: Acc = Default::default();
    let mut grid: Grid = Default::default();
    let mut roles: PerRole = Default::default();
    let mut labels: Vec<(String, u32)> = Vec::new();
    let mut def: Def = Default::default();
    let mut def_lag: Vec<(String, f64, f64, f64)> = Vec::new();
    let mut depth: Depth = [[[0.0; 6]; 2]; 20];
    let mut fronts: Front = Vec::new();
    for (a, g, r, l, d, dl, dp, f) in &res {
        for (k, v) in f {
            match fronts.iter_mut().find(|(l, _)| l == k) {
                Some(e) => (0..5).for_each(|i| e.1[i] += v[i]),
                None => fronts.push((k.clone(), *v)),
            }
        }
        for i in 0..20 {
            for j in 0..2 {
                for k in 0..6 {
                    depth[i][j][k] += dp[i][j][k];
                }
            }
        }
        for (k, n, v, w) in dl {
            match def_lag.iter_mut().find(|(x, _, _, _)| x == k) {
                Some(e) => {
                    e.1 += n;
                    e.2 += v;
                    e.3 += w;
                }
                None => def_lag.push((k.clone(), *n, *v, *w)),
            }
        }
        for k in 0..8 {
            def.0[k] += d.0[k];
        }
        for (k, v) in &d.1 {
            for _ in 0..*v {
                bump(&mut def.1, k.clone());
            }
        }
        for (k, v) in &d.2 {
            for _ in 0..*v {
                bump(&mut def.2, k.clone());
            }
        }
        for (k, v) in l {
            match labels.iter_mut().find(|(x, _)| x == k) {
                Some(e) => e.1 += v,
                None => labels.push((k.clone(), *v)),
            }
        }
        for b in 0..5 {
            for k in 0..10 {
                acc[b][k] += a[b][k];
            }
            for i in 0..6 {
                for j in 0..3 {
                    grid[b][i][j] += g[b][i][j];
                }
            }
        }
        for i in 0..7 {
            for k in 0..8 {
                roles[i][k] += r[i][k];
            }
        }
    }
    println!("{matches} matches; open-play passes by distance to the goal centre");
    for (b, nm) in [(2, "16.5-22"), (3, "22-30"), (4, "30-40")] {
        let a = &acc[b];
        let n = a[0].max(1.0);
        println!(
            "{nm:<8} n {:6}  mates within 15 m ahead / behind {:4.2} / {:4.2}  in the box {:4.2}  most advanced mate {:4.1} m from the goal line  opponents within 15 m {:4.2}  in the box {:4.2}  deepest opponent {:4.1} m  4 deepest opponents {:4.1} m  2 most advanced mates {:4.1} m",
            a[0], a[1] / n, a[2] / n, a[3] / n, a[4] / n, a[5] / n, a[6] / n, a[7] / n, a[8] / n, a[9] / n
        );
        println!("         mates within 25 m per pass, dx toward goal (rows) x |dy| 0-5 / 5-10 / 10-25 m:");
        for i in (0..6).rev() {
            println!(
                "           dx {:+4} … {:+4}: {}",
                DX[i],
                DX[i + 1],
                (0..3).map(|j| format!("{:5.2}", grid[b][i][j] / n)).collect::<Vec<_>>().join(" ")
            );
        }
    }
    println!("by role at 22-30 m (team-mates of the passer): dx to the ball, share within 15 m, distance to the goal line, anchor dx to the ball, distance to the anchor, target x − anchor x, position x − target x");
    for (i, r) in ROLES.iter().enumerate() {
        let x = &roles[i];
        let n = x[0].max(1.0);
        println!(
            "  {:<3} n {:6}  dx {:+5.1}  within 15 m {:4.0}%  to the goal line {:4.1}  anchor dx {:+5.1}  off the anchor {:4.1} m  target − anchor {:+5.1}  pos − target {:+5.1}",
            format!("{r:?}"),
            x[0],
            x[1] / n,
            100.0 * x[2] / n,
            x[3] / n,
            x[4] / n,
            x[5] / n,
            x[6] / n,
            x[7] / n
        );
    }
    show("strikers at 22-30 m", labels);
    let n = def.0[0].max(1.0);
    println!(
        "defending back line (CB, FB) at 22-30 m: from the goal line {:4.1} m, block line {:4.1} m, target {:4.1} m",
        def.0[1] / n,
        def.0[2] / n,
        def.0[3] / n
    );
    let m = def.0[4].max(1.0);
    println!(
        "attacking block front {:4.1} m, brain's offside line {:4.1} m, offside line now {:4.1} m",
        def.0[5] / m,
        def.0[6] / m,
        def.0[7] / m
    );
    show("  doing", def.1);
    def_lag.sort_by(|a, b| b.1.total_cmp(&a.1));
    println!(
        "  trailing the target towards the own goal / target above the block line: {}",
        def_lag.iter().take(8).map(|(l, n, v, w)| format!("{l} {:+.1} / {:+.1} m", v / n, w / n)).collect::<Vec<_>>().join(", ")
    );
    show("  defending team's phase", def.2);
    fronts.sort_by(|a, b| b.1[0].total_cmp(&a.1[0]));
    let tot: f64 = fronts.iter().map(|f| f.1[0]).sum();
    println!("most advanced team-mate, ball 22-46 m from the goal line (x, + = towards goal):");
    for (k, v) in fronts.iter().take(10) {
        let n = v[0];
        println!(
            "  {k:<28} {:4.0}%  anchor − offside line {:+5.1}  target − anchor {:+5.1}  pos − target {:+5.1}  deepest opponent − offside line {:+5.1}",
            100.0 * n / tot,
            v[1] / n,
            v[2] / n,
            v[3] / n,
            v[4] / n
        );
    }
    for (w, name) in [(0, "central"), (1, "wide")] {
        println!("{name}");
        for b in 0..10 {
            let mut row = format!("ball {:2}-{:2} m", 10 + 4 * b, 14 + 4 * b);
            for (pr, e) in depth[w * 10 + b].iter().enumerate() {
                let n = e[0].max(1.0);
                row += &format!(
                    "   {} n {:5} deepest {:4.1} 2nd {:4.1} share {:4.2} front {:4.1} box {:3.1}",
                    if pr == 1 { "pressed" } else { "free   " },
                    e[0],
                    e[1] / n,
                    e[2] / n,
                    1.0 - e[1] / e[3].max(1e-9),
                    e[4] / n,
                    e[5] / n
                );
            }
            println!("{row}");
        }
    }
}
