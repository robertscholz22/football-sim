//! Pressed receptions, as in StatsBomb 360 (`sb_receipt360.py`): every open-play pass the intended receiver gets
//! (controls, or plays first time), by the nearest opponent to him at that moment, and what he does next: plays his
//! next pass or shot first (kept), or loses the ball first (tackled or poked loose, a heavy touch the opponents then
//! win, as StatsBomb logs a miscontrol only when it loses the ball; an opponent gets it; the opponents get the
//! restart). Kept receptions split into first time (next kick within 0.5 s) and the time on the
//! ball; his next pass's direction and completion. Also by where that opponent stands: in front (goal-side, within
//! 60° of the direction of attack), to the side, or behind. Env `ATTRS=68 AWAY=47`: uniform attributes per side (as
//! `layers`); `LOFT=1` includes lofted passes.
//! `cargo run -p match_engine --release --example receptions -- [matches]`

use match_engine::Simulation;
use match_engine::events::EventKind;
use match_engine::duel::TackleOutcome;
use match_engine::match_state::side_of;
use match_engine::player::{Attributes, Intent, KickKind};

#[derive(Clone, Copy, Default)]
struct Rec {
    near: f32,
    /// 0 in front, 1 to the side, 2 behind.
    rel: u8,
    /// 0 kept, 1 lost, 2 other.
    res: u8,
    dt: f32,
    /// Loss cause: 0 tackled / poked loose, 1 heavy touch, 2 opponent got it, 3 restart to the opponents.
    how: u8,
    /// Next pass: 0 none (shot / not kept), 1 forward, 2 lateral, 3 back.
    dir: u8,
    next_ok: bool,
    away: bool,
    /// What he was doing when he lost it: 0 settling (before his first decision), 1 holding / shielding, 2 setting up
    /// a kick, 3 dribbling, 4 other.
    phase: u8,
    /// His first decision after the control, as `decide` labels it (numbers stripped).
    first: &'static str,
    /// At that decision: best pass p, its utility minus the best carrying option's, the layer-1 best is a pass, the
    /// best pass is to the player who passed to him, seconds after the control.
    best_pass_p: f32,
    pass_gap: f32,
    l1_pass: bool,
    back_to_passer: bool,
    t_first: f32,
    passer: usize,
    /// His reception decision (ATT-06, pressed receivers only): the control option's keep and pressed, and whether
    /// he made one.
    ctl_p: f32,
    ctl_pressed: f32,
    decided: bool,
}

fn uniform(sim: &mut Simulation) {
    let get = |k: &str| std::env::var(k).ok().and_then(|s| s.parse::<f32>().ok());
    for (side, v) in [(0usize, get("ATTRS")), (1, get("AWAY").or(get("ATTRS")))] {
        let Some(v) = v else { continue };
        for p in sim.state.players.iter_mut().filter(|p| p.side.index() == side) {
            let a = &p.attrs;
            p.attrs = Attributes { preferred_foot: a.preferred_foot, height_cm: a.height_cm, weight_kg: a.weight_kg, ..Attributes::uniform(v) };
            p.confidence = 1.0;
            p.energy = 1.0;
            p.familiarity = [1.0; 2];
        }
    }
    let c = sim.state.conditions;
    sim.set_conditions(c);
}

fn play(seed: u64) -> Vec<Rec> {
    let mut sim = Simulation::from_builtin("northbridge", "harbour", seed);
    uniform(&mut sim);
    let dump: usize = std::env::var("DUMP").ok().and_then(|s| s.parse().ok()).unwrap_or(0);
    sim.decision_log = Some(Vec::new());
    let loft = std::env::var("LOFT").is_ok();
    let mut out: Vec<Rec> = Vec::new();
    // Pass on its way to its receiver: receiver.
    let mut pending: Option<usize> = None;
    // Reception being followed: receiver, time, index in `out`.
    let mut active: Option<(usize, f32, usize)> = None;
    // A reception whose next pass is on its way: index in `out`.
    let mut next_of: Option<usize> = None;
    let mut seen = 0;
    // Phase of the reception being followed as of the previous tick (the loss has already changed it).
    let mut phase = 4u8;
    let mut dumped = 0;
    let mut passer = 0usize;
    // A reception whose receiver has made a heavy touch: index in `out`. Lost (heavy touch) only if the opponents
    // then get the ball first, as StatsBomb logs a miscontrol only when the touch loses it.
    let mut fumble: Option<usize> = None;
    let open = |st: &match_engine::match_state::MatchState, r: usize| {
        let p = &st.players[r];
        let fr = st.frame(p.side);
        let me = fr.to_team(p.pos);
        st.squad(p.side.opponent())
            .iter()
            .map(|o| (o.pos - p.pos, fr.to_team(o.pos) - me))
            .min_by(|a, b| a.0.length().total_cmp(&b.0.length()))
            .map(|(d, t)| {
                let ang = t.y.atan2(t.x).abs().to_degrees();
                (d.length(), if ang < 60.0 { 0 } else if ang > 120.0 { 2 } else { 1 })
            })
            .unwrap_or((99.0, 1))
    };
    while !sim.finished() {
        sim.step();
        if let Some(l) = &mut sim.decision_log
            && l.len() > 400
        {
            l.drain(..300);
        }
        let st = &sim.state;
        let now = st.time;
        while seen < sim.events.len() {
            let e = sim.events[seen].clone();
            seen += 1;
            let kick_by = match e.kind {
                EventKind::Pass { by, .. } | EventKind::Shot { by, .. } => Some(by),
                _ => None,
            };
            // The next pass of a reception resolves on the next kick or interception, or below on control.
            if let Some(i) = next_of
                && let EventKind::Pass { by, .. } | EventKind::Shot { by, .. } | EventKind::Interception { by } = e.kind
            {
                out[i].next_ok = side_of(by).index() == out[i].away as usize;
                next_of = None;
            }
            if let Some(by) = kick_by {
                // A first-time kick by the receiver of the pass on its way: received and played at once.
                if pending == Some(by) && active.is_none() {
                    let (near, rel) = open(st, by);
                    out.push(Rec { near, rel, away: st.players[by].side.index() == 1, ..Default::default() });
                    active = Some((by, now, out.len() - 1));
                    fumble = None;
                    pending = None;
                }
                if let Some((r, t0, i)) = active
                    && r == by
                {
                    out[i].res = 0;
                    out[i].dt = now - t0;
                    if let EventKind::Pass { .. } = e.kind {
                        let fr = st.frame(st.players[r].side);
                        let v = fr.to_team(st.ball.vel.truncate());
                        let ang = v.y.atan2(v.x).abs().to_degrees();
                        out[i].dir = if ang < 45.0 { 1 } else if ang > 135.0 { 3 } else { 2 };
                        next_of = Some(i);
                    }
                    active = None;
                }
            }
            match e.kind {
                EventKind::Pass { by, to, kind, headed, .. } => {
                    pending = to.filter(|_| !headed && (kind == KickKind::GroundPass || loft) && st.is_live());
                    passer = by;
                }
                EventKind::Shot { .. } => pending = None,
                EventKind::Miscontrol { by } => {
                    if let Some((_, t0, i)) = active.filter(|a| a.0 == by) {
                        if fumble.is_none() {
                            out[i] = Rec { dt: now - t0, phase, ..out[i] };
                            fumble = Some(i);
                        }
                    } else if pending == Some(by) && active.is_none() {
                        // A heavy first touch: StatsBomb logs the receipt and then the miscontrol.
                        let (near, rel) = open(st, by);
                        out.push(Rec { near, rel, away: st.players[by].side.index() == 1, ..Default::default() });
                        active = Some((by, now, out.len() - 1));
                        fumble = Some(out.len() - 1);
                        pending = None;
                    }
                }
                EventKind::Tackle { on, outcome, .. } if outcome != TackleOutcome::Beaten => {
                    if let Some((_, t0, i)) = active.filter(|a| a.0 == on) {
                        out[i] = Rec { res: 1, how: 0, dt: now - t0, phase, ..out[i] };
                        active = None;
                    }
                }
                EventKind::Restart { side, .. } => {
                    if let Some((r, t0, i)) = active {
                        let lost = side != st.players[r].side;
                        out[i] = if fumble == Some(i) {
                            Rec { res: if lost { 1 } else { 2 }, how: 1, ..out[i] }
                        } else {
                            Rec { res: if lost { 1 } else { 2 }, how: 3, dt: now - t0, phase, ..out[i] }
                        };
                        active = None;
                    }
                    if let Some(i) = next_of.take() {
                        out[i].next_ok = false;
                    }
                    pending = None;
                }
                EventKind::Goal { .. } | EventKind::HalfTime => {
                    if let Some((_, t0, i)) = active.take() {
                        out[i] = Rec { res: 2, dt: now - t0, ..out[i] };
                    }
                    pending = None;
                    next_of = None;
                }
                _ => {}
            }
        }
        if let Some(c) = st.control {
            if let Some(r) = pending {
                if c == r && active.is_none() {
                    let (near, rel) = open(st, r);
                    let mut rec = Rec { near, rel, away: st.players[r].side.index() == 1, passer, ..Default::default() };
                    if let Some((_, _, opts, _)) = sim
                        .decision_log
                        .as_ref()
                        .and_then(|l| l.iter().rev().take_while(|d| now - d.0 < 3.0).find(|d| d.1.carrier == r && d.1.first_time))
                        && let Some(c) = opts.iter().find(|o| o.label == "control")
                    {
                        (rec.ctl_p, rec.ctl_pressed, rec.decided) = (c.p, c.pressed, true);
                    }
                    out.push(rec);
                    active = Some((r, now, out.len() - 1));
                    fumble = None;
                }
                pending = None;
            }
            if st.flight.is_none()
                && let Some(i) = next_of.take()
            {
                out[i].next_ok = side_of(c).index() == out[i].away as usize;
            }
            if let Some((r, t0, i)) = active
                && c != r
            {
                let lost = side_of(c) != st.players[r].side;
                out[i] = if fumble == Some(i) {
                    Rec { res: if lost { 1 } else { 2 }, how: 1, ..out[i] }
                } else {
                    Rec { res: if lost { 1 } else { 2 }, how: 2, dt: now - t0, phase, ..out[i] }
                };
                active = None;
            }
            if active.is_some_and(|a| a.0 == c) {
                fumble = None;
            }
        }
        if let Some((r, t0, i)) = active {
            let p = &st.players[r];
            phase = match p.intent {
                Intent::Carry { .. } if p.last_decision < t0 => 0,
                Intent::Hold => 1,
                Intent::SetUp { .. } => 2,
                Intent::Carry { .. } => 3,
                _ => 4,
            };
            if out[i].first.is_empty()
                && p.last_decision >= t0
                && let Some((_, _, opts, _)) = sim.decision_log.as_ref().and_then(|l| l.iter().rev().find(|d| d.1.carrier == r))
            {
                let is_pass = |o: &&match_engine::ai::on_ball::OptionEval| matches!(o.action, match_engine::ai::on_ball::Action::Kick(k) if k.receiver.is_some());
                let best_pass = opts.iter().filter(is_pass).max_by(|a, b| a.u_opt.total_cmp(&b.u_opt));
                let best_carry = opts.iter().filter(|o| !matches!(o.action, match_engine::ai::on_ball::Action::Kick(_))).map(|o| o.u_opt).fold(f32::MIN, f32::max);
                let best = opts.iter().max_by(|a, b| a.u_opt.total_cmp(&b.u_opt));
                if let Some(bp) = best_pass {
                    out[i].best_pass_p = bp.p;
                    out[i].pass_gap = bp.u_opt - best_carry;
                    out[i].back_to_passer = matches!(bp.action, match_engine::ai::on_ball::Action::Kick(k) if k.receiver == Some(out[i].passer));
                }
                out[i].l1_pass = best.is_some_and(|b| is_pass(&b));
                out[i].t_first = now - t0;
            }
            if out[i].first.is_empty() && p.last_decision >= t0 {
                let l = p.debug.label.split(" (").next().unwrap_or("").trim_start_matches("set up → ");
                out[i].first = [("hold / shield", "hold / shield"), ("dribble", "dribble"), ("take on", "take on"), ("pass", "pass"), ("lofted", "pass"), ("through", "pass"), ("shot", "shot"), ("clear", "clear")]
                    .into_iter()
                    .find(|k| l.starts_with(k.0))
                    .map_or("other", |k| k.1);
                // `DUMP=n`: the options of the first n first decisions of receivers with an opponent within 1.5 m: the
                // four likeliest and every pass.
                if out[i].near < 1.5
                    && dumped < dump
                    && let Some((_, _, opts, chosen)) = sim.decision_log.as_ref().and_then(|l| l.iter().rev().find(|d| d.1.carrier == r))
                {
                    dumped += 1;
                    let mut o: Vec<_> = opts.iter().collect();
                    o.sort_by(|a, b| b.prob.total_cmp(&a.prob));
                    eprintln!("seed {seed} t {now:.1} player {} near {:.1} m, {:.2} s after control, chose {}:", p.id, out[i].near, now - t0, opts[*chosen].label);
                    for (k, e) in o.iter().enumerate() {
                        if k < 4 || e.label.contains("pass") || e.label.contains("ball →") {
                            eprintln!("    {:<34} p {:.2} pressed {:.2} u {:+.4} pers {:+.4} prob {:.2}", e.label, e.p, e.pressed, e.u_opt, e.pers, e.prob);
                        }
                    }
                }
            }
        }
        if let Some((_, t0, i)) = active
            && now - t0 > 20.0
        {
            out[i] = Rec { res: 2, dt: now - t0, ..out[i] };
            active = None;
        }
    }
    if let Some((_, _, i)) = active {
        out[i].res = 2;
    }
    out
}

fn main() {
    let n: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(10);
    let recs: Vec<Rec> = std::thread::scope(|scope| {
        let hs: Vec<_> = (1..=n).map(|seed| scope.spawn(move || play(seed))).collect();
        hs.into_iter().flat_map(|h| h.join().unwrap()).collect()
    });
    let pct = |a: usize, b: usize| 100.0 * a as f32 / b.max(1) as f32;
    let median = |mut v: Vec<f32>| {
        v.sort_by(f32::total_cmp);
        if v.is_empty() { f32::NAN } else { v[v.len() / 2] }
    };
    println!("{n} matches, {:.0} receptions per team-match (÷ 1.48: {:.0})", recs.len() as f32 / (2 * n) as f32, recs.len() as f32 / (2 * n) as f32 / 1.48);
    println!(
        "{:<12} {:>6} {:>7} {:>7} {:>7} {:>9} {:>8} {:>8} {:>7} {:>9} {:>9} {:>9}",
        "nearest opp", "share", "1st", "lost", "other", "lost ctl", "t kept", "t>2s", "fwd", "back", "next ok", "lost: tk/mis/opp/out"
    );
    let bands = [(0.0, 1.5), (1.5, 3.0), (3.0, 5.0), (5.0, 8.0), (8.0, 999.0), (0.0, 999.0)];
    for (lo, hi) in bands {
        let b: Vec<&Rec> = recs.iter().filter(|r| (lo..hi).contains(&r.near)).collect();
        let kept: Vec<&&Rec> = b.iter().filter(|r| r.res == 0).collect();
        let ft = kept.iter().filter(|r| r.dt <= 0.5).count();
        let lost: Vec<&&Rec> = b.iter().filter(|r| r.res == 1).collect();
        let controlled = b.len() - ft;
        let passes: Vec<&&&Rec> = kept.iter().filter(|r| r.dir > 0).collect();
        let how: Vec<String> = (0..4).map(|h| format!("{:.0}", pct(lost.iter().filter(|r| r.how == h).count(), lost.len()))).collect();
        println!(
            "{:<12} {:>5.1}% {:>6.1}% {:>6.1}% {:>6.1}% {:>8.1}% {:>7.2}s {:>7.1}% {:>6.0}% {:>8.0}% {:>8.0}% {:>9}",
            if hi > 100.0 && lo == 0.0 { "all".to_string() } else { format!("{lo}–{hi} m") },
            pct(b.len(), recs.len()),
            pct(ft, b.len()),
            pct(lost.len(), b.len()),
            pct(b.iter().filter(|r| r.res == 2).count(), b.len()),
            pct(lost.len(), controlled),
            median(kept.iter().filter(|r| r.dt > 0.5).map(|r| r.dt).collect()),
            pct(kept.iter().filter(|r| r.dt > 2.0).count(), kept.len()),
            pct(passes.iter().filter(|r| r.dir == 1).count(), passes.len()),
            pct(passes.iter().filter(|r| r.dir == 3).count(), passes.len()),
            pct(passes.iter().filter(|r| r.next_ok).count(), passes.len()),
            how.join("/")
        );
    }
    println!("controlled receptions within 3 m: what he was doing when he lost it (settling / holding / setting up / dribbling / other), and his first decision (share; lost of those):");
    let close: Vec<&Rec> = recs.iter().filter(|r| r.near < 3.0 && !(r.res == 0 && r.dt <= 0.5)).collect();
    let lost: Vec<&&Rec> = close.iter().filter(|r| r.res == 1 && r.how != 1).collect();
    let ph: Vec<String> = (0..5).map(|k| format!("{:.0}", pct(lost.iter().filter(|r| r.phase == k).count(), lost.len()))).collect();
    let lt: Vec<f32> = lost.iter().map(|r| r.dt).collect();
    println!("  lost {} of {} (heavy touches aside); phase {} %; time to the loss median {:.2} s", lost.len(), close.len(), ph.join(" / "), median(lt));
    for k in ["hold / shield", "dribble", "take on", "pass", "shot", "clear", "other", ""] {
        let b: Vec<&&Rec> = close.iter().filter(|r| r.first == k).collect();
        if b.is_empty() {
            continue;
        }
        let l = b.iter().filter(|r| r.res == 1).count();
        println!("  first {:<14} {:5.1}%  lost {:5.1}%", if k.is_empty() { "(none)" } else { k }, pct(b.len(), close.len()), pct(l, b.len()));
    }
    println!("loss hazard of controlled receptions: losses per second on the ball (kept: time to the next kick; lost: time to the loss); mean time on the ball:");
    for (lo, hi) in [(0.0, 1.5), (1.5, 3.0), (3.0, 5.0), (5.0, 8.0), (8.0, 999.0)] {
        let b: Vec<&Rec> = recs.iter().filter(|r| (lo..hi).contains(&r.near) && r.res < 2 && !(r.res == 0 && r.dt <= 0.5)).collect();
        let tot: f32 = b.iter().map(|r| r.dt).sum();
        let nl = b.iter().filter(|r| r.res == 1).count();
        println!("  {lo}–{hi} m: n {}, lost {:.1}%, {:.3}/s, mean time on the ball {:.2} s", b.len(), pct(nl, b.len()), nl as f32 / tot, tot / b.len() as f32);
    }
    let pl = match_engine::config::SimConfig::default().tactics.att25_pressed_loss;
    println!("controlled receptions after an ATT-06 reception decision: the AI's implied loss of \"control\" (1 − keep) + keep × {pl} × pressed, against lost before the next kick:");
    for (lo, hi) in [(0.0, 1.5), (1.5, 3.0), (3.0, 5.0), (5.0, 999.0)] {
        let b: Vec<&Rec> = recs.iter().filter(|r| (lo..hi).contains(&r.near) && r.decided && !(r.res == 0 && r.dt <= 0.5)).collect();
        if b.is_empty() {
            continue;
        }
        let implied = b.iter().map(|r| (1.0 - r.ctl_p) + r.ctl_p * pl * r.ctl_pressed).sum::<f32>() / b.len() as f32;
        println!("  {lo}–{hi} m: n {}, keep {:.2}, implied {implied:.2}, lost {:.2}", b.len(), b.iter().map(|r| r.ctl_p).sum::<f32>() / b.len() as f32, b.iter().filter(|r| r.res == 1).count() as f32 / b.len() as f32);
    }
    println!("first decisions after control, by nearest opponent at the reception: seconds after control, best pass p (mean; share ≥ 0.85), best pass − best carrying option utility (median), layer-1 best is a pass, best pass goes back to the passer, chosen a pass:");
    for (lo, hi) in [(0.0, 1.5), (1.5, 3.0), (3.0, 5.0), (5.0, 999.0)] {
        let b: Vec<&Rec> = recs.iter().filter(|r| (lo..hi).contains(&r.near) && !r.first.is_empty()).collect();
        if b.is_empty() {
            continue;
        }
        let nb = b.len();
        println!(
            "  {lo}–{hi} m: {:.2} s, best pass p {:.2} ({:.0}% ≥ 0.85), gap {:+.4}, layer-1 pass {:.0}%, back to passer {:.0}%, chose pass {:.0}%",
            b.iter().map(|r| r.t_first).sum::<f32>() / nb as f32,
            b.iter().map(|r| r.best_pass_p).sum::<f32>() / nb as f32,
            pct(b.iter().filter(|r| r.best_pass_p >= 0.85).count(), nb),
            median(b.iter().map(|r| r.pass_gap).collect()),
            pct(b.iter().filter(|r| r.l1_pass).count(), nb),
            pct(b.iter().filter(|r| r.back_to_passer).count(), nb),
            pct(b.iter().filter(|r| r.first == "pass").count(), nb)
        );
    }
    println!("by where the nearest opponent stands (within 3 m): share / first time / lost / lost of controlled");
    for (k, name) in ["in front", "side", "behind"].iter().enumerate() {
        let b: Vec<&Rec> = recs.iter().filter(|r| r.near < 3.0 && r.rel as usize == k).collect();
        let ft = b.iter().filter(|r| r.res == 0 && r.dt <= 0.5).count();
        let lost = b.iter().filter(|r| r.res == 1).count();
        println!("  {name:<9} {:5.1}% {:6.1}% {:6.1}% {:6.1}%", pct(b.len(), recs.iter().filter(|r| r.near < 3.0).count()), pct(ft, b.len()), pct(lost, b.len()), pct(lost, b.len() - ft));
    }
}
