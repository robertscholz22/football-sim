//! Risky passes by free carriers: real passers with no opponent within 5 m complete 97–99 % of their ground passes
//! (`sb_press_compl.py`), so a free carrier should rarely pick a pass the AI itself gives a low success chance. For every
//! live decision of a carrier with no opponent within 5 m of the ball: what he chose, and for the passes he chose
//! with p < 0.85 whether a safe pass (p ≥ 0.95) was on, how their layer-1 utilities compared, and whether the risky
//! pass was the layer-1 best (else layers 2–3 picked it). By third.
//! Env `DUMP=n`: print the first n risky free choices of seed 1 with their best options.
//! `cargo run -p match_engine --release --example free_risk -- [matches]`

use match_engine::Simulation;
use match_engine::ai::personality::OptionKind;
use match_engine::pitch::HALF_LENGTH;

#[derive(Default, Clone, Copy)]
struct Rec {
    third: usize,
    /// 0 pass, 1 dribble, 2 hold, 3 shot, 4 clear.
    kind: usize,
    p: f32,
    /// A pass with p ≥ 0.95 was on; its best layer-1 utility minus the chosen option's.
    safe_on: bool,
    safe_gap: f32,
    /// The chosen option had the highest layer-1 utility.
    argmax: bool,
    /// Layer-1 utility of the chosen option minus holding the ball.
    over_hold: f32,
    first_time: bool,
}

fn play(seed: u64) -> Vec<Rec> {
    let mut sim = Simulation::from_builtin("northbridge", "harbour", seed);
    sim.decision_log = Some(Vec::new());
    let dump: usize = std::env::var("DUMP").ok().and_then(|s| s.parse().ok()).filter(|_| seed == 1).unwrap_or(0);
    let mut dumped = 0;
    let mut out = Vec::new();
    let mut seen = 0;
    while !sim.finished() {
        sim.step();
        let st = &sim.state;
        let log = sim.decision_log.as_ref().unwrap();
        while seen < log.len() {
            let (_, sit, opts, chosen) = &log[seen];
            seen += 1;
            if !st.is_live() || sit.restart || sit.gk_hands {
                continue;
            }
            let Some(c) = opts.get(*chosen) else { continue };
            let p = &st.players[sit.carrier];
            let ball = sit.ball.truncate();
            let near = st.squad(p.side.opponent()).iter().map(|q| (q.pos - ball).length()).fold(f32::MAX, f32::min);
            if near < 5.0 {
                continue;
            }
            let x = HALF_LENGTH + st.frame(p.side).to_team(ball).x;
            let third = ((x / 35.0) as usize).min(2);
            let kind = match c.kind {
                OptionKind::Pass { .. } => 0,
                OptionKind::Dribble { .. } => 1,
                OptionKind::Hold => 2,
                OptionKind::Shot { .. } => 3,
                OptionKind::Clear => 4,
            };
            let safe = opts.iter().filter(|o| matches!(o.kind, OptionKind::Pass { .. }) && o.p >= 0.95).map(|o| o.u_opt).fold(f32::MIN, f32::max);
            let hold = opts.iter().filter(|o| matches!(o.kind, OptionKind::Hold)).map(|o| o.u_opt).fold(f32::MIN, f32::max);
            let best = opts.iter().map(|o| o.u_opt).fold(f32::MIN, f32::max);
            let r = Rec {
                third,
                kind,
                p: c.p,
                safe_on: safe > f32::MIN,
                safe_gap: if safe > f32::MIN { safe - c.u_opt } else { 0.0 },
                argmax: c.u_opt >= best - 1e-6,
                over_hold: if hold > f32::MIN { c.u_opt - hold } else { f32::NAN },
                first_time: sit.first_time,
            };
            if kind == 0 && c.p < 0.85 && dumped < dump {
                dumped += 1;
                println!("t {:.0} player {} team-x {:.0} nearest opponent {:.1} m first_time {}", st.time, sit.carrier, x, near, sit.first_time);
                let mut sorted: Vec<_> = opts.iter().collect();
                sorted.sort_by(|a, b| b.u_opt.total_cmp(&a.u_opt));
                for o in sorted.iter().take(8) {
                    let mark = if std::ptr::eq(*o, c) { "*" } else { " " };
                    println!("  {mark} u {:+.4} (instr {:+.4} pers {:+.4}) p {:.2} pressed {:.2} prob {:.2}  {}  [{}]", o.u_opt, o.instr, o.pers, o.p, o.pressed, o.prob, o.label, o.rules.join(","));
                }
            }
            out.push(r);
        }
        if let Some(log) = sim.decision_log.as_mut()
            && log.len() > 400
        {
            log.drain(..200);
            seen -= 200;
        }
    }
    out
}

fn main() {
    let n: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(6);
    let res: Vec<Rec> = std::thread::scope(|scope| {
        let hs: Vec<_> = (1..=n).map(|seed| scope.spawn(move || play(seed))).collect();
        hs.into_iter().flat_map(|h| h.join().unwrap()).collect()
    });
    let nf = n as f32;
    println!("decisions of free carriers (no opponent within 5 m): {:.0}/match", res.len() as f32 / nf);
    for (t, name) in ["own third", "middle third", "final third", "all"].iter().enumerate() {
        let r: Vec<&Rec> = res.iter().filter(|r| t == 3 || r.third == t).collect();
        let k = r.len().max(1) as f32;
        let share = |kind: usize| 100.0 * r.iter().filter(|x| x.kind == kind).count() as f32 / k;
        let passes: Vec<&&Rec> = r.iter().filter(|x| x.kind == 0).collect();
        let risky: Vec<&&Rec> = passes.iter().copied().filter(|x| x.p < 0.85).collect();
        let kr = risky.len().max(1) as f32;
        let mean = |v: &[&&Rec], f: &dyn Fn(&Rec) -> f32| v.iter().map(|x| f(x)).sum::<f32>() / v.len().max(1) as f32;
        println!(
            "{name:<13} {:5.0}/match: pass {:4.1}% dribble {:4.1}% hold {:4.1}% shot {:4.1}% | passes with p < 0.85: {:4.1}% of passes (first time {:3.0}%); of those: layer-1 best {:3.0}%, a safe pass on {:3.0}% (its utility − the chosen {:+.4}), chosen − hold {:+.4}",
            k / nf,
            share(0),
            share(1),
            share(2),
            share(3),
            100.0 * kr / passes.len().max(1) as f32,
            100.0 * risky.iter().filter(|x| x.first_time).count() as f32 / kr,
            100.0 * risky.iter().filter(|x| x.argmax).count() as f32 / kr,
            100.0 * risky.iter().filter(|x| x.safe_on).count() as f32 / kr,
            mean(&risky.iter().copied().filter(|x| x.safe_on).collect::<Vec<_>>(), &|x| x.safe_gap),
            mean(&risky.iter().copied().filter(|x| !x.over_hold.is_nan()).collect::<Vec<_>>(), &|x| x.over_hold),
        );
    }
}
