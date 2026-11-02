//! First opponent contacts on passes (set W+A, A1/A2): every first touch by an opponent on a team's open-play pass,
//! as the touch itself (controlled, heavy touch or deflection, first-time pass, clearance) and as where the ball went
//! next (the toucher's side controls it, the passer's side gets it back, a restart); blocks of passes and of shots;
//! clearances per team; and (set W+A, C1) how often the first contact is clean by the reach to the ball (how far the
//! ball's path relative to him passes from his centre, in `control_radius`) and the ball's speed relative to him, for
//! these contacts and for the intended receivers of passes; (set K) the receivers' clean share by their own speed
//! towards the ball and the nearest opponent, and passes whose first touch is a team-mate who was not the receiver
//! (a body in the path). StatsBomb (scratchpad `an/sb_miscount.py`): interceptions won clean 39 %, to a team-mate 20 %,
//! to the opponent 22 %, out 18 %; first opponent events on incomplete passes interception 16 / block 26 / recovery
//! 25 / clearance 22 %; per team raw blocks 19.8, clearances 18.1.
//! Usage: contacts [matches]

use std::collections::BTreeMap;

use glam::Vec2;
use match_engine::Simulation;
use match_engine::events::EventKind;
use match_engine::match_state::side_of;
use match_engine::player::KickKind;

fn main() {
    let matches: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(4);
    let mut touch: BTreeMap<&str, u32> = BTreeMap::new();
    let mut next: BTreeMap<(&str, &str), u32> = BTreeMap::new();
    let mut blocks = [0u32; 2];
    let mut clearances = 0u32;
    let mut ft_clear = 0u32;
    // Reach band × relative speed band: contacts by classification.
    let mut geo: BTreeMap<(usize, usize), BTreeMap<&str, u32>> = BTreeMap::new();
    // The same for intended receivers: (clean, all).
    let mut rx_geo: BTreeMap<(usize, usize), (u32, u32)> = BTreeMap::new();
    // Intended receivers by their speed towards the ball (< 1, 1–3, 3–5, ≥ 5 m/s) × nearest opponent (< 3 m, ≥ 3 m):
    // (clean, all).
    let mut rx_close: BTreeMap<(usize, usize), (u32, u32)> = BTreeMap::new();
    // Own passes first touched by a team-mate who was not the receiver: all, within 0.3 s of the release, failed.
    let mut mate_hit = [0u32; 3];
    let mut own_passes = 0u32;
    for seed in 1..=matches {
        let mut sim = Simulation::from_builtin("northbridge", "harbour", seed);
        // `CFG=file.ron`: `SimConfig` overrides, as `match_lab --config`.
        if let Ok(path) = std::env::var("CFG") {
            sim.cfg = match_engine::config::SimConfig::from_ron(&std::fs::read_to_string(path).unwrap()).unwrap();
        }
        let mut seen = 0usize;
        let mut last_shot = false;
        // An open first contact: toucher, its classification, time, (reach band, speed band).
        let mut open: Option<(usize, &'static str, f32, (usize, usize))> = None;
        // The intended receiver of the team's pass in flight, and when it was played.
        let mut rx: Option<usize> = None;
        let mut rx_t0 = 0.0f32;
        while !sim.finished() {
            // The geometry the contact is resolved from: the ball and the players before the step.
            let (bpos, bvel) = (sim.state.ball.pos, sim.state.ball.vel);
            let prev: Vec<(Vec2, Vec2)> = sim.state.players.iter().map(|p| (p.pos, p.vel)).collect();
            let radius = sim.cfg.player.control_radius;
            let bands = |id: usize| {
                let (pos, vel) = prev[id];
                let rel = bvel.truncate() - vel;
                let off = if rel.length() > 0.3 { rel.normalize().perp_dot(bpos.truncate() - pos).abs() } else { 0.0 };
                let speed = (bvel - vel.extend(0.0)).length();
                (((off / radius) * 4.0).min(3.0) as usize, usize::from(speed >= 10.0) + usize::from(speed >= 16.0))
            };
            let close = |id: usize| {
                let (pos, vel) = prev[id];
                let closing = -vel.dot(bvel.truncate().normalize_or_zero());
                let near = prev
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| side_of(*j) != side_of(id))
                    .map(|(_, q)| (q.0 - pos).length())
                    .fold(f32::MAX, f32::min);
                (usize::from(closing >= 1.0) + usize::from(closing >= 3.0) + usize::from(closing >= 5.0), usize::from(near >= 3.0))
            };
            sim.step();
            let now = sim.state.time;
            let evs: Vec<EventKind> = sim.events[seen..].iter().map(|e| e.kind.clone()).collect();
            seen = sim.events.len();
            for e in &evs {
                if let Some(r) = rx {
                    match e {
                        EventKind::Miscontrol { by } if *by == r => {
                            rx_geo.entry(bands(r)).or_default().1 += 1;
                            rx_close.entry(close(r)).or_default().1 += 1;
                            rx = None;
                        }
                        EventKind::Pass { by, headed, .. } if *by == r => {
                            if !*headed {
                                let c = rx_geo.entry(bands(r)).or_default();
                                c.0 += 1;
                                c.1 += 1;
                                let c = rx_close.entry(close(r)).or_default();
                                c.0 += 1;
                                c.1 += 1;
                            }
                            rx = None;
                        }
                        EventKind::Miscontrol { by } | EventKind::Pass { by, .. } if side_of(*by) == side_of(r) => {
                            mate_hit[0] += 1;
                            mate_hit[1] += u32::from(now - rx_t0 < 0.3);
                            mate_hit[2] += u32::from(matches!(e, EventKind::Miscontrol { .. }));
                            rx = None;
                        }
                        EventKind::Interception { .. } | EventKind::Block { .. } | EventKind::Header { .. } | EventKind::Restart { .. } | EventKind::Goal { .. } | EventKind::HalfTime => {
                            rx = None;
                        }
                        _ => {}
                    }
                }
                if let EventKind::Pass { to: Some(r), headed: false, .. } = e
                    && sim.state.is_live()
                {
                    rx = Some(*r);
                    rx_t0 = now;
                    own_passes += 1;
                }
                match e {
                    EventKind::Interception { by } => {
                        open = Some((*by, if sim.state.players[*by].is_gk() { "keeper" } else { "controlled" }, now, bands(*by)));
                    }
                    EventKind::Header { by } if open.is_some_and(|o| o.0 == *by && o.2 == now) => {
                        open.as_mut().unwrap().1 = "header";
                    }
                    EventKind::Miscontrol { by } if open.is_some_and(|o| o.0 == *by && o.2 == now) => {
                        open.as_mut().unwrap().1 = "heavy touch / deflection";
                    }
                    EventKind::Pass { by, kind, first_time, headed, .. } => {
                        if *kind == KickKind::Clearance {
                            clearances += 1;
                            ft_clear += u32::from(*first_time);
                        }
                        if let Some(o) = open.as_mut().filter(|o| o.0 == *by && now - o.2 < 0.05) {
                            o.1 = if *headed {
                                "header"
                            } else if *kind == KickKind::Clearance {
                                "first-time clearance"
                            } else if *first_time {
                                "first-time pass"
                            } else {
                                o.1
                            };
                        }
                        last_shot = false;
                    }
                    EventKind::Shot { .. } => last_shot = true,
                    EventKind::Block { by } => {
                        blocks[usize::from(last_shot)] += 1;
                        if !last_shot {
                            open = Some((*by, "blocked (within reaction)", now, bands(*by)));
                        }
                    }
                    EventKind::Restart { side, .. } => {
                        if let Some((by, how, _, g)) = open.take() {
                            *touch.entry(how).or_default() += 1;
                            *geo.entry(g).or_default().entry(how).or_default() += 1;
                            *next.entry((how, if *side == side_of(by) { "restart, toucher's side" } else { "restart, passer's side" })).or_default() += 1;
                        }
                    }
                    EventKind::Goal { .. } | EventKind::HalfTime => {
                        if let Some((_, how, _, g)) = open.take() {
                            *touch.entry(how).or_default() += 1;
                            *geo.entry(g).or_default().entry(how).or_default() += 1;
                            *next.entry((how, "goal / half")).or_default() += 1;
                        }
                    }
                    _ => {}
                }
            }
            if let Some(r) = rx
                && let Some(c) = sim.state.control
            {
                if c == r {
                    let g = rx_geo.entry(bands(r)).or_default();
                    g.0 += 1;
                    g.1 += 1;
                    let g = rx_close.entry(close(r)).or_default();
                    g.0 += 1;
                    g.1 += 1;
                } else if side_of(c) == side_of(r) {
                    mate_hit[0] += 1;
                    mate_hit[1] += u32::from(now - rx_t0 < 0.3);
                }
                rx = None;
            }
            // Where it went: the first control after the touch (by someone else, or by the toucher after the touch).
            if let Some((by, how, t0, g)) = open
                && let Some(c) = sim.state.control
                && sim.state.is_live()
                && (c != by || (how == "controlled" && now > t0) || now - t0 > 0.3)
                && now > t0
            {
                open = None;
                *touch.entry(how).or_default() += 1;
                *geo.entry(g).or_default().entry(how).or_default() += 1;
                *next.entry((how, if side_of(c) == side_of(by) { "toucher's side" } else { "passer's side" })).or_default() += 1;
            }
        }
    }
    let n = (2 * matches) as f32;
    let total: u32 = touch.values().sum();
    println!("{matches} matches; first opponent touches on passes per team {:.1}", total as f32 / n);
    for (k, v) in &touch {
        let line: Vec<String> = next.iter().filter(|((h, _), _)| h == k).map(|((_, w), c)| format!("{w} {:.0} %", 100.0 * *c as f32 / *v as f32)).collect();
        println!("  {k:<26} {:5.1} per team {:4.0} %   next: {}", *v as f32 / n, 100.0 * *v as f32 / total.max(1) as f32, line.join(", "));
    }
    let all = |w: &str| next.iter().filter(|((_, x), _)| x.starts_with(w)).map(|(_, c)| *c).sum::<u32>();
    println!(
        "  all: next control by the toucher's side {:.0} %, the passer's side {:.0} %, restarts {:.0} % (real won clean + to a mate 59 %, to the opponent 22 %, out 18 %)",
        100.0 * all("toucher") as f32 / total.max(1) as f32,
        100.0 * all("passer") as f32 / total.max(1) as f32,
        100.0 * all("restart") as f32 / total.max(1) as f32
    );
    println!(
        "blocks of passes / shots per team {:.1} / {:.1} (real all 19.8 raw); clearances per team {:.1}, first-time {:.1} (real 18.1 raw)",
        blocks[0] as f32 / n,
        blocks[1] as f32 / n,
        clearances as f32 / n,
        ft_clear as f32 / n
    );
    println!("foot contacts on passes (no header, keeper or block): clean (controlled or first time) / all, by reach × speed relative to him");
    println!("  reach (× control radius)   < 10 m/s          10–16 m/s         ≥ 16 m/s");
    for r in 0..4 {
        let cells: Vec<String> = (0..3)
            .map(|v| {
                let m = geo.get(&(r, v));
                let get = |k: &str| m.and_then(|m| m.get(k)).copied().unwrap_or(0);
                let clean = get("controlled") + get("first-time pass") + get("first-time clearance");
                let all = clean + get("heavy touch / deflection");
                format!("{:5.0} % of {:<5}", 100.0 * clean as f32 / all.max(1) as f32, all)
            })
            .collect();
        println!("  {:.2}–{:.2}                  {}", r as f32 / 4.0, (r + 1) as f32 / 4.0, cells.join("  "));
    }
    println!("intended receivers of team-mates' passes (feet): clean (controlled or first time) / all, by reach × speed");
    println!("  reach (× control radius)   < 10 m/s          10–16 m/s         ≥ 16 m/s");
    for r in 0..4 {
        let cells: Vec<String> = (0..3)
            .map(|v| {
                let (c, a) = rx_geo.get(&(r, v)).copied().unwrap_or((0, 0));
                format!("{:5.0} % of {:<5}", 100.0 * c as f32 / a.max(1) as f32, a)
            })
            .collect();
        println!("  {:.2}–{:.2}                  {}", r as f32 / 4.0, (r + 1) as f32 / 4.0, cells.join("  "));
    }
    println!("intended receivers (feet): clean / all by his own speed towards the ball × nearest opponent");
    println!("  towards the ball     opponent < 3 m      ≥ 3 m");
    for (k, name) in ["< 1 m/s", "1–3 m/s", "3–5 m/s", "≥ 5 m/s"].iter().enumerate() {
        let cells: Vec<String> = (0..2)
            .map(|v| {
                let (c, a) = rx_close.get(&(k, v)).copied().unwrap_or((0, 0));
                format!("{:5.0} % of {:<5}", 100.0 * c as f32 / a.max(1) as f32, a)
            })
            .collect();
        println!("  {name:<18} {}", cells.join("  "));
    }
    println!(
        "own ground passes first touched by a team-mate who was not the receiver: {:.1} per team ({:.2} % of {:.0}), within 0.3 s of the release {:.1}, failed touches {:.1}",
        mate_hit[0] as f32 / n,
        100.0 * mate_hit[0] as f32 / own_passes.max(1) as f32,
        own_passes as f32 / n,
        mate_hit[1] as f32 / n,
        mate_hit[2] as f32 / n
    );
}
