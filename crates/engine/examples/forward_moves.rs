//! Forwards against the last line, as in StatsBomb 360 (scratchpad `sb_front_pin.py`, `sb_fwd_recv.py`).
//! - At every open-play pass with the ball 10–50 m from the goal line: g = a team-mate's distance to the goal line
//!   minus the deepest outfield opponent's (g < 2 m: on or beyond the line, "pinning"). By the space behind the line
//!   (that opponent's distance to the goal line) and whether an opponent is within 3 m of the passer: passes with
//!   0 / 1 / 2+ team-mates pinning, the front team-mate's g, the second one's when the front one pins, team-mates
//!   between (g 4–15 m, nearer the goal than the ball).
//!   Real, all lines, free / pressed: pinning 0 / 1 / 2+ 33 / 46 / 21 and 38 / 43 / 19 %; front g < 2 m 67 / 62 %;
//!   with the line 0–18 m 75 %, 25–35 m 51–57 %.
//! - At every reception of an open-play ground or lofted pass in the attacking half by a forward or attacking
//!   midfielder: g of the receiver, on the line (< 2 m) / short (2–8) / dropped (≥ 8), by line and passer pressure,
//!   and per player. Real centre-forwards 21 / 46 / 33 % (line 0–15 m 42 / 42 / 15, 22–30 m 13 / 42 / 45); dropped
//!   share over 28 centre-forwards p10 15, p50 32, p90 51 % (Lukaku 9, Giroud 15, Benzema 51, Messi 70); wingers
//!   13 / 37 / 50, attacking midfielders 9 / 29 / 62.
//!
//! - Balls in behind: samples every 0.2 s in possession (runs on, attackers labelled "run in behind" or beyond the
//!   line), runs in behind started per player-match by role, through balls, receptions beyond the line at the
//!   control. Real: Number 9s "offers in behind" 26.46 per 90 minutes (FIFA TSG, WC2022); StatsBomb 360
//!   (`sb_behind.py`, raw per team-match, ≈ 57 min in play, ×1.48 for the engine): through balls 1.35 (35 %
//!   completed), receptions in the attacking half beyond the deepest visible outfield opponent 5.2 (CF 1.1, W 1.3,
//!   AM 0.4; 2.8 % of receptions there).
//!
//! Env `ATTRS=68 AWAY=47`: uniform attributes per side (as `layers`). `PROFILE=runner|target|link`: the home side's
//! strikers get that profile on top of their own attributes (`profile`).
//! `cargo run -p match_engine --release --example forward_moves -- [matches]`

use match_engine::Simulation;
use match_engine::events::EventKind;
use match_engine::match_state::side_of;
use match_engine::pitch::HALF_LENGTH;
use match_engine::player::{Attributes, KickKind};
use match_engine::team::Role;

const PIN_LINES: [f32; 6] = [0.0, 12.0, 18.0, 25.0, 35.0, 60.0];
const RECV_LINES: [f32; 5] = [0.0, 15.0, 22.0, 30.0, 60.0];
/// Role groups: strikers (CF, ST2), wide players (W, WM), attacking midfielders, central midfielders (CM8, CMP).
const ROLES: [&str; 4] = ["ST", "W", "AM", "CM"];

fn group(r: Role) -> Option<usize> {
    match r {
        Role::CF | Role::ST2 => Some(0),
        Role::W | Role::WM => Some(1),
        Role::AM => Some(2),
        Role::CM8 | Role::CMP => Some(3),
        _ => None,
    }
}

/// Per line bin × pressed: passes, pinning 0 / 1 / 2+, front g, front g < 2 / 2–5 / 5–10 / 10+, passes where the
/// front one pins, second g then, second g ≥ 5 then, between; then by the team-mates' targets instead of their
/// positions: nobody pinning, front g.
type Pin = [[[f64; 15]; 2]; 5];
/// Per role × (line bin, or 4 = all) × pressed: receptions on the line / short / dropped.
type Recv = [[[[f64; 3]; 2]; 5]; 4];
/// Per player: (name, role, side, receptions on / short / dropped, then sampled every 0.2 s in possession with the
/// ball 10–50 m from the goal line: target on / short / dropped, position on / short / dropped).
type PerPlayer = Vec<(String, usize, usize, [f64; 9])>;
/// Striker receptions by his label when the pass was played: count, g of his position and of his target then, g at
/// the control, dropped then.
type Why = Vec<(String, [f64; 5])>;
/// Strikers sampled in possession, by seconds since the team gained it (0–3, 3–6, 6–10, 10–20, 20+): samples, g of
/// the target, g of the position, position dropped (≥ 8 m) while the target is not.
type Lag = [[f64; 4]; 5];
/// Balls in behind: [0] possession samples, [1] with runs on, [2] attacker samples (ST / W / AM), [3] labelled "run in
/// behind", [4] position beyond the line, [5..8] runs started by ST / W / AM, [8..11] player-matches by ST / W / AM,
/// [11] through balls, [12] completed, [13] controlled beyond the line, [14..18] receptions in the attacking half by
/// ST / W / AM / other, [18..22] of those beyond the line at the control, [22] receptions after a pass played while
/// the receiver was labelled "run in behind".
type Behind = [f64; 23];

fn uniform(sim: &mut Simulation) {
    let get = |k: &str| std::env::var(k).ok().and_then(|s| s.parse::<f32>().ok());
    for (side, v) in [(0usize, get("ATTRS")), (1, get("AWAY").or(get("ATTRS")))] {
        let Some(v) = v else { continue };
        for p in sim.state.players.iter_mut().filter(|p| p.side as usize == side) {
            let a = &p.attrs;
            p.attrs = Attributes { preferred_foot: a.preferred_foot, height_cm: a.height_cm, weight_kg: a.weight_kg, ..Attributes::uniform(v) };
        }
    }
}

/// Forward profiles for the response test: a runner (speed, quickness), a target man (strength, in the air, hold-up),
/// a link player (touch, short passing, intelligence).
fn profile(sim: &mut Simulation) {
    let Ok(kind) = std::env::var("PROFILE") else { return };
    let st = &mut sim.state;
    let strikers: Vec<usize> = st.players.iter().filter(|p| p.side as usize == 0 && p.role.is_striker()).map(|p| p.id).collect();
    for id in strikers {
        let a = &mut st.players[id].attrs;
        match kind.as_str() {
            "runner" => {
                (a.speed, a.quickness, a.intelligence, a.instinct) = (88.0, 85.0, 65.0, 75.0);
                (a.short_passing, a.first_touch, a.strength, a.heading) = (40.0, 50.0, 50.0, 45.0);
            }
            "target" => {
                (a.strength, a.heading, a.jumping, a.first_touch, a.height_cm) = (88.0, 85.0, 85.0, 70.0, 192.0);
                (a.speed, a.quickness, a.short_passing) = (45.0, 45.0, 48.0);
            }
            "link" => {
                (a.short_passing, a.intelligence, a.first_touch, a.dribbling, a.instinct) = (88.0, 85.0, 85.0, 80.0, 75.0);
                (a.speed, a.quickness, a.strength, a.heading) = (50.0, 55.0, 45.0, 40.0);
            }
            k => panic!("PROFILE {k}: runner, target or link"),
        }
    }
}

fn bin(edges: &[f32], v: f32) -> Option<usize> {
    (0..edges.len() - 1).find(|&i| v >= edges[i] && v < edges[i + 1])
}

fn main() {
    let matches: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(12);
    let res: Vec<(Pin, Recv, PerPlayer, Why, Lag, Behind)> = std::thread::scope(|scope| {
        let handles: Vec<_> = (1..=matches)
            .map(|seed| {
                scope.spawn(move || {
                    let mut sim = Simulation::from_builtin("northbridge", "harbour", seed);
                    // `CFG=file.ron`: `SimConfig` overrides, as `match_lab --config`.
                    if let Ok(path) = std::env::var("CFG") {
                        sim.cfg = match_engine::config::SimConfig::from_ron(&std::fs::read_to_string(path).unwrap()).unwrap();
                    }
                    uniform(&mut sim);
                    profile(&mut sim);
                    let mut pin: Pin = Default::default();
                    let mut recv: Recv = Default::default();
                    let mut per: PerPlayer = Vec::new();
                    let mut why: Why = Vec::new();
                    let mut lag: Lag = Default::default();
                    let mut bh: Behind = [0.0; 23];
                    let att = |r: Role| group(r).filter(|&k| k < 3);
                    for t in &sim.state.teams {
                        t.formation.slots.iter().filter_map(|s| att(s.role)).for_each(|k| bh[8 + k] += 1.0);
                    }
                    let mut was_run = vec![false; sim.state.players.len()];
                    // Pending pass: receiver, passer pressed, receiver's label, g of his position and target, through ball.
                    let mut pending: Option<(usize, bool, String, f32, f32, bool)> = None;
                    let mut seen = 0;
                    let mut restart = true;
                    let mut tick = 0u32;
                    while !sim.finished() {
                        sim.step();
                        tick += 1;
                        if tick.is_multiple_of(10) && sim.state.is_live()
                            && let Some(c) = sim.state.control
                        {
                            let st = &sim.state;
                            let side = side_of(c);
                            let fr = st.frame(side);
                            let bx = HALF_LENGTH - fr.to_team(st.players[c].pos).x;
                            let dl = st.squad(side.opponent()).iter().filter(|o| !o.is_gk()).map(|o| HALF_LENGTH - fr.to_team(o.pos).x).fold(f32::MAX, f32::min);
                            bh[0] += 1.0;
                            bh[1] += st.team_view(side).runs_on as u32 as f64;
                            for m in st.squad(side.opponent()) {
                                was_run[m.id] = false;
                            }
                            for m in st.squad(side).iter().filter(|m| m.id != c) {
                                let Some(k) = att(st.team(side).formation.slots[m.slot].role) else { continue };
                                let run = m.debug.label == "run in behind";
                                bh[2] += 1.0;
                                bh[3] += run as u32 as f64;
                                bh[4] += (HALF_LENGTH - fr.to_team(m.pos).x < dl) as u32 as f64;
                                bh[5 + k] += (run && !was_run[m.id]) as u32 as f64;
                                was_run[m.id] = run;
                            }
                            if (10.0..50.0).contains(&bx) {
                                for m in st.squad(side).iter().filter(|m| m.id != c && group(m.role).is_some()) {
                                    let cls = |g: f32| if g < 2.0 { 0 } else if g < 8.0 { 1 } else { 2 };
                                    let (g_t, g_p) = (HALF_LENGTH - fr.to_team(m.target).x - dl, HALF_LENGTH - fr.to_team(m.pos).x - dl);
                                    let (gt, gp) = (cls(g_t), cls(g_p));
                                    if m.role.is_striker() {
                                        let since = st.time - st.possession_since;
                                        let l = &mut lag[[3.0, 6.0, 10.0, 20.0, f32::MAX].iter().position(|&e| since < e).unwrap()];
                                        l[0] += 1.0;
                                        l[1] += g_t as f64;
                                        l[2] += g_p as f64;
                                        l[3] += (gp == 2 && gt < 2) as u32 as f64;
                                    }
                                    let e = match per.iter_mut().position(|e| e.0 == m.name) {
                                        Some(i) => &mut per[i],
                                        None => {
                                            per.push((m.name.clone(), group(m.role).unwrap(), side as usize, [0.0; 9]));
                                            per.last_mut().unwrap()
                                        }
                                    };
                                    e.3[3 + gt] += 1.0;
                                    e.3[6 + gp] += 1.0;
                                }
                            }
                        }
                        while seen < sim.events.len() {
                            let e = sim.events[seen].clone();
                            seen += 1;
                            let (by, to, kind, headed) = match e.kind {
                                EventKind::Restart { .. } | EventKind::Kickoff { .. } => {
                                    restart = true;
                                    pending = None;
                                    continue;
                                }
                                EventKind::Pass { by, to, kind, headed, .. } => (by, to, kind, headed),
                                EventKind::Shot { .. } | EventKind::Interception { .. } => {
                                    restart = false;
                                    pending = None;
                                    continue;
                                }
                                _ => continue,
                            };
                            pending = None;
                            if std::mem::take(&mut restart) {
                                continue;
                            }
                            let st = &sim.state;
                            let side = side_of(by);
                            let fr = st.frame(side);
                            let ball = fr.to_team(st.players[by].pos);
                            let pressed = st.squad(side.opponent()).iter().any(|o| (fr.to_team(o.pos) - ball).length() < 3.0);
                            if !headed && matches!(kind, KickKind::GroundPass | KickKind::LoftedPass) {
                                let dl = st.squad(side.opponent()).iter().filter(|o| !o.is_gk()).map(|o| HALF_LENGTH - fr.to_team(o.pos).x).fold(f32::MAX, f32::min);
                                let through = st.players[by].debug.label.starts_with("through ball");
                                bh[11] += through as u32 as f64;
                                pending = to.map(|t| {
                                    let r = &st.players[t];
                                    (t, pressed, r.debug.label.clone(), HALF_LENGTH - fr.to_team(r.pos).x - dl, HALF_LENGTH - fr.to_team(r.target).x - dl, through)
                                });
                            }
                            let bx = HALF_LENGTH - ball.x;
                            if !(10.0..50.0).contains(&bx) {
                                continue;
                            }
                            let dline = st.squad(side.opponent()).iter().filter(|o| !o.is_gk()).map(|o| HALF_LENGTH - fr.to_team(o.pos).x).fold(f32::MAX, f32::min);
                            let Some(lb) = bin(&PIN_LINES, dline) else { continue };
                            let mut gs: Vec<(f32, f32)> = st
                                .squad(side)
                                .iter()
                                .filter(|m| m.id != by && !m.is_gk())
                                .map(|m| {
                                    let d = HALF_LENGTH - fr.to_team(m.pos).x;
                                    (d - dline, d)
                                })
                                .collect();
                            gs.sort_by(|a, b| a.0.total_cmp(&b.0));
                            let a = &mut pin[lb][pressed as usize];
                            let pins = gs.iter().filter(|g| g.0 < 2.0).count();
                            a[0] += 1.0;
                            a[1 + pins.min(2)] += 1.0;
                            let g1 = gs[0].0;
                            a[4] += g1 as f64;
                            a[5 + if g1 < 2.0 { 0 } else if g1 < 5.0 { 1 } else if g1 < 10.0 { 2 } else { 3 }] += 1.0;
                            if g1 < 2.0 {
                                a[9] += 1.0;
                                a[10] += gs[1].0 as f64;
                                a[11] += (gs[1].0 >= 5.0) as u32 as f64;
                            }
                            a[12] += gs.iter().filter(|g| (4.0..15.0).contains(&g.0) && g.1 < bx).count() as f64;
                            let gt = st.squad(side).iter().filter(|m| m.id != by && !m.is_gk()).map(|m| HALF_LENGTH - fr.to_team(m.target).x - dline).fold(f32::MAX, f32::min);
                            a[13] += (gt >= 2.0) as u32 as f64;
                            a[14] += gt as f64;
                        }
                        // Reception: the intended receiver gets control.
                        let st = &sim.state;
                        if let (Some((to, pressed, label, g_pos, g_tgt, through)), Some(c)) = (pending.clone(), st.control) {
                            pending = None;
                            if c != to || !st.is_live() {
                                continue;
                            }
                            let side = side_of(c);
                            let role = st.team(side).formation.slots[st.players[c].slot].role;
                            let fr = st.frame(side);
                            let q = fr.to_team(st.players[c].pos);
                            let dline = st.squad(side.opponent()).iter().filter(|o| !o.is_gk()).map(|o| HALF_LENGTH - fr.to_team(o.pos).x).fold(f32::MAX, f32::min);
                            let g = HALF_LENGTH - q.x - dline;
                            if through {
                                bh[12] += 1.0;
                                bh[13] += (g < 0.0) as u32 as f64;
                            }
                            if q.x >= 0.0 {
                                let k = att(role).unwrap_or(3);
                                bh[14 + k] += 1.0;
                                bh[18 + k] += (g < 0.0) as u32 as f64;
                                bh[22] += (label == "run in behind") as u32 as f64;
                            }
                            let Some(ri) = group(role) else { continue };
                            if q.x < 0.0 {
                                continue;
                            }
                            let cls = if g < 2.0 { 0 } else if g < 8.0 { 1 } else { 2 };
                            if let Some(lb) = bin(&RECV_LINES, dline) {
                                recv[ri][lb][pressed as usize][cls] += 1.0;
                            }
                            recv[ri][4][pressed as usize][cls] += 1.0;
                            if role.is_striker() {
                                let v = [1.0, g_pos as f64, g_tgt as f64, g as f64, (cls == 2) as u32 as f64];
                                match why.iter_mut().find(|e| e.0 == label) {
                                    Some(e) => (0..5).for_each(|i| e.1[i] += v[i]),
                                    None => why.push((label, v)),
                                }
                            }
                            let name = st.players[c].name.clone();
                            match per.iter_mut().find(|e| e.0 == name) {
                                Some(e) => e.3[cls] += 1.0,
                                None => {
                                    let mut v = [0.0; 9];
                                    v[cls] = 1.0;
                                    per.push((name, ri, side as usize, v));
                                }
                            }
                        }
                    }
                    (pin, recv, per, why, lag, bh)
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut pin: Pin = Default::default();
    let mut recv: Recv = Default::default();
    let mut per: PerPlayer = Vec::new();
    let mut why: Why = Vec::new();
    let mut lag: Lag = Default::default();
    let mut bh: Behind = [0.0; 23];
    for (p, r, pp, wy, lg, b) in res {
        bh.iter_mut().zip(b).for_each(|(t, v)| *t += v);
        (0..5).for_each(|i| (0..4).for_each(|k| lag[i][k] += lg[i][k]));
        for (l, v) in wy {
            match why.iter_mut().find(|e| e.0 == l) {
                Some(e) => (0..5).for_each(|i| e.1[i] += v[i]),
                None => why.push((l, v)),
            }
        }
        for (i, row) in p.iter().enumerate() {
            for (j, cell) in row.iter().enumerate() {
                (0..15).for_each(|k| pin[i][j][k] += cell[k]);
            }
        }
        for (i, a) in r.iter().enumerate() {
            for (j, b) in a.iter().enumerate() {
                for (k, c) in b.iter().enumerate() {
                    (0..3).for_each(|l| recv[i][j][k][l] += c[l]);
                }
            }
        }
        for (name, role, side, v) in pp {
            match per.iter_mut().find(|e| e.0 == name) {
                Some(e) => (0..9).for_each(|l| e.3[l] += v[l]),
                None => per.push((name, role, side, v)),
            }
        }
    }
    println!("{matches} matches. Open-play passes, ball 10–50 m from the goal line; g = team-mate's distance short of the deepest outfield opponent");
    println!("{:10} {:8} {:>6} | pinning 0 / 1 / 2+  | front g mean, <2 / 2-5 / 5-10 / 10+ m | 2nd g when front pins, >=5 m | between | targets: pinning 0, front g", "line (m)", "passer", "n");
    for lb in 0..6 {
        for pr in 0..2 {
            let a = if lb < 5 {
                pin[lb][pr]
            } else {
                let mut s = [0.0; 15];
                for row in &pin[..5] {
                    s.iter_mut().zip(&row[pr]).for_each(|(t, v)| *t += v);
                }
                s
            };
            let n = a[0].max(1.0);
            let n1 = a[9].max(1.0);
            let lab = if lb < 5 { format!("{}-{}", PIN_LINES[lb], PIN_LINES[lb + 1]) } else { "all".into() };
            println!(
                "{lab:10} {:8} {:6} | {:4.0} {:4.0} {:4.0} %    | {:4.1}  {:3.0} {:3.0} {:3.0} {:3.0} %            | {:4.1}  {:3.0} %            | {:4.2}    | {:4.0} %  {:4.1}",
                if pr == 1 { "pressed" } else { "free" },
                a[0],
                100.0 * a[1] / n,
                100.0 * a[2] / n,
                100.0 * a[3] / n,
                a[4] / n,
                100.0 * a[5] / n,
                100.0 * a[6] / n,
                100.0 * a[7] / n,
                100.0 * a[8] / n,
                a[10] / n1,
                100.0 * a[11] / n1,
                a[12] / n,
                100.0 * a[13] / n,
                a[14] / n,
            );
        }
    }
    println!();
    println!("receptions in the attacking half: on the line (g < 2) / short (2-8) / dropped (>= 8 m), %");
    for (ri, role) in ROLES.iter().enumerate() {
        for lb in [4, 0, 1, 2, 3] {
            let lab = if lb == 4 { "all lines".to_string() } else { format!("line {}-{} m", RECV_LINES[lb], RECV_LINES[lb + 1]) };
            let mut row = format!("{:3} {lab:15}", role);
            for pr in [None, Some(0), Some(1)] {
                let c = match pr {
                    None => [0, 1, 2].map(|l| recv[ri][lb][0][l] + recv[ri][lb][1][l]),
                    Some(p) => recv[ri][lb][p],
                };
                let n: f64 = c.iter().sum::<f64>().max(1.0);
                let tag = match pr {
                    None => "any",
                    Some(0) => "free",
                    _ => "pressed",
                };
                row += &format!("  {tag:7} n {:5} {:3.0}/{:3.0}/{:3.0}", c.iter().sum::<f64>(), 100.0 * c[0] / n, 100.0 * c[1] / n, 100.0 * c[2] / n);
            }
            println!("{row}");
        }
    }
    println!();
    println!("striker receptions by his label when the pass was played: share, g of his position / target then, g at the control, dropped");
    why.sort_by(|a, b| b.1[0].total_cmp(&a.1[0]));
    let nw: f64 = why.iter().map(|e| e.1[0]).sum::<f64>().max(1.0);
    for (l, v) in &why {
        let n = v[0];
        println!("  {l:28} {:4.0} %  pos {:5.1}  target {:5.1}  control {:5.1}  dropped {:3.0} %", 100.0 * n / nw, v[1] / n, v[2] / n, v[3] / n, 100.0 * v[4] / n);
    }
    println!();
    println!("strikers in possession by seconds since the team gained it: share of samples, g of target / position, position dropped while the target is not");
    let nl: f64 = lag.iter().map(|l| l[0]).sum::<f64>().max(1.0);
    for (i, lab) in ["0-3 s", "3-6 s", "6-10 s", "10-20 s", "20+ s"].iter().enumerate() {
        let l = lag[i];
        let n = l[0].max(1.0);
        println!("  {lab:8} {:3.0} %  target {:5.1}  position {:5.1}  {:3.0} %", 100.0 * l[0] / nl, l[1] / n, l[2] / n, 100.0 * l[3] / n);
    }
    println!();
    println!("per player (>= 30 receptions in the attacking half): receptions on the line / short / dropped; every 0.2 s in possession (ball 10-50 m out): target, position");
    per.sort_by_key(|a| (a.1, a.2));
    for (name, role, side, v) in &per {
        let n: f64 = v[..3].iter().sum();
        let nt: f64 = v[3..6].iter().sum::<f64>().max(1.0);
        if n >= 30.0 {
            println!(
                "  {:22} {:3} side {side}  n {n:5}  {:3.0} / {:3.0} / {:3.0} %   target {:3.0} / {:3.0} / {:3.0} %   position {:3.0} / {:3.0} / {:3.0} %",
                name,
                ROLES[*role],
                100.0 * v[0] / n,
                100.0 * v[1] / n,
                100.0 * v[2] / n,
                100.0 * v[3] / nt,
                100.0 * v[4] / nt,
                100.0 * v[5] / nt,
                100.0 * v[6] / nt,
                100.0 * v[7] / nt,
                100.0 * v[8] / nt
            );
        }
    }
    let tm = 2.0 * matches as f64;
    let pc = |a: f64, b: f64| 100.0 * a / b.max(1.0);
    println!();
    println!("balls in behind (per team-match; real counts from StatsBomb are per ≈ 57 min in play, ×1.48 for the engine)");
    println!(
        "  in possession: runs on {:.0} % of samples; attackers labelled \"run in behind\" {:.1} %, beyond the line {:.1} % of samples",
        pc(bh[1], bh[0]),
        pc(bh[3], bh[2]),
        pc(bh[4], bh[2])
    );
    println!(
        "  runs in behind started per player-match: ST {:.1}  W {:.1}  AM {:.1}   (real: Number 9s offer in behind 26.5 per 90 min)",
        bh[5] / bh[8].max(1.0),
        bh[6] / bh[9].max(1.0),
        bh[7] / bh[10].max(1.0)
    );
    println!(
        "  through balls {:.2}, completed {:.0} %, controlled beyond the line {:.0} % of completed   (real 1.35 → ×1.48 2.0, 35 %)",
        bh[11] / tm,
        pc(bh[12], bh[11]),
        pc(bh[13], bh[12])
    );
    println!(
        "  receptions in the attacking half beyond the line at the control: all {:.1} ({:.1} %), ST {:.2}, W {:.2}, AM {:.2}; after a pass to a runner {:.2}   (real 5.2 → 7.6, 2.8 %; CF 1.1, W 1.3, AM 0.4)",
        (bh[18] + bh[19] + bh[20] + bh[21]) / tm,
        pc(bh[18] + bh[19] + bh[20] + bh[21], bh[14] + bh[15] + bh[16] + bh[17]),
        bh[18] / tm,
        bh[19] / tm,
        bh[20] / tm,
        bh[22] / tm
    );
}
