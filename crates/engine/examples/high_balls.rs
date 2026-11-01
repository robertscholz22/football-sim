//! Lofted open-play passes of 30–45 m, measured as `sb_high_balls.py` measures StatsBomb high passes: per match,
//! outcome of the first contact after the kick (a team-mate: complete; out of play; an opponent: failed), whether it
//! was an aerial duel or an opponent's header, and the nearest opponent to the contact point at release and to the
//! player making contact at the contact. Split into all and forward (> 5 m) passes.
//! `cargo run -p match_engine --release --example high_balls -- [matches]`

use match_engine::Simulation;
use match_engine::Vec2;
use match_engine::events::EventKind;
use match_engine::match_state::side_of;
use match_engine::pitch::{HALF_LENGTH, HALF_WIDTH};
use match_engine::player::{Intent, KickKind};
use match_engine::team::Role;
use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq)]
enum Outcome {
    Complete,
    Out,
    Failed,
}

#[derive(Clone, Copy)]
struct Rec {
    forward: bool,
    outcome: Outcome,
    aerial: bool,
    /// The aerial duel at the end was won by the passing team.
    aerial_won: bool,
    opp_header: bool,
    /// A header by the passing team with no duel.
    own_header: bool,
    /// Success probability the passer's AI gave the pass.
    p: f32,
    /// Nearest opponent to the contact point at release / to the player making contact at the contact (m).
    at_release: f32,
    at_contact: f32,
    /// How long (s) the kicker was setting the kick up (0: struck at once).
    setup: f32,
    /// Nearest team-mate (not the kicker or keeper) to the contact point at release (m).
    mate_at_release: f32,
    /// Kicker's and intended receiver's role, the kicker's distance from his own goal line and the aim's lateral
    /// distance from him (m).
    roles: (Role, Option<Role>),
    x: f32,
    dy: f32,
    /// Aim's distance from the nearer touchline (m).
    line: f32,
}

struct Open {
    kicker: usize,
    forward: bool,
    /// Opponent / team-mate (outfield, not the kicker) positions at release.
    opponents: Vec<Vec2>,
    mates: Vec<Vec2>,
    aerial: bool,
    aerial_won: bool,
    header: Option<usize>,
    setup: f32,
    /// `TRACE=out`: intended receiver, and the closest the ball came to him (horizontal m, height m, time, his label).
    receiver: Option<usize>,
    closest: (f32, f32, f32, String),
    /// `TRACE=contest`: every 0.2 s, (time since release, ball, receiver position and label, opponents' position,
    /// speed and label).
    log: Vec<Snap>,
    release: f32,
    p: f32,
    roles: (Role, Option<Role>),
    x: f32,
    dy: f32,
    line: f32,
}

type Snap = (f32, Vec2, Vec2, String, Vec<(Vec2, f32, String)>);

fn nearest(ps: &[Vec2], at: Vec2) -> f32 {
    ps.iter().map(|p| (*p - at).length()).fold(f32::MAX, f32::min)
}

fn play(seed: u64) -> Vec<Rec> {
    let mut sim = Simulation::from_builtin("northbridge", "harbour", seed);
    let mut out = Vec::new();
    let mut open: Option<Open> = None;
    let mut seen = 0;
    let mut live = false;
    // Since when each player has been setting up a lofted pass.
    let mut setup_since = [f32::NAN; 22];
    while !sim.finished() {
        sim.step();
        let st = &sim.state;
        while seen < sim.events.len() {
            let e = &sim.events[seen];
            seen += 1;
            match e.kind {
                EventKind::Pass { by, kind: KickKind::LoftedPass, length, to, headed: false, .. }
                    if live && (30.0..45.0).contains(&length) && st.flight.is_some() =>
                {
                    let p = &st.players[by];
                    let f = st.flight.unwrap();
                    let fr = st.frame(p.side);
                    open = Some(Open {
                        kicker: by,
                        forward: fr.to_team(f.target).x - fr.to_team(p.pos).x > 5.0,
                        opponents: st.squad(p.side.opponent()).iter().map(|q| q.pos).collect(),
                        mates: st.squad(p.side).iter().filter(|q| q.id != by && !q.is_gk()).map(|q| q.pos).collect(),
                        aerial: false,
                        aerial_won: false,
                        header: None,
                        setup: if setup_since[by].is_nan() { 0.0 } else { e.t - setup_since[by] },
                        receiver: to,
                        closest: (f32::MAX, 0.0, 0.0, String::new()),
                        log: Vec::new(),
                        release: e.t,
                        p: p.debug.chosen_p,
                        roles: (p.role, to.map(|r| st.players[r].role)),
                        x: HALF_LENGTH + fr.to_team(p.pos).x,
                        dy: (fr.to_team(f.target).y - fr.to_team(p.pos).y).abs(),
                        line: HALF_WIDTH - f.target.y.abs(),
                    });
                }
                EventKind::Aerial { won, .. } => {
                    if let Some(o) = open.as_mut()
                        && !o.aerial
                    {
                        o.aerial = true;
                        o.aerial_won = side_of(won) == side_of(o.kicker);
                    }
                }
                EventKind::Header { by } => {
                    if let Some(o) = open.as_mut() {
                        o.header.get_or_insert(by);
                    }
                }
                EventKind::Restart { .. } => {
                    if let Some(o) = open.take() {
                        let at = st.ball.ground_pos();
                        if std::env::var("TRACE").is_ok_and(|v| v == "out") && seed == 1 {
                            println!(
                                "out: receiver {:?} closest {:.2} m at height {:.2} m, t {:.2}, '{}' | ball out at {:?}",
                                o.receiver.map(|r| st.players[r].role),
                                o.closest.0,
                                o.closest.1,
                                o.closest.2,
                                o.closest.3,
                                at
                            );
                        }
                        let at_release = nearest(&o.opponents, at);
                        out.push(Rec {
                            forward: o.forward,
                            outcome: Outcome::Out,
                            aerial: false,
                            aerial_won: false,
                            opp_header: false,
                            own_header: false,
                            p: o.p,
                            at_release,
                            at_contact: f32::NAN,
                            setup: o.setup,
                            mate_at_release: nearest(&o.mates, at),
                            roles: o.roles,
                            x: o.x,
                            dy: o.dy,
                            line: o.line,
                        });
                    }
                }
                _ => {}
            }
        }
        live = st.is_live();
        for p in &st.players {
            let lofted = matches!(p.intent, Intent::SetUp { plan, .. } if plan.kind == KickKind::LoftedPass);
            if !lofted {
                setup_since[p.id] = f32::NAN;
            } else if setup_since[p.id].is_nan() {
                setup_since[p.id] = st.time - sim.cfg.timing.dt;
            }
        }
        if let Some(o) = open.as_mut()
            && let Some(r) = o.receiver
        {
            let d = (st.ball.pos.truncate() - st.players[r].pos).length();
            if d < o.closest.0 {
                o.closest = (d, st.ball.pos.z, st.time, st.players[r].debug.label.clone());
            }
        }
        let tracing = std::env::var("TRACE").is_ok_and(|v| v == "contest" || v == "labels");
        if tracing
            && let Some(o) = open.as_mut()
            && let Some(r) = o.receiver
            && ((st.time - o.release) / 0.2).fract() < sim.cfg.timing.dt / 0.2
        {
            let q = &st.players[r];
            let opp = st.squad(q.side.opponent()).iter().map(|x| (x.pos, x.vel.length(), x.debug.label.clone())).collect();
            o.log.push((st.time - o.release, st.ball.ground_pos(), q.pos, q.debug.label.clone(), opp));
        }
        let Some(o) = open.as_ref() else { continue };
        let Some(t) = st.last_touch.filter(|&t| t != o.kicker) else { continue };
        let side = side_of(o.kicker);
        let at = st.players[t].pos;
        let opponents: Vec<Vec2> = st.squad(side.opponent()).iter().map(|q| q.pos).collect();
        let mine = side_of(t) == side;
        let margin = nearest(&o.opponents, at) - nearest(&o.mates, at);
        if std::env::var("TRACE").is_ok_and(|v| v == "labels") && margin < 0.0 {
            let (k, _) = o.opponents.iter().enumerate().min_by(|a, b| (*a.1 - at).length().total_cmp(&(*b.1 - at).length())).unwrap();
            let snap = o.log.iter().find(|s| s.0 >= 0.6).or(o.log.last());
            if let Some(s) = snap {
                println!("label {} {}", if mine { "complete" } else { "failed" }, s.4[k].2.split(" →").next().unwrap_or(""));
            }
        }
        if tracing && seed == 1 && std::env::var("TRACE").is_ok_and(|v| v == "contest") && margin < 0.0 && mine && o.header.is_some_and(|h| h == t) && !o.aerial {
            let (k, _) = o.opponents.iter().enumerate().min_by(|a, b| (*a.1 - at).length().total_cmp(&(*b.1 - at).length())).unwrap();
            println!("contested ({margin:.1} m) won by an own header of {:?}:", st.players[t].role);
            for (dt, ball, rp, rl, opp) in &o.log {
                let (dp, dv, dl) = &opp[k];
                println!(
                    "  t+{dt:.1} receiver {:.1} m from contact '{rl}' | defender {:.1} m from contact, {:.1} m/s '{dl}' | ball {:.1} m from contact",
                    (*rp - at).length(),
                    (*dp - at).length(),
                    dv,
                    (*ball - at).length()
                );
            }
        }
        out.push(Rec {
            forward: o.forward,
            outcome: if mine { Outcome::Complete } else { Outcome::Failed },
            aerial: o.aerial,
            aerial_won: o.aerial_won,
            opp_header: o.header.is_some_and(|h| side_of(h) != side) && !o.aerial,
            own_header: o.header.is_some_and(|h| side_of(h) == side) && !o.aerial,
            p: o.p,
            at_release: nearest(&o.opponents, at),
            at_contact: if mine { nearest(&opponents, at) } else { f32::NAN },
            setup: o.setup,
            mate_at_release: nearest(&o.mates, at),
            roles: o.roles,
            x: o.x,
            dy: o.dy,
            line: o.line,
        });
        open = None;
    }
    out
}

fn median(mut v: Vec<f32>) -> f32 {
    v.sort_by(f32::total_cmp);
    v.get(v.len() / 2).copied().unwrap_or(f32::NAN)
}

fn share(v: &[f32], f: impl Fn(f32) -> bool) -> f32 {
    100.0 * v.iter().filter(|&&x| f(x)).count() as f32 / v.len().max(1) as f32
}

fn main() {
    let n: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(10);
    let res: Vec<Rec> = std::thread::scope(|scope| {
        let hs: Vec<_> = (1..=n).map(|seed| scope.spawn(move || play(seed))).collect();
        hs.into_iter().flat_map(|h| h.join().unwrap()).collect()
    });
    println!("{n} matches, open-play lofted passes of 30–45 m (both teams)");
    for (label, fwd_only) in [("all", false), ("forward", true)] {
        let r: Vec<&Rec> = res.iter().filter(|x| !fwd_only || x.forward).collect();
        let k = r.len().max(1) as f32;
        let pct = |f: &dyn Fn(&Rec) -> bool| 100.0 * r.iter().filter(|x| f(x)).count() as f32 / k;
        println!(
            "{label:<8} {:5.1}/match: complete {:.0}%, failed {:.0}%, out {:.0}% | aerial duel at the end {:.0}%, opponent header (no duel) {:.0}%",
            r.len() as f32 / n as f32,
            pct(&|x| x.outcome == Outcome::Complete),
            pct(&|x| x.outcome == Outcome::Failed),
            pct(&|x| x.outcome == Outcome::Out),
            pct(&|x| x.aerial),
            pct(&|x| x.opp_header),
        );
        let a = r.iter().filter(|x| x.aerial).count().max(1) as f32;
        println!(
            "    aerial duels won by the passing team {:.0}%, pass complete {:.0}%",
            100.0 * r.iter().filter(|x| x.aerial && x.aerial_won).count() as f32 / a,
            100.0 * r.iter().filter(|x| x.aerial && x.outcome == Outcome::Complete).count() as f32 / a
        );
        let c: Vec<&&Rec> = r.iter().filter(|x| x.at_release - x.mate_at_release < 3.0).collect();
        let k = c.len().max(1) as f32;
        let pc = |f: &dyn Fn(&Rec) -> bool| 100.0 * c.iter().filter(|x| f(x)).count() as f32 / k;
        println!(
            "    contested (margin < 3 m) {:.0}%: complete by aerial {:.0}%, own header {:.0}%, other {:.0}% | failed by aerial {:.0}%, opponent header {:.0}%, other {:.0}% | out {:.0}%",
            100.0 * k / r.len().max(1) as f32,
            pc(&|x| x.outcome == Outcome::Complete && x.aerial),
            pc(&|x| x.outcome == Outcome::Complete && x.own_header),
            pc(&|x| x.outcome == Outcome::Complete && !x.aerial && !x.own_header),
            pc(&|x| x.outcome == Outcome::Failed && x.aerial),
            pc(&|x| x.outcome == Outcome::Failed && x.opp_header),
            pc(&|x| x.outcome == Outcome::Failed && !x.aerial && !x.opp_header),
            pc(&|x| x.outcome == Outcome::Out),
        );
        let su: Vec<f32> = r.iter().map(|x| x.setup).collect();
        println!("    set-up before the kick: median {:.2} s, none {:.0}%, ≥ 0.3 s {:.0}%", median(su.clone()), share(&su, |t| t == 0.0), share(&su, |t| t >= 0.3));
        for (lo, hi) in [(0.0, 5.0), (5.0, 10.0), (10.0, 15.0), (15.0, 99.0)] {
            let c: Vec<&&Rec> = r.iter().filter(|x| (lo..hi).contains(&x.at_release)).collect();
            let k = c.len().max(1) as f32;
            let d1: Vec<f32> = c.iter().filter(|x| x.outcome == Outcome::Complete).map(|x| x.at_contact).collect();
            println!(
                "    release nearest {lo:2.0}-{hi:2.0} m: {:5.1}/match complete {:3.0}%, out {:3.0}% | complete: contact med {:4.1} m, < 2 m {:3.0}%",
                c.len() as f32 / n as f32,
                100.0 * c.iter().filter(|x| x.outcome == Outcome::Complete).count() as f32 / k,
                100.0 * c.iter().filter(|x| x.outcome == Outcome::Out).count() as f32 / k,
                median(d1.clone()),
                share(&d1, |d| d < 2.0)
            );
        }
        for (lo, hi) in [(-99.0, -5.0), (-5.0, 0.0), (0.0, 3.0), (3.0, 6.0), (6.0, 10.0), (10.0, 99.0)] {
            let c: Vec<&&Rec> = r.iter().filter(|x| (lo..hi).contains(&(x.at_release - x.mate_at_release))).collect();
            let k = c.len().max(1) as f32;
            println!(
                "    margin (opponent - team-mate at release) {lo:3.0}..{hi:3.0} m: {:3.0}% complete {:3.0}%, out {:3.0}% | AI p {:3.0}%",
                100.0 * c.len() as f32 / r.len().max(1) as f32,
                100.0 * c.iter().filter(|x| x.outcome == Outcome::Complete).count() as f32 / k,
                100.0 * c.iter().filter(|x| x.outcome == Outcome::Out).count() as f32 / k,
                100.0 * c.iter().map(|x| x.p).sum::<f32>() / k,
            );
        }
        let cm: Vec<f32> = r.iter().filter(|x| x.outcome == Outcome::Complete).map(|x| x.mate_at_release).collect();
        println!(
            "    complete: nearest team-mate to the contact point at release med {:4.1} m, < 3 m {:3.0}%, > 8 m {:3.0}%",
            median(cm.clone()),
            share(&cm, |d| d < 3.0),
            share(&cm, |d| d > 8.0)
        );
        for (lo, hi) in [(0.0, 4.0), (4.0, 8.0), (8.0, 99.0)] {
            let c: Vec<&&Rec> =
                r.iter().filter(|x| x.outcome == Outcome::Complete && (lo..hi).contains(&x.mate_at_release)).collect();
            let d0: Vec<f32> = c.iter().map(|x| x.at_release).collect();
            let d1: Vec<f32> = c.iter().map(|x| x.at_contact).collect();
            println!(
                "    complete, team-mate {lo:.0}-{hi:.0} m from the contact point at release: {:5.1}/match nearest opponent at release med {:4.1} m -> contact med {:4.1} m, < 2 m {:3.0}%",
                c.len() as f32 / n as f32,
                median(d0),
                median(d1.clone()),
                share(&d1, |d| d < 2.0)
            );
        }
        for (name, oc) in [("complete", true), ("failed  ", false)] {
            let c: Vec<&&Rec> = r.iter().filter(|x| (x.outcome == Outcome::Complete) == oc).collect();
            let d0: Vec<f32> = c.iter().map(|x| x.at_release).collect();
            let d1: Vec<f32> = c.iter().map(|x| x.at_contact).filter(|d| d.is_finite()).collect();
            print!("    {name}: release med {:4.1} m, < 5 m {:3.0}%, < 8 m {:3.0}%", median(d0.clone()), share(&d0, |d| d < 5.0), share(&d0, |d| d < 8.0));
            if oc {
                print!(" | contact med {:4.1} m, < 2 m {:3.0}%, < 5 m {:3.0}%", median(d1.clone()), share(&d1, |d| d < 2.0), share(&d1, |d| d < 5.0));
            }
            println!();
        }
    }
    // Who plays and receives them (`sb_long_who.py` for StatsBomb), free (no opponent within 15 m of the contact point
    // at release) and contested.
    for (name, free) in [("free (opponent ≥ 15 m)", true), ("contested (< 15 m)", false)] {
        let r: Vec<&Rec> = res.iter().filter(|x| (x.at_release >= 15.0) == free).collect();
        let k = r.len().max(1) as f32;
        let by = |f: &dyn Fn(&Rec) -> String| {
            let mut m: BTreeMap<String, u32> = BTreeMap::new();
            for x in &r {
                *m.entry(f(x)).or_default() += 1;
            }
            let mut v: Vec<(String, u32)> = m.into_iter().collect();
            v.sort_by_key(|x| std::cmp::Reverse(x.1));
            v.iter().take(8).map(|(q, c)| format!("{q} {:.0}%", 100.0 * *c as f32 / k)).collect::<Vec<_>>().join(", ")
        };
        let third = |x: f32| if x < 35.0 { "own" } else if x < 70.0 { "middle" } else { "final" };
        println!(
            "{name}: {:.1}/match complete {:.0}%, lateral ≥ 25 m {:.0}%, forward {:.0}%",
            r.len() as f32 / n as f32,
            100.0 * r.iter().filter(|x| x.outcome == Outcome::Complete).count() as f32 / k,
            100.0 * r.iter().filter(|x| x.dy >= 25.0).count() as f32 / k,
            100.0 * r.iter().filter(|x| x.forward).count() as f32 / k
        );
        println!("    from: {}", by(&|x| format!("{} {:?}", third(x.x), x.roles.0)));
        println!("    to:   {}", by(&|x| x.roles.1.map_or("-".to_string(), |q| format!("{q:?}"))));
        for (lo, hi) in [(-9.0, 3.0), (3.0, 6.0), (6.0, 10.0), (10.0, 40.0)] {
            let c: Vec<&&Rec> = r.iter().filter(|x| (lo..hi).contains(&x.line)).collect();
            let k = c.len().max(1) as f32;
            println!(
                "    aim {lo:2.0}-{hi:2.0} m from the touchline: {:3.0}%, complete {:3.0}%, out {:3.0}%",
                100.0 * c.len() as f32 / r.len().max(1) as f32,
                100.0 * c.iter().filter(|x| x.outcome == Outcome::Complete).count() as f32 / k,
                100.0 * c.iter().filter(|x| x.outcome == Outcome::Out).count() as f32 / k
            );
        }
    }
}
