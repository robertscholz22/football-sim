//! Shots by distance band, followed down to the keeper. At release: on target (predicted to enter the goal) or not,
//! the shot variant, ball speed, where it crosses the goal line and how far that is from the keeper (sideways) and
//! how deep he stands. Outcome: blocked by an outfield player, saved, goal after a failed save roll (the ball came
//! within the keeper's standing reach of his body), goal out of his reach, or off target / other.
//! Usage: saves [matches] [minutes]; env ANGLE, ELEV, EXP override the shot execution error, GK_NARROW, GK_PER_M,
//! GK_MAX, GK_FRAC the GK-01 depth, SAVE_BIAS, SAVE_SLOPE the keeper's save odds, REACH, LEG_P, LEG_H the shot
//! blocks (calibration sweeps).

use std::collections::BTreeMap;

use glam::Vec2;
use match_engine::config::SimConfig;
use match_engine::pitch::PitchConditions;
use match_engine::team::TeamData;
use match_engine::Simulation;
use match_engine::duel::save_prob;
use match_engine::events::EventKind;
use match_engine::match_state::side_of;
use match_engine::pitch::{GOAL_HALF_WIDTH, HALF_LENGTH};
use match_engine::player::Body;

const BANDS: [(f32, f32, &str); 6] =
    [(0.0, 6.0, "0–6 m"), (6.0, 11.0, "6–11 m"), (11.0, 16.5, "11–16.5 m"), (16.5, 22.0, "16.5–22 m"), (22.0, 30.0, "22–30 m"), (30.0, 99.0, "30+ m")];
const OUTCOMES: [&str; 5] = ["blocked", "saved", "goal, save failed", "goal, out of reach", "off target / other"];

struct Shot {
    band: usize,
    /// Distance (m) from goal where the shot was struck.
    dist: f32,
    by: usize,
    t0: f32,
    on_goal: bool,
    variant: String,
    /// Lateral distance (m) from the keeper to the predicted crossing point, keeper depth (m), flight time to the line.
    gk_lateral: f32,
    gk_depth: f32,
    flight_t: f32,
    speed: f32,
    xg: f32,
    dived: bool,
    p_save: Option<f32>,
    /// Keeper's label at the shot and |y| where the ball crosses the goal line.
    gk_label: String,
    cross_y: f32,
    /// Predicted miss at release: 0 wide, 1 over, 2 short of the line (None if on target).
    miss: Option<usize>,
    reach_frac: f32,
    react: f32,
    /// Save probability the keeper would have if the ball reached him now (kept from the last tick before contact).
    p_cur: f32,
    /// Outfield opponents in the shooter–posts triangle at the shot.
    tri: usize,
}

#[derive(Default)]
struct Acc {
    n: u32,
    on_goal: u32,
    outcome: [u32; 5],
    xg: f64,
    speed: f64,
    flight_t: f64,
    /// Per outcome: summed keeper lateral distance and depth, dives.
    lateral: [f64; 5],
    depth: [f64; 5],
    dived: [u32; 5],
    p_save: f64,
    attempts: u32,
    variants: BTreeMap<String, (u32, u32, u32)>,
    /// Keeper label at the shot → (shots, goals out of reach).
    gk_labels: BTreeMap<String, (u32, u32)>,
    /// On-target shots by crossing third of the goal (central, middle, outer) → (n, saved).
    thirds: [(u32, u32); 3],
    misses: [u32; 3],
    reach_frac: f64,
    react: f64,
    /// By outfield opponents in the shooting triangle (0, 1, 2+): (shots, blocked).
    tri: [(u32, u32); 3],
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let matches: u64 = args.first().and_then(|s| s.parse().ok()).unwrap_or(10);
    let mins: f32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(90.0);
    let mut acc: Vec<Acc> = BANDS.iter().map(|_| Acc::default()).collect();
    let env = |k: &str| std::env::var(k).ok().and_then(|v| v.parse::<f32>().ok());
    let mut cfg = SimConfig::default();
    if let Some(v) = env("ANGLE") {
        cfg.execution.angle_shot = v;
    }
    if let Some(v) = env("ELEV") {
        cfg.execution.elevation_shot = v;
    }
    if let Some(v) = env("EXP") {
        cfg.execution.shot_speed_exp = v;
    }
    for (k, f) in [
        ("GK_NARROW", &mut cfg.tactics.gk01_narrow_depth),
        ("GK_PER_M", &mut cfg.tactics.gk01_depth_per_m),
        ("GK_MAX", &mut cfg.tactics.gk01_max_depth),
        ("GK_FRAC", &mut cfg.tactics.gk01_depth_frac),
        ("SAVE_BIAS", &mut cfg.duel.save_bias),
        ("SAVE_SLOPE", &mut cfg.duel.save_slope),
        ("REACH", &mut cfg.player.shot_block_reach),
        ("LEG_P", &mut cfg.player.shot_leg_block_p),
        ("LEG_H", &mut cfg.player.shot_block_height),
    ] {
        if let Some(v) = env(k) {
            *f = v;
        }
    }
    let (home, away) = (TeamData::builtin("northbridge").unwrap(), TeamData::builtin("harbour").unwrap());
    for seed in 1..=matches {
        let mut sim = Simulation::new(&home, &away, cfg.clone(), PitchConditions::default(), seed);
        sim.state.half_length = mins * 30.0;
        let mut seen = 0;
        let mut open: Option<Shot> = None;
        while !sim.finished() {
            sim.step();
            let mut close: Option<usize> = None;
            if let Some(s) = open.as_mut() {
                let st = &sim.state;
                let dside = side_of(s.by).opponent();
                if let Some(g) = st.squad(dside).iter().find(|p| p.is_gk()) {
                    s.dived |= matches!(g.body, Body::Diving { .. });
                    if st.flight.is_some_and(|f| f.kicker == s.by && !f.save_attempted) {
                        let reach_frac = match g.body {
                            Body::Diving { dir, .. } => (dir.length() / sim.cfg.player.gk_dive_speed).clamp(0.0, 1.0),
                            _ => 0.0,
                        };
                        s.p_cur = save_prob(g, s.dist, st.ball.speed(), reach_frac, st.time - s.t0, &sim.cfg.duel);
                        s.reach_frac = reach_frac;
                        s.react = st.time - s.t0;
                    }
                    if s.p_save.is_none() && st.flight.is_some_and(|f| f.save_attempted && f.kicker == s.by) {
                        s.p_save = Some(s.p_cur);
                    }
                }
                close = match st.last_touch {
                    Some(x) if x != s.by && side_of(x) != side_of(s.by) && !st.players[x].is_gk() => Some(0),
                    Some(x) if x != s.by => Some(4),
                    _ if st.time - s.t0 > 3.0 || st.control.is_some_and(|c| c != s.by) => Some(4),
                    _ => None,
                };
            }
            let mut new_shots = Vec::new();
            while seen < sim.events.len() {
                let e = sim.events[seen].clone();
                seen += 1;
                match e.kind {
                    EventKind::Shot { by, xg, .. } => new_shots.push((by, xg, e.t)),
                    EventKind::Goal { .. } if open.is_some() => {
                        let s = open.as_ref().unwrap();
                        close = Some(if s.p_save.is_some() { 2 } else { 3 });
                    }
                    EventKind::Save { .. } if open.is_some() => {
                        let s = open.as_mut().unwrap();
                        s.p_save.get_or_insert(s.p_cur);
                        close = Some(1);
                    }
                    EventKind::Restart { .. } if open.is_some() && close.is_none() => close = Some(4),
                    _ => {}
                }
            }
            if let Some(k) = close
                && let Some(s) = open.take()
            {
                let a = &mut acc[s.band];
                a.n += 1;
                a.on_goal += s.on_goal as u32;
                a.outcome[k] += 1;
                a.xg += s.xg as f64;
                a.speed += s.speed as f64;
                a.flight_t += s.flight_t as f64;
                a.lateral[k] += s.gk_lateral as f64;
                a.depth[k] += s.gk_depth as f64;
                a.dived[k] += s.dived as u32;
                if let Some(p) = s.p_save {
                    a.p_save += p as f64;
                    a.attempts += 1;
                    a.reach_frac += s.reach_frac as f64;
                    a.react += s.react as f64;
                }
                if let Some(m) = s.miss {
                    a.misses[m] += 1;
                }
                let l = a.gk_labels.entry(s.gk_label).or_default();
                l.0 += 1;
                l.1 += (k == 3) as u32;
                if (1..4).contains(&k) && s.cross_y < GOAL_HALF_WIDTH {
                    let third = ((s.cross_y / (GOAL_HALF_WIDTH / 3.0)) as usize).min(2);
                    a.thirds[third].0 += 1;
                    a.thirds[third].1 += (k == 1) as u32;
                }
                let tri = &mut a.tri[s.tri.min(2)];
                tri.0 += 1;
                tri.1 += (k == 0) as u32;
                let v = a.variants.entry(s.variant).or_default();
                v.0 += 1;
                v.1 += s.on_goal as u32;
                v.2 += (k == 2 || k == 3) as u32;
            }
            // New shots (after closing the previous one: a rebound shot opens a new record).
            for (by, xg, t0) in new_shots {
                let st = &sim.state;
                let fr = st.frame(side_of(by));
                let Some(f) = st.flight.filter(|f| f.kicker == by) else { continue };
                let from = fr.to_team(f.start_pos);
                let d = (Vec2::new(HALF_LENGTH, 0.0) - from).length();
                let Some(band) = BANDS.iter().position(|&(lo, hi, _)| d >= lo && d < hi) else { continue };
                // Where the predicted path crosses the goal line (the samples stop at the exit, often just short of
                // the line: extrapolate the last one).
                let cross = sim.traj.samples.iter().find(|q| fr.to_team(q.pos.truncate()).x >= HALF_LENGTH).or(sim.traj.samples.last()).and_then(|q| {
                    let (p, v) = (fr.to_team(q.pos.truncate()), fr.to_team(q.vel.truncate()));
                    let dt = if p.x >= HALF_LENGTH { 0.0 } else if v.x > 0.1 { (HALF_LENGTH - p.x) / v.x } else { return None };
                    (dt < 0.5).then(|| ((p + v * dt).y, q.pos.z + q.vel.z * dt, q.t + dt))
                });
                let (cy, cz, ct) = cross.unwrap_or((0.0, 0.0, 0.0));
                let miss = (!f.on_goal).then(|| match cross {
                    None => 2,
                    Some(_) if cy.abs() > GOAL_HALF_WIDTH => 0,
                    Some(_) if cz > 2.2 => 1,
                    _ => 2,
                });
                let gk_label = st
                    .squad(side_of(by).opponent())
                    .iter()
                    .find(|p| p.is_gk())
                    .map_or(String::new(), |g| g.debug.label.clone());
                let (gk_lateral, gk_depth) = st
                    .squad(side_of(by).opponent())
                    .iter()
                    .find(|p| p.is_gk())
                    .map_or((0.0, 0.0), |g| {
                        let q = fr.to_team(g.pos);
                        ((q.y - cy).abs(), HALF_LENGTH - q.x)
                    });
                let (pl, pr) = (Vec2::new(HALF_LENGTH, GOAL_HALF_WIDTH), Vec2::new(HALF_LENGTH, -GOAL_HALF_WIDTH));
                let tri = st
                    .squad(side_of(by).opponent())
                    .iter()
                    .filter(|q| !q.is_gk())
                    .filter(|q| {
                        let q = fr.to_team(q.pos);
                        let (d1, d2, d3) = ((q - from).perp_dot(pl - from), (q - pl).perp_dot(pr - pl), (q - pr).perp_dot(from - pr));
                        (d1 < 0.0) == (d2 < 0.0) && (d2 < 0.0) == (d3 < 0.0)
                    })
                    .count();
                let label = &st.players[by].debug.label;
                let variant = ["placed", "power", "curled"].iter().find(|v| label.contains(*v)).map_or("other", |v| v);
                open = Some(Shot {
                    band,
                    dist: d,
                    by,
                    t0,
                    on_goal: f.on_goal,
                    variant: variant.to_string(),
                    gk_lateral,
                    gk_depth,
                    flight_t: ct,
                    speed: f.speed,
                    xg,
                    dived: false,
                    p_save: None,
                    gk_label,
                    cross_y: cy.abs(),
                    miss,
                    reach_frac: 0.0,
                    react: 0.0,
                    p_cur: 0.0,
                    tri,
                });
            }
        }
    }
    let n = matches as f64;
    let total: u32 = acc.iter().map(|a| a.n).sum();
    let goals: u32 = acc.iter().map(|a| a.outcome[2] + a.outcome[3]).sum();
    let saved: u32 = acc.iter().map(|a| a.outcome[1]).sum();
    let on_t = goals + saved;
    let blocked: u32 = acc.iter().map(|a| a.outcome[0]).sum();
    println!(
        "{matches} matches: {:.1} shots, {:.1} on target ({:.0} %; blocked {:.0} %, on target of unblocked {:.0} %), {:.1} goals per match; save % {:.0}; goals/shot {:.1} %",
        total as f64 / n,
        on_t as f64 / n,
        100.0 * on_t as f64 / total.max(1) as f64,
        100.0 * blocked as f64 / total.max(1) as f64,
        100.0 * on_t as f64 / (total - blocked).max(1) as f64,
        goals as f64 / n,
        100.0 * saved as f64 / on_t.max(1) as f64,
        100.0 * goals as f64 / total.max(1) as f64
    );
    for ((_, _, name), a) in BANDS.iter().zip(&acc) {
        let m = a.n.max(1) as f64;
        let pct = |k: usize| 100.0 * a.outcome[k] as f64 / m;
        let on = a.outcome[1] + a.outcome[2] + a.outcome[3];
        println!(
            "\n{name}: {:.1} shots per match, mean xG {:.3}, speed {:.1} m/s, flight to the line {:.2} s, predicted on target at release {:.0} %",
            a.n as f64 / n,
            a.xg / m,
            a.speed / m,
            a.flight_t / m,
            100.0 * a.on_goal as f64 / m
        );
        println!(
            "  outcome: blocked {:.0} %, saved {:.0} %, goal after failed save {:.0} %, goal out of reach {:.0} %, off target / other {:.0} %  →  on target {:.0} %, save % {:.0}, conversion {:.0} %",
            pct(0),
            pct(1),
            pct(2),
            pct(3),
            pct(4),
            100.0 * on as f64 / m,
            100.0 * a.outcome[1] as f64 / on.max(1) as f64,
            pct(2) + pct(3)
        );
        let per: Vec<String> = (1..4)
            .filter(|&k| a.outcome[k] > 0)
            .map(|k| {
                let c = a.outcome[k] as f64;
                format!("{}: keeper {:.1} m sideways, {:.1} m deep, dived {:.0} %", OUTCOMES[k], a.lateral[k] / c, a.depth[k] / c, 100.0 * a.dived[k] as f64 / c)
            })
            .collect();
        println!("  {}", per.join(" | "));
        let at = a.attempts.max(1) as f64;
        println!(
            "  save attempts: {}, mean p_save {:.2}, reach fraction {:.2}, time since the shot {:.2} s; predicted misses: wide {}, over {}, other (post, short) {}",
            a.attempts,
            a.p_save / at,
            a.reach_frac / at,
            a.react / at,
            a.misses[0],
            a.misses[1],
            a.misses[2]
        );
        let v: Vec<String> = a
            .variants
            .iter()
            .map(|(k, (c, o, g))| format!("{k} {c} (on target {:.0} %, goals {:.0} %)", 100.0 * *o as f64 / *c as f64, 100.0 * *g as f64 / *c as f64))
            .collect();
        println!("  variants: {}", v.join(", "));
        let l: Vec<String> = a.gk_labels.iter().map(|(k, (c, g))| format!("{k} {c} ({g} beaten out of reach)")).collect();
        println!("  keeper at the shot: {}", l.join(", "));
        let t: Vec<String> = ["central", "middle", "outer"]
            .iter()
            .zip(a.thirds)
            .map(|(k, (c, sv))| format!("{k} {c} (saved {:.0} %)", 100.0 * sv as f64 / c.max(1) as f64))
            .collect();
        println!("  on target by third of the goal: {}", t.join(", "));
        let n_tri: u32 = a.tri.iter().map(|t| t.0).sum();
        let mean_tri = (a.tri[1].0 + 2 * a.tri[2].0) as f64 / n_tri.max(1) as f64;
        let by: Vec<String> = a.tri.iter().map(|(c, b)| format!("{:.0} % (n {c})", 100.0 * *b as f64 / (*c).max(1) as f64)).collect();
        println!("  opponents in the shooting triangle ≥ {mean_tri:.2} per shot; blocked with 0 / 1 / 2+ in it: {}", by.join(" / "));
    }
}
