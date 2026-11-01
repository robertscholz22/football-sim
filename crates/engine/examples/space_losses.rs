//! Losses in space: controlled receptions of ground passes with no opponent within 5 m at the control, followed
//! until the receiver's next pass or shot (kept) or the loss (tackled or poked loose, a heavy touch after his dribble
//! ran away from him, an opponent got it, the opponents' restart). For the losses: how long he had the ball, how near
//! the nearest opponent was at his last decision, what that decision was, why the tackler committed and from where,
//! and how the dribble touch before a lost ball ran. Env `TRACE=n`: the first n losses at 8+ m, every 0.1 s.
//! `cargo run -p match_engine --release --example space_losses -- [matches]`

use glam::Vec2;
use match_engine::Simulation;
use match_engine::duel::TackleOutcome;
use match_engine::events::EventKind;
use match_engine::match_state::{MatchState, side_of};
use match_engine::player::{Intent, KickKind};

#[derive(Clone, Default)]
struct Track {
    r: usize,
    t0: f32,
    near0: f32,
    decisions: u32,
    last_dec: f32,
    /// Nearest opponent at his last decision, and what it was (`category`).
    dec_near: f32,
    dec_kind: &'static str,
    /// Softmax probability of the chosen option at his last decision, and of the likeliest one.
    dec_prob: f32,
    dec_top: f32,
    /// The ball left his control (his dribble ran beyond the leash): gap to him and ball speed relative to him then.
    loose: Option<(f32, f32, f32)>,
    /// Last dribble touch: ball speed minus his speed along it, time.
    touch: Option<(f32, f32)>,
    trace: Vec<String>,
}

#[derive(Clone, Default)]
struct End {
    near0: f32,
    dt: f32,
    /// 0 kept, 1 lost, 2 other.
    res: u8,
    /// 0 tackled, 1 heavy touch, 2 opponent got it, 3 out.
    how: u8,
    decisions: u32,
    dec_near: f32,
    dec_kind: &'static str,
    dec_prob: f32,
    dec_top: f32,
    since_dec: f32,
    reason: Option<usize>,
    /// Tackler in front of / beside / behind the carrier's motion; ball nearer the tackler; carrier's speed.
    rel: u8,
    nearer_def: bool,
    car_speed: f32,
    loose: Option<(f32, f32, f32)>,
    touch: Option<(f32, f32)>,
    /// Out: distance of the ball from the nearer touch / goal line at his last decision.
    line_d: f32,
}

fn category(label: &str) -> &'static str {
    let l = label.trim_start_matches("set up → ");
    [("hold", "hold"), ("dribble", "dribble"), ("take on", "take on"), ("pass", "pass"), ("lofted", "pass"), ("through", "pass"), ("shot", "shot"), ("clear", "clear")]
        .into_iter()
        .find(|k| l.starts_with(k.0))
        .map_or("other", |k| k.1)
}

fn nearest(st: &MatchState, r: usize) -> (f32, usize) {
    let p = &st.players[r];
    st.squad(p.side.opponent()).iter().map(|o| ((o.pos - p.pos).length(), o.id)).min_by(|a, b| a.0.total_cmp(&b.0)).unwrap_or((99.0, 0))
}

type Blind = [[u32; 4]; 3];
/// One match: the receptions followed, the loss traces, carrying time by gap and overdue decision, fast carrying.
type Run = (Vec<End>, Vec<String>, Blind, [u32; 3]);

fn play(seed: u64, trace_n: usize) -> Run {
    let mut blind = [[0u32; 4]; 3];
    let mut fast = [0u32; 3];
    let mut sim = Simulation::from_builtin("northbridge", "harbour", seed);
    sim.decision_log = Some(Vec::new());
    let mut out = Vec::new();
    let mut traces = Vec::new();
    let mut seen = 0;
    let mut pending: Option<usize> = None;
    let mut tr: Option<Track> = None;
    let mut prev_ctl: Option<usize> = None;
    let mut prev_ball_v = Vec2::ZERO;
    let mut step_n = 0u64;
    while !sim.finished() {
        let reasons_before = [sim.stats.teams[0].tackle_reasons, sim.stats.teams[1].tackle_reasons];
        let snap: Vec<(Vec2, Vec2)> = sim.state.players.iter().map(|p| (p.pos, p.vel)).collect();
        let ball_before = sim.state.ball.ground_pos();
        sim.step();
        step_n += 1;
        if let Some(l) = &mut sim.decision_log
            && l.len() > 400
        {
            l.drain(..300);
        }
        let st = &sim.state;
        let now = st.time;
        let mut end: Option<End> = None;
        while seen < sim.events.len() {
            let e = sim.events[seen].clone();
            seen += 1;
            let Some(t) = &tr else {
                if let EventKind::Pass { to, kind, headed, .. } = e.kind {
                    pending = to.filter(|_| !headed && kind == KickKind::GroundPass && st.is_live());
                } else if matches!(e.kind, EventKind::Shot { .. } | EventKind::Restart { .. } | EventKind::Goal { .. } | EventKind::HalfTime) {
                    pending = None;
                }
                continue;
            };
            let base = End {
                near0: t.near0,
                dt: now - t.t0,
                decisions: t.decisions,
                dec_near: t.dec_near,
                dec_kind: t.dec_kind,
                dec_prob: t.dec_prob,
                dec_top: t.dec_top,
                since_dec: now - t.last_dec,
                loose: t.loose,
                touch: t.touch,
                ..Default::default()
            };
            match e.kind {
                EventKind::Pass { by, to, kind, headed, .. } => {
                    if by == t.r {
                        end = Some(End { res: 0, ..base });
                    }
                    pending = to.filter(|_| !headed && kind == KickKind::GroundPass && st.is_live());
                }
                EventKind::Shot { by, .. } if by == t.r => end = Some(End { res: 0, ..base }),
                EventKind::Tackle { by, on, outcome } if on == t.r && outcome != TackleOutcome::Beaten => {
                    let k = st.players[by].side.index();
                    let reason = (0..6).find(|&i| sim.stats.teams[k].tackle_reasons[i] > reasons_before[k][i]);
                    let (cp, cv) = snap[on];
                    let (dp, _) = snap[by];
                    let mv = cv.normalize_or(Vec2::X);
                    let cos = (dp - cp).normalize_or(mv).dot(mv);
                    let rel = if cos > 0.5 { 0 } else if cos < -0.5 { 2 } else { 1 };
                    let nearer_def = (ball_before - dp).length() < (ball_before - cp).length();
                    end = Some(End { res: 1, how: 0, reason, rel, nearer_def, car_speed: cv.length(), ..base });
                }
                EventKind::Miscontrol { by } if by == t.r => end = Some(End { res: 1, how: 1, ..base }),
                EventKind::Restart { side, .. } => {
                    let lost = side != st.players[t.r].side;
                    end = Some(End { res: if lost { 1 } else { 2 }, how: 3, ..base });
                    pending = None;
                }
                EventKind::Goal { .. } | EventKind::HalfTime => {
                    end = Some(End { res: 2, ..base });
                    pending = None;
                }
                _ => {}
            }
            if end.is_some() {
                break;
            }
        }
        if end.is_none()
            && let Some(t) = &tr
        {
            if let Some(c) = st.control
                && c != t.r
            {
                end = Some(End {
                    near0: t.near0,
                    dt: now - t.t0,
                    res: if side_of(c) != st.players[t.r].side { 1 } else { 2 },
                    how: 2,
                    decisions: t.decisions,
                    dec_near: t.dec_near,
                    dec_kind: t.dec_kind,
                    dec_prob: t.dec_prob,
                    dec_top: t.dec_top,
                    since_dec: now - t.last_dec,
                    loose: t.loose,
                    touch: t.touch,
                    ..Default::default()
                });
            } else if now - t.t0 > 20.0 {
                end = Some(End { res: 2, ..Default::default() });
            }
        }
        if let Some(mut e) = end {
            let t = tr.take().unwrap();
            if e.res == 1 && e.how == 3 {
                // Ball position at his last decision is in the trace; use his position now as a proxy.
                let fr = st.frame(st.players[t.r].side);
                let q = fr.to_team(st.ball.ground_pos());
                e.line_d = (34.0 - q.y.abs()).min(52.5 - q.x.abs());
            }
            if e.res == 1 && t.near0 >= 8.0 && traces.len() < trace_n {
                let how = ["tackled", "heavy touch", "opponent got it", "out"][e.how as usize];
                let reason = e.reason.map_or("", |r| ["BOX-01 shot", "IND-03 last ditch", "loose touch", "odds, ball nearer him", "odds, controlled", "aggression"][r]);
                traces.push(format!("seed {seed} t {:.1}: player {} near0 {:.1} m, lost after {:.2} s: {how} {reason}\n{}", t.t0, t.r, t.near0, e.dt, t.trace.join("\n")));
            }
            out.push(e);
        }
        // Start following a controlled reception.
        if tr.is_none()
            && let (Some(c), Some(r)) = (st.control, pending)
        {
            if c == r {
                let (near0, _) = nearest(st, r);
                if near0 >= 5.0 {
                    let p = &st.players[r];
                    tr = Some(Track { r, t0: now, near0, last_dec: p.last_decision, dec_near: near0, dec_kind: "settling", ..Default::default() });
                }
            }
            pending = None;
        }
        if let Some(t) = &mut tr {
            let p = &st.players[t.r];
            if p.last_decision > t.last_dec && p.last_decision >= t.t0 {
                t.decisions += 1;
                t.last_dec = p.last_decision;
                t.dec_near = nearest(st, t.r).0;
                t.dec_kind = category(&p.debug.label);
                if let Some((_, _, opts, chosen)) = sim.decision_log.as_ref().and_then(|l| l.iter().rev().find(|d| d.1.carrier == t.r)) {
                    t.dec_prob = opts[*chosen].prob;
                    t.dec_top = opts.iter().map(|o| o.prob).fold(0.0, f32::max);
                }
                // The options at this decision: the four likeliest and the best pass by layer-1 utility.
                if t.near0 >= 8.0
                    && let Some((_, _, opts, chosen)) = sim.decision_log.as_ref().and_then(|l| l.iter().rev().find(|d| d.1.carrier == t.r))
                {
                    let mut o: Vec<_> = opts.iter().collect();
                    o.sort_by(|a, b| b.prob.total_cmp(&a.prob));
                    let best_pass = opts.iter().filter(|e| matches!(e.kind, match_engine::ai::personality::OptionKind::Pass { .. })).max_by(|a, b| a.u_opt.total_cmp(&b.u_opt));
                    let line: Vec<String> = o.iter().take(4).chain(best_pass.iter()).map(|e| format!("{} p {:.2} u {:+.4} pers {:+.4} prob {:.2}", e.label, e.p, e.u_opt, e.pers, e.prob)).collect();
                    t.trace.push(format!("    decision → {}: {}", opts[*chosen].label, line.join(" | ")));
                }
            }
            let bv = st.ball.vel.truncate();
            if st.control == Some(t.r) && (bv - prev_ball_v).length() > 0.8 {
                let dir = bv.normalize_or(Vec2::X);
                t.touch = Some((bv.length() - p.vel.dot(dir), now));
            }
            if prev_ctl == Some(t.r) && st.control.is_none() && t.loose.is_none() {
                let gap = (st.ball.ground_pos() - p.pos).length();
                t.loose = Some((gap, (bv - p.vel).length(), now - t.t0));
            }
            if t.near0 >= 8.0 && trace_n > 0 && step_n.is_multiple_of(5) {
                let (d, o) = nearest(st, t.r);
                let q = &st.players[o];
                let close = -(q.pos - p.pos).normalize_or(Vec2::X).dot(q.vel - p.vel);
                let intent = match p.intent {
                    Intent::Carry { speed, .. } => format!("carry {speed:.1}"),
                    Intent::Hold => "hold".into(),
                    Intent::SetUp { .. } => "set up".into(),
                    _ => "-".into(),
                };
                let towards = p.vel.dot((st.ball.ground_pos() - p.pos).normalize_or_zero()).max(0.0);
                let reach = sim.cfg.player.control_radius + sim.cfg.player.run_reach_s * towards;
                t.trace.push(format!(
                    "  {:4.1} s  v {:4.1}  ball {:4.1} m {:4.1} m/s{}  reach {:3.1}  due {:+4.2} cd {:+4.2}{}  opp {:4.1} m closing {:4.1} m/s  {intent:<9} {}",
                    now - t.t0,
                    p.vel.length(),
                    (st.ball.ground_pos() - p.pos).length(),
                    bv.length(),
                    if st.control == Some(t.r) { "" } else { " LOOSE" },
                    reach,
                    now - p.next_decision,
                    p.touch_cooldown_until - now,
                    if p.can_act(now) { "" } else { " busy" },
                    d,
                    close,
                    if p.last_decision >= t.t0 { p.debug.label.as_str() } else { "(settling)" }
                ));
            }
        }
        // Every carrying tick: is his decision overdue (due, but the ball not at his feet), and the ball's gap.
        if let Some(c) = st.control
            && let Intent::Carry { .. } = st.players[c].intent
        {
            let p = &st.players[c];
            let gap = (st.ball.ground_pos() - p.pos).length();
            let k = if gap <= sim.cfg.player.control_radius { 0 } else if gap <= 1.5 { 1 } else { 2 };
            let over = now - p.next_decision;
            let o = if over <= 0.0 { 0 } else if over <= 0.3 { 1 } else if over <= 1.0 { 2 } else { 3 };
            blind[k][o] += 1;
            if p.vel.length() > 5.0 {
                fast[k] += 1;
            }
        }
        prev_ctl = st.control;
        prev_ball_v = st.ball.vel.truncate();
    }
    (out, traces, blind, fast)
}

fn main() {
    let n: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(8);
    let trace_n: usize = std::env::var("TRACE").ok().and_then(|s| s.parse().ok()).unwrap_or(0);
    let runs: Vec<Run> = std::thread::scope(|scope| {
        let hs: Vec<_> = (1..=n).map(|seed| scope.spawn(move || play(seed, if seed == 1 { trace_n } else { 0 }))).collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let all: Vec<&End> = runs.iter().flat_map(|r| r.0.iter()).collect();
    let pct = |a: usize, b: usize| 100.0 * a as f32 / b.max(1) as f32;
    let q = |mut v: Vec<f32>, f: f32| {
        v.sort_by(f32::total_cmp);
        if v.is_empty() { f32::NAN } else { v[((v.len() - 1) as f32 * f) as usize] }
    };
    println!("{n} matches; controlled receptions of ground passes, nearest opponent ≥ 5 m at the control");
    for (lo, hi) in [(5.0, 8.0), (8.0, 999.0)] {
        let b: Vec<&&End> = all.iter().filter(|e| (lo..hi).contains(&e.near0) && e.res < 2).collect();
        let lost: Vec<&&&End> = b.iter().filter(|e| e.res == 1).collect();
        let tot: f32 = b.iter().map(|e| e.dt).sum();
        let how: Vec<String> = (0..4).map(|h| format!("{:.0}", pct(lost.iter().filter(|e| e.how == h).count(), lost.len()))).collect();
        println!(
            "\n{lo}–{hi} m: n {} ({:.0}/match), lost {:.1}%, {:.3}/s, mean time on the ball {:.2} s; lost by tackle / heavy / opp / out {} %",
            b.len(),
            b.len() as f32 / n as f32,
            pct(lost.len(), b.len()),
            lost.len() as f32 / tot,
            tot / b.len() as f32,
            how.join(" / ")
        );
        let ld = |f| q(lost.iter().map(|e| e.dt).collect(), f);
        println!("  time to the loss p25/50/75 {:.2}/{:.2}/{:.2} s; decisions before it p50 {:.0} p75 {:.0}", ld(0.25), ld(0.5), ld(0.75), q(lost.iter().map(|e| e.decisions as f32).collect(), 0.5), q(lost.iter().map(|e| e.decisions as f32).collect(), 0.75));
        let dn = |f| q(lost.iter().map(|e| e.dec_near).collect(), f);
        println!("  nearest opponent at his last decision p25/50/75 {:.1}/{:.1}/{:.1} m; seconds from it to the loss p50 {:.2}", dn(0.25), dn(0.5), dn(0.75), q(lost.iter().map(|e| e.since_dec).collect(), 0.5));
        let kinds = ["settling", "dribble", "take on", "hold", "pass", "shot", "other"];
        let kc: Vec<String> = kinds.iter().map(|k| format!("{k} {:.0}", pct(lost.iter().filter(|e| e.dec_kind == *k).count(), lost.len()))).collect();
        println!("  last decision before the loss: {} %", kc.join(", "));
        let kept: Vec<&&&End> = b.iter().filter(|e| e.res == 0).collect();
        let kk: Vec<String> = kinds.iter().map(|k| format!("{k} {:.0}", pct(kept.iter().filter(|e| e.dec_kind == *k).count(), kept.len()))).collect();
        println!("  kept: decisions p50 {:.0}, time p50 {:.2} s; decision before the kick: {} %", q(kept.iter().map(|e| e.decisions as f32).collect(), 0.5), q(kept.iter().map(|e| e.dt).collect(), 0.5), kk.join(", "));
        let bins = [(0.0, 0.15), (0.15, 0.35), (0.35, 0.6), (0.6, 1.01)];
        let carry = |e: &&&End| matches!(e.dec_kind, "dribble" | "take on" | "hold");
        let row = |v: &[&&End]| bins.iter().map(|&(a, z)| format!("{:.0}", pct(v.iter().filter(|e| (a..z).contains(&e.dec_prob)).count(), v.len()))).collect::<Vec<_>>().join(" / ");
        let lc: Vec<&&End> = lost.iter().filter(|e| carry(e)).map(|e| **e).collect();
        let kc: Vec<&&End> = b.iter().filter(|e| e.res == 0 && e.decisions > 0).copied().collect();
        println!(
            "  chosen option's softmax prob at the last decision < 0.15 / 0.15–0.35 / 0.35–0.6 / ≥ 0.6: losses after carrying {} %, kept (last decision before the kick) {} %; losses picked below the likeliest by half: {:.0} %",
            row(&lc),
            row(&kc),
            pct(lc.iter().filter(|e| e.dec_prob < 0.5 * e.dec_top).count(), lc.len())
        );
        let tk: Vec<&&&&End> = lost.iter().filter(|e| e.how == 0).collect();
        let rs: Vec<String> = ["shot", "last ditch", "loose touch", "odds near", "odds ctl", "aggression"]
            .iter()
            .enumerate()
            .map(|(i, k)| format!("{k} {:.0}", pct(tk.iter().filter(|e| e.reason == Some(i)).count(), tk.len())))
            .collect();
        println!(
            "  tackles: {} %; tackler in front / side / behind {:.0}/{:.0}/{:.0} %; ball nearer him {:.0} %; carrier speed p50 {:.1} m/s",
            rs.join(", "),
            pct(tk.iter().filter(|e| e.rel == 0).count(), tk.len()),
            pct(tk.iter().filter(|e| e.rel == 1).count(), tk.len()),
            pct(tk.iter().filter(|e| e.rel == 2).count(), tk.len()),
            pct(tk.iter().filter(|e| e.nearer_def).count(), tk.len()),
            q(tk.iter().map(|e| e.car_speed).collect(), 0.5)
        );
        let hv: Vec<&&&&End> = lost.iter().filter(|e| e.how == 1).collect();
        let lo_: Vec<(f32, f32, f32)> = hv.iter().filter_map(|e| e.loose).collect();
        println!(
            "  heavy touches: ball ran loose first {:.0} %, gap then p50 {:.1} m, ball speed relative to him p50 {:.1} m/s; last dribble touch faster than him by p25/50/75 {:.1}/{:.1}/{:.1} m/s",
            pct(lo_.len(), hv.len()),
            q(lo_.iter().map(|l| l.0).collect(), 0.5),
            q(lo_.iter().map(|l| l.1).collect(), 0.5),
            q(hv.iter().filter_map(|e| e.touch.map(|t| t.0)).collect(), 0.25),
            q(hv.iter().filter_map(|e| e.touch.map(|t| t.0)).collect(), 0.5),
            q(hv.iter().filter_map(|e| e.touch.map(|t| t.0)).collect(), 0.75)
        );
        let ot: Vec<&&&&End> = lost.iter().filter(|e| e.how == 3).collect();
        let okc: Vec<String> = kinds.iter().map(|k| format!("{k} {:.0}", pct(ot.iter().filter(|e| e.dec_kind == *k).count(), ot.len()))).collect();
        println!("  out: ball loose first {:.0} %; last decision {} %", pct(ot.iter().filter(|e| e.loose.is_some()).count(), ot.len()), okc.join(", "));
        let tl: Vec<&&&&End> = lost.iter().filter(|e| e.how == 0).collect();
        println!("  tackled after the ball ran loose {:.0} %", pct(tl.iter().filter(|e| e.loose.is_some()).count(), tl.len()));
    }
    let mut blind = [[0u32; 4]; 3];
    let mut fast = [0u32; 3];
    for r in &runs {
        for k in 0..3 {
            fast[k] += r.3[k];
            for (b, v) in blind[k].iter_mut().zip(r.2[k]) {
                *b += v;
            }
        }
    }
    let tot: u32 = blind.iter().flatten().sum();
    println!("\ncarrying (any carrier): share of the time by the ball's gap to him (at feet ≤ control radius / ≤ 1.5 m / more), and within each how overdue his decision is (not due / ≤ 0.3 s / ≤ 1 s / more); running faster than 5 m/s:");
    for (k, name) in ["at feet", "≤ 1.5 m", "> 1.5 m"].iter().enumerate() {
        let row: u32 = blind[k].iter().sum();
        println!(
            "  {name:<8} {:5.1}% of carrying: {} %; fast {:.0} %",
            pct(row as usize, tot as usize),
            blind[k].iter().map(|&v| format!("{:.0}", pct(v as usize, row as usize))).collect::<Vec<_>>().join(" / "),
            pct(fast[k] as usize, row as usize)
        );
    }
    for t in runs.iter().flat_map(|r| r.1.iter()) {
        println!("\n{t}");
    }
}
