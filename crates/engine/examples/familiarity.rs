//! Familiarity bench (docs/player-brain.md section 15): one squad against itself, settled against a familiarity level
//! (`Settledness`: new manager / new system, several signings, just assembled). Per level: the settled side's xG and
//! goal difference per match (targets +0.04, +0.08 and +0.25 goals) and both sides' coordination errors per minute of
//! live play: no press and double press (out of possession in the block with an opponent on the ball: nobody, or two
//! or more, claim the first defender), a man marked twice, the rest defence short of four, two runs in behind into
//! the same space; with the mean length of no-press and double-press episodes. In possession also the share of
//! team-mates whose movement is not the concept's (`Plan::concept`), and of those the share the carrier has not read
//! yet. Seeds alternate the squad and which side is settled. Usage: familiarity [matches per level] [levels:
//! new,signings,assembled or role:partners:practice, e.g. 0.6:0.9:0.9]; CONFIG=<file.ron>.

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use match_engine::ai::brain::{AttDuty, DefDuty, OutPhase, TeamPhase};
use match_engine::ai::reading;
use match_engine::config::SimConfig;
use match_engine::match_state::{PLAYERS_PER_TEAM, PlayPhase};
use match_engine::pitch::{PitchConditions, Side};
use match_engine::team::{Movement, Partners, Practice, Settledness, TeamData};
use match_engine::{Simulation, Vec2};

/// Per side: live minutes sampled, no press, double press, marked twice, rest short, duplicate runs (samples), and
/// no-press / double-press episodes and their summed length (samples).
#[derive(Clone, Copy, Default)]
struct Errors {
    samples: f64,
    no_press: f64,
    double_press: f64,
    marked_twice: f64,
    rest_short: f64,
    duplicate_runs: f64,
    episodes: [f64; 2],
    episode_len: [f64; 2],
    /// In possession: team-mates of the carrier sampled, off the concept, of those not yet read by the carrier.
    mates: f64,
    off_concept: f64,
    unread: f64,
    /// Per match: passes, completed passes, shots, xG.
    passes: f64,
    completed: f64,
    shots: f64,
    xg: f64,
}

struct Played {
    level: usize,
    /// Settled side's xG and goals minus the other side's.
    xg_diff: f64,
    goal_diff: f64,
    /// [settled, level].
    errors: [Errors; 2],
}

const SAMPLE_EVERY: u32 = 10;

/// A level: its name and (role familiarity, partners, practice).
type Level = (String, (f32, f32, f32));

fn with_level(mut t: TeamData, (role, partners, practice): (f32, f32, f32)) -> TeamData {
    for p in &mut t.players {
        p.familiarity = role;
        p.role_familiarity.clear();
    }
    t.partners = Partners { default: partners, pairs: Vec::new() };
    t.practice = Practice { formation: practice, concept: practice };
    t
}

fn play(level: (f32, f32, f32), seed: u64, cfg: &SimConfig) -> ([f64; 2], [Errors; 2]) {
    let squad = if seed.is_multiple_of(2) { "northbridge" } else { "harbour" };
    let base = TeamData::builtin(squad).unwrap();
    let settled = base.clone().settled(Settledness::Settled);
    let mut other = with_level(base, level);
    other.short = format!("{}*", other.short);
    let settled_home = (seed / 2).is_multiple_of(2);
    let (home, away) = if settled_home { (&settled, &other) } else { (&other, &settled) };
    let mut sim = Simulation::new(home, away, cfg.clone(), PitchConditions::default(), seed);
    let sides = if settled_home { [Side::Home, Side::Away] } else { [Side::Away, Side::Home] };
    let mut errors = [Errors::default(); 2];
    let mut run = [[0u32; 2]; 2];
    let mut tick = 0u32;
    let overlap = cfg.ai.offer_overlap_m;
    while !sim.finished() {
        sim.step();
        tick += 1;
        let st = &sim.state;
        if st.phase != PlayPhase::Live || !tick.is_multiple_of(SAMPLE_EVERY) {
            continue;
        }
        for (k, &side) in sides.iter().enumerate() {
            let e = &mut errors[k];
            let squad = st.squad(side);
            let own = |i: usize| &squad[i].beliefs;
            e.samples += 1.0;
            let mut now = [false; 2];
            if st.possession == side {
                let rest = (0..PLAYERS_PER_TEAM).filter(|&i| own(i).att_duties[i] == AttDuty::RestDefence).count();
                if rest < 4 {
                    e.rest_short += 1.0;
                }
                let runs: Vec<Vec2> = squad
                    .iter()
                    .filter(|p| p.plan.cue.in_possession && p.plan.kind == Movement::RunInBehind)
                    .filter_map(|p| p.plan.offer)
                    .collect();
                if runs.iter().enumerate().any(|(a, p)| runs[a + 1..].iter().any(|q| (*p - *q).length() < overlap)) {
                    e.duplicate_runs += 1.0;
                }
                if let Some(c) = st.control.filter(|&c| st.players[c].side == side) {
                    for m in squad.iter().filter(|m| m.id != c && !m.is_gk()) {
                        e.mates += 1.0;
                        if m.plan.concept.is_some() {
                            e.off_concept += 1.0;
                            if st.time - m.plan.since < reading::delay(st, &cfg.ai, c, m.id) {
                                e.unread += 1.0;
                            }
                        }
                    }
                }
            } else if st.control.is_some_and(|c| st.players[c].side != side) && own(0).phase == TeamPhase::Out(OutPhase::Block) {
                let firsts = (0..PLAYERS_PER_TEAM).filter(|&i| own(i).def_duties[i] == DefDuty::First).count();
                now = [firsts == 0, firsts >= 2];
                e.no_press += f64::from(u8::from(now[0]));
                e.double_press += f64::from(u8::from(now[1]));
                let marks: Vec<usize> = (0..PLAYERS_PER_TEAM)
                    .filter_map(|i| match own(i).def_duties[i] {
                        DefDuty::Mark(o) | DefDuty::Track(o) => Some(o),
                        _ => None,
                    })
                    .collect();
                if marks.iter().enumerate().any(|(a, o)| marks[a + 1..].contains(o)) {
                    e.marked_twice += 1.0;
                }
            }
            for x in 0..2 {
                if now[x] {
                    run[k][x] += 1;
                } else if run[k][x] > 0 {
                    e.episodes[x] += 1.0;
                    e.episode_len[x] += f64::from(run[k][x]);
                    run[k][x] = 0;
                }
            }
        }
    }
    let t = &sim.stats.teams;
    for (k, side) in sides.iter().enumerate() {
        let ts = &t[side.index()];
        let e = &mut errors[k];
        (e.passes, e.completed, e.shots, e.xg) = (f64::from(ts.passes), f64::from(ts.passes_completed), f64::from(ts.shots), f64::from(ts.xg));
    }
    let (s, o) = (sides[0].index(), sides[1].index());
    ([f64::from(t[s].xg - t[o].xg), f64::from(t[s].goals) - f64::from(t[o].goals)], errors)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let matches: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(40);
    let names = args.get(2).map_or("new,signings,assembled", |s| s.as_str());
    let levels: Vec<Level> = names
        .split(',')
        .map(|n| {
            let named = |l: Settledness| (format!("{l:?}"), l.levels());
            match n {
                "settled" => named(Settledness::Settled),
                "new" => named(Settledness::NewSystem),
                "signings" => named(Settledness::Signings),
                "assembled" => named(Settledness::Assembled),
                _ => {
                    let v: Vec<f32> = n.split(':').map(|x| x.parse().expect("role:partners:practice")).collect();
                    (n.to_string(), (v[0], v[1], v[2]))
                }
            }
        })
        .collect();
    let cfg = std::env::var("CONFIG")
        .map_or_else(|_| SimConfig::default(), |p| SimConfig::from_ron(&std::fs::read_to_string(p).expect("read config")).expect("parse config"));
    let jobs: Vec<(usize, u64)> = (0..levels.len()).flat_map(|l| (1..=matches).map(move |s| (l, s))).collect();
    let next = AtomicUsize::new(0);
    let out = Mutex::new(Vec::new());
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                loop {
                    let k = next.fetch_add(1, Ordering::Relaxed);
                    let Some(&(l, seed)) = jobs.get(k) else { break };
                    let (d, errors) = play(levels[l].1, seed, &cfg);
                    out.lock().unwrap().push(Played { level: l, xg_diff: d[0], goal_diff: d[1], errors });
                }
            });
        }
    });
    let out = out.into_inner().unwrap();
    let targets = |name: &str| match name {
        "Settled" => "0",
        "NewSystem" => "+0.04 (0 to +0.10)",
        "Signings" => "+0.08 (+0.02 to +0.17)",
        "Assembled" => "+0.25 (+0.10 to +0.50)",
        _ => "-",
    };
    println!("{matches} matches per level; settled side's difference per match (target in goals), mean ± standard error");
    for (l, (level, _)) in levels.iter().enumerate() {
        let mine: Vec<&Played> = out.iter().filter(|p| p.level == l).collect();
        let n = mine.len() as f64;
        let stat = |f: &dyn Fn(&Played) -> f64| {
            let m = mine.iter().map(|p| f(p)).sum::<f64>() / n;
            let v = mine.iter().map(|p| (f(p) - m).powi(2)).sum::<f64>() / (n - 1.0).max(1.0);
            (m, (v / n).sqrt())
        };
        let (xg, xg_se) = stat(&|p| p.xg_diff);
        let (gd, gd_se) = stat(&|p| p.goal_diff);
        println!("{level}: xG {xg:+.3} ± {xg_se:.3}, goals {gd:+.3} ± {gd_se:.3}   [target {}]", targets(level));
        for (k, who) in ["settled", "level"].iter().enumerate() {
            let mut e = Errors::default();
            for p in &mine {
                let x = p.errors[k];
                e.samples += x.samples;
                e.no_press += x.no_press;
                e.double_press += x.double_press;
                e.marked_twice += x.marked_twice;
                e.rest_short += x.rest_short;
                e.duplicate_runs += x.duplicate_runs;
                for i in 0..2 {
                    e.episodes[i] += x.episodes[i];
                    e.episode_len[i] += x.episode_len[i];
                }
                e.mates += x.mates;
                e.off_concept += x.off_concept;
                e.unread += x.unread;
                e.passes += x.passes;
                e.completed += x.completed;
                e.shots += x.shots;
                e.xg += x.xg;
            }
            // Share of the sampled time × 60 s; an episode's samples × the sampling interval.
            let rate = |x: f64| x / e.samples * 60.0;
            let dt = f64::from(SAMPLE_EVERY) * f64::from(cfg.timing.dt);
            let len = |i: usize| if e.episodes[i] > 0.0 { e.episode_len[i] / e.episodes[i] * dt } else { 0.0 };
            println!(
                "  {who:8} error-seconds per live minute: no press {:.2} (episodes {:.2} s), double press {:.2} ({:.2} s), marked twice {:.2}, rest short {:.2}, duplicate runs {:.2}; off the concept {:.1} %, unread by the carrier {:.1} %",
                rate(e.no_press),
                len(0),
                rate(e.double_press),
                len(1),
                rate(e.marked_twice),
                rate(e.rest_short),
                rate(e.duplicate_runs),
                100.0 * e.off_concept / e.mates.max(1.0),
                100.0 * e.unread / e.mates.max(1.0)
            );
            println!(
                "  {who:8} per match: passes {:.0} ({:.1} % completed), shots {:.1}, xG {:.2}",
                e.passes / n,
                100.0 * e.completed / e.passes.max(1.0),
                e.shots / n,
                e.xg / n
            );
        }
    }
}
