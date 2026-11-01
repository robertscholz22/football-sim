//! Wide play (M4), measured as `sb_cross.py` measures StatsBomb 360 open data: kicked open-play passes from the wide
//! final third (attacking third, outside the penalty-area lines) by depth band, the share that are crosses
//! (`pitch::is_cross`) and cut-backs (a ground ball from the last 10.5 m back into the box); per cross the first
//! contact (team-mate, opponent, keeper, out), a shot by the crossing team within 4 s before the opponents control the
//! ball or play stops, a goal within 6 s, and the team-mates and defenders in the box at release and at the first
//! contact. `cargo run -p match_engine --release --example wide -- [matches]`, `CONFIG=file.ron`.

use match_engine::Simulation;
use match_engine::config::SimConfig;
use match_engine::events::EventKind;
use match_engine::match_state::{MatchState, side_of};
use match_engine::pitch::{PENALTY_AREA_HALF_WIDTH, Side, in_attacking_box, is_cross};
use match_engine::player::KickKind;
use match_engine::team::Movement;
use std::collections::BTreeMap;

/// Real (StatsBomb 360, 166 matches, open play; `sb_cross.py`).
const REAL: &str = "real: crosses 7.7 per team-match (× 1.48 = 11.4), completed 30 % (ground 35, high 30), shot 16 % \
                    (ground 21), keeper first 2.3 %, in the box at release 2.35 team-mates / 4.54 defenders (FIFA: 3.3 \
                    attackers at arrival); cross share of wide passes 5 / 18 / 36 % by band, team spread p10–p90 5–23 %; \
                    cut-backs 0.62 per team-match, 62 % completed";

/// Depth bands of the wide zone (team-frame x): StatsBomb x 80–96, 96–108, 108–120.
const BANDS: [f32; 3] = [17.5, 31.5, 42.0];

#[derive(Clone, Copy, PartialEq)]
enum Contact {
    Mate,
    Opponent,
    Keeper,
    Out,
}

#[derive(Clone, Copy)]
struct Cross {
    /// Depth band of the release (`BANDS`; 0 also below the first).
    band: usize,
    lofted: bool,
    cutback: bool,
    contact: Contact,
    shot: bool,
    goal: bool,
    header_shot: bool,
    aerial: bool,
    mates_release: u32,
    opps_release: u32,
    mates_contact: u32,
}

struct Open {
    rec: Cross,
    side: Side,
    kicker: usize,
    t: f32,
    touched: bool,
    lost: bool,
}

#[derive(Default)]
struct Tally {
    /// Per band: wide kicked passes, crosses, cut-backs.
    zone: [[u32; 3]; 3],
    /// Per side: wide passes, crosses.
    team: [[u32; 2]; 2],
    crosses: Vec<Cross>,
    /// At each cross: the movements team-mates (outfield, not the crosser) hold, with how many of them stand in the box.
    held: BTreeMap<String, [u32; 2]>,
}

fn in_box_count(st: &MatchState, side: Side, attackers: bool, except: usize) -> u32 {
    let fr = st.frame(side);
    let who = if attackers { side } else { side.opponent() };
    st.squad(who).iter().filter(|p| p.id != except && !p.is_gk() && in_attacking_box(fr.to_team(p.pos))).count() as u32
}

fn play(seed: u64, cfg: &SimConfig) -> Tally {
    let mut sim = Simulation::from_builtin("northbridge", "harbour", seed);
    sim.cfg = cfg.clone();
    let mut tally = Tally::default();
    let mut open: Vec<Open> = Vec::new();
    let mut seen = 0;
    let mut live = false;
    // The last header (player, time): a shot by the same player at the same time is a headed shot.
    let mut header = (usize::MAX, f32::NAN);
    while !sim.finished() {
        sim.step();
        let st = &sim.state;
        while seen < sim.events.len() {
            let e = sim.events[seen].clone();
            seen += 1;
            match e.kind {
                EventKind::Pass { by, kind, headed: false, .. }
                    if live && matches!(kind, KickKind::GroundPass | KickKind::LoftedPass) && st.flight.is_some() =>
                {
                    let p = &st.players[by];
                    let f = st.flight.unwrap();
                    let fr = st.frame(p.side);
                    let (from, to) = (fr.to_team(f.start_pos), fr.to_team(f.target));
                    let lofted = kind == KickKind::LoftedPass;
                    let cross = is_cross(from, to, lofted);
                    let cutback = !lofted && !cross && from.x >= BANDS[2] && in_attacking_box(to) && to.x <= from.x - 1.75;
                    if from.x >= BANDS[0] && from.y.abs() > PENALTY_AREA_HALF_WIDTH {
                        let band = BANDS.iter().rposition(|&b| from.x >= b).unwrap();
                        tally.zone[band][0] += 1;
                        tally.zone[band][1] += u32::from(cross);
                        tally.zone[band][2] += u32::from(cutback);
                        tally.team[p.side.index()][0] += 1;
                        tally.team[p.side.index()][1] += u32::from(cross);
                    }
                    if cross {
                        let fr = st.frame(p.side);
                        for m in st.squad(p.side).iter().filter(|m| m.id != by && !m.is_gk()) {
                            let k = if m.plan.cue.in_possession { m.plan.kind } else { Movement::Hold };
                            let e = tally.held.entry(format!("{k:?}")).or_default();
                            e[0] += 1;
                            e[1] += u32::from(in_attacking_box(fr.to_team(m.pos)));
                        }
                    }
                    if cross || cutback {
                        open.push(Open {
                            rec: Cross {
                                band: BANDS.iter().rposition(|&b| from.x >= b).unwrap_or(0),
                                lofted,
                                cutback,
                                contact: Contact::Out,
                                shot: false,
                                goal: false,
                                header_shot: false,
                                aerial: false,
                                mates_release: in_box_count(st, p.side, true, by),
                                opps_release: in_box_count(st, p.side, false, by),
                                mates_contact: 0,
                            },
                            side: p.side,
                            kicker: by,
                            t: e.t,
                            touched: false,
                            lost: false,
                        });
                    }
                }
                EventKind::Shot { by, .. } => {
                    for o in open.iter_mut().filter(|o| side_of(by) == o.side && !o.lost && e.t - o.t <= 4.0) {
                        o.rec.shot = true;
                        o.rec.header_shot |= header == (by, e.t);
                    }
                }
                EventKind::Header { by } => header = (by, e.t),
                EventKind::Aerial { .. } => {
                    for o in open.iter_mut().filter(|o| !o.touched) {
                        o.rec.aerial = true;
                    }
                }
                EventKind::Goal { side, own_goal: false, .. } => {
                    for o in open.iter_mut().filter(|o| side == o.side && e.t - o.t <= 6.0) {
                        o.rec.goal = true;
                    }
                }
                EventKind::Restart { .. } => {
                    for o in &mut open {
                        o.lost = true;
                    }
                }
                _ => {}
            }
        }
        live = st.is_live();
        for o in &mut open {
            if !o.touched
                && let Some(t) = st.last_touch.filter(|&t| t != o.kicker)
            {
                o.touched = true;
                let q = &st.players[t];
                o.rec.contact = if q.side == o.side {
                    Contact::Mate
                } else if q.is_gk() {
                    Contact::Keeper
                } else {
                    Contact::Opponent
                };
                o.rec.mates_contact = in_box_count(st, o.side, true, usize::MAX);
            }
            if st.control.is_some_and(|c| side_of(c) != o.side) {
                o.lost = true;
            }
        }
        let now = st.time;
        open.retain(|o| {
            let done = now - o.t > 6.0;
            if done {
                tally.crosses.push(o.rec);
            }
            !done
        });
    }
    tally.crosses.extend(open.into_iter().map(|o| o.rec));
    tally
}

fn pct(n: usize, d: usize) -> f64 {
    100.0 * n as f64 / d.max(1) as f64
}

fn report(name: &str, cs: &[&Cross], team_matches: f64) {
    let n = cs.len();
    let count = |f: &dyn Fn(&Cross) -> bool| cs.iter().filter(|c| f(c)).count();
    let mean = |f: &dyn Fn(&Cross) -> u32| cs.iter().map(|c| f(c) as f64).sum::<f64>() / n.max(1) as f64;
    let touched: Vec<&&Cross> = cs.iter().filter(|c| c.contact != Contact::Out).collect();
    let at_contact = touched.iter().map(|c| c.mates_contact as f64).sum::<f64>() / touched.len().max(1) as f64;
    println!(
        "{name:18} n {n:5}  per team-match {:5.2}  completed {:4.1} %  opponent {:4.1} %  keeper {:4.1} %  out {:4.1} %  \
         shot {:4.1} % (headed {:4.1} %)  goal {:4.1} %  aerial {:4.1} %  in the box at release {:.2} team-mates / {:.2} \
         defenders, at first contact {:.2} team-mates",
        n as f64 / team_matches,
        pct(count(&|c| c.contact == Contact::Mate), n),
        pct(count(&|c| c.contact == Contact::Opponent), n),
        pct(count(&|c| c.contact == Contact::Keeper), n),
        pct(count(&|c| c.contact == Contact::Out), n),
        pct(count(&|c| c.shot), n),
        pct(count(&|c| c.header_shot), n),
        pct(count(&|c| c.goal), n),
        pct(count(&|c| c.aerial), n),
        mean(&|c| c.mates_release),
        mean(&|c| c.opps_release),
        at_contact,
    );
}

fn main() {
    let matches: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(20);
    let cfg = std::env::var("CONFIG").map_or_else(
        |_| SimConfig::default(),
        |p| ron::from_str(&std::fs::read_to_string(p).expect("read CONFIG")).expect("parse CONFIG"),
    );
    let tallies: Vec<Tally> = std::thread::scope(|scope| {
        let hs: Vec<_> = (1..=matches).map(|seed| scope.spawn({ let cfg = &cfg; move || play(seed, cfg) })).collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let team_matches = 2.0 * matches as f64;
    let crosses: Vec<&Cross> = tallies.iter().flat_map(|t| &t.crosses).collect();
    println!("{matches} matches; {REAL}");
    let real_cross: Vec<&Cross> = crosses.iter().copied().filter(|c| !c.cutback).collect();
    report("crosses (all)", &real_cross, team_matches);
    report("  ground", &real_cross.iter().copied().filter(|c| !c.lofted).collect::<Vec<_>>(), team_matches);
    report("  lofted", &real_cross.iter().copied().filter(|c| c.lofted).collect::<Vec<_>>(), team_matches);
    for (b, name) in ["  from x 80-96", "  from x 96-108", "  from x 108-120"].iter().enumerate() {
        report(name, &real_cross.iter().copied().filter(|c| c.band == b).collect::<Vec<_>>(), team_matches);
    }
    report("cut-backs", &crosses.iter().copied().filter(|c| c.cutback).collect::<Vec<_>>(), team_matches);
    println!("wide final-third kicked passes: share that are crosses / cut-backs");
    for (b, name) in ["x 80-96", "x 96-108", "x 108-120"].iter().enumerate() {
        let z = tallies.iter().fold([0u32; 3], |a, t| [a[0] + t.zone[b][0], a[1] + t.zone[b][1], a[2] + t.zone[b][2]]);
        println!(
            "  {name:10} passes per team-match {:5.1}  crosses {:4.1} %  cut-backs {:4.1} %",
            z[0] as f64 / team_matches,
            pct(z[1] as usize, z[0] as usize),
            pct(z[2] as usize, z[0] as usize)
        );
    }
    let mut held: BTreeMap<String, [u32; 2]> = BTreeMap::new();
    for t in &tallies {
        for (k, v) in &t.held {
            let e = held.entry(k.clone()).or_default();
            e[0] += v[0];
            e[1] += v[1];
        }
    }
    let n_cross = real_cross.len().max(1) as f64;
    let line: Vec<String> =
        held.iter().map(|(k, v)| format!("{k} {:.2} ({:.2} in the box)", v[0] as f64 / n_cross, v[1] as f64 / n_cross)).collect();
    println!("team-mates' movements per cross: {}", line.join(", "));
    let mut shares: Vec<f64> =
        tallies.iter().flat_map(|t| t.team).filter(|tm| tm[0] >= 15).map(|tm| tm[1] as f64 / tm[0] as f64).collect();
    shares.sort_by(f64::total_cmp);
    if !shares.is_empty() {
        let q = |f: f64| 100.0 * shares[(f * (shares.len() - 1) as f64) as usize];
        println!("  per team-match (>= 15 wide passes, n {}): cross share p10 {:.0} %, p50 {:.0} %, p90 {:.0} %", shares.len(), q(0.1), q(0.5), q(0.9));
    }
}
