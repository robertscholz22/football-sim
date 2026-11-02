//! Where is the ball won, and what happens to counter-attacks? Records every change of possession (location in the
//! winning team's frame, cause) and every TR-01 counter-attack (start, progress, duration, how it ended).
//! Usage: turnovers [matches] [minutes]

use std::collections::BTreeMap;

use match_engine::Simulation;
use match_engine::ai::brain::{InPhase, TeamPhase};
use match_engine::events::EventKind;
use match_engine::match_state::PlayPhase;
use match_engine::pitch::{HALF_LENGTH, Side};

#[derive(Default)]
struct Counter {
    start_t: f32,
    start_x: f32,
    max_x: f32,
    shot: bool,
}

fn bin(dist_to_goal: f32) -> usize {
    ((dist_to_goal / 15.0) as usize).min(6)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let matches: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(4);
    let minutes: f32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(90.0);

    // Regains in open play, by distance from the goal the winning team attacks (15 m bins).
    let mut regain_bins = [0u32; 7];
    let mut causes: BTreeMap<&str, u32> = BTreeMap::new();
    let mut cause_bins: BTreeMap<&str, [u32; 7]> = BTreeMap::new();
    let mut restarts = 0u32;
    // Possession lengths in open play.
    let mut poss_durations: Vec<f32> = Vec::new();
    // Counter-attacks: start bin → (count, duration sum, distance sum); end reasons.
    let mut ca_bins = [(0u32, 0.0f32, 0.0f32); 7];
    let mut ca_end: BTreeMap<&str, u32> = BTreeMap::new();

    for seed in 1..=matches {
        let mut sim = Simulation::from_builtin("northbridge", "harbour", seed);
        sim.cfg.timing.half_length_s = minutes * 30.0;
        sim.state.half_length = sim.cfg.timing.half_length_s;
        let mut poss = sim.state.possession;
        let mut poss_since = 0.0f32;
        let mut seen_events = 0usize;
        let mut cause = "start";
        let mut restart_since_change = true;
        let mut counters: [Option<Counter>; 2] = [None, None];
        let mut prev_control = None;
        while !sim.finished() {
            sim.step();
            let now = sim.state.time;
            let mut shot_by: Option<Side> = None;
            for e in &sim.events[seen_events..] {
                match &e.kind {
                    EventKind::Pass { to: None, .. } => cause = "clearance / ball into space",
                    EventKind::Pass { .. } => cause = "other",
                    EventKind::Tackle { .. } => cause = "tackle",
                    EventKind::Interception { .. } => cause = "interception",
                    EventKind::Miscontrol { .. } => cause = "heavy touch / loose",
                    EventKind::Save { .. } => cause = "keeper save",
                    EventKind::Header { .. } => cause = "header",
                    EventKind::Shot { by, .. } => {
                        cause = "after shot";
                        shot_by = Some(sim.state.players[*by].side);
                    }
                    EventKind::Restart { .. } | EventKind::Kickoff { .. } | EventKind::Goal { .. } => {
                        restart_since_change = true
                    }
                    _ => {}
                }
            }
            let quiet_tick = seen_events == sim.events.len();
            seen_events = sim.events.len();
            // The carrier lost the ball without any event: his dribble touch ran away from him (leash) or he
            // was pushed off it.
            if quiet_tick && prev_control.is_some() && sim.state.control.is_none() && cause == "other" {
                cause = "carrier lost own dribble";
            }

            if sim.state.possession != poss {
                let side = sim.state.possession;
                if restart_since_change || sim.state.phase != PlayPhase::Live {
                    restarts += 1;
                } else {
                    let x = sim.state.ball_in(side).x;
                    regain_bins[bin(HALF_LENGTH - x)] += 1;
                    if cause == "other" {
                        cause = if prev_control.is_some() { "nicked off a dribble" } else { "loose ball picked up" };
                    }
                    *causes.entry(cause).or_default() += 1;
                    cause_bins.entry(cause).or_default()[bin(HALF_LENGTH - x)] += 1;
                    poss_durations.push(now - poss_since);
                }
                poss = side;
                poss_since = now;
                restart_since_change = false;
                cause = "other";
            }
            prev_control = sim.state.control;

            for side in [Side::Home, Side::Away] {
                let si = side.index();
                let in_ca = sim.state.team_view(side).phase == TeamPhase::In(InPhase::CounterAttack);
                let x = sim.state.ball_in(side).x;
                if let Some(c) = counters[si].as_mut() {
                    c.max_x = c.max_x.max(x);
                    if shot_by == Some(side) {
                        c.shot = true;
                    }
                }
                match (counters[si].is_some(), in_ca) {
                    (false, true) => counters[si] = Some(Counter { start_t: now, start_x: x, max_x: x, shot: false }),
                    (true, false) => {
                        let c = counters[si].take().unwrap();
                        let reason = if c.shot {
                            "shot"
                        } else if sim.state.possession != side {
                            "ball lost"
                        } else if sim.state.phase != PlayPhase::Live {
                            "ball out of play"
                        } else {
                            "slowed down (kept the ball)"
                        };
                        *ca_end.entry(reason).or_default() += 1;
                        let b = &mut ca_bins[bin(HALF_LENGTH - c.start_x)];
                        b.0 += 1;
                        b.1 += now - c.start_t;
                        b.2 += c.max_x - c.start_x;
                    }
                    _ => {}
                }
            }
        }
    }

    let per = |n: u32| n as f32 / matches as f32;
    let total: u32 = regain_bins.iter().sum();
    println!("open-play regains per match: {:.1} (+ {:.1} possessions from restarts)", per(total), per(restarts));
    println!("\nwhere the ball is won (distance from the goal the winning team attacks):");
    println!("  band          per match   share   cumulative   counters  mean dur  mean dist");
    let mut cum = 0u32;
    for (i, &n) in regain_bins.iter().enumerate() {
        cum += n;
        let lo = i as f32 * 15.0;
        let band = if i == 6 { format!("{lo:>3.0}+ m     ") } else { format!("{lo:>3.0}–{:<3.0} m   ", lo + 15.0) };
        let (cn, cd, cx) = ca_bins[i];
        let ca = if cn > 0 {
            format!("{:>8.1}  {:>6.1} s  {:>6.1} m", per(cn), cd / cn as f32, cx / cn as f32)
        } else {
            String::new()
        };
        println!(
            "  {band}  {:>8.1}   {:>4.0} %   {:>6.0} %   {ca}",
            per(n),
            100.0 * n as f32 / total.max(1) as f32,
            100.0 * cum as f32 / total.max(1) as f32
        );
    }
    println!("\nhow the ball is won (open play):");
    let mut cv: Vec<_> = causes.into_iter().collect();
    cv.sort_by_key(|&(_, n)| std::cmp::Reverse(n));
    for (c, n) in cv {
        println!("  {c:<22} {:>6.1} per match  ({:.0} %)", per(n), 100.0 * n as f32 / total.max(1) as f32);
    }
    println!("\nby band (0–15 … 90+ m from the goal attacked), per match:");
    for (c, b) in &cause_bins {
        println!("  {c:<26} {}", b.iter().map(|&n| format!("{:>5.1}", per(n))).collect::<Vec<_>>().join(" "));
    }
    poss_durations.sort_by(f32::total_cmp);
    let q = |f: f32| poss_durations.get(((poss_durations.len() as f32 - 1.0) * f) as usize).copied().unwrap_or(0.0);
    println!(
        "\nopen-play possession length: median {:.1} s, quartiles {:.1} / {:.1} s, 90th pct {:.1} s",
        q(0.5),
        q(0.25),
        q(0.75),
        q(0.9)
    );
    let ca_total: u32 = ca_end.values().sum();
    println!("\ncounter-attacks: {:.1} per match; how they end:", per(ca_total));
    for (r, n) in ca_end {
        println!("  {r:<28} {:>4.0} %", 100.0 * n as f32 / ca_total.max(1) as f32);
    }
}
