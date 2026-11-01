//! Why the on-ball AI picks long passes: every open-play decision (not first time, not a restart) with all its
//! options. Prints the share of chosen options by kind and pass length, the softmax mass and option count per band,
//! and for chosen 30–45 m passes how they compare with the best short (< 20 m) pass on offer: whether layer 1 already
//! preferred them, the gap in optimum utility, success probability and receiver pressure. `PASSER=CB` restricts the
//! long-pass breakdown to one passer role. Also, for the first decision on the ball and later ones, free or pressed:
//! the choice between passing, dribbling and holding, and the continuation premium (best pass − value at the ball).
//! `cargo run -p match_engine --release --example long_choice -- [matches]`

use match_engine::Simulation;
use match_engine::ai::on_ball::{Action, OptionEval};
use match_engine::ai::personality::OptionKind;
use match_engine::ai::analysis::possession_value;
use match_engine::pitch::HALF_LENGTH;

const BANDS: [&str; 5] = ["0-10", "10-20", "20-30", "30-45", "45+"];

fn band(len: f32) -> usize {
    match len {
        l if l < 10.0 => 0,
        l if l < 20.0 => 1,
        l if l < 30.0 => 2,
        l if l < 45.0 => 3,
        _ => 4,
    }
}

/// A pass option at a decision: band, length, forward metres, p, optimum utility, final utility, choice probability,
/// receiver pressed (ATT-25), lofted.
#[derive(Clone, Copy)]
struct PassOpt {
    band: usize,
    len: f32,
    fwd: f32,
    p: f32,
    u: f32,
    util: f32,
    prob: f32,
    pressed: bool,
    lofted: bool,
    /// Bit k set when rule `RULES[k]` applied.
    rules: u8,
    /// Possession value gained, in units of the local value scale.
    gain: f32,
}

const RULES: [&str; 4] = ["ATT-01", "ATT-03", "ATT-17", "ATT-21"];

/// Per decision: chosen kind index (0..5 pass bands, 5 dribble, 6 hold, 7 shot, 8 other), pass options, chosen pass,
/// argmax-u_opt option is the chosen one, argmax utility is the chosen one, passer role, passer x from own goal,
/// nearest team-mate distance.
struct Dec {
    chosen_kind: usize,
    passes: Vec<PassOpt>,
    chosen: Option<PassOpt>,
    opt_best: bool,
    util_best: bool,
    role: String,
    x: f32,
    nearest_mate: f32,
    /// The carrier's first decision since he gained control.
    first: bool,
    /// Softmax mass of passes, dribbles and holding; best layer-1 utility of a pass, a dribble, holding; pressure.
    mass: [f32; 3],
    best: [f32; 3],
    pressure: f32,
    /// Best pass utility minus the possession value at the ball (the continuation premium before fading).
    premium: f32,
}

fn play(seed: u64) -> Vec<Dec> {
    let mut sim = Simulation::from_builtin("northbridge", "harbour", seed);
    sim.decision_log = Some(Vec::new());
    let mut out = Vec::new();
    let mut seen = 0;
    let mut stall = 0u32;
    // Per player: when he gained control at his latest decision.
    let mut last_ctrl = [f32::NAN; 22];
    while !sim.finished() {
        sim.step();
        let st = &sim.state;
        let log = sim.decision_log.as_ref().unwrap();
        for (_, sit, opts, chosen) in &log[seen..] {
            if sit.first_time || sit.restart || sit.gk_hands {
                continue;
            }
            let p = &st.players[sit.carrier];
            let first = last_ctrl[p.id] != p.control_since;
            last_ctrl[p.id] = p.control_since;
            let fr = st.frame(p.side);
            let from = sit.ball.truncate();
            let from_t = fr.to_team(from);
            let pass_of = |o: &OptionEval| -> Option<PassOpt> {
                let (OptionKind::Pass { lofted, .. }, Action::Kick(k)) = (o.kind, o.action) else {
                    return None;
                };
                let len = (k.target - from).length();
                let ai = &sim.cfg.ai;
                let scale = possession_value(from_t, ai) + ai.value_base;
                Some(PassOpt {
                    rules: RULES.iter().enumerate().filter(|(_, r)| o.rules.contains(r)).map(|(k, _)| 1u8 << k).sum(),
                    gain: (possession_value(fr.to_team(k.target), ai) - possession_value(from_t, ai)) / scale,
                    band: band(len),
                    len,
                    fwd: fr.to_team(k.target).x - from_t.x,
                    p: o.p,
                    u: o.u_opt,
                    util: o.utility(),
                    prob: o.prob,
                    pressed: o.rules.contains(&"ATT-25"),
                    lofted,
                })
            };
            let passes: Vec<PassOpt> = opts.iter().filter_map(pass_of).collect();
            let Some(c) = opts.get(*chosen) else { continue };
            // `STALL=seed`: print every 200th hold of that match with time, carrier, position and options.
            if matches!(c.kind, OptionKind::Hold)
                && std::env::var("STALL").ok().and_then(|s| s.parse::<u64>().ok()) == Some(seed)
            {
                stall += 1;
                if stall % 200 == 1 {
                    let near = st.squad(p.side.opponent()).iter().map(|o| (o.pos - from).length()).fold(f32::MAX, f32::min);
                    println!("STALL t {:.1} player {} {:?} team-pos ({:.1}, {:.1}) nearest opp {near:.1} m, phase {:?}; now: control {:?}, ball {:?}, player at {:?}", st.time, p.id, p.side, from_t.x, from_t.y, st.phase, st.control, st.ball.pos, p.pos);
                    let mut sorted: Vec<&OptionEval> = opts.iter().collect();
                    sorted.sort_by(|a, b| b.u_opt.total_cmp(&a.u_opt));
                    for o in sorted.iter().take(5) {
                        println!("      u {:+.4} p {:.2} prob {:.2}  {}  [{}]", o.u_opt, o.p, o.prob, o.label, o.rules.join(","));
                    }
                }
            }
            let cp = pass_of(c);
            let chosen_kind = match (cp, c.kind) {
                (Some(pp), _) => pp.band,
                (None, OptionKind::Dribble { .. }) => 5,
                (None, OptionKind::Hold) => 6,
                (None, OptionKind::Shot { .. }) => 7,
                _ => 8,
            };
            let argmax = |f: &dyn Fn(&OptionEval) -> f32| {
                (0..opts.len()).max_by(|&a, &b| f(&opts[a]).total_cmp(&f(&opts[b]))).unwrap_or(0)
            };
            let nearest_mate = st
                .squad(p.side)
                .iter()
                .filter(|m| m.id != p.id && !m.is_gk())
                .map(|m| (m.pos - from).length())
                .fold(f32::MAX, f32::min);
            out.push(Dec {
                chosen_kind,
                passes,
                chosen: cp,
                opt_best: argmax(&|o| o.u_opt) == *chosen,
                util_best: argmax(&|o| o.utility()) == *chosen,
                role: format!("{:?}", st.team(p.side).formation.slots[p.slot].role),
                x: from_t.x + HALF_LENGTH,
                nearest_mate,
                first,
                mass: [0, 1, 2].map(|k| opts.iter().filter(|o| kind_ix(o) == k).map(|o| o.prob).sum()),
                best: [0, 1, 2].map(|k| opts.iter().filter(|o| kind_ix(o) == k).map(|o| o.u_opt).fold(f32::NAN, f32::max)),
                pressure: match_engine::ai::analysis::pressure_at(st, p.side, from),
                premium: opts.iter().filter(|o| kind_ix(o) == 0).map(|o| o.u_opt).fold(f32::MIN, f32::max)
                    - possession_value(from_t, &sim.cfg.ai),
            });
        }
        seen = log.len();
    }
    out
}

fn kind_ix(o: &OptionEval) -> usize {
    match o.kind {
        OptionKind::Pass { .. } => 0,
        OptionKind::Dribble { .. } => 1,
        OptionKind::Hold => 2,
        _ => 3,
    }
}

fn median(mut v: Vec<f32>) -> f32 {
    if v.is_empty() {
        return f32::NAN;
    }
    v.sort_by(f32::total_cmp);
    v[v.len() / 2]
}

fn main() {
    let n: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(4);
    let only = std::env::var("PASSER").ok();
    let decs: Vec<Dec> = (1..=n).flat_map(play).collect();
    let m = n as f32;
    let total = decs.len() as f32;
    println!("open-play decisions per match: {:.0}", total / m);
    let names = ["pass 0-10", "pass 10-20", "pass 20-30", "pass 30-45", "pass 45+", "dribble", "hold", "shot", "other"];
    for (k, name) in names.iter().enumerate() {
        let c = decs.iter().filter(|d| d.chosen_kind == k).count() as f32;
        println!("  chosen {name:<11} {:6.1}/match  {:4.1} %", c / m, 100.0 * c / total);
    }
    let mut roles: Vec<(String, usize)> = Vec::new();
    for d in decs.iter().filter(|d| d.chosen_kind == 6) {
        match roles.iter_mut().find(|r| r.0 == d.role) {
            Some(r) => r.1 += 1,
            None => roles.push((d.role.clone(), 1)),
        }
    }
    roles.sort_by_key(|r| std::cmp::Reverse(r.1));
    let line: Vec<String> = roles.iter().map(|(r, k)| format!("{r} {:.1}", *k as f32 / m)).collect();
    println!("  holds by role per match: {}", line.join(", "));
    println!("\npass options per decision and softmax mass, by band:");
    for (b, name) in BANDS.iter().enumerate() {
        let cnt: f32 = decs.iter().map(|d| d.passes.iter().filter(|p| p.band == b).count() as f32).sum::<f32>() / total;
        let mass: f32 = decs.iter().map(|d| d.passes.iter().filter(|p| p.band == b).map(|p| p.prob).sum::<f32>()).sum::<f32>() / total;
        let best_u = median(
            decs.iter().filter_map(|d| d.passes.iter().filter(|p| p.band == b).map(|p| p.u).reduce(f32::max)).collect(),
        );
        let best_p = median(
            decs.iter()
                .filter_map(|d| d.passes.iter().filter(|p| p.band == b).max_by(|a, c| a.u.total_cmp(&c.u)).map(|p| p.p))
                .collect(),
        );
        println!(
            "  {name:>5} m: {cnt:4.1} options, mass {:4.1} %, best option median U {best_u:+.4}, its p {best_p:.2}",
            100.0 * mass
        );
    }
    for (name, is_first) in [("first decision on the ball", true), ("later decisions", false)] {
        for (pn, plo, phi) in [("free", 0.0, 0.3), ("pressed", 0.3, 2.0)] {
            let v: Vec<&Dec> = decs.iter().filter(|d| d.first == is_first && (plo..phi).contains(&d.pressure)).collect();
            let k = v.len().max(1) as f32;
            let ch = |c: &dyn Fn(usize) -> bool| 100.0 * v.iter().filter(|d| c(d.chosen_kind)).count() as f32 / k;
            let pr: Vec<f32> = v.iter().map(|d| d.premium).filter(|x| *x > -1.0).collect();
            println!(
                "    premium > 0 in {:.0} %, median {:+.4}, p75 {:+.4}; hold U median {:+.4}",
                100.0 * pr.iter().filter(|x| **x > 0.0).count() as f32 / pr.len().max(1) as f32,
                median(pr.clone()),
                { let mut q = pr.clone(); q.sort_by(f32::total_cmp); q.get(q.len() * 3 / 4).copied().unwrap_or(f32::NAN) },
                median(v.iter().map(|d| d.best[2] + 0.0).filter(|x| x.is_finite()).collect()),
            );
            println!(
                "{name}, {pn}: {:.0}/match, chosen pass {:.0} % dribble {:.0} % hold {:.0} % | mass pass {:.0} % dribble {:.0} % hold {:.0} % | best U pass {:+.4} dribble {:+.4} hold {:+.4}",
                v.len() as f32 / m,
                ch(&|c| c <= 4),
                ch(&|c| c == 5),
                ch(&|c| c == 6),
                100.0 * v.iter().map(|d| d.mass[0]).sum::<f32>() / k,
                100.0 * v.iter().map(|d| d.mass[1]).sum::<f32>() / k,
                100.0 * v.iter().map(|d| d.mass[2]).sum::<f32>() / k,
                median(v.iter().map(|d| d.best[0]).filter(|x| x.is_finite()).collect()),
                median(v.iter().map(|d| d.best[1]).filter(|x| x.is_finite()).collect()),
                median(v.iter().map(|d| d.best[2]).filter(|x| x.is_finite()).collect()),
            );
        }
    }
    let mates = decs.iter().filter(|d| d.nearest_mate < 10.0).count() as f32;
    println!("\nnearest outfield team-mate to the ball: median {:.1} m, < 10 m in {:.0} % of decisions", median(decs.iter().map(|d| d.nearest_mate).collect()), 100.0 * mates / total);

    let long: Vec<&Dec> =
        decs.iter().filter(|d| d.chosen_kind == 3 && only.as_ref().is_none_or(|r| &d.role == r)).collect();
    let nl = long.len() as f32;
    println!("\nchosen 30-45 m passes{}: {:.1}/match", only.as_ref().map_or(String::new(), |r| format!(" by {r}")), nl / m);
    let c = |f: &dyn Fn(&&Dec) -> bool| 100.0 * long.iter().filter(|d| f(d)).count() as f32 / nl;
    println!("  layer-1 best option {:.0} %, best after biases {:.0} %", c(&|d| d.opt_best), c(&|d| d.util_best));
    println!(
        "  lofted {:.0} %, forward > 5 m {:.0} %, backward < -3 m {:.0} %, receiver pressed {:.0} %",
        c(&|d| d.chosen.unwrap().lofted),
        c(&|d| d.chosen.unwrap().fwd > 5.0),
        c(&|d| d.chosen.unwrap().fwd < -3.0),
        c(&|d| d.chosen.unwrap().pressed)
    );
    println!("  a short (< 20 m) pass was on offer {:.0} %", c(&|d| d.passes.iter().any(|p| p.band <= 1)));
    let pairs: Vec<(PassOpt, PassOpt)> = long
        .iter()
        .filter_map(|d| {
            let s = d.passes.iter().filter(|p| p.band <= 1).max_by(|a, b| a.u.total_cmp(&b.u))?;
            Some((d.chosen.unwrap(), *s))
        })
        .collect();
    let med = |f: &dyn Fn(&(PassOpt, PassOpt)) -> f32| median(pairs.iter().map(f).collect());
    println!(
        "  chosen long: U {:+.4} p {:.2} | best short: U {:+.4} p {:.2} len {:.1} m fwd {:+.1} m, pressed {:.0} % | U long − short {:+.4}, util long − short {:+.4}",
        med(&|q| q.0.u),
        med(&|q| q.0.p),
        med(&|q| q.1.u),
        med(&|q| q.1.p),
        med(&|q| q.1.len),
        med(&|q| q.1.fwd),
        100.0 * pairs.iter().filter(|q| q.1.pressed).count() as f32 / pairs.len().max(1) as f32,
        med(&|q| q.0.u - q.1.u),
        med(&|q| q.0.util - q.1.util),
    );
    let share = |v: &[PassOpt], k: usize| 100.0 * v.iter().filter(|p| p.rules & (1 << k) != 0).count() as f32 / v.len().max(1) as f32;
    for (name, sel) in [("long fwd", 0), ("long back", 1), ("long lat", 2), ("best short", 3)] {
        let v: Vec<PassOpt> = match sel {
            3 => pairs.iter().map(|q| q.1).collect(),
            _ => pairs
                .iter()
                .map(|q| q.0)
                .filter(|p| match sel {
                    0 => p.fwd > 5.0,
                    1 => p.fwd < -3.0,
                    _ => (-3.0..=5.0).contains(&p.fwd),
                })
                .collect(),
        };
        let rules: Vec<String> = RULES.iter().enumerate().map(|(k, r)| format!("{r} {:.0} %", share(&v, k))).collect();
        println!(
            "  {name:<10} n {:5.1}/match: value gain median {:+.2} scale, U {:+.4}, p {:.2}; {}",
            v.len() as f32 / m,
            median(v.iter().map(|p| p.gain).collect()),
            median(v.iter().map(|p| p.u).collect()),
            median(v.iter().map(|p| p.p).collect()),
            rules.join(", ")
        );
    }
    println!("  passer x from own goal: median {:.1} m", median(long.iter().map(|d| d.x).collect()));
    let mut roles: Vec<(String, usize)> = Vec::new();
    for d in &long {
        match roles.iter_mut().find(|r| r.0 == d.role) {
            Some(r) => r.1 += 1,
            None => roles.push((d.role.clone(), 1)),
        }
    }
    roles.sort_by_key(|r| std::cmp::Reverse(r.1));
    let line: Vec<String> = roles.iter().map(|(r, k)| format!("{r} {:.1}", *k as f32 / m)).collect();
    println!("  by passer role per match: {}", line.join(", "));
}
