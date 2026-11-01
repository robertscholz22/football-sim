//! Does a perfect player follow the optimum, and do worse ones deviate from it in each layer? The same match with
//! every player's attributes set to one value (`ATTRS=100,60,20`): off the ball, the layer-3 drift from the layer-1+2
//! target; on the ball, how often the layer-1 best option is the one chosen and the gap between them; in execution,
//! the pass completion and the AI's predicted p against it.
//! Env: `ATTRS` (values to run), `EGO` / `FLAIR` (fixed instead), `AWAY` (the away side's value), `CONF` / `ENERGY`
//! (confidence and energy of everyone, default 1), `TEMP` (`ai.temperature_best`), `LOOSE` (`duel.tackle_loose_margin`), `GROUP` (all | body | technique |
//! mind | character | gk | a comma list of rating names: only these take the away value, the rest stay at the home value).
//! Without `EGO`, egotism is v·(1 − 0.6·v/100): the egotism the old teamwork damping gave at teamwork = v.
//! `ATTRS=100,60,20 cargo run -p match_engine --release --example layers -- [matches]`

use match_engine::Simulation;
use match_engine::events::EventKind;
use match_engine::player::{Attributes, GROUPS};

struct Agg {
    drift: f64,
    drift_n: u64,
    off_target: f64,
    dec: u64,
    best: u64,
    gap: f64,
    passes: u64,
    completed: u64,
    predicted: f64,
    pred_n: u64,
    goals: [u64; 2],
    shots: [u64; 2],
    xg: [f64; 2],
    tackles_won: [u64; 2],
    /// Per side: passes, completed, tackles attempted, interceptions, miscontrols, possessions (spells).
    side: [[u64; 6]; 2],
    /// Per side: possession seconds in own / middle / final third, turnovers in own / middle / final third.
    thirds: [[f64; 6]; 2],
    /// Per side: forward passes (> 3 m up the pitch), backward passes (> 3 m back), sum of chosen p, count.
    dirs: [[f64; 4]; 2],
    /// Per side: passes attempted / completed by length (< 10, 10–30, ≥ 30 m).
    lens: [[[f64; 2]; 3]; 2],
    /// Per side: possessions won in the own third, of which reached the final third within 10 s, shots within 10 s.
    counters: [[f64; 3]; 2],
    /// Per side, passes ≥ 30 m: forward (> 10 m gain), other, forward completed, then after a completed forward one:
    /// still kept after 5 s, lost within 5 s, shot within 5 s.
    longs: [[f64; 6]; 2],
    /// Per side, forward passes ≥ 30 m by target: into the final third (attempted, completed), behind the last
    /// outfield opponent (attempted, completed).
    zones: [[f64; 4]; 2],
    /// Per side (the kicking side): aerials lost to a defender after our kick, of which we had the ball 4 s later.
    second: [[f64; 2]; 2],
    near: u64,
    poss: [f64; 2],
}

fn play(seed: u64, v: f32) -> Agg {
    let mut sim = Simulation::from_builtin("northbridge", "harbour", seed);
    let ego: Option<f32> = std::env::var("EGO").ok().and_then(|s| s.parse().ok());
    let flair: Option<f32> = std::env::var("FLAIR").ok().and_then(|s| s.parse().ok());
    let away: Option<f32> = std::env::var("AWAY").ok().and_then(|s| s.parse().ok());
    let conf: f32 = std::env::var("CONF").ok().and_then(|s| s.parse().ok()).unwrap_or(1.0);
    let energy: f32 = std::env::var("ENERGY").ok().and_then(|s| s.parse().ok()).unwrap_or(1.0);
    let group = std::env::var("GROUP").unwrap_or("all".into());
    for p in &mut sim.state.players {
        let (home_v, v) = (v, if p.side == match_engine::pitch::Side::Away { away.unwrap_or(v) } else { v });
        let mut a = Attributes::uniform(home_v);
        // `GROUP`: only this attribute group takes the away value, the rest stay at the home value.
        let set = |a: &mut Attributes, names: &[&str], v: f32| {
            for name in names {
                *a.rating_mut(name).unwrap_or_else(|| panic!("unknown rating {name}")) = v;
            }
        };
        let group_name = if group == "gk" { "goalkeeping" } else { group.as_str() };
        match GROUPS.iter().find(|(g, _)| g.eq_ignore_ascii_case(group_name)) {
            _ if group == "all" => GROUPS.iter().for_each(|(_, names)| set(&mut a, names, v)),
            Some((_, names)) => set(&mut a, names, v),
            None => set(&mut a, &group.split(',').collect::<Vec<_>>(), v),
        }
        // `STRIKER=speed=75,heading=70`: the away side's strikers get these attributes on top.
        if p.side == match_engine::pitch::Side::Away && p.role.is_striker()
            && let Ok(spec) = std::env::var("STRIKER")
        {
            for kv in spec.split(',') {
                if let Some((k, val)) = kv.split_once('=')
                    && let Ok(val) = val.parse::<f32>()
                {
                    set(&mut a, &[k], val);
                }
            }
        }
        a.egotism = ego.unwrap_or(v * (1.0 - 0.6 * v / 100.0));
        a.flair = flair.unwrap_or(v);
        p.attrs = Attributes { preferred_foot: p.attrs.preferred_foot, height_cm: p.attrs.height_cm, weight_kg: p.attrs.weight_kg, ..a };
        p.confidence = conf;
        p.energy = energy;
        p.familiarity = [1.0; 2];
    }
    let c = sim.state.conditions;
    sim.set_conditions(c);
    if let Some(t) = std::env::var("TEMP").ok().and_then(|s| s.parse().ok()) {
        sim.cfg.ai.temperature_best = t;
    }
    if let Some(m) = std::env::var("LOOSE").ok().and_then(|s| s.parse().ok()) {
        sim.cfg.duel.tackle_loose_margin = m;
    }
    for (k, f) in [("AWON", &mut sim.cfg.ai.aerial_won_keep), ("ALOST", &mut sim.cfg.ai.aerial_lost_keep), ("AVAL", &mut sim.cfg.ai.aerial_value_factor)] {
        if let Some(v) = std::env::var(k).ok().and_then(|s| s.parse().ok()) {
            *f = v;
        }
    }
    sim.decision_log = Some(Vec::new());
    let mut ag = Agg { drift: 0.0, drift_n: 0, off_target: 0.0, dec: 0, best: 0, gap: 0.0, passes: 0, completed: 0, predicted: 0.0, pred_n: 0, goals: [0, 0], shots: [0, 0], xg: [0.0, 0.0], tackles_won: [0, 0], side: [[0; 6]; 2], thirds: [[0.0; 6]; 2], dirs: [[0.0; 4]; 2], lens: [[[0.0; 2]; 3]; 2], counters: [[0.0; 3]; 2], longs: [[0.0; 6]; 2], zones: [[0.0; 4]; 2], second: [[0.0; 2]; 2], near: 0, poss: [0.0, 0.0] };
    let mut seen = 0;
    let mut tick = 0u64;
    let mut last_poss = sim.state.possession;
    let mut open_pass: Option<(usize, usize)> = None;
    let mut aerial_open: Option<(usize, f32)> = None;
    let mut counter: Option<(match_engine::pitch::Side, f32, bool)> = None;
    // Completed forward long pass by the side: time, side; resolved 5 s later.
    let mut long_open: Option<(usize, f32, usize)> = None;
    while !sim.finished() {
        sim.step();
        tick += 1;
        if tick.is_multiple_of(50) && sim.state.is_live() {
            for p in sim.state.players.iter().filter(|p| !p.is_gk()) {
                ag.drift += p.drift.length() as f64;
                ag.off_target += (p.pos - p.target).length() as f64;
                ag.drift_n += 1;
            }
        }
        while seen < sim.events.len() {
            let sd = |id: usize| match_engine::match_state::side_of(id).index();
            match sim.events[seen].kind {
                EventKind::Pass { by, to: Some(_), length, headed: false, .. } => {
                    ag.pred_n += 1;
                    let bin = if length < 10.0 { 0 } else if length < 30.0 { 1 } else { 2 };
                    ag.lens[sd(by)][bin][0] += 1.0;
                    let p = &sim.state.players[by];
                    let fwd = bin == 2 && sim.state.flight.is_some_and(|f| sim.state.frame(p.side).to_team(f.target - p.pos).x > 10.0);
                    let mut code = bin + if fwd { 10 } else { 0 };
                    if bin == 2 {
                        ag.longs[sd(by)][if fwd { 0 } else { 1 }] += 1.0;
                    }
                    if fwd && let Some(f) = sim.state.flight {
                        let fr = sim.state.frame(p.side);
                        let tx = fr.to_team(f.target).x;
                        let last = sim.state.squad(p.side.opponent()).iter().filter(|o| !o.is_gk()).map(|o| fr.to_team(o.pos).x).fold(f32::MIN, f32::max);
                        if tx > 17.5 {
                            ag.zones[sd(by)][0] += 1.0;
                            code += 100;
                        }
                        if tx > last {
                            ag.zones[sd(by)][2] += 1.0;
                            code += 1000;
                        }
                    }
                    open_pass = Some((sd(by), code));
                    let p = &sim.state.players[by];
                    ag.predicted += p.debug.chosen_p as f64;
                    let d = &mut ag.dirs[sd(by)];
                    d[2] += p.debug.chosen_p as f64;
                    d[3] += 1.0;
                    if let Some(f) = sim.state.flight {
                        let dx = sim.state.frame(p.side).to_team(f.target - p.pos).x;
                        if dx > 3.0 {
                            d[0] += 1.0;
                        } else if dx < -3.0 {
                            d[1] += 1.0;
                        }
                    }
                }
                EventKind::Shot { by, .. } => {
                    if let Some((side, since, _)) = counter
                        && side.index() == sd(by)
                        && sim.state.time - since <= 10.0
                    {
                        ag.counters[side.index()][2] += 1.0;
                        counter = None;
                    }
                    if let Some((side, _, _)) = long_open
                        && side == sd(by)
                    {
                        ag.longs[side][5] += 1.0;
                        long_open = None;
                    }
                }
                EventKind::Aerial { won, lost } => {
                    if sd(won) != sd(lost) && sd(lost) == last_poss.index() {
                        ag.second[sd(lost)][0] += 1.0;
                        aerial_open = Some((sd(lost), sim.state.time));
                    }
                }
                EventKind::Tackle { by, .. } => ag.side[sd(by)][2] += 1,
                EventKind::Interception { by } => ag.side[sd(by)][3] += 1,
                EventKind::Miscontrol { by } => ag.side[sd(by)][4] += 1,
                _ => {}
            }
            seen += 1;
        }
        let third = |side: match_engine::pitch::Side, st: &match_engine::match_state::MatchState| {
            let x = st.frame(side).to_team(st.ball.ground_pos()).x;
            if x < -17.5 { 0 } else if x > 17.5 { 2 } else { 1 }
        };
        if let Some((side, bin)) = open_pass
            && sim.state.flight.is_none()
        {
            if sim.state.possession.index() == side {
                ag.lens[side][bin % 10][1] += 1.0;
                if bin % 100 >= 10 {
                    ag.longs[side][2] += 1.0;
                    long_open = Some((side, sim.state.time, 0));
                }
                if (bin / 100) % 10 == 1 {
                    ag.zones[side][1] += 1.0;
                }
                if bin >= 1000 {
                    ag.zones[side][3] += 1.0;
                }
            }
            open_pass = None;
        }
        if sim.state.possession != last_poss {
            ag.thirds[last_poss.index()][3 + third(last_poss, &sim.state)] += 1.0;
            last_poss = sim.state.possession;
            ag.side[last_poss.index()][5] += 1;
            counter = None;
            if third(last_poss, &sim.state) == 0 {
                ag.counters[last_poss.index()][0] += 1.0;
                counter = Some((last_poss, sim.state.time, false));
            }
            if let Some((side, _, _)) = long_open {
                ag.longs[side][4] += 1.0;
                long_open = None;
            }
        }
        if let Some((side, since, false)) = counter
            && sim.state.time - since <= 10.0
            && third(side, &sim.state) == 2
        {
            ag.counters[side.index()][1] += 1.0;
            counter = Some((side, since, true));
        }
        if let Some((side, t)) = aerial_open
            && (sim.state.time - t > 4.0 || !sim.state.is_live())
        {
            if sim.state.is_live() && sim.state.possession.index() == side {
                ag.second[side][1] += 1.0;
            }
            aerial_open = None;
        }
        if let Some((side, t, _)) = long_open
            && sim.state.time - t > 5.0
        {
            ag.longs[side][3] += 1.0;
            long_open = None;
        }
        if sim.state.is_live() {
            ag.thirds[last_poss.index()][third(last_poss, &sim.state)] += sim.cfg.timing.dt as f64;
        }
    }
    let log = sim.decision_log.take().unwrap();
    // `DUMP=n`: print the first n away-side decisions taken in the own third with the ball on the ground.
    if let Some(n) = std::env::var("DUMP").ok().and_then(|s| s.parse::<usize>().ok())
        && seed == 0
    {
        let mut shown = 0;
        for (t, sit, opts, chosen) in &log {
            let who = sit.carrier;
            let p = &sim.state.players[who];
            let bx = sim.state.frame(p.side).to_team(sit.ball.truncate()).x;
            if p.side != match_engine::pitch::Side::Away || p.is_gk() || opts.len() < 3 || *t < 60.0 || bx > -5.0 {
                continue;
            }
            let x = opts[*chosen].label.clone();
            let mut sorted: Vec<&match_engine::ai::on_ball::OptionEval> = opts.iter().collect();
            sorted.sort_by(|a, b| b.u_opt.total_cmp(&a.u_opt));
            // Only decisions where a long forward pass was on the table.
            if !sorted.iter().any(|o| o.label.contains("lofted") || o.label.contains("long")) {
                continue;
            }
            println!("  t {t:.0} player {who} chose '{x}'");
            for o in sorted.iter().take(8) {
                println!("      u {:+.4} p {:.2} pressed {:.2} prob {:.2}  {}  [{}]", o.u_opt, o.p, o.pressed, o.prob, o.label, o.rules.join(","));
            }
            shown += 1;
            if shown >= n {
                break;
            }
        }
    }
    for (_, _, opts, chosen) in &log {
        if opts.len() < 2 {
            continue;
        }
        let best = opts.iter().enumerate().max_by(|a, b| a.1.u_opt.total_cmp(&b.1.u_opt)).unwrap();
        ag.dec += 1;
        if best.0 == *chosen {
            ag.best += 1;
        }
        ag.gap += (best.1.u_opt - opts[*chosen].u_opt) as f64;
        if best.1.u_opt - opts[*chosen].u_opt <= 0.0005 {
            ag.near += 1;
        }
    }
    // Completion from the stats: a pass counted complete when the team kept the ball.
    let st = &sim.stats.teams;
    ag.completed = st.iter().map(|s| s.passes_completed as u64).sum();
    ag.passes = st.iter().map(|s| s.passes as u64).sum();
    ag.goals = [st[0].goals as u64, st[1].goals as u64];
    ag.shots = [st[0].shots as u64, st[1].shots as u64];
    ag.xg = [st[0].xg as f64, st[1].xg as f64];
    ag.tackles_won = [st[0].tackles_won as u64, st[1].tackles_won as u64];
    ag.poss = [st[0].possession_s as f64, st[1].possession_s as f64];
    for (side, team) in ag.side.iter_mut().zip(st) {
        side[0] = team.passes as u64;
        side[1] = team.passes_completed as u64;
    }
    ag
}

fn main() {
    let n: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(4);
    let vals: Vec<f32> = std::env::var("ATTRS").unwrap_or("100,60,20".into()).split(',').filter_map(|s| s.parse().ok()).collect();
    println!("{n} matches each; every outfield and keeper attribute set to the value (feet, height and weight kept)");
    println!("{:>6} | {:>10} {:>12} | {:>8} {:>12} {:>12} | {:>8} {:>10} {:>6}", "attrs", "drift m", "off target m", "on-ball", "layer-1 best", "within 0.0005", "passes", "completed", "AI p");
    for v in vals {
        let ags: Vec<Agg> = std::thread::scope(|s| {
            let hs: Vec<_> = (0..n).map(|seed| s.spawn(move || play(seed, v))).collect();
            hs.into_iter().map(|h| h.join().unwrap()).collect()
        });
        let sum = |f: &dyn Fn(&Agg) -> f64| ags.iter().map(f).sum::<f64>();
        let dn = sum(&|a| a.drift_n as f64);
        let dec = sum(&|a| a.dec as f64);
        let passes = sum(&|a| a.passes as f64);
        println!(
            "{v:>6.0} | {:>10.2} {:>12.2} | {:>8.0} {:>11.0}% {:>13.0}% | {:>8.0} {:>9.0}% {:>5.0}%",
            sum(&|a| a.drift) / dn,
            sum(&|a| a.off_target) / dn,
            dec / n as f64,
            100.0 * sum(&|a| a.best as f64) / dec,
            100.0 * sum(&|a| a.near as f64) / dec,
            passes / n as f64,
            100.0 * sum(&|a| a.completed as f64) / passes,
            100.0 * sum(&|a| a.predicted) / sum(&|a| a.pred_n as f64)
        );
        let m = n as f64;
        println!(
            "         home / away per match: goals {:.2} / {:.2}, shots {:.1} / {:.1}, xG {:.2} / {:.2}, tackles won {:.0} / {:.0}, possession home {:.0}%",
            sum(&|a| a.goals[0] as f64) / m, sum(&|a| a.goals[1] as f64) / m, sum(&|a| a.shots[0] as f64) / m, sum(&|a| a.shots[1] as f64) / m,
            sum(&|a| a.xg[0]) / m, sum(&|a| a.xg[1]) / m, sum(&|a| a.tackles_won[0] as f64) / m, sum(&|a| a.tackles_won[1] as f64) / m,
            100.0 * sum(&|a| a.poss[0]) / sum(&|a| a.poss[0] + a.poss[1])
        );
        if std::env::var("SIDES").is_ok() {
            for (i, name) in ["home", "away"].iter().enumerate() {
                let f = |j: usize| sum(&|a| a.side[i][j] as f64) / m;
                println!(
                    "         {name}: passes {:.0} ({:.0}% completed), tackles {:.0} ({:.0}% won), interceptions {:.0}, miscontrols {:.0}, possessions {:.0} ({:.1} s each)",
                    f(0), 100.0 * f(1) / f(0).max(1.0), f(2), 100.0 * sum(&|a| a.tackles_won[i] as f64) / m / f(2).max(1.0), f(3), f(4), f(5),
                    sum(&|a| a.poss[i]) / m / f(5).max(1.0)
                );
                let t = |j: usize| sum(&|a| a.thirds[i][j]) / m;
                let l = |b: usize, j: usize| sum(&|a| a.lens[i][b][j]) / m;
                let c = |j: usize| sum(&|a| a.counters[i][j]) / m;
                println!(
                    "               passes < 10 m {:.0} ({:.0}%), 10–30 m {:.0} ({:.0}%), ≥ 30 m {:.0} ({:.0}%); won in own third {:.0}, final third within 10 s {:.0}, shot within 10 s {:.1}",
                    l(0, 0), 100.0 * l(0, 1) / l(0, 0).max(1.0), l(1, 0), 100.0 * l(1, 1) / l(1, 0).max(1.0), l(2, 0), 100.0 * l(2, 1) / l(2, 0).max(1.0),
                    c(0), c(1), c(2)
                );
                let g = |j: usize| sum(&|a| a.longs[i][j]) / m;
                println!(
                    "               ≥ 30 m forward {:.0} ({:.0}% completed) other {:.0}; after a completed forward one: kept 5 s {:.0}, lost {:.0}, shot {:.1}",
                    g(0), 100.0 * g(2) / g(0).max(1.0), g(1), g(3), g(4), g(5)
                );
                let z = |j: usize| sum(&|a| a.zones[i][j]) / m;
                println!(
                    "               of which into the final third {:.0} ({:.0}% completed), behind the last defender {:.0} ({:.0}% completed)",
                    z(0), 100.0 * z(1) / z(0).max(1.0), z(2), 100.0 * z(3) / z(2).max(1.0)
                );
                let sb = |j: usize| sum(&|a| a.second[i][j]) / m;
                println!("               aerials lost after our kick {:.1}, ball back with us 4 s later {:.0}%", sb(0), 100.0 * sb(1) / sb(0).max(0.1));
                let d = |j: usize| sum(&|a| a.dirs[i][j]);
                println!(
                    "               possession by third own/mid/final {:.0}/{:.0}/{:.0} s, turnovers there {:.0}/{:.0}/{:.0}; passes forward {:.0}% back {:.0}%, chosen p {:.2}",
                    t(0), t(1), t(2), t(3), t(4), t(5), 100.0 * d(0) / d(3).max(1.0), 100.0 * d(1) / d(3).max(1.0), d(2) / d(3).max(1.0)
                );
            }
        }
    }
}
