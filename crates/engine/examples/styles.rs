//! Style archetypes against the real spread. Real matches differ by style: long high balls to a strong centre
//! forward, deep blocks that counter, short passing with hardly a high ball. Volume and mix stats are therefore not
//! calibration targets: an archetype (squad profile + layer-2 instructions) has to land inside the real spread of
//! its style and move in the right direction, and the situation-conditional stats (completion by the pressure on the
//! passer, first-time share) have to stay where they are in every style, as they do in StatsBomb
//! (`sb_invariance.py`).
//!
//! Each archetype plays as Northbridge (home, 4-3-3) against the default Harbour and as Harbour (away, 4-4-2)
//! against the default Northbridge; two head-to-heads pit contrasting archetypes against each other. Squad overlays
//! trade attributes roughly evenly so that quality stays about the same.
//!
//! Per team-match features as `sb_team_styles.py` (counts scaled to a real match's ≈ 57 minutes in play, ÷ 1.48):
//! directness (share of open-play passes ≥ 30 m), lofted share, possession share, PPDA (opponent passes in their own
//! 60 % per tackle / interception / aerial duel there), height of the team's tackles and interceptions (m from its
//! own goal), open-play passes, first-time share, shots, xG. Env `STYLES_OUT=file.json` writes them for
//! `sb_style_clusters.py`, which places each in its nearest real cluster.
//! `cargo run -p match_engine --release --example styles -- [seeds]`

use match_engine::Simulation;
use match_engine::events::EventKind;
use match_engine::match_state::side_of;
use match_engine::pitch::{HALF_LENGTH, Side};
use match_engine::player::KickKind;
use match_engine::team::data::TeamData;
use match_engine::team::Role;
use match_engine::team::instructions::{BlockHeight, TeamInstructions};
use match_engine::player::Attributes;
use std::fmt::Write as _;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Arch {
    Default,
    /// Real cluster C0 (Spain, England, Germany): short passing, possession, high block.
    Possession,
    /// C1 (Switzerland, France): deep block, few pressing actions high up, quick forwards to counter.
    DeepCounter,
    /// C2 (Serbia, Denmark): high press, balanced passing.
    HighPress,
    /// C3 (Wales, Poland, Slovenia): direct, long high balls to a strong centre forward, little possession.
    TargetMan,
}

impl Arch {
    fn name(self) -> &'static str {
        match self {
            Arch::Default => "default",
            Arch::Possession => "possession",
            Arch::DeepCounter => "deep block + counter",
            Arch::HighPress => "high press",
            Arch::TargetMan => "direct to a target man",
        }
    }

    fn instructions(self) -> TeamInstructions {
        let mut i = TeamInstructions::default();
        match self {
            Arch::Default => {}
            Arch::Possession => {
                i.directness = Some(-1.0);
                i.tempo = Some(-0.5);
                i.width = Some(1.0);
                i.block_height = Some(BlockHeight::High);
                i.pressing_intensity = Some(0.7);
            }
            Arch::DeepCounter => {
                i.block_height = Some(BlockHeight::Low);
                i.pressing_intensity = Some(0.0);
                i.directness = Some(0.5);
                i.tempo = Some(0.7);
            }
            Arch::HighPress => {
                i.block_height = Some(BlockHeight::High);
                i.pressing_intensity = Some(1.0);
                i.tempo = Some(0.5);
            }
            Arch::TargetMan => {
                i.directness = Some(1.0);
                i.tempo = Some(0.3);
                i.block_height = Some(BlockHeight::Mid);
                i.pressing_intensity = Some(0.3);
            }
        }
        i
    }

    /// Squad overlay: what this style recruits for, paid for with what it needs less.
    fn overlay(self, role: Role, a: &mut Attributes, target_man: bool) {
        let add = |v: &mut f32, d: f32| *v = (*v + d).clamp(1.0, 100.0);
        if role == Role::GK {
            return;
        }
        match self {
            Arch::Default => {}
            Arch::Possession => {
                for v in [&mut a.short_passing, &mut a.long_passing, &mut a.first_touch, &mut a.composure, &mut a.intelligence] {
                    add(v, 12.0);
                }
                for v in [&mut a.strength, &mut a.jumping, &mut a.heading, &mut a.speed] {
                    add(v, -12.0);
                }
            }
            Arch::DeepCounter => {
                if matches!(role, Role::W | Role::WM | Role::AM | Role::CF | Role::ST2) {
                    for v in [&mut a.speed, &mut a.quickness] {
                        add(v, 15.0);
                    }
                    for v in [&mut a.short_passing, &mut a.long_passing] {
                        add(v, -10.0);
                    }
                } else {
                    for v in [&mut a.tackling, &mut a.intelligence, &mut a.heading, &mut a.composure] {
                        add(v, 10.0);
                    }
                    for v in [&mut a.short_passing, &mut a.dribbling, &mut a.first_touch] {
                        add(v, -10.0);
                    }
                }
            }
            Arch::HighPress => {
                for v in [&mut a.stamina, &mut a.work_rate, &mut a.aggression, &mut a.quickness] {
                    add(v, 12.0);
                }
                for v in [&mut a.composure, &mut a.dribbling, &mut a.flair] {
                    add(v, -12.0);
                }
            }
            Arch::TargetMan => {
                if target_man {
                    a.height_cm = 194.0;
                    a.weight_kg = 92.0;
                    for v in [&mut a.strength, &mut a.heading, &mut a.jumping] {
                        *v = v.max(88.0);
                    }
                    for v in [&mut a.speed, &mut a.quickness, &mut a.dribbling] {
                        add(v, -15.0);
                    }
                } else if matches!(role, Role::W | Role::WM | Role::FB | Role::WB) {
                    add(&mut a.crossing, 12.0);
                    add(&mut a.dribbling, -6.0);
                } else {
                    for v in [&mut a.strength, &mut a.heading, &mut a.aggression] {
                        add(v, 8.0);
                    }
                    for v in [&mut a.dribbling, &mut a.first_touch, &mut a.intelligence] {
                        add(v, -8.0);
                    }
                }
            }
        }
    }
}

/// Features of one team in one match, and its passes for the conditional checks.
#[derive(Clone, Default)]
struct TeamMatch {
    label: String,
    arch: usize,
    op_passes: f32,
    long30: f32,
    lofted: f32,
    first_time: f32,
    poss: f32,
    opp_passes_own60: f32,
    def_actions_high: f32,
    def_x: Vec<f32>,
    shots: f32,
    xg: f32,
    /// Ground passes 5–40 m: nearest opponent to the passer at release, first time, completed.
    ground: Vec<(f32, bool, bool)>,
}

const ARCHES: [Arch; 5] = [Arch::Default, Arch::Possession, Arch::DeepCounter, Arch::HighPress, Arch::TargetMan];

fn setup(home: Arch, away: Arch, seed: u64) -> Simulation {
    let mut h = TeamData::builtin("northbridge").unwrap();
    let mut a = TeamData::builtin("harbour").unwrap();
    h.instructions = home.instructions();
    a.instructions = away.instructions();
    let mut sim = Simulation::new(&h, &a, Default::default(), Default::default(), seed);
    for side in [Side::Home, Side::Away] {
        let arch = if side == Side::Home { home } else { away };
        // The target man: the striker with the best heading.
        let tm = sim
            .state
            .squad(side)
            .iter()
            .filter(|p| p.role.is_striker())
            .max_by(|x, y| x.attrs.heading.total_cmp(&y.attrs.heading))
            .map(|p| p.id);
        for p in sim.state.players.iter_mut().filter(|p| p.side == side) {
            arch.overlay(p.role, &mut p.attrs, Some(p.id) == tm);
        }
    }
    let c = sim.state.conditions;
    sim.set_conditions(c);
    sim.refresh_tactics();
    sim
}

fn play(home: Arch, away: Arch, seed: u64) -> [TeamMatch; 2] {
    let mut sim = setup(home, away, seed);
    let mut tms: [TeamMatch; 2] = Default::default();
    for (i, (arch, opp)) in [(home, away), (away, home)].into_iter().enumerate() {
        tms[i].label = format!("{} v {} ({})", arch.name(), opp.name(), if i == 0 { "NBA" } else { "HBC" });
        tms[i].arch = ARCHES.iter().position(|&a| a == arch).unwrap();
    }
    // Ground pass awaiting its next controller: side index, index into that side's `ground`.
    let mut pending: Option<(usize, usize)> = None;
    let mut seen = 0;
    while !sim.finished() {
        sim.step();
        let st = &sim.state;
        while seen < sim.events.len() {
            let e = &sim.events[seen];
            seen += 1;
            // The next kick or interception settles a pass that nobody controlled at rest (a first-time ball on).
            if let EventKind::Pass { by, .. } | EventKind::Shot { by, .. } | EventKind::Interception { by } = e.kind
                && let Some((s, i)) = pending.take()
            {
                tms[s].ground[i].2 = side_of(by).index() == s;
            }
            match e.kind {
                EventKind::Pass { by, kind, length, first_time, headed, .. } if st.is_live() => {
                    let p = &st.players[by];
                    let s = p.side.index();
                    let x = HALF_LENGTH + st.frame(p.side).to_team(p.pos).x;
                    let t = &mut tms[s];
                    t.op_passes += 1.0;
                    t.long30 += (length >= 30.0) as u8 as f32;
                    t.lofted += (kind == KickKind::LoftedPass && !headed) as u8 as f32;
                    t.first_time += first_time as u8 as f32;
                    if x < 63.0 {
                        tms[1 - s].opp_passes_own60 += 1.0;
                    }
                    if kind == KickKind::GroundPass && !headed && (5.0..40.0).contains(&length) {
                        let near = st.squad(p.side.opponent()).iter().map(|q| (q.pos - p.pos).length()).fold(f32::MAX, f32::min);
                        tms[s].ground.push((near, first_time, false));
                        pending = Some((s, tms[s].ground.len() - 1));
                    } else {
                        pending = None;
                    }
                }
                EventKind::Tackle { by, .. } | EventKind::Interception { by } | EventKind::Aerial { lost: by, .. } => {
                    let p = &st.players[by];
                    let s = p.side.index();
                    let x = HALF_LENGTH + st.frame(p.side).to_team(p.pos).x;
                    if !matches!(e.kind, EventKind::Aerial { .. }) {
                        tms[s].def_x.push(x);
                    }
                    if x > 42.0 {
                        tms[s].def_actions_high += 1.0;
                    }
                }
                EventKind::Shot { by, xg, .. } => {
                    let s = side_of(by).index();
                    tms[s].shots += 1.0;
                    tms[s].xg += xg;
                }
                EventKind::Restart { .. } | EventKind::Goal { .. } | EventKind::HalfTime => pending = None,
                _ => {}
            }
        }
        if let Some(c) = st.control
            && st.flight.is_none()
            && let Some((s, i)) = pending.take()
        {
            tms[s].ground[i].2 = side_of(c).index() == s;
        }
    }
    for (i, t) in tms.iter_mut().enumerate() {
        t.poss = sim.stats.teams[i].possession_s / (sim.stats.teams[0].possession_s + sim.stats.teams[1].possession_s).max(1.0);
    }
    tms
}

fn main() {
    let n: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(6);
    let mut games: Vec<(Arch, Arch)> = Vec::new();
    for &a in &ARCHES[1..] {
        games.push((a, Arch::Default));
        games.push((Arch::Default, a));
    }
    games.push((Arch::Default, Arch::Default));
    games.push((Arch::Possession, Arch::DeepCounter));
    games.push((Arch::TargetMan, Arch::HighPress));
    let jobs: Vec<(Arch, Arch, u64)> = games.iter().flat_map(|&(h, a)| (1..=n).map(move |s| (h, a, s))).collect();
    let res: Vec<TeamMatch> = std::thread::scope(|scope| {
        let hs: Vec<_> = jobs.iter().map(|&(h, a, s)| scope.spawn(move || play(h, a, s))).collect();
        hs.into_iter().flat_map(|h| h.join().unwrap()).collect()
    });
    const SCALE: f32 = 1.48;
    let feat = |t: &TeamMatch| {
        let ppda = t.opp_passes_own60 / t.def_actions_high.max(1.0);
        [
            t.long30 / t.op_passes.max(1.0),
            t.lofted / t.op_passes.max(1.0),
            t.poss,
            ppda,
            t.def_x.iter().sum::<f32>() / t.def_x.len().max(1) as f32,
            t.op_passes / SCALE,
            t.first_time / t.op_passes.max(1.0),
            t.shots / SCALE,
            t.xg / SCALE,
        ]
    };
    let names = ["direct", "high_share", "poss_share", "ppda", "def_height_np", "op_passes", "ft_share", "shots", "xg"];
    println!("{n} seeds per pairing; per team-match means (counts ÷ {SCALE}); real cluster medians from sb_style_clusters.py:");
    println!("  {:<24} {}", "", names.map(|s| format!("{s:>13}")).join(""));
    let real = [
        ("real C0 possession", [0.088, 0.106, 0.628, 9.58, 49.6, 599.0, 0.087, 16.0, 1.71]),
        ("real C1 deeper block", [0.110, 0.140, 0.467, 15.0, 39.2, 467.0, 0.104, 11.0, 0.99]),
        ("real C2 pressing", [0.127, 0.171, 0.500, 9.58, 46.5, 445.0, 0.101, 11.0, 1.16]),
        ("real C3 direct", [0.170, 0.254, 0.350, 14.1, 41.0, 295.0, 0.136, 9.0, 1.00]),
    ];
    for (name, v) in real {
        println!("  {name:<24} {}", v.map(|x| format!("{x:>13.3}")).join(""));
    }
    for (k, arch) in ARCHES.iter().enumerate() {
        let ts: Vec<&TeamMatch> = res.iter().filter(|t| t.arch == k).collect();
        let mut m = [0.0f32; 9];
        for t in &ts {
            for (a, b) in m.iter_mut().zip(feat(t)) {
                *a += b / ts.len() as f32;
            }
        }
        println!("  {:<24} {}", arch.name(), m.map(|x| format!("{x:>13.3}")).join(""));
    }
    println!("conditional (should not depend on style; real: sb_invariance.py, spread across style terciles in brackets):");
    println!("  ground 5–40 m completion by the nearest opponent to the passer 0–1.5 / 1.5–3 / 3–5 / 5–8 / 8+ m; first-time share under 3 m / from 3 m");
    println!("  real                       86–89 / 90–94 / 94–96 / 96–97 / 99 %;  first time 6.8–8.3 / 7.2–8.1 %");
    for (k, arch) in ARCHES.iter().enumerate() {
        let g: Vec<&(f32, bool, bool)> = res.iter().filter(|t| t.arch == k).flat_map(|t| t.ground.iter()).collect();
        let mut line = format!("  {:<24} ", arch.name());
        for (lo, hi) in [(0.0, 1.5), (1.5, 3.0), (3.0, 5.0), (5.0, 8.0), (8.0, 999.0)] {
            let b: Vec<_> = g.iter().filter(|r| (lo..hi).contains(&r.0)).collect();
            let _ = write!(line, " {:4.1}", 100.0 * b.iter().filter(|r| r.2).count() as f32 / b.len().max(1) as f32);
        }
        for (lo, hi) in [(0.0, 3.0), (3.0, 999.0)] {
            let b: Vec<_> = g.iter().filter(|r| (lo..hi).contains(&r.0)).collect();
            let _ = write!(line, "  ft {:4.1}", 100.0 * b.iter().filter(|r| r.1).count() as f32 / b.len().max(1) as f32);
        }
        println!("{line}");
    }
    if let Ok(path) = std::env::var("STYLES_OUT") {
        let rows: Vec<String> = res
            .iter()
            .map(|t| {
                let f = feat(t);
                let mut s = format!("{{\"label\": \"{}\"", t.label);
                for (n, v) in names.iter().zip(f) {
                    let _ = write!(s, ", \"{n}\": {v}");
                }
                let _ = write!(s, ", \"log_ppda\": {}}}", f[3].max(1.0).ln());
                s
            })
            .collect();
        std::fs::write(&path, format!("[{}]\n", rows.join(",\n"))).unwrap();
    }
}
