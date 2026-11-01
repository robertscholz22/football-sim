//! How intended receivers of open-play ground passes move while the ball travels: metres moved towards the passer
//! (along the pass line) between release and the controlling touch, the planned reception point's distance in front
//! of the pass target, his speed at control and the nearest opponent then; by how close the nearest opponent was to
//! the receiver at release. Also the share of passes he did not get first and who got them. Then, as in StatsBomb 360
//! (`sb_tight.py`): ground passes of 5–40 m by the nearest opponent at release to where the pass ended (reception or
//! interception), and the share completed (the receiver's team touched it first, as StatsBomb counts a received but
//! miscontrolled pass as complete; otherwise the passer's team has the ball next). `OUT=1` traces passes that go out,
//! `TRACE=1` prints failed passes to tightly marked receivers in the pass-line frame, `LOFT=1` includes lofted passes.
//! `cargo run -p match_engine --release --example receive -- [matches]`

use match_engine::Simulation;
use match_engine::Vec2;
use match_engine::events::EventKind;
use match_engine::player::{Intent, KickKind};

/// Nearest opponent at release (m), moved towards the passer (m), planned point ahead of the target (m), speed at
/// control (m/s), nearest opponent at control (m); None for the last four when he did not get the ball first.
type Rec = (f32, Option<(f32, f32, f32, f32)>, f32);

/// Pass in flight: receiver, kicker, his position at release, unit vector from the target back to the passer,
/// target, nearest opponent at release, planned point ahead of the target (set on the next tick).
type Open = (usize, usize, Vec2, Vec2, Vec2, f32, Option<f32>, Vec<Vec2>, f32, f32);

/// Ground pass of 5–40 m: nearest opponent at release to where it ended (m), completed, predicted p.
type End = (f32, bool, f32, u8);

fn play(seed: u64) -> (Vec<Rec>, Vec<End>) {
    let mut sim = Simulation::from_builtin("northbridge", "harbour", seed);
    let mut out = Vec::new();
    let mut ends: Vec<End> = Vec::new();
    let mut open: Option<Open> = None;
    let mut seen = 0;
    let mut tr: Vec<String> = Vec::new();
    // First touch on the pass by the receiver's team (StatsBomb: a pass is complete if received, even if miscontrolled).
    let mut first: Option<bool> = None;
    while !sim.finished() {
        sim.step();
        let st = &sim.state;
        while seen < sim.events.len() {
            let e = &sim.events[seen];
            seen += 1;
            if let Some(o) = &open
                && first.is_none()
            {
                match e.kind {
                    EventKind::Interception { by } | EventKind::Miscontrol { by } => {
                        first = Some(match_engine::match_state::side_of(by) == st.players[o.0].side);
                    }
                    _ => {}
                }
            }
            if let EventKind::Pass { by, to: Some(to), kind, length, headed: false, .. } = e.kind
                && (kind == KickKind::GroundPass || std::env::var("LOFT").is_ok() && kind == KickKind::LoftedPass)
                && st.is_live()
                && let Some(f) = st.flight
            {
                if let Some(o) = open.take() {
                    // The next pass came first: a first-time pass by the receiver (or a team-mate) completes it.
                    let mine = match_engine::match_state::side_of(by) == st.players[o.0].side;
                    end_of(&mut ends, &o, first, st.ball.ground_pos(), mine, 2);
                    out.push((o.5, None, -1.0));
                }
                let r = &st.players[to];
                let near = st.squad(r.side.opponent()).iter().map(|o| (o.pos - r.pos).length()).fold(f32::MAX, f32::min);
                let back = (f.start_pos - f.target).normalize_or_zero();
                let opps = st.squad(r.side.opponent()).iter().map(|q| q.pos).collect();
                tr.clear();
                first = None;
                open = Some((to, by, r.pos, back, f.target, near, None, opps, length, st.players[by].debug.chosen_p));
            } else if let EventKind::Shot { by, .. } = e.kind
                && let Some(o) = open.take()
            {
                // A first-time shot (or one after the touch) completes the pass.
                let mine = match_engine::match_state::side_of(by) == st.players[o.0].side;
                end_of(&mut ends, &o, first, st.ball.ground_pos(), mine, 2);
                out.push((o.5, None, -1.0));
            } else if matches!(e.kind, EventKind::Restart { .. } | EventKind::Kickoff { .. })
                && let Some(o) = open.take()
            {
                if std::env::var("OUT").is_ok() && seed <= 2 {
                    let evs: Vec<String> = sim.events[seen.saturating_sub(6)..seen].iter().map(|e| format!("{:.1} {:?}", e.t, e.kind)).collect();
                    println!("OUT len {:.1} target {:?} recv at release {:.1} m from target; events: {}", o.8, o.4, (o.2 - o.4).length(), evs.join(" | "));
                    for l in &tr {
                        println!("{l}");
                    }
                }
                end_of(&mut ends, &o, first, st.ball.ground_pos(), false, 1);
                out.push((o.5, None, -1.0));
            }
        }
        let Some(o) = open.as_mut() else { continue };
        if (std::env::var("OUT").is_ok() || std::env::var("TRACE2").is_ok() && (5.0..8.0).contains(&o.5)) && ((st.time * 50.0).round() as i64) % 5 == 0 {
            let (bx, by) = (o.3, o.3.perp());
            let rel = |p: Vec2| Vec2::new((p - o.4).dot(bx), (p - o.4).dot(by));
            let r = &st.players[o.0];
            let q = st.squad(r.side.opponent()).iter().min_by(|a, b| (a.pos - r.pos).length().total_cmp(&(b.pos - r.pos).length())).unwrap();
            let pt = if let Intent::Receive { point, t, .. } = r.intent { format!("{:.1},{:.1} @{:.2}", rel(point).x, rel(point).y, t - st.time) } else { "-".into() };
            tr.push(format!(
                "  {:.2} ball {:5.1},{:4.1}  recv {:5.1},{:4.1} v {:4.1},{:4.1} plan {pt}  opp {:?} {:5.1},{:4.1} v {:4.1},{:4.1} {}",
                st.time, rel(st.ball.ground_pos()).x, rel(st.ball.ground_pos()).y, rel(r.pos).x, rel(r.pos).y, r.vel.dot(bx), r.vel.dot(by),
                q.role, rel(q.pos).x, rel(q.pos).y, q.vel.dot(bx), q.vel.dot(by), q.debug.label
            ));
        }
        if o.6.is_none()
            && let Intent::Receive { point, .. } = st.players[o.0].intent
        {
            o.6 = Some((point - o.4).dot(o.3));
        }
        if let Some(c) = st.control
            && c != o.1
        {
            let o = open.take().unwrap();
            let last = (st.ball.ground_pos() - o.4).dot(o.3) < 0.3 * o.8;
            if std::env::var("TRACE2").is_ok() && (5.0..8.0).contains(&o.5) && seed == 1 && c != o.0 && last && match_engine::match_state::side_of(c) != st.players[o.0].side {
                println!("pass {:.1} m, taken by {}:", o.8, if match_engine::match_state::side_of(c) == st.players[o.0].side { "mate" } else { "opp" });
                for l in &tr {
                    println!("{l}");
                }
            }
            end_of(&mut ends, &o, first, st.ball.ground_pos(), match_engine::match_state::side_of(c) == st.players[o.0].side, 0);
            if c == o.0 {
                let r = &st.players[c];
                let near = st.squad(r.side.opponent()).iter().map(|q| (q.pos - r.pos).length()).fold(f32::MAX, f32::min);
                out.push((o.5, Some(((r.pos - o.2).dot(o.3), o.6.unwrap_or(f32::NAN), r.vel.length(), near)), -1.0));
            } else {
                if std::env::var("TRACE").is_ok() && o.5 < 3.0 && seed <= 2 {
                    // Pass-line frame: x towards the passer from the target, y to the left of that.
                    let (bx, by) = (o.3, o.3.perp());
                    let rel = |p: Vec2| Vec2::new((p - o.4).dot(bx), (p - o.4).dot(by));
                    let f = sim.state.players[o.1].pos;
                    let r = &st.players[o.0];
                    let opp = &st.players[c];
                    println!(
                        "t {:.1} len {:.1}: receiver at release {:?} now {:?} intent {:?}; {} {:?} {} now {:?}; ball {:?}; passer {:?}",
                        st.time,
                        (f - o.4).length(),
                        rel(o.2),
                        rel(r.pos),
                        r.debug.label,
                        if match_engine::match_state::side_of(c) == r.side { "mate" } else { "opp" },
                        opp.role,
                        opp.debug.label,
                        rel(opp.pos),
                        rel(st.ball.ground_pos()),
                        rel(f),
                    );
                }
                // Fraction of the way from the passer to the target where an opponent (2.0 + frac) or a team-mate
                // (frac) took it.
                let start = sim.state.players[o.1].pos;
                let len = (o.4 - start).length().max(0.1);
                let frac = ((st.ball.ground_pos() - start).dot((o.4 - start) / len) / len).clamp(0.0, 1.5);
                let opp = match_engine::match_state::side_of(c) != st.players[o.0].side;
                out.push((o.5, None, if opp { 2.0 + frac } else { frac }));
            }
        }
    }
    (out, ends)
}

fn end_of(ends: &mut Vec<End>, o: &Open, first: Option<bool>, at: Vec2, completed: bool, how: u8) {
    let completed = first.unwrap_or(completed);
    if (5.0..40.0).contains(&o.8) {
        let near = o.7.iter().map(|q| (*q - at).length()).fold(f32::MAX, f32::min);
        ends.push((near, completed, o.9, how));
    }
}

fn main() {
    let n: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(10);
    let res: Vec<(Vec<Rec>, Vec<End>)> = std::thread::scope(|s| {
        let hs: Vec<_> = (1..=n).map(|seed| s.spawn(move || play(seed))).collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let recs: Vec<Rec> = res.iter().flat_map(|r| r.0.iter().copied()).collect();
    let ends: Vec<End> = res.iter().flat_map(|r| r.1.iter().copied()).collect();
    let med = |mut v: Vec<f32>| {
        v.retain(|x| x.is_finite());
        v.sort_by(f32::total_cmp);
        if v.is_empty() { f32::NAN } else { v[v.len() / 2] }
    };
    println!("{n} matches: intended receivers of open-play ground passes, by nearest opponent at release");
    println!("opponent    /match  not first  moved to ball med  p25  planned ahead med  speed at control  opponent at control");
    for (lo, hi) in [(0.0, 3.0), (3.0, 5.0), (5.0, 8.0), (8.0, 100.0)] {
        let r: Vec<&Rec> = recs.iter().filter(|r| (lo..hi).contains(&r.0)).collect();
        if r.is_empty() {
            continue;
        }
        let got: Vec<(f32, f32, f32, f32)> = r.iter().filter_map(|r| r.1).collect();
        let lost: Vec<f32> = r.iter().filter(|r| r.2 >= 2.0).map(|r| r.2 - 2.0).collect();
        let share = |a: f32, b: f32| 100.0 * lost.iter().filter(|&&f| (a..b).contains(&f)).count() as f32 / r.len() as f32;
        println!(
            "    taken by a team-mate {:.0}%, dead ball / next pass first {:.0}%; by an opponent {:.0}%: in the first 20 % of the path {:.0}%, 20–70 % {:.0}%, last 30 % or beyond {:.0}%",
            100.0 * r.iter().filter(|r| (0.0..2.0).contains(&r.2)).count() as f32 / r.len() as f32,
            100.0 * r.iter().filter(|r| r.1.is_none() && r.2 < 0.0).count() as f32 / r.len() as f32,
            100.0 * lost.len() as f32 / r.len() as f32,
            share(0.0, 0.2),
            share(0.2, 0.7),
            share(0.7, 9.0)
        );
        let mut moved: Vec<f32> = got.iter().map(|g| g.0).collect();
        moved.sort_by(f32::total_cmp);
        println!(
            "{lo:>3.0}–{hi:<3.0} m {:7.1}  {:8.0}%  {:16.1}  {:4.1}  {:16.1}  {:16.1}  {:19.1}",
            r.len() as f32 / n as f32,
            100.0 * (r.len() - got.len()) as f32 / r.len() as f32,
            med(moved.clone()),
            moved.get(moved.len() / 4).copied().unwrap_or(f32::NAN),
            med(got.iter().map(|g| g.1).collect()),
            med(got.iter().map(|g| g.2).collect()),
            med(got.iter().map(|g| g.3).collect()),
        );
    }
    println!("\nground passes 5–40 m: {:.0} per match, completed {:.0}%  [StatsBomb 360: 94 %]", ends.len() as f32 / n as f32, 100.0 * ends.iter().filter(|e| e.1).count() as f32 / ends.len() as f32);
    let real = [(2.3, 53.0), (3.6, 66.0), (14.2, 86.0), (28.5, 95.0), (51.2, 99.0)];
    for ((lo, hi), (rs, rc)) in [(0.0, 2.0), (2.0, 3.0), (3.0, 5.0), (5.0, 8.0), (8.0, 999.0)].into_iter().zip(real) {
        let b: Vec<&End> = ends.iter().filter(|e| (lo..hi).contains(&e.0)).collect();
        println!(
            "  nearest opponent to the end {lo:.0}–{hi:.0} m: {:4.1}% of passes, completed {:3.0}% (predicted {:3.0}%)   [StatsBomb {rs:.1}%, {rc:.0}%]",
            100.0 * b.len() as f32 / ends.len() as f32,
            100.0 * b.iter().filter(|e| e.1).count() as f32 / b.len().max(1) as f32,
            100.0 * b.iter().map(|e| e.2).sum::<f32>() / b.len().max(1) as f32
        );
        let f = |h: u8| 100.0 * b.iter().filter(|e| !e.1 && e.3 == h).count() as f32 / b.len().max(1) as f32;
        println!("      failed: opponent control {:.1}%, out of play {:.1}%, opponent's pass first {:.1}%", f(0), f(1), f(2));
    }
}
