//! Open-play ground passes of 5–40 m to a team-mate, as `sb_intercepts.py` measures them in StatsBomb 360: where the
//! pass ended (the first touch by anyone but the passer, or where it left the pitch), the nearest opponent at release
//! to that point, and for the passes an opponent touched first: how long after release, how far along the line, and
//! who it was (his own distance to the point at release, his distance from the pass line and speed towards the point
//! at release, what he was doing).
//! `cargo run -p match_engine --release --example intercepts -- [matches]`

use match_engine::Simulation;
use match_engine::Vec2;
use match_engine::events::EventKind;
use match_engine::match_state::side_of;
use match_engine::player::{Intent, KickKind};
use std::collections::BTreeMap;
use std::fmt::Write as _;

const BANDS: [(f32, f32); 5] = [(0.0, 2.0), (2.0, 3.0), (3.0, 5.0), (5.0, 8.0), (8.0, 999.0)];

struct Open {
    kicker: usize,
    start: Vec2,
    target: Vec2,
    /// Opponents at release: id, position, velocity, what he was doing.
    opps: Vec<(usize, Vec2, Vec2, String)>,
    release: f32,
    length: f32,
    receiver: usize,
    /// Receiver's distance from the target at release.
    recv0: f32,
    /// Receiver's closest approach to the ball (m) while it was low enough to control, whether he was on the ball's
    /// line (within the control radius of the remaining path) before the ball got there, whether the pass was given
    /// up as a loose ball, whether his first plan was a reachable reception.
    closest: f32,
    on_line_early: bool,
    gave_up: bool,
    planned: bool,
    how: &'static str,
    recv_v: f32,
    p_pred: f32,
    /// Nearest opponent to the passer at release; +1 if the kicker attacks towards +x, else −1.
    near_pass: f32,
    att_x: f32,
    /// Passer's team-frame x at release.
    x0: f32,
}

/// One pass: nearest opponent at release to the end, outcome (0 complete, 1 opponent first, 2 out of play), length,
/// and for an opponent's touch: time since release, fraction along the line, his run (m), his distance from the line
/// at release, his speed towards the point at release, his label.
struct Rec {
    near_end: f32,
    outcome: u8,
    length: f32,
    t: f32,
    u: f32,
    run: f32,
    lat: f32,
    v_to: f32,
    label: String,
    /// Receiver's distance from the point when the opponent touched it, his distance from the target at release, the
    /// point's distance from the intended line (execution error and roll).
    recv: f32,
    recv0: f32,
    off: f32,
    closest: f32,
    on_line_early: bool,
    gave_up: bool,
    planned: bool,
    /// Nearest opponent at release to the line from the passer to where the pass ended.
    lane: f32,
    /// Where the nearest opponent to the line stands along it (0 passer, 1 end point).
    lane_u: f32,
    /// The first toucher at the touch: his distance from the ball and his speed.
    touch_d: f32,
    touch_v: f32,
    /// "through", "first time" or "pass"; the receiver's speed at release.
    how: &'static str,
    recv_v: f32,
    /// The AI's success probability for the pass it chose.
    p_pred: f32,
    /// Distance from the passer to where the pass ended.
    dist: f32,
    /// Nearest opponent at release to where the pass ended: his distance from the line passer → end point.
    near_lat: f32,
    /// Nearest opponent to the passer at release; how far forward the pass ended (team frame); an opponent's touch
    /// after which the passing team had the ball next (within 3 s): StatsBomb logs that as a complete, deflected pass.
    near_pass: f32,
    dx: f32,
    /// Passer's team-frame x at release.
    x0: f32,
    kept: bool,
}

/// Nearest opponent to the segment a–b: his distance from it and his fraction along it.
fn lane_of(opps: &[(usize, Vec2, Vec2, String)], a: Vec2, b: Vec2) -> (f32, f32) {
    let ab = b - a;
    opps.iter()
        .map(|q| (seg_dist(q.1, a, b), ((q.1 - a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0)))
        .fold((f32::MAX, 0.0), |m, x| if x.0 < m.0 { x } else { m })
}

fn seg_dist(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let u = ((p - a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
    (a + ab * u - p).length()
}

fn play(seed: u64) -> Vec<Rec> {
    let mut sim = Simulation::from_builtin("northbridge", "harbour", seed);
    // `CFG=file.ron`: `SimConfig` overrides, as `match_lab --config`.
    if let Ok(path) = std::env::var("CFG") {
        sim.cfg = match_engine::config::SimConfig::from_ron(&std::fs::read_to_string(path).unwrap()).unwrap();
    }
    let mut out: Vec<Rec> = Vec::new();
    let mut open: Option<Open> = None;
    let mut seen = 0;
    // `TRACE=lo`: seed 1, lost passes whose nearest opponent at release to the end was at least lo m away, in the
    // pass frame (x along the pass from the target back towards the passer, y to the side).
    let trace: Option<f32> = std::env::var("TRACE").ok().and_then(|s| s.parse().ok()).filter(|_| seed == 1);
    let mut tr: Vec<String> = Vec::new();
    let mut traced = 0;
    // Passes an opponent touched first, until someone controls the ball: index into `out`, kicker, deadline.
    let mut deflected: Vec<(usize, usize, f32)> = Vec::new();
    while !sim.finished() {
        sim.step();
        if let Some(c) = sim.state.control {
            for &(i, k, _) in &deflected {
                out[i].kept = side_of(c) == side_of(k);
            }
            deflected.clear();
        }
        let now = sim.state.time;
        deflected.retain(|d| d.2 > now && sim.state.is_live());
        let st = &sim.state;
        while seen < sim.events.len() {
            let e = &sim.events[seen];
            seen += 1;
            match e.kind {
                EventKind::Pass { by, to: Some(to), kind: KickKind::GroundPass, length, headed: false, first_time }
                    if st.is_live() && (5.0..40.0).contains(&length) && st.flight.is_some() =>
                {
                    let f = st.flight.unwrap();
                    let side = st.players[by].side;
                    open = Some(Open {
                        kicker: by,
                        start: f.start_pos,
                        target: f.target,
                        opps: st
                            .squad(side.opponent())
                            .iter()
                            .map(|q| (q.id, q.pos, q.vel, q.debug.label.split([' ', ':', '(']).next().unwrap_or("").to_string()))
                            .collect(),
                        release: e.t,
                        length,
                        receiver: to,
                        recv0: (st.players[to].pos - f.target).length(),
                        closest: f32::MAX,
                        on_line_early: false,
                        gave_up: false,
                        planned: false,
                        how: if st.players[by].debug.label.contains("through") {
                            "through"
                        } else if first_time {
                            "first time"
                        } else {
                            "pass"
                        },
                        recv_v: st.players[to].vel.length(),
                        p_pred: st.players[by].debug.chosen_p,
                        near_pass: st.squad(side.opponent()).iter().map(|q| (q.pos - f.start_pos).length()).fold(f32::MAX, f32::min),
                        att_x: (st.frame(side).to_team(f.start_pos + Vec2::X).x - st.frame(side).to_team(f.start_pos).x).signum(),
                        x0: st.frame(side).to_team(f.start_pos).x,
                    });
                }
                EventKind::Restart { .. } | EventKind::Kickoff { .. } | EventKind::Goal { .. } => {
                    if let Some(o) = open.take() {
                        let end = st.ball.ground_pos();
                        let near_end = o.opps.iter().map(|q| (q.1 - end).length()).fold(f32::MAX, f32::min);
                        let (lane, lane_u) = lane_of(&o.opps, o.start, end);
                        out.push(Rec { near_end, outcome: 2, length: o.length, t: 0.0, u: 0.0, run: 0.0, lat: 0.0, v_to: 0.0, label: String::new(), recv: 0.0, recv0: o.recv0, off: 0.0, closest: o.closest, on_line_early: o.on_line_early, gave_up: o.gave_up, planned: o.planned, lane, lane_u, touch_d: 0.0, touch_v: 0.0, how: o.how, recv_v: o.recv_v, p_pred: o.p_pred, dist: (end - o.start).length(), near_lat: 0.0, near_pass: o.near_pass, dx: (end - o.start).x * o.att_x, x0: o.x0, kept: false });
                    }
                }
                _ => {}
            }
        }
        let Some(o) = open.as_mut() else {
            tr.clear();
            continue;
        };
        if trace.is_some() && ((st.time * 50.0).round() as i64) % 5 == 0 {
            let back = (o.start - o.target).normalize_or_zero();
            let fr = |q: Vec2| Vec2::new((q - o.target).dot(back), back.perp_dot(q - o.target));
            let r = &st.players[o.receiver];
            let (plan, pt) = match r.intent {
                Intent::Receive { point, t, meet } => (format!("{} {:5.1},{:4.1} @{:.2}", if meet { "meet" } else { "recv" }, fr(point).x, fr(point).y, t - st.time), true),
                _ => (format!("{:?}", r.intent).chars().take(12).collect(), false),
            };
            let _ = pt;
            let near = st
                .squad(r.side.opponent())
                .iter()
                .min_by(|a, b| (a.pos - st.ball.ground_pos()).length().total_cmp(&(b.pos - st.ball.ground_pos()).length()))
                .unwrap();
            tr.push(format!(
                "  {:7.2} ball {:5.1},{:4.1} z{:.1} v{:4.1}  recv {:5.1},{:4.1} v {:4.1},{:4.1} brake {} {} [{}]  opp {:5.1},{:4.1} {}",
                st.time,
                fr(st.ball.ground_pos()).x,
                fr(st.ball.ground_pos()).y,
                st.ball.pos.z,
                st.ball.vel.truncate().length(),
                fr(r.pos).x,
                fr(r.pos).y,
                r.vel.dot(back),
                back.perp_dot(r.vel),
                r.brake as u8,
                plan,
                r.debug.label,
                fr(near.pos).x,
                fr(near.pos).y,
                near.debug.label
            ));
        }
        if let Some(f) = st.flight {
            let r = &st.players[o.receiver];
            if f.receiver != Some(o.receiver) {
                o.gave_up = true;
            }
            if st.time - o.release < 0.03 {
                o.planned = matches!(r.intent, Intent::Receive { meet: false, .. });
            }
            if st.ball.pos.z < 1.0 {
                o.closest = o.closest.min((r.pos - st.ball.ground_pos()).length());
            }
            // On the remaining path ahead of the ball, more than 0.3 s before it gets there.
            let b = st.ball.ground_pos();
            let v = st.ball.vel.truncate();
            if v.length() > 1.0 {
                let d = v.normalize();
                let along = (r.pos - b).dot(d);
                if along > 0.3 * v.length() && (r.pos - b).perp_dot(d).abs() < 0.75 {
                    o.on_line_early = true;
                }
            }
        }
        let Some(t) = st.last_touch.filter(|&t| t != o.kicker) else { continue };
        let o = open.take().unwrap();
        let end = st.ball.ground_pos();
        let near_end = o.opps.iter().map(|q| (q.1 - end).length()).fold(f32::MAX, f32::min);
        let (lane, lane_u) = lane_of(&o.opps, o.start, end);
        let (touch_d, touch_v) = ((st.players[t].pos - end).length(), st.players[t].vel.length());
        if side_of(t) == side_of(o.kicker) {
            out.push(Rec { near_end, outcome: 0, length: o.length, t: 0.0, u: 0.0, run: 0.0, lat: 0.0, v_to: 0.0, label: String::new(), recv: 0.0, recv0: o.recv0, off: 0.0, closest: o.closest, on_line_early: o.on_line_early, gave_up: o.gave_up, planned: o.planned, lane, lane_u, touch_d, touch_v, how: o.how, recv_v: o.recv_v, p_pred: o.p_pred, dist: (end - o.start).length(), near_lat: 0.0, near_pass: o.near_pass, dx: (end - o.start).x * o.att_x, x0: o.x0, kept: false });
            continue;
        }
        let max_len: f32 = std::env::var("TRLEN").ok().and_then(|s| s.parse().ok()).unwrap_or(99.0);
        let near_miss = std::env::var("TRNEAR").is_err() || (0.75..1.5).contains(&o.closest);
        // `TRU1=1`: only passes whose nearest opponent to the line at release stood within 2 m beyond the end point.
        let u1 = std::env::var("TRU1").is_err() || (lane_u >= 0.999 && lane < 2.0);
        if trace.is_some_and(|lo| near_end >= lo) && o.length < max_len && near_miss && u1 && traced < 12 {
            traced += 1;
            let back = (o.start - o.target).normalize_or_zero();
            let fr = |q: Vec2| Vec2::new((q - o.target).dot(back), back.perp_dot(q - o.target));
            let p0 = o.opps.iter().find(|q| q.0 == t).map_or(Vec2::ZERO, |q| q.1);
            println!(
                "lost pass {:.1} m ({}, p {:.2}), receiver {} at release {:.1} m from the target, taken by {} ({}): at release at {:.1},{:.1}, touched at {:.1},{:.1} (target 0,0; passer at {:.1})",
                o.length, o.how, o.p_pred, o.receiver, o.recv0, t, st.players[t].debug.label, fr(p0).x, fr(p0).y, fr(end).x, fr(end).y, fr(o.start).x
            );
            for l in &tr {
                println!("{l}");
            }
        }
        tr.clear();
        let (_, p0, v0, label) = o.opps.iter().find(|q| q.0 == t).unwrap();
        let near_lat = o
            .opps
            .iter()
            .min_by(|a, b| (a.1 - end).length().total_cmp(&(b.1 - end).length()))
            .map_or(99.0, |q| (q.1 - o.start).perp_dot((end - o.start).normalize_or_zero()).abs());
        let ab = o.target - o.start;
        out.push(Rec {
            near_end,
            outcome: 1,
            length: o.length,
            t: st.time - o.release,
            u: (end - o.start).dot(ab) / ab.length_squared().max(1e-6),
            run: (end - *p0).length(),
            lat: seg_dist(*p0, o.start, o.target),
            v_to: v0.dot((end - *p0).normalize_or_zero()),
            label: label.clone(),
            recv: (st.players[o.receiver].pos - end).length(),
            recv0: o.recv0,
            off: { let d = ab.normalize_or_zero(); (end - o.start).perp_dot(d).abs() },
            closest: o.closest,
            on_line_early: o.on_line_early,
            gave_up: o.gave_up,
            planned: o.planned,
            lane,
            lane_u,
            touch_d,
            touch_v,
            how: o.how,
            recv_v: o.recv_v,
            p_pred: o.p_pred,
            dist: (end - o.start).length(),
            near_lat,
            near_pass: o.near_pass,
            dx: (end - o.start).x * o.att_x,
            x0: o.x0,
            kept: false,
        });
        deflected.push((out.len() - 1, o.kicker, st.time + 3.0));
    }
    out
}

fn median(mut v: Vec<f32>) -> f32 {
    v.sort_by(f32::total_cmp);
    v.get(v.len() / 2).copied().unwrap_or(f32::NAN)
}

fn main() {
    let n: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(10);
    let res: Vec<Rec> = std::thread::scope(|scope| {
        let hs: Vec<_> = (1..=n).map(|seed| scope.spawn(move || play(seed))).collect();
        hs.into_iter().flat_map(|h| h.join().unwrap()).collect()
    });
    let k = res.len().max(1) as f32;
    println!(
        "{n} matches, open-play ground passes 5–40 m: {:.0}/match, completed {:.0}%, opponent first {:.1}%, out {:.1}%",
        k / n as f32,
        100.0 * res.iter().filter(|r| r.outcome == 0).count() as f32 / k,
        100.0 * res.iter().filter(|r| r.outcome == 1).count() as f32 / k,
        100.0 * res.iter().filter(|r| r.outcome == 2).count() as f32 / k
    );
    // Outcome-independent conditioning, as `sb_press_compl.py`: completion (a deflection the passing team kept counts as
    // complete, as StatsBomb logs it) by the nearest opponent to the passer at release × passer-to-end distance.
    let done = |r: &Rec| r.outcome == 0 || r.kept;
    let lens = [(5.0, 10.0), (10.0, 20.0), (20.0, 30.0), (30.0, 40.0), (5.0, 40.0)];
    println!(
        "completion by the nearest opponent to the passer at release (share of passes) x distance to the end 5-10/10-20/20-30/30-40/all m; opponent touches kept {:.1}% of passes:",
        100.0 * res.iter().filter(|r| r.kept).count() as f32 / k
    );
    let cells = |name: String, f: &dyn Fn(&Rec) -> bool| {
        let mut line = format!("  {name:<11}");
        for (lo, hi) in lens {
            let b: Vec<&Rec> = res.iter().filter(|r| f(r) && (lo..hi).contains(&r.dist)).collect();
            let _ = write!(line, " {:5.1}% ({:4.1}%)", 100.0 * b.iter().filter(|r| done(r)).count() as f32 / b.len().max(1) as f32, 100.0 * b.len() as f32 / k);
        }
        println!("{line}");
    };
    for (lo, hi) in [(0.0, 1.5), (1.5, 3.0), (3.0, 5.0), (5.0, 8.0), (8.0, 999.0)] {
        cells(format!("{lo}-{hi} m"), &|r: &Rec| (lo..hi).contains(&r.near_pass));
    }
    cells("fwd".into(), &|r: &Rec| r.dx > 5.0);
    cells("lat".into(), &|r: &Rec| (-3.0..=5.0).contains(&r.dx));
    cells("back".into(), &|r: &Rec| r.dx < -3.0);
    for (lo, hi) in [(0.0, 1.5), (1.5, 3.0), (3.0, 5.0), (5.0, 8.0), (8.0, 999.0)] {
        let b: Vec<&Rec> = res.iter().filter(|r| (lo..hi).contains(&r.near_pass)).collect();
        let (f, o): (Vec<&Rec>, Vec<&Rec>) = b.iter().partition(|r| r.how == "first time");
        let c = |v: &[&Rec]| 100.0 * v.iter().filter(|r| done(r)).count() as f32 / v.len().max(1) as f32;
        println!(
            "  nearest opponent {lo}-{hi} m: first time {:4.1}% of them, completed {:5.1}% (others {:5.1}%)",
            100.0 * f.len() as f32 / b.len().max(1) as f32,
            c(&f),
            c(&o)
        );
    }
    println!("  real (sb_ft_press.py): first time 6.6/8.1/9.6/6.9/3.5%, completed 83.8/88.5/92.4/94.7/97.7% (others 88.2/92.6/95.4/97.3/99.2%)");
    for (name, lo, hi) in [("free (> 5 m)", 5.0, 999.0), ("pressed (< 3 m)", 0.0, 3.0)] {
        let mut line = format!("  {name}, by the AI's p:");
        for (plo, phi) in [(0.0, 0.5), (0.5, 0.7), (0.7, 0.85), (0.85, 0.95), (0.95, 1.01)] {
            let b: Vec<&Rec> = res.iter().filter(|r| (lo..hi).contains(&r.near_pass) && (plo..phi).contains(&r.p_pred)).collect();
            let _ = write!(line, "  p {plo:.2}-{:.2}: {:4.1}% of passes, {:3.0}% compl", phi.min(1.0), 100.0 * b.len() as f32 / k, 100.0 * b.iter().filter(|r| done(r)).count() as f32 / b.len().max(1) as f32);
        }
        println!("{line}");
    }
    // ATT-02: by the passer's third (own / middle / final, 35 m each) and direction.
    for (name, lo, hi) in [("own third", -99.0, -17.5), ("middle third", -17.5, 17.5), ("final third", 17.5, 99.0)] {
        let b: Vec<&Rec> = res.iter().filter(|r| (lo..hi).contains(&r.x0)).collect();
        let c = |v: &[&Rec]| 100.0 * v.iter().filter(|r| done(r)).count() as f32 / v.len().max(1) as f32;
        let mut line = format!("  {name:<13} {:4.1}% of passes, completed {:5.1}% |", 100.0 * b.len() as f32 / k, c(&b));
        for (dn, f) in [("fwd", &(|r: &Rec| r.dx > 5.0) as &dyn Fn(&Rec) -> bool), ("lat", &|r: &Rec| (-3.0..=5.0).contains(&r.dx)), ("back", &|r: &Rec| r.dx < -3.0)] {
            let d: Vec<&Rec> = b.iter().copied().filter(|r| f(r)).collect();
            let pm = d.iter().map(|r| r.p_pred).sum::<f32>() / d.len().max(1) as f32;
            let _ = write!(line, " {dn} {:5.1}% p {:3.0}% ({:4.1}%)", c(&d), 100.0 * pm, 100.0 * d.len() as f32 / b.len().max(1) as f32);
        }
        // What the opponent who got to it first was doing at the release.
        let mut labels: BTreeMap<&str, u32> = BTreeMap::new();
        let lost: Vec<&&Rec> = b.iter().filter(|r| r.outcome == 1).collect();
        for r in &lost {
            *labels.entry(r.label.as_str()).or_default() += 1;
        }
        let mut lv: Vec<(&str, u32)> = labels.into_iter().collect();
        lv.sort_by_key(|e| std::cmp::Reverse(e.1));
        let _ = write!(line, " | taken by");
        for (l, c) in lv.iter().take(5) {
            let _ = write!(line, " {l} {:.0}%", 100.0 * *c as f32 / lost.len().max(1) as f32);
        }
        println!("{line}");
    }
    println!("  real (sb_compl_third.py, 40 matches): own 23.3% 97.4% | fwd 95.7 (48%) lat 98.7 (26%) back 99.0 (27%); middle 54.4% 97.0% | fwd 93.9 (39%) lat 98.5 (27%) back 99.4 (34%); final 22.3% 91.2% | fwd 86.3 (31%) lat 86.6 (25%) back 97.3 (44%)");
    println!("  real (sb_press_compl.py): 0-1.5 m 87.9/89.1/85.3/80.6/87.9 (12.7%), 1.5-3 92.2/93.4/89.9/86.0/92.3 (28.6%), 3-5 95.2/95.7/93.9/90.3/95.1 (27.6%), 5-8 97.5/97.7/96.2/90.5/97.1 (18.0%), 8+ 99.5/99.3/98.9/97.7/99.1 (13.2%); fwd 89.8 (37.2%), lat 94.8 (25.1%), back 98.4 (37.7%)");
    for how in ["pass", "through", "first time"] {
        let b: Vec<&Rec> = res.iter().filter(|r| r.how == how).collect();
        let kb = b.len().max(1) as f32;
        println!(
            "{how:<10} {:5.1}% of passes, completed {:3.0}% (predicted {:3.0}%), opponent first {:4.1}%, out {:4.1}%; receiver from the target at release med {:.1} m (completed {:.1} m), speed med {:.1} m/s",
            100.0 * kb / k,
            100.0 * b.iter().filter(|r| r.outcome == 0).count() as f32 / kb,
            100.0 * b.iter().map(|r| r.p_pred).sum::<f32>() / kb,
            100.0 * b.iter().filter(|r| r.outcome == 1).count() as f32 / kb,
            100.0 * b.iter().filter(|r| r.outcome == 2).count() as f32 / kb,
            median(b.iter().map(|r| r.recv0).collect()),
            median(b.iter().filter(|r| r.outcome == 0).map(|r| r.recv0).collect()),
            median(b.iter().map(|r| r.recv_v).collect())
        );
    }
    println!("opponent first by the distance of his touch from the passer, % of all passes (completed passes of that length):");
    for (lo, hi) in [(5.0, 10.0), (10.0, 15.0), (15.0, 20.0), (20.0, 30.0), (30.0, 40.0)] {
        println!(
            "  {lo}-{hi} m: opponent first {:4.2}% of all, completed passes of that length {:4.1}% of all",
            100.0 * res.iter().filter(|r| r.outcome == 1 && (lo..hi).contains(&r.dist)).count() as f32 / k,
            100.0 * res.iter().filter(|r| r.outcome == 0 && (lo..hi).contains(&r.dist)).count() as f32 / k
        );
    }
    println!("opponent first per match by the distance of his touch from the passer (real ×1.48: 19.6 / 13.9 / 11.6 / 8.6 / 7.9 / 2.3), nearest opponent at release to the touch point (real med 2.8 / 4.3 / 5.9 / 10.0 / 19.8 m for 3–10 / 10–15 / 15–20 / 20–30 / 30–40, off the line 1.4 / 2.3 / 3.3 / 4.7 / 6.8 m):");
    for (lo, hi) in [(0.0, 5.0), (5.0, 10.0), (10.0, 15.0), (15.0, 20.0), (20.0, 30.0), (30.0, 40.0)] {
        let b: Vec<&Rec> = res.iter().filter(|r| r.outcome == 1 && (lo..hi).contains(&r.dist)).collect();
        println!(
            "  {lo}-{hi} m: {:5.1}/match, nearest opponent at release med {:.1} m (< 2 m {:.0}%, > 5 m {:.0}%), off the line med {:.1} m",
            b.len() as f32 / n as f32,
            median(b.iter().map(|r| r.near_end).collect()),
            100.0 * b.iter().filter(|r| r.near_end < 2.0).count() as f32 / b.len().max(1) as f32,
            100.0 * b.iter().filter(|r| r.near_end > 5.0).count() as f32 / b.len().max(1) as f32,
            median(b.iter().map(|r| r.near_lat).collect())
        );
    }
    let lost: Vec<&Rec> = res.iter().filter(|r| r.outcome != 0).collect();
    println!(
        "lost passes by the receiver's closest approach to the low ball, % of all passes: ≤ 0.75 m {:.1}%, 0.75–1.5 m {:.1}%, 1.5–3 m {:.1}%, > 3 m {:.1}%",
        100.0 * lost.iter().filter(|r| r.closest <= 0.75).count() as f32 / k,
        100.0 * lost.iter().filter(|r| (0.75..1.5).contains(&r.closest)).count() as f32 / k,
        100.0 * lost.iter().filter(|r| (1.5..3.0).contains(&r.closest)).count() as f32 / k,
        100.0 * lost.iter().filter(|r| r.closest >= 3.0).count() as f32 / k
    );
    println!("completion by the nearest opponent to the line at release × where he stands along it (real: sb_lane_u.py):");
    for (lo, hi, real) in [(0.0, 1.0, "84/82/72/55/64"), (1.0, 2.0, "93/91/85/72/77"), (2.0, 3.0, "97/96/92/83/89"), (3.0, 5.0, "99/98/98/95/97")] {
        let mut line = format!("  lane {lo}-{hi} m:");
        for (ulo, uhi) in [(0.0, 0.2), (0.2, 0.5), (0.5, 0.8), (0.8, 0.999), (0.999, 1.01)] {
            let b: Vec<&Rec> = res.iter().filter(|r| (lo..hi).contains(&r.lane) && (ulo..uhi).contains(&r.lane_u)).collect();
            let _ = write!(
                line,
                "  u {ulo:.1}–{:.1} {:4.1}% {:3.0}% (p {:3.0}%)",
                uhi.min(1.0),
                100.0 * b.len() as f32 / k,
                100.0 * b.iter().filter(|r| r.outcome == 0).count() as f32 / b.len().max(1) as f32,
                100.0 * b.iter().map(|r| r.p_pred).sum::<f32>() / b.len().max(1) as f32
            );
        }
        println!("{line}  [real compl {real}]");
    }
    println!("how the first toucher met the ball (distance from it, speed), by the nearest opponent to the line at release:");
    for (name, f) in [("lane 1–5 m, u < 0.2", (1.0f32, 5.0f32, 0.0f32, 0.2f32)), ("lane 0–1 m, u < 0.2", (0.0, 1.0, 0.0, 0.2)), ("u = 1 (beyond the end)", (0.0, 5.0, 0.999, 1.01)), ("all", (0.0, 999.0, 0.0, 1.01))] {
        let b: Vec<&Rec> = res.iter().filter(|r| (f.0..f.1).contains(&r.lane) && (f.2..f.3).contains(&r.lane_u) && r.outcome != 2).collect();
        let (opp, mate): (Vec<&Rec>, Vec<&Rec>) = b.iter().partition(|r| r.outcome == 1);
        let desc = |v: &Vec<&Rec>| {
            format!(
                "dist med {:.2} m (> 0.5 m {:.0}%), speed med {:.1} m/s (> 4 m/s {:.0}%)",
                median(v.iter().map(|r| r.touch_d).collect()),
                100.0 * v.iter().filter(|r| r.touch_d > 0.5).count() as f32 / v.len().max(1) as f32,
                median(v.iter().map(|r| r.touch_v).collect()),
                100.0 * v.iter().filter(|r| r.touch_v > 4.0).count() as f32 / v.len().max(1) as f32
            )
        };
        println!("  {name}: opponent {} | receiver {}", desc(&opp), desc(&mate));
    }
    println!("by the receiver's distance from the target at release:");
    for (lo, hi) in [(0.0, 1.0), (1.0, 2.0), (2.0, 4.0), (4.0, 7.0), (7.0, 999.0)] {
        let b: Vec<&Rec> = res.iter().filter(|r| (lo..hi).contains(&r.recv0)).collect();
        let kb = b.len().max(1) as f32;
        println!(
            "  {lo}-{hi} m: {:4.1}% of passes, completed {:3.0}%, opponent first {:4.1}%, out {:4.1}%",
            100.0 * kb / k,
            100.0 * b.iter().filter(|r| r.outcome == 0).count() as f32 / kb,
            100.0 * b.iter().filter(|r| r.outcome == 1).count() as f32 / kb,
            100.0 * b.iter().filter(|r| r.outcome == 2).count() as f32 / kb
        );
    }
    println!("by the nearest opponent to the line at release:");
    for (lo, hi) in [(0.0, 1.0), (1.0, 2.0), (2.0, 3.0), (3.0, 5.0), (5.0, 999.0)] {
        let b: Vec<&Rec> = res.iter().filter(|r| (lo..hi).contains(&r.lane)).collect();
        println!(
            "  {lo}-{hi} m: {:4.1}% of passes, completed {:3.0}%",
            100.0 * b.len() as f32 / k,
            100.0 * b.iter().filter(|r| r.outcome == 0).count() as f32 / b.len().max(1) as f32
        );
    }
    for (lo, hi) in BANDS {
        let b: Vec<&Rec> = res.iter().filter(|r| (lo..hi).contains(&r.near_end)).collect();
        let kb = b.len().max(1) as f32;
        let f: Vec<&&Rec> = b.iter().filter(|r| r.outcome == 1).collect();
        println!(
            "  nearest opponent to the end {lo}-{hi} m: {:4.1}% of passes, completed {:3.0}%, opponent first {:4.1}% of all passes, out {:4.1}% of all",
            100.0 * kb / k,
            100.0 * b.iter().filter(|r| r.outcome == 0).count() as f32 / kb,
            100.0 * f.len() as f32 / k,
            100.0 * b.iter().filter(|r| r.outcome == 2).count() as f32 / k
        );
        if f.is_empty() {
            continue;
        }
        let mut labels: BTreeMap<&str, u32> = BTreeMap::new();
        for r in &f {
            *labels.entry(r.label.as_str()).or_default() += 1;
        }
        let mut lv: Vec<(&str, u32)> = labels.into_iter().collect();
        lv.sort_by_key(|x| std::cmp::Reverse(x.1));
        println!(
            "      opponent first: t med {:.2} s, along the line med {:.2}, length med {:.1} m, his run med {:.1} m, off the line at release med {:.1} m, speed towards it {:.1} m/s | {}",
            median(f.iter().map(|r| r.t).collect()),
            median(f.iter().map(|r| r.u).collect()),
            median(f.iter().map(|r| r.length).collect()),
            median(f.iter().map(|r| r.run).collect()),
            median(f.iter().map(|r| r.lat).collect()),
            median(f.iter().map(|r| r.v_to).collect()),
            lv.iter().take(6).map(|(l, c)| format!("{l} {:.0}%", 100.0 * *c as f32 / f.len() as f32)).collect::<Vec<_>>().join(", ")
        );
        println!(
            "      receiver then {:.1} m from it (at release {:.1} m from the target), point off the line {:.1} m; along < 0.2 {:.0}%, 0.2–0.7 {:.0}%, 0.7–1.0 {:.0}%, past the target {:.0}%",
            median(f.iter().map(|r| r.recv).collect()),
            median(f.iter().map(|r| r.recv0).collect()),
            median(f.iter().map(|r| r.off).collect()),
            100.0 * f.iter().filter(|r| r.u < 0.2).count() as f32 / f.len() as f32,
            100.0 * f.iter().filter(|r| (0.2..0.7).contains(&r.u)).count() as f32 / f.len() as f32,
            100.0 * f.iter().filter(|r| (0.7..1.0).contains(&r.u)).count() as f32 / f.len() as f32,
            100.0 * f.iter().filter(|r| r.u >= 1.0).count() as f32 / f.len() as f32
        );
        let lost: Vec<&&Rec> = b.iter().filter(|r| r.outcome != 0).collect();
        let kl = lost.len().max(1) as f32;
        let ok: Vec<&&Rec> = b.iter().filter(|r| r.outcome == 0).collect();
        println!(
            "      lost passes: receiver planned a free reception {:.0}% (completed ones {:.0}%), on the line early {:.0}% (completed {:.0}%), given up {:.0}%, closest approach med {:.1} m, ≤ 1.5 m {:.0}%",
            100.0 * lost.iter().filter(|r| r.planned).count() as f32 / kl,
            100.0 * ok.iter().filter(|r| r.planned).count() as f32 / ok.len().max(1) as f32,
            100.0 * lost.iter().filter(|r| r.on_line_early).count() as f32 / kl,
            100.0 * ok.iter().filter(|r| r.on_line_early).count() as f32 / ok.len().max(1) as f32,
            100.0 * lost.iter().filter(|r| r.gave_up).count() as f32 / kl,
            median(lost.iter().map(|r| r.closest).collect()),
            100.0 * lost.iter().filter(|r| r.closest <= 1.5).count() as f32 / kl
        );
    }
}
