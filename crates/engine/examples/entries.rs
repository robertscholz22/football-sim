//! Final-third stays: how often a team gets the ball under control in the final third (≥ 70 m from its own goal) and
//! how the stay ends: a shot, the opponents win the ball, the ball back under control in the middle third, a restart
//! (to either side), the end of a half. Per stay: duration and passes played from the final third; per entry: by a
//! pass (a team-mate controls it there), a carry (the carrier crosses the line) or a regain there. StatsBomb
//! counterpart: `an/sb_entries.py` (scratchpad).
//! Usage: entries [matches]

use std::collections::BTreeMap;

use match_engine::Simulation;
use match_engine::events::EventKind;
use match_engine::pitch::{PENALTY_AREA_HALF_WIDTH, Side};

const LINE: f32 = 17.5;

struct Stay {
    side: Side,
    t0: f32,
    passes: u32,
    how: &'static str,
    wide: bool,
}

fn main() {
    let matches: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(4);
    let mut ends: BTreeMap<&str, (u32, f32, u32)> = BTreeMap::new();
    let mut hows: BTreeMap<&str, u32> = BTreeMap::new();
    let mut ft_passes = 0u32;
    let mut all_passes = 0u32;
    // Passes from the middle third into the final third by the target's lane (central, wide): attempts, completed.
    let mut into = [[0u32; 2]; 2];
    let mut lanes = [0u32; 2];
    // Middle-third open-play passes: team-mates / opponents in the ball-side wide lane 5+ m ahead of the ball, and in
    // the ball-side wide final third (as `sb_wide_avail.py`); passes with any team-mate there.
    let mut avail = [0u32; 6];
    // … and the most advanced ball-side wide team-mate relative to the last line (highest outfield opponent).
    let mut rel: Vec<f32> = Vec::new();
    let mut no_wide = 0u32;
    for seed in 1..=matches {
        let mut sim = Simulation::from_builtin("northbridge", "harbour", seed);
        let mut seen = 0usize;
        let mut poss: Option<Side> = None;
        let mut holder: Option<usize> = None;
        let mut stay: Option<Stay> = None;
        let mut pending: Option<(Side, usize)> = None;
        let mut end = |stay: &mut Option<Stay>, why: &'static str, now: f32| {
            if let Some(s) = stay.take() {
                let e = ends.entry(why).or_default();
                e.0 += 1;
                e.1 += now - s.t0;
                e.2 += s.passes;
                *hows.entry(s.how).or_default() += 1;
                lanes[s.wide as usize] += 1;
            }
        };
        while !sim.finished() {
            sim.step();
            let now = sim.state.time;
            for e in &sim.events[seen..] {
                let side = |p: usize| sim.state.players[p].side;
                match &e.kind {
                    EventKind::Pass { by, .. } => {
                        all_passes += 1;
                        if let Some(f) = sim.state.flight.filter(|_| sim.state.is_live()) {
                            let fr = sim.state.frame(side(*by));
                            let (a, b) = (fr.to_team(f.start_pos), fr.to_team(f.target));
                            if a.x >= -LINE && a.x < LINE {
                                let wide = |y: f32| y.signum() == a.y.signum() && y.abs() > PENALTY_AREA_HALF_WIDTH;
                                let mut cnt = [0u32; 4];
                                for q in sim.state.players.iter().filter(|q| q.id != *by && !q.is_gk()) {
                                    let t = fr.to_team(q.pos);
                                    if wide(t.y) && t.x >= a.x + 5.0 {
                                        let mate = usize::from(q.side != side(*by));
                                        cnt[mate] += 1;
                                        if t.x >= LINE {
                                            cnt[2 + mate] += 1;
                                        }
                                    }
                                }
                                let last = sim.state.players.iter().filter(|q| q.side != side(*by) && !q.is_gk()).map(|q| fr.to_team(q.pos).x).fold(f32::MIN, f32::max);
                                let front = sim
                                    .state
                                    .players
                                    .iter()
                                    .filter(|q| q.side == side(*by) && q.id != *by && !q.is_gk() && wide(fr.to_team(q.pos).y))
                                    .map(|q| fr.to_team(q.pos).x)
                                    .fold(f32::MIN, f32::max);
                                if front > f32::MIN {
                                    rel.push(front - last);
                                } else {
                                    no_wide += 1;
                                }
                                avail[0] += 1;
                                avail[1] += cnt[0];
                                avail[2] += cnt[2];
                                avail[3] += cnt[1];
                                avail[4] += u32::from(cnt[0] > 0);
                                avail[5] += u32::from(cnt[2] > 0);
                            }
                            if a.x >= -LINE && a.x < LINE && b.x >= LINE {
                                let lane = usize::from(b.y.abs() > PENALTY_AREA_HALF_WIDTH);
                                into[lane][0] += 1;
                                pending = Some((side(*by), lane));
                            }
                        }
                        if let Some(s) = stay.as_mut().filter(|s| s.side == side(*by)) {
                            s.passes += 1;
                            ft_passes += 1;
                        }
                    }
                    EventKind::Shot { by, .. } if stay.as_ref().is_some_and(|s| s.side == side(*by)) => end(&mut stay, "shot", now),
                    EventKind::Restart { side: to, .. } => {
                        pending = None;
                        let why = if stay.as_ref().is_some_and(|s| s.side == *to) { "restart, kept" } else { "restart, lost" };
                        end(&mut stay, why, now);
                        poss = Some(*to);
                        holder = None;
                    }
                    EventKind::Goal { .. } | EventKind::HalfTime => {
                        end(&mut stay, "goal / half", now);
                        poss = None;
                        holder = None;
                    }
                    _ => {}
                }
            }
            seen = sim.events.len();
            if !sim.state.is_live() {
                continue;
            }
            let Some(c) = sim.state.control else { continue };
            let s = sim.state.players[c].side;
            if let Some((ps, lane)) = pending.take() {
                into[lane][1] += u32::from(ps == s);
            }
            let x = sim.state.frame(s).to_team(sim.state.ball.ground_pos()).x;
            let regain = poss != Some(s);
            if regain {
                end(&mut stay, "lost", now);
                poss = Some(s);
            }
            if x >= LINE && stay.is_none() {
                let how = if regain || holder.is_none() {
                    "regain there"
                } else if holder == Some(c) {
                    "carry"
                } else {
                    "pass"
                };
                let wide = sim.state.ball.pos.y.abs() > PENALTY_AREA_HALF_WIDTH;
                stay = Some(Stay { side: s, t0: now, passes: 0, how, wide });
            } else if x < LINE - 1.0 && stay.is_some() {
                end(&mut stay, "back to the middle third", now);
            }
            holder = Some(c);
        }
    }
    let n = (2 * matches) as f32;
    let total: u32 = ends.values().map(|e| e.0).sum();
    println!("{matches} matches; final-third stays per team-match {:.1}; passes from the final third {:.1} per team ({:.1} % of all)", total as f32 / n, ft_passes as f32 / n, 100.0 * ft_passes as f32 / all_passes.max(1) as f32);
    println!("entered by:");
    for (k, v) in &hows {
        println!("  {k:<26} {:5.1} per team  {:4.0} %", *v as f32 / n, 100.0 * *v as f32 / total.max(1) as f32);
    }
    println!("ended by:                     per team  share   mean s  passes per stay");
    for (k, (c, t, p)) in &ends {
        println!("  {k:<26} {:5.1}  {:4.0} %  {:6.1}  {:5.2}", *c as f32 / n, 100.0 * *c as f32 / total.max(1) as f32, t / (*c).max(1) as f32, *p as f32 / (*c).max(1) as f32);
    }
    println!("entries central / wide: {:.1} / {:.1} per team", lanes[0] as f32 / n, lanes[1] as f32 / n);
    for (lane, name) in [(0, "central"), (1, "wide")] {
        println!("  passes from the middle third into the final third, {name}: {:.1} per team, completed {:.0} %", into[lane][0] as f32 / n, 100.0 * into[lane][1] as f32 / into[lane][0].max(1) as f32);
    }
    let m = avail[0].max(1) as f32;
    println!(
        "middle-third passes: ball-side wide lane 5+ m ahead: mates {:.2} (any {:.0} %), opponents {:.2}; wide final third: mates {:.2} (any {:.0} %)",
        avail[1] as f32 / m,
        100.0 * avail[4] as f32 / m,
        avail[3] as f32 / m,
        avail[2] as f32 / m,
        100.0 * avail[5] as f32 / m
    );
    rel.sort_by(f32::total_cmp);
    let q = |f: f32| rel.get((f * (rel.len().max(1) - 1) as f32) as usize).copied().unwrap_or(f32::NAN);
    println!(
        "  no ball-side wide mate {:.0} %; most advanced one minus the last line p25 {:.1} median {:.1} p75 {:.1} m; within 4.4 m of it or beyond {:.0} % (real 13 %; -12.8 / -7.2 / -3.2 m; 28 %)",
        100.0 * no_wide as f32 / m,
        q(0.25),
        q(0.5),
        q(0.75),
        100.0 * rel.iter().filter(|&&v| v > -4.4).count() as f32 / m
    );
    let (t, p): (f32, u32) = ends.values().fold((0.0, 0), |a, e| (a.0 + e.1, a.1 + e.2));
    println!("  all stays: mean {:.1} s, {:.2} passes; time in the final third per team-match {:.1} min", t / total.max(1) as f32, p as f32 / total.max(1) as f32, t / n / 60.0);
}
