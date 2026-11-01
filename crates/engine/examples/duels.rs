//! 1 v 1 duels, per carrier–defender pair: the first defender (any defender with `ALL=1`) is engaged when he is set
//! in front of the carrier (on the carrier's line to the goal centre: ≥ 1 m ahead, ≤ 1.5 m to the side, within
//! `ENGAGE` metres); the pair is followed even if the role is handed over. The pair ends when
//! the defender wins the ball in a tackle, the carrier releases it (pass / shot) or loses it otherwise, the defender
//! is beaten (farther from his goal than the carrier) in or without a tackle, he drops off (> 5 m away, still
//! goal-side), or after `HOLD_S` seconds still in front ("held up"). Per end: count per match, duration, metres the
//! carrier gained towards goal, speeds at the end, duel skill gap. `TRACE=1` prints the beaten pairs.
//! Usage: duels [matches] [minutes]

use std::collections::BTreeMap;

use glam::Vec2;
use match_engine::Simulation;
use match_engine::duel::{TackleOutcome, take_on_skill, tackle_skill};
use match_engine::events::EventKind;
use match_engine::match_state::{PLAYERS_PER_TEAM, side_of};
use match_engine::pitch::TeamFrame;
use match_engine::player::Intent;

const ENGAGE: f32 = 3.0;
const HOLD_S: f32 = 3.0;

struct Pair {
    t0: f32,
    carrier: usize,
    defender: usize,
    goal_d0: f32,
    tackled: Option<TackleOutcome>,
    gap: f32,
    in_range: bool,
    /// At engagement: defender relative to the carrier (defender's frame), carrier and defender speeds.
    start: (Vec2, f32, f32),
    /// Closest the ball came to the defender, minus his tackle reach (m).
    min_gap: f32,
    /// Carrier decisions during the pair.
    decisions: u32,
}

#[derive(Default)]
struct End {
    n: u32,
    dur: f64,
    gained: f64,
    car_v: f64,
    def_v: f64,
    gap: f64,
    in_range: u32,
    /// The ball came within the defender's tackle reach.
    in_reach: u32,
    decisions: f64,
    /// The defender still had the first-defender role at the end.
    still_first: u32,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let matches: u64 = args.first().and_then(|s| s.parse().ok()).unwrap_or(4);
    let mins: f32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(90.0);
    let trace = std::env::var("TRACE").is_ok();
    let all = std::env::var("ALL").is_ok();
    let mut ends: BTreeMap<&'static str, End> = BTreeMap::new();
    let mut tackles: BTreeMap<String, u32> = BTreeMap::new();
    let mut labels: BTreeMap<String, u32> = BTreeMap::new();
    for seed in 1..=matches {
        let mut sim = Simulation::from_builtin("northbridge", "harbour", seed);
        sim.state.half_length = mins * 30.0;
        let mut seen = 0;
        let mut open: Vec<Pair> = Vec::new();
        while !sim.finished() {
            let label_before: Vec<String> = sim.state.players.iter().map(|p| p.debug.label.clone()).collect();
            sim.step();
            let st = &sim.state;
            while seen < sim.events.len() {
                let e = sim.events[seen].clone();
                seen += 1;
                if let EventKind::Tackle { by, on, outcome } = e.kind {
                    let in_pair = open.iter_mut().find(|x| x.defender == by && x.carrier == on);
                    let ctx = if in_pair.is_some() { "set 1v1" } else { "other" };
                    *tackles.entry(format!("{ctx:<8} {outcome:?}")).or_default() += 1;
                    if let Some(x) = in_pair {
                        x.tackled = Some(outcome);
                    }
                }
            }
            let carrier = st.control.filter(|&c| !matches!(st.players[c].intent, Intent::GkHold { .. }));
            let goal = TeamFrame::own_goal();
            for x in open.iter_mut() {
                let d = &st.players[x.defender];
                x.min_gap = x.min_gap.min((d.pos - st.ball.ground_pos()).length() - d.kin.tackle_reach);
                x.decisions += (st.players[x.carrier].debug.label != label_before[x.carrier]) as u32;
            }
            let mut closed = Vec::new();
            for (i, x) in open.iter().enumerate() {
                let fr = st.frame(side_of(x.defender));
                let car = fr.to_team(st.players[x.carrier].pos);
                let def = fr.to_team(st.players[x.defender].pos);
                let end: Option<&'static str> = match x.tackled {
                    Some(o) if o != TackleOutcome::Beaten => Some("tackle won"),
                    _ if carrier != Some(x.carrier) => Some(if st.flight.is_some_and(|f| f.kicker == x.carrier) {
                        "released (pass / shot)"
                    } else {
                        "ball lost otherwise"
                    }),
                    _ if (def - goal).length() > (car - goal).length() + 0.5 => {
                        Some(if x.tackled.is_some() { "beaten in a tackle" } else { "beaten without a tackle" })
                    }
                    _ if (def - car).length() > ENGAGE + 2.0 => Some("dropped off"),
                    _ if st.time - x.t0 > HOLD_S => Some("held up"),
                    _ => None,
                };
                if let Some(k) = end {
                    closed.push(i);
                    let e = ends.entry(k).or_default();
                    e.n += 1;
                    e.dur += (st.time - x.t0) as f64;
                    e.gained += (x.goal_d0 - (car - goal).length()) as f64;
                    e.car_v += st.players[x.carrier].vel.length() as f64;
                    e.def_v += st.players[x.defender].vel.length() as f64;
                    e.gap += x.gap as f64;
                    e.in_range += x.in_range as u32;
                    e.in_reach += (x.min_gap <= 0.0) as u32;
                    e.decisions += x.decisions as f64;
                    let dside = side_of(x.defender);
                    e.still_first += (st.team_view(dside).first.map(|f| f + dside.index() * PLAYERS_PER_TEAM) == Some(x.defender)) as u32;
                    if k.starts_with("beaten") {
                        let l = label_before[x.carrier].split(" (").next().unwrap_or("").to_string();
                        *labels.entry(l).or_default() += 1;
                        if trace {
                            let (c, d) = (&st.players[x.carrier], &st.players[x.defender]);
                            let rel = def - car;
                            println!(
                                "{:7.1} {} beat {} ({k}) after {:.1} s: start rel ({:5.1},{:5.1}) v {:.1}/{:.1}, end rel ({:5.1},{:5.1}) v {:.1}/{:.1} {:?} | {} | {}",
                                st.time, c.name, d.name, st.time - x.t0, x.start.0.x, x.start.0.y, x.start.1, x.start.2, rel.x, rel.y,
                                c.vel.length(), d.vel.length(), d.body, label_before[x.carrier], label_before[x.defender]
                            );
                        }
                    }
                }
            }
            for i in closed.into_iter().rev() {
                open.remove(i);
            }
            let Some(c) = carrier else { continue };
            let dside = side_of(c).opponent();
            let fr = st.frame(dside);
            let car = fr.to_team(st.players[c].pos);
            let line = (goal - car).normalize_or_zero();
            let first = st.team_view(dside).first.map(|f| f + dside.index() * PLAYERS_PER_TEAM);
            for d in st.squad(dside) {
                let rel = fr.to_team(d.pos) - car;
                let set = (all || first == Some(d.id))
                    && !d.is_gk() && d.can_act(st.time) && rel.dot(line) > 1.0 && rel.perp_dot(line).abs() < 1.5 && rel.length() < ENGAGE;
                if set && !open.iter().any(|x| x.carrier == c && x.defender == d.id) {
                    let goal_d0 = (car - goal).length();
                    open.push(Pair {
                        t0: st.time,
                        carrier: c,
                        defender: d.id,
                        goal_d0,
                        tackled: None,
                        gap: take_on_skill(&st.players[c], &sim.cfg.duel) - tackle_skill(d, &sim.cfg.duel),
                        in_range: goal_d0 < sim.cfg.tactics.box01_range,
                        start: (rel, st.players[c].vel.length(), d.vel.length()),
                        min_gap: f32::MAX,
                        decisions: 0,
                    });
                }
            }
        }
    }
    let n = matches as f64;
    let total: u32 = ends.values().map(|e| e.n).sum();
    println!("{matches} matches: {:.1} set 1v1 pairs per match (defender in front within {ENGAGE} m)", total as f64 / n);
    println!(
        "{:<26} {:>7} {:>6} {:>7} {:>8} {:>6} {:>6} {:>7} {:>9} {:>8} {:>8} {:>6}",
        "end", "/match", "share", "dur s", "gained m", "car v", "def v", "gap", "in range", "first", "in reach", "decs"
    );
    for (k, e) in &ends {
        let m = e.n.max(1) as f64;
        println!(
            "{:<26} {:>7.1} {:>5.0}% {:>7.2} {:>8.1} {:>6.1} {:>6.1} {:>7.2} {:>8.0}% {:>7.0}% {:>7.0}% {:>6.1}",
            k, e.n as f64 / n, 100.0 * e.n as f64 / total.max(1) as f64, e.dur / m, e.gained / m, e.car_v / m, e.def_v / m,
            e.gap / m, 100.0 * e.in_range as f64 / m, 100.0 * e.still_first as f64 / m, 100.0 * e.in_reach as f64 / m,
            e.decisions / m
        );
    }
    let get = |k: &str| ends.get(k).map_or(0, |e| e.n) as f64;
    let beaten = get("beaten in a tackle") + get("beaten without a tackle");
    let won = get("tackle won");
    println!(
        "\ntake-ons per team per match: {:.1} successful (defender beaten), {:.1} lost in a tackle, success {:.0} %",
        beaten / n / 2.0,
        won / n / 2.0,
        100.0 * beaten / (beaten + won).max(1.0)
    );
    println!("\ntackles per match by context and outcome:");
    for (k, v) in &tackles {
        println!("  {k:<22} {:.1}", *v as f64 / n);
    }
    let mut l: Vec<_> = labels.into_iter().collect();
    l.sort_by_key(|x| std::cmp::Reverse(x.1));
    let top: Vec<String> = l.iter().take(8).map(|(k, v)| format!("{k} {v}")).collect();
    println!("\ncarrier's decision when he beat the defender: {}", top.join(", "));
}
