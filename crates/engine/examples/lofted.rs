//! Lofted open-play passes to a team-mate: the success probability the on-ball AI gave the pass against what
//! happened, by how far the struck ball came down from where it was aimed. The drop point is where the ball first
//! falls below control height (`player.control_height`) after its apex; its error is split into along the pass
//! (+ = overhit) and across it. Outcome: the intended receiver controls it, another team-mate, an opponent, or the
//! ball goes out of play; with the first contact (header, aerial duel, heavy touch, interception), the receiver's and
//! nearest opponent's distance to the drop point when it dropped, and the target's distance to the nearest line. Also
//! the pure execution error: where the struck ball would first land in free flight, from the target. `TRACE=1`
//! prints passes rated ≥ 0.95 that the receiver did not get.
//! `cargo run -p match_engine --release --example lofted -- [matches]`

use match_engine::Simulation;
use match_engine::Vec2;
use match_engine::ball::predict;
use match_engine::events::EventKind;
use match_engine::match_state::side_of;
use match_engine::pitch::{HALF_LENGTH, HALF_WIDTH};
use match_engine::player::KickKind;

#[derive(Clone, Copy)]
struct Rec {
    p: f32,
    len: f32,
    /// Drop error along / across the pass (m; NaN: never dropped).
    along: f32,
    across: f32,
    /// 0 receiver, 1 other mate, 2 opponent, 3 out of play.
    outcome: u8,
    how: &'static str,
    /// Receiver's / nearest opponent's distance to the drop point when it dropped (NaN: never dropped).
    recv_d: f32,
    opp_d: f32,
    /// Target's distance to the nearest touch or goal line.
    line_d: f32,
    /// Execution error: where the struck ball would first land in free flight, from the target, along / across.
    kick_along: f32,
    kick_across: f32,
    first_time: bool,
}

#[derive(Clone)]
struct Open {
    kicker: usize,
    receiver: usize,
    start: Vec2,
    target: Vec2,
    rec: Rec,
    dropped: bool,
    /// `TRACE=1`: what happened, printed for passes rated ≥ 0.95 that the receiver did not get.
    trace: String,
}

fn play(seed: u64) -> Vec<Rec> {
    let mut sim = Simulation::from_builtin("northbridge", "harbour", seed);
    let control_height = sim.cfg.player.control_height;
    let mut out = Vec::new();
    let mut open: Option<Open> = None;
    let mut seen = 0;
    let tracing = std::env::var("TRACE").is_ok();
    let close = |o: Open, outcome: u8, out: &mut Vec<Rec>, now: String| {
        if tracing && outcome != 0 && o.rec.p >= 0.95 {
            println!("seed {seed} outcome {outcome} {}{} | {now}", o.rec.how, o.trace);
        }
        out.push(Rec { outcome, ..o.rec });
    };
    let at = |st: &match_engine::match_state::MatchState, r: usize| {
        let p = &st.players[r];
        format!(
            "t {:.1} ball ({:.1},{:.1},{:.1}) v ({:.1},{:.1},{:.1}) receiver ({:.1},{:.1}) v ({:.1},{:.1}) intent {:?}",
            st.time, st.ball.pos.x, st.ball.pos.y, st.ball.pos.z, st.ball.vel.x, st.ball.vel.y, st.ball.vel.z, p.pos.x, p.pos.y, p.vel.x, p.vel.y, p.intent
        )
    };
    while !sim.finished() {
        sim.step();
        let st = &sim.state;
        while seen < sim.events.len() {
            let e = &sim.events[seen];
            seen += 1;
            if let Some(o) = open.as_mut()
                && o.rec.how.is_empty()
            {
                let mine = |by: usize| side_of(by) == side_of(o.kicker);
                o.rec.how = match e.kind {
                    EventKind::Header { by } if by == o.receiver => "receiver header",
                    EventKind::Header { by } if mine(by) => "mate header",
                    EventKind::Header { .. } => "opponent header",
                    EventKind::Aerial { won, .. } if mine(won) => "aerial won",
                    EventKind::Aerial { .. } => "aerial lost",
                    EventKind::Miscontrol { by } if by == o.receiver => "receiver heavy touch",
                    EventKind::Miscontrol { by } if mine(by) => "mate heavy touch",
                    EventKind::Miscontrol { .. } => "opponent touch",
                    EventKind::Interception { by } if !mine(by) => "intercepted",
                    _ => "",
                };
            }
            match e.kind {
                EventKind::Pass { by, to, kind, first_time, headed: false, .. } => {
                    if let Some(o) = open.take() {
                        let (now, outcome) = (at(st, o.receiver), if side_of(by) == side_of(o.kicker) { 1 } else { 2 });
                        close(o, outcome, &mut out, now);
                    }
                    if kind == KickKind::LoftedPass
                        && st.is_live()
                        && let (Some(to), Some(f)) = (to, st.flight)
                    {
                        let line_d = (HALF_WIDTH - f.target.y.abs()).min(HALF_LENGTH - f.target.x.abs());
                        let free = predict(&st.ball, &sim.plan, 8.0, 0.01);
                        let land = free
                            .samples
                            .windows(2)
                            .find(|w| w[0].t > 0.3 && w[0].vel.z < 0.0 && w[1].vel.z >= 0.0)
                            .map_or(Vec2::splat(f32::NAN), |w| w[1].pos.truncate());
                        let dir = (f.target - f.start_pos).normalize_or_zero();
                        let ke = land - f.target;
                        let rec = Rec {
                            p: st.players[by].debug.chosen_p,
                            len: (f.target - f.start_pos).length(),
                            along: f32::NAN,
                            across: f32::NAN,
                            outcome: 0,
                            how: "",
                            recv_d: f32::NAN,
                            opp_d: f32::NAN,
                            line_d,
                            kick_along: ke.dot(dir),
                            kick_across: ke.perp_dot(dir),
                            first_time,
                        };
                        let trace = if tracing {
                            format!(
                                "\n  kick: from ({:.1},{:.1}) target ({:.1},{:.1}) line {line_d:.1} m, arrival {:.2} s, kick error along {:+.1} across {:+.1}, free landing ({:.1},{:.1})\n  release: {}",
                                f.start_pos.x,
                                f.start_pos.y,
                                f.target.x,
                                f.target.y,
                                f.arrival_t - f.start_t,
                                rec.kick_along,
                                rec.kick_across,
                                land.x,
                                land.y,
                                at(st, to)
                            )
                        } else {
                            String::new()
                        };
                        open = Some(Open { kicker: by, receiver: to, start: f.start_pos, target: f.target, rec, dropped: false, trace });
                    }
                }
                EventKind::Restart { .. } | EventKind::Kickoff { .. } => {
                    if let Some(o) = open.take() {
                        let now = at(st, o.receiver);
                        close(o, 3, &mut out, now);
                    }
                }
                _ => {}
            }
        }
        if let Some(o) = open.as_mut()
            && !o.dropped
            && st.ball.vel.z < 0.0
            && st.ball.pos.z < control_height
        {
            let dir = (o.target - o.start).normalize_or_zero();
            let b = st.ball.ground_pos();
            let d = b - o.target;
            o.rec.along = d.dot(dir);
            o.rec.across = d.perp_dot(dir);
            o.rec.recv_d = (st.players[o.receiver].pos - b).length();
            o.rec.opp_d = st.squad(st.players[o.kicker].side.opponent()).iter().map(|p| (p.pos - b).length()).fold(99.0, f32::min);
            o.dropped = true;
            if tracing {
                let line = format!("\n  drop: {}", at(st, o.receiver));
                o.trace.push_str(&line);
            }
        }
        if let Some(o) = open.take_if(|o| st.control.is_some_and(|c| c != o.kicker)) {
            let c = st.control.unwrap();
            let now = format!("control by {c}: {}", at(st, o.receiver));
            let outcome = if c == o.receiver { 0 } else if side_of(c) == side_of(o.kicker) { 1 } else { 2 };
            close(o, outcome, &mut out, now);
        }
    }
    out
}

fn main() {
    let n: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(10);
    let recs: Vec<Rec> = std::thread::scope(|s| {
        let hs: Vec<_> = (1..=n).map(|seed| s.spawn(move || play(seed))).collect();
        hs.into_iter().flat_map(|h| h.join().unwrap()).collect()
    });
    let show = |name: String, r: &[&Rec]| {
        if r.is_empty() {
            return;
        }
        let k = r.len() as f32;
        let share = |o: u8| 100.0 * r.iter().filter(|x| x.outcome == o).count() as f32 / k;
        println!(
            "{name:<30} {:6.1}/match  predicted {:3.0}%  receiver {:3.0}%  other mate {:3.0}%  opponent {:3.0}%  out {:3.0}%",
            k / n as f32,
            100.0 * r.iter().map(|x| x.p).sum::<f32>() / k,
            share(0),
            share(1),
            share(2),
            share(3)
        );
    };
    let hows = |r: &[&Rec]| {
        let mut hs: Vec<(&str, usize)> = Vec::new();
        for x in r.iter().filter(|x| x.outcome != 0) {
            let key = if x.how.is_empty() { "no contact" } else { x.how };
            match hs.iter_mut().find(|h| h.0 == key) {
                Some(h) => h.1 += 1,
                None => hs.push((key, 1)),
            }
        }
        hs.sort_by_key(|h| std::cmp::Reverse(h.1));
        let s: Vec<String> = hs.iter().map(|(h, c)| format!("{h} {:.0}%", 100.0 * *c as f32 / r.len().max(1) as f32)).collect();
        println!("{:32}failed: {}", "", s.join(", "));
    };
    println!("{} lofted passes to a team-mate, {n} matches", recs.len());
    let all: Vec<&Rec> = recs.iter().collect();
    show("all".into(), &all);
    hows(&all);
    let err = |x: &Rec| x.along.hypot(x.across);
    println!("by drop error (distance from the target):");
    for (lo, hi) in [(0.0, 2.0), (2.0, 4.0), (4.0, 6.0), (6.0, 99.0)] {
        let r: Vec<&Rec> = recs.iter().filter(|x| (lo..hi).contains(&err(x))).collect();
        show(format!("  {lo:.0}–{hi:.0} m"), &r);
        hows(&r);
    }
    let r: Vec<&Rec> = recs.iter().filter(|x| x.along.is_nan()).collect();
    show("  never dropped".into(), &r);
    println!("by error along the pass (+ = overhit):");
    for (lo, hi) in [(-99.0, -4.0), (-4.0, -2.0), (-2.0, 0.0), (0.0, 2.0), (2.0, 4.0), (4.0, 99.0)] {
        let r: Vec<&Rec> = recs.iter().filter(|x| (lo..hi).contains(&x.along)).collect();
        show(format!("  {lo:+.0}–{hi:+.0} m"), &r);
    }
    println!("drop error < 4 m, by receiver's distance to the drop point when it dropped:");
    for (lo, hi) in [(0.0, 1.0), (1.0, 2.0), (2.0, 4.0), (4.0, 99.0)] {
        let r: Vec<&Rec> = recs.iter().filter(|x| err(x) < 4.0 && (lo..hi).contains(&x.recv_d)).collect();
        show(format!("  {lo:.0}–{hi:.0} m"), &r);
    }
    println!("drop error < 4 m, by nearest opponent's distance to the drop point when it dropped:");
    for (lo, hi) in [(0.0, 2.0), (2.0, 4.0), (4.0, 8.0), (8.0, 99.0)] {
        let r: Vec<&Rec> = recs.iter().filter(|x| err(x) < 4.0 && (lo..hi).contains(&x.opp_d)).collect();
        show(format!("  {lo:.0}–{hi:.0} m"), &r);
        hows(&r);
    }
    println!("by target's distance to the nearest line:");
    for (lo, hi) in [(0.0, 2.0), (2.0, 5.0), (5.0, 10.0), (10.0, 99.0)] {
        let r: Vec<&Rec> = recs.iter().filter(|x| (lo..hi).contains(&x.line_d)).collect();
        show(format!("  {lo:.0}–{hi:.0} m"), &r);
    }
    println!("by predicted p:");
    for (lo, hi) in [(0.0, 0.7), (0.7, 0.85), (0.85, 0.95), (0.95, 1.01)] {
        let r: Vec<&Rec> = recs.iter().filter(|x| (lo..hi).contains(&x.p)).collect();
        show(format!("  p {lo:.2}–{hi:.2}"), &r);
    }
    println!("by length:");
    for (lo, hi) in [(0.0, 30.0), (30.0, 45.0), (45.0, 99.0)] {
        let r: Vec<&Rec> = recs.iter().filter(|x| (lo..hi).contains(&x.len)).collect();
        show(format!("  {lo:.0}–{hi:.0} m"), &r);
    }
    for (name, ft) in [("not first time", false), ("first time", true)] {
        let mut ke: Vec<(f32, f32)> = recs.iter().filter(|x| x.first_time == ft && !x.kick_along.is_nan()).map(|x| (x.kick_along, x.kick_across)).collect();
        if ke.is_empty() {
            continue;
        }
        let k = ke.len() as f32;
        let rms = |f: fn(&(f32, f32)) -> f32| (ke.iter().map(|x| f(x) * f(x)).sum::<f32>() / k).sqrt();
        let (ra, rc) = (rms(|x| x.0), rms(|x| x.1));
        let mean_a = ke.iter().map(|x| x.0).sum::<f32>() / k;
        ke.sort_by(|a, b| a.0.hypot(a.1).total_cmp(&b.0.hypot(b.1)));
        let qk = |f: f32| {
            let x = ke[((k * f) as usize).min(ke.len() - 1)];
            x.0.hypot(x.1)
        };
        println!(
            "kick landing error, {name} ({:.0}/match): rms along {ra:.1} m (mean {mean_a:+.1}), across {rc:.1} m; distance median {:.1}, p90 {:.1}, > 6 m {:.0}%",
            k / n as f32,
            qk(0.5),
            qk(0.9),
            100.0 * ke.iter().filter(|x| x.0.hypot(x.1) > 6.0).count() as f32 / k
        );
    }
    let mut errs: Vec<f32> = recs.iter().filter(|x| !x.along.is_nan()).map(err).collect();
    errs.sort_by(f32::total_cmp);
    let q = |f: f32| errs[((errs.len() as f32 * f) as usize).min(errs.len() - 1)];
    let mean_abs = |f: fn(&Rec) -> f32| {
        let v: Vec<f32> = recs.iter().map(f).filter(|v| !v.is_nan()).map(f32::abs).collect();
        v.iter().sum::<f32>() / v.len() as f32
    };
    println!(
        "drop error median {:.1} m, p75 {:.1}, p90 {:.1}; mean |along| {:.1} m, mean |across| {:.1} m",
        q(0.5),
        q(0.75),
        q(0.9),
        mean_abs(|x| x.along),
        mean_abs(|x| x.across)
    );
}
