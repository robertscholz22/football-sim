//! Headless batch runner: plays N seeded matches in parallel and prints aggregate statistics, the calibration
//! metrics from `docs/tactics/principles.md` and a scorecard against them. Calibration lab (M8): `--experiment` plays
//! several arms (teams, instructions, attributes, configs) over the same seeds and compares them; `--learn-value`
//! learns the possession value field by self-play.

mod experiment;
mod value;

use std::path::{Path, PathBuf};
use std::time::Instant;

use clap::Parser;
use match_engine::events::{EventKind, hash_events};
use match_engine::pitch::{PitchConditions, Side};
use match_engine::stats::{Calibration, TeamStats};
use match_engine::team::TeamData;
use match_engine::{Simulation, config::SimConfig};
use rayon::prelude::*;

#[derive(Parser, Debug)]
#[command(about = "Run seeded headless matches and report statistics")]
struct Args {
    /// Number of matches.
    #[arg(long, default_value_t = 20)]
    matches: u64,
    /// First seed (match i uses seed + i).
    #[arg(long, default_value_t = 1)]
    seed: u64,
    /// Home squad: a builtin (northbridge | harbour) or a `TeamData` RON file.
    #[arg(long, default_value = "northbridge")]
    home: String,
    /// Away squad, as `--home`.
    #[arg(long, default_value = "harbour")]
    away: String,
    /// Length of one half in minutes (shorter halves for quick experiments).
    #[arg(long, default_value_t = 45.0)]
    half_minutes: f32,
    /// Wet pitch.
    #[arg(long)]
    wet: bool,
    /// Print the event log of every match.
    #[arg(long)]
    events: bool,
    /// Optional RON file with `SimConfig` overrides.
    #[arg(long)]
    config: Option<PathBuf>,
    /// Run the experiment in this RON file (`data/experiments`): every arm over the same seeds, compared by paired
    /// differences, instead of the report.
    #[arg(long)]
    experiment: Option<PathBuf>,
    /// After the report, learn the possession value field from these matches (self-play) and write it to this RON
    /// file as a `SimConfig` override (pass it to `--config` to play with it).
    #[arg(long)]
    learn_value: Option<PathBuf>,
}

/// Minutes of ball in play in a real 90-minute match (Opta via The Analyst: Premier League 2022/23 54:49, 2023/24 58:11).
const REAL_IN_PLAY_MIN: f32 = 57.0;

struct Result {
    seed: u64,
    score: [u32; 2],
    teams: [TeamStats; 2],
    cal: Calibration,
    hash: u64,
    secs: f64,
    log: Vec<String>,
    personal: Vec<(String, u32, u32, u32)>,
}

/// A squad by builtin name or from a `TeamData` RON file.
fn load_team(name: &str) -> TeamData {
    TeamData::builtin(name).unwrap_or_else(|| {
        let text = std::fs::read_to_string(name).unwrap_or_else(|e| panic!("team {name}: not a builtin squad, and {e}"));
        TeamData::from_ron(&text).unwrap_or_else(|e| panic!("team {name}: {e}"))
    })
}

/// The configuration from an optional override file, with the lab's half length.
fn load_config(path: Option<&Path>, half_minutes: f32) -> SimConfig {
    let mut cfg = match path {
        Some(p) => {
            let text = std::fs::read_to_string(p).unwrap_or_else(|e| panic!("config {}: {e}", p.display()));
            SimConfig::from_ron(&text).unwrap_or_else(|e| panic!("config {}: {e}", p.display()))
        }
        None => SimConfig::default(),
    };
    cfg.timing.half_length_s = half_minutes * 60.0;
    cfg
}

/// Play one match; `events`: keep its event log as text.
fn play(home: &TeamData, away: &TeamData, cfg: &SimConfig, cond: PitchConditions, seed: u64, events: bool) -> Result {
    let start = Instant::now();
    let mut sim = Simulation::new(home, away, cfg.clone(), cond, seed);
    sim.run_to_end();
    let st = &sim.state;
    // Names as they were at each event: the starting eleven, changed by each substitution.
    let mut names: Vec<String> =
        [home, away].iter().flat_map(|t| t.players.iter().take(11).map(|p| format!("{} {}", p.number, p.name))).collect();
    let mut log = Vec::new();
    if events {
        for e in &sim.events {
            if let EventKind::Substitution { side, off, on } = &e.kind {
                let team = if *side == Side::Home { home } else { away };
                let range = side.index() * 11..side.index() * 11 + 11;
                let number = team.bench.iter().find(|b| &b.name == on).map_or(0, |b| b.number);
                if let Some(i) = range.into_iter().find(|&i| names[i].split_once(' ').is_some_and(|(_, n)| n == off)) {
                    names[i] = format!("{number} {on}");
                }
            }
            log.push(e.describe(|id: usize| names[id].clone()));
        }
    }
    // Per-player shots, dribble-ish carries (tackles suffered) and passes: personality check.
    let mut personal = Vec::new();
    for p in &st.players {
        let shots = sim.events.iter().filter(|e| matches!(e.kind, EventKind::Shot { by, .. } if by == p.id)).count() as u32;
        let passes = sim.events.iter().filter(|e| matches!(e.kind, EventKind::Pass { by, .. } if by == p.id)).count() as u32;
        let duels = sim.events.iter().filter(|e| matches!(e.kind, EventKind::Tackle { on, .. } if on == p.id)).count() as u32;
        personal.push((format!("{} {} ({:?})", p.number, p.name, p.side), shots, passes, duels));
    }
    Result {
        seed,
        score: st.score,
        teams: sim.stats.teams.clone(),
        cal: sim.stats.cal.clone(),
        hash: hash_events(&sim.events),
        secs: start.elapsed().as_secs_f64(),
        log,
        personal,
    }
}

fn main() {
    let args = Args::parse();
    let cond = if args.wet { PitchConditions::wet_flat() } else { PitchConditions::default() };
    if let Some(path) = &args.experiment {
        experiment::run(&args, path, cond);
        return;
    }
    let home = load_team(&args.home);
    let away = load_team(&args.away);
    let cfg = load_config(args.config.as_deref(), args.half_minutes);

    let t0 = Instant::now();
    let results: Vec<Result> =
        (0..args.matches).into_par_iter().map(|i| play(&home, &away, &cfg, cond, args.seed + i, args.events)).collect();
    let wall = t0.elapsed().as_secs_f64();

    let n = results.len() as f32;
    let mut cal = Calibration::default();
    let mut tot = [TeamStats::default(), TeamStats::default()];
    let mut goals = 0u32;
    for r in &results {
        if args.events {
            println!("--- seed {} ---", r.seed);
            for l in &r.log {
                println!("{l}");
            }
        }
        println!(
            "seed {:>4}: {} {}-{} {}  shots {}-{}  xG {:.2}-{:.2}  poss {:.0}%  pass {:.0}%/{:.0}%  ({:.2}s, hash {:016x})",
            r.seed,
            home.short,
            r.score[0],
            r.score[1],
            away.short,
            r.teams[0].shots,
            r.teams[1].shots,
            r.teams[0].xg,
            r.teams[1].xg,
            100.0 * r.teams[0].possession_s / (r.teams[0].possession_s + r.teams[1].possession_s).max(1.0),
            100.0 * r.teams[0].pass_completion(),
            100.0 * r.teams[1].pass_completion(),
            r.secs,
            r.hash
        );
        if r.cal.longest_still_s >= 30.0 {
            println!("           ball stood still {:.0} s in live play: a stuck state", r.cal.longest_still_s);
        }
        cal.merge(&r.cal);
        goals += r.score[0] + r.score[1];
        for s in [Side::Home, Side::Away] {
            tot[s.index()].merge(&r.teams[s.index()]);
        }
    }

    println!("\n=== {} matches ({:.1} s wall, {:.2} s per match single-threaded) ===", results.len(), wall,
        results.iter().map(|r| r.secs).sum::<f64>() / results.len().max(1) as f64);
    // Real per-match figures contain only ≈ 57 min of ball in play (PL 2022/23 54:49, 2023/24 58:11); the engine has
    // no injuries or time-wasting and restarts as soon as the players are in place, so it plays much more of the 90
    // minutes. Real per-match targets for counts are scaled by the engine's in-play time over the real one.
    let live_min = results.iter().map(|r| r.teams[0].possession_s + r.teams[1].possession_s).sum::<f32>() / 60.0 / n;
    let scale = live_min / REAL_IN_PLAY_MIN;
    println!(
        "goals per match        {:.2}   (sanity 1.5–4 per real match → {:.1}–{:.1} at {:.1} min in play)",
        goals as f32 / n,
        1.5 * scale,
        4.0 * scale,
        live_min
    );
    for s in [Side::Home, Side::Away] {
        let t = &tot[s.index()];
        let name = if s == Side::Home { &home.name } else { &away.name };
        println!(
            "{name:<22} shots {:.1} (on target {:.1})  xG {:.2}  passes {:.0} ({:.0}% completed)  tackles {:.1} ({:.0}% won)  interceptions {:.1}  saves {:.1}  corners {:.1}  throw-ins {:.1}",
            t.shots as f32 / n,
            t.on_target as f32 / n,
            t.xg / n,
            t.passes as f32 / n,
            100.0 * t.pass_completion(),
            t.tackles as f32 / n,
            100.0 * t.tackles_won as f32 / t.tackles.max(1) as f32,
            t.interceptions as f32 / n,
            t.saves as f32 / n,
            t.corners as f32 / n,
            t.throw_ins as f32 / n,
        );
    }
    // Laws (M7), per team and match; StatsBomb open data (40 matches) in brackets, × the in-play scale where a count.
    let both = {
        let mut b = tot[0].clone();
        b.merge(&tot[1]);
        b
    };
    let pt = |v: u32| v as f32 / (2.0 * n);
    let fb = both.fouls_by_third.map(|v| 100.0 * v as f32 / both.fouls.max(1) as f32);
    println!(
        "fouls                  {:.1} (own / middle / attacking third {:.0} / {:.0} / {:.0} %, tactical {:.1}), advantage {:.1}   [{:.1} (17 / 53 / 30 %, tactical ≈ {:.1}–{:.1}), {:.1}]",
        pt(both.fouls),
        fb[0],
        fb[1],
        fb[2],
        pt(both.fouls_tactical),
        pt(both.advantages),
        12.11 * scale,
        1.7 * scale,
        3.0 * scale,
        1.64 * scale
    );
    println!(
        "cards and offside      yellow {:.2}, red {:.3}, offside {:.2}   [{:.2}, {:.3}, {:.2}]",
        pt(both.yellows),
        pt(both.reds),
        pt(both.offsides),
        2.12 * scale,
        0.04 * scale,
        1.49 * scale
    );
    println!(
        "penalties              {:.2} awarded, {:.0} % scored   [{:.2}; 78–80 %, PL 2023/24 89.6 %]",
        pt(both.penalties),
        100.0 * both.penalty_goals as f32 / both.penalties.max(1) as f32,
        0.11 * scale
    );
    println!(
        "free kicks             {:.1} awarded; direct shots {:.2} (on target {:.2}, xG {:.3} each, goals {:.3})   [direct shots {:.2}, on target {:.2}, xG 0.04–0.06]",
        pt(both.free_kicks),
        pt(both.fk_shots),
        pt(both.fk_on_target),
        both.fk_xg / both.fk_shots.max(1) as f32,
        pt(both.fk_goals),
        0.41 * scale,
        0.12 * scale
    );
    println!(
        "corners                {:.2} (short {:.2}), shots / goals within 10 s {:.2} / {:.3}   [{:.2} (short {:.2}), {:.2} / {:.3}]",
        pt(both.corners),
        pt(both.corners_short),
        pt(both.corner_shots),
        pt(both.corner_goals),
        4.97 * scale,
        0.60 * scale,
        1.61 * scale,
        0.11 * scale
    );
    let mut mins = both.sub_minutes.clone();
    mins.sort_by(f32::total_cmp);
    let q = |f: f32| if mins.is_empty() { 0.0 } else { mins[((mins.len() - 1) as f32 * f).round() as usize] };
    println!(
        "goal kicks and subs    goal kicks {:.1} (short {:.1}); substitutions {:.2}, minute p10 / median / p90 {:.0} / {:.0} / {:.0}   [{:.1} ({:.1}); 4.39, 51 / 72 / 87]",
        pt(both.goal_kicks),
        pt(both.goal_kicks_short),
        pt(both.subs),
        q(0.1),
        q(0.5),
        q(0.9),
        7.59 * scale,
        3.85 * scale
    );
    println!(
        "added time             shown {:.1} / {:.1} min, played {:.1} min per match   [≈ 3–6 per half; StatsBomb last event +2.0 / +4.9]",
        cal.added_shown_s[0] / 60.0 / n,
        cal.added_shown_s[1] / 60.0 / n,
        cal.stoppage_played_s / 60.0 / n
    );
    let poss = tot[0].possession_s / (tot[0].possession_s + tot[1].possession_s).max(1.0);
    println!("possession             {:.0}% / {:.0}%", 100.0 * poss, 100.0 * (1.0 - poss));
    // Possession time is only counted while the ball is live, so both teams' sum is the net playing time.
    println!(
        "ball in play           {:.1} min of {:.0}  [real ≈ {REAL_IN_PLAY_MIN:.0}: real per-match counts × {:.2}]",
        live_min,
        2.0 * args.half_minutes,
        scale
    );

    println!("\n--- calibration (target in brackets) ---");
    println!("mid-block length       {:.1} m   [≈27.2; teams 22.5–29.1]  ({} samples)", cal.mid_block_length(), cal.mid_block_samples);
    println!("mid-block width        {:.1} m   [≈40.1; teams 38.4–40.75]", cal.mid_block_width());
    println!("mid-block line         {:.1} m   [≈37.2–37.9 from own goal]", cal.mid_block_line());
    println!(
        "low block              {:.1} × {:.1} m, line {:.1} m   [25.2 × 35.5–36.0, 17.6–18.1]  ({} samples)",
        cal.low_block_length(),
        cal.low_block_width(),
        cal.low_block_line(),
        cal.low_block_samples
    );
    // FIFA WC2022 post-match reports, means over 64 matches [SPORMETRE-WC22]: length × width, back line from own goal.
    for (i, name, target) in [
        (0, "build-up", "39.2 × 53.5–54.5, 19.5"),
        (1, "progression", "33.0 × 55.0–55.5, 40.0–40.6"),
        (2, "final third", "36.2–36.6 × 43.3–45.2, 52.6–53.7"),
    ] {
        let (l, w, h) = cal.in_shape(i);
        println!("in possession {name:<12} {l:.1} × {w:.1} m, line {h:.1} m   [{target}]");
    }
    println!("GK distance goal line  {:.1} m   [≈13.1]", cal.gk_goal_dist());
    println!("GK distance def. line  {:.1} m   [≈20.4]", cal.gk_line_dist());
    let (one, two_three, four) = cal.presser_shares();
    println!("pressers 1 / 2–3 / 4+  {:.0}% / {:.0}% / {:.0}%   [single-team evidence: 2–3 ≈58%, 4+ ≈25%]", 100.0 * one, 100.0 * two_three, 100.0 * four);
    println!(
        "counter-press          {:.1} per match, {:.1} s mean, {:.0}% regained   [window ≈5 s]",
        cal.counter_presses as f32 / n,
        cal.counter_press_duration(),
        100.0 * cal.counter_press_regains as f32 / cal.counter_presses.max(1) as f32
    );
    println!(
        "one-touch goals        {:.0}%   [63–80%]",
        100.0 * cal.goals_one_touch as f32 / cal.goals.max(1) as f32
    );
    println!(
        "goals from 1–5 passes  {:.0}%   [≈53%]",
        100.0 * cal.goals_short_moves as f32 / cal.goals.max(1) as f32
    );
    println!(
        "counter-attacks        {:.1} per match, {:.1} s, {:.0} m   [≈9–11 s / ≈70 m]",
        cal.counters as f32 / n,
        cal.counter_duration(),
        cal.counter_distance()
    );
    println!("longest ball standstill {:.1} s in live play   [stuck-state guard: < 30 s]", cal.longest_still_s);
    // Fatigue (M6): per outfield player who started and is still on, per player-minute (the last period of each half
    // includes its added time); second half vs first [FIFA-WC22-RUN], high-intensity running in the last vs the first
    // 15 minutes [BRADLEY09; MOHR03]; goals by period (real shares include stoppage time).
    let per_90 = |v: &[f32; 6]| 5400.0 * v.iter().sum::<f32>() / cal.run_s.iter().sum::<f32>().max(1.0) / 1000.0;
    let rate = |v: &[f32; 6], r: std::ops::Range<usize>| {
        v[r.clone()].iter().sum::<f32>() / cal.run_s[r].iter().sum::<f32>().max(1.0)
    };
    println!(
        "running                {:.1} km per outfield player and 90 min, high-intensity {:.2} km; 2nd half {:+.0}%   [≈ −7%]; high-intensity last 15 vs first 15 min {:+.0}%   [−20 to −45%]",
        per_90(&cal.run_m),
        per_90(&cal.hi_m),
        100.0 * (rate(&cal.run_m, 3..6) / rate(&cal.run_m, 0..3) - 1.0),
        100.0 * (rate(&cal.hi_m, 5..6) / rate(&cal.hi_m, 0..1) - 1.0)
    );
    let g = cal.goals_by_period.iter().sum::<u32>().max(1) as f32;
    let shares: Vec<String> = cal.goals_by_period.iter().map(|&k| format!("{:.0}", 100.0 * k as f32 / g)).collect();
    println!("goals by 15 min        {} %   [real, with stoppage time: 2nd half ≈ 55–58 %, last 15 ≈ 20–25 %]", shares.join(" / "));
    let s = cal.shots_by_period.iter().sum::<u32>().max(1) as f32;
    let shares: Vec<String> = cal.shots_by_period.iter().map(|&k| format!("{:.1}", 100.0 * k as f32 / s)).collect();
    println!("shots by 15 min        {} %", shares.join(" / "));
    // Confidence (M6), per team: goal rate within 10 minutes of its own previous goal vs its rate overall [HEUER10:
    // no self-affirmation], pass completion within 10 minutes of conceding vs overall [HORRILLO23: lower for 3–9 min].
    let after: Vec<String> = (0..2)
        .map(|k| {
            let rate = |g: u32, t: f32| g as f32 / t.max(1.0);
            let ratio = rate(cal.goals_after_scoring[k], cal.after_scoring_s[k]) / rate(tot[k].goals, cal.team_s).max(1e-9);
            let [att, comp] = cal.passes_after_conceding[k];
            let dc = 100.0 * (comp as f32 / att.max(1) as f32 - tot[k].pass_completion());
            format!("×{ratio:.2} / {dc:+.1} pts")
        })
        .collect();
    println!(
        "after a goal (10 min)  goal rate after scoring vs own, completion after conceding vs own: {}   [≈ ×1; a little lower]",
        after.join(", ")
    );
    // ATT-05 scanning (M6): scans per second in the 10 s before receiving [JORDET20], completion of the next pass by
    // scans in that window [JORDET13].
    let by: Vec<String> =
        cal.passes_by_scans.iter().map(|[a, c]| format!("{:.0}", 100.0 * *c as f32 / (*a).max(1) as f32)).collect();
    println!(
        "scanning               {:.2} scans/s before receiving; next pass completed by scans 0–2 / 3–4 / 5+: {} %   [0.44 ± 0.30; 64 / 68 / 81 % low / medium / high]",
        cal.scans_before_receiving as f32 / (10.0 * cal.receptions.max(1) as f32),
        by.join(" / ")
    );
    println!(
        "lapses and calls       {:.1} lapses per outfield player; calls per team: claims {:.0}, line {:.0}, mine {:.0}, man on {:.0}",
        cal.lapses as f32 / (20.0 * n),
        cal.calls[0] as f32 / (2.0 * n),
        cal.calls[1] as f32 / (2.0 * n),
        cal.calls[2] as f32 / (2.0 * n),
        cal.calls[3] as f32 / (2.0 * n)
    );

    scorecard(&cal, &both, n, scale);

    println!("\n--- per player (per match): shots / passes / tackles suffered ---");
    let mut agg: Vec<(String, f32, f32, f32)> = results[0].personal.iter().map(|p| (p.0.clone(), 0.0, 0.0, 0.0)).collect();
    for r in &results {
        for (a, p) in agg.iter_mut().zip(&r.personal) {
            a.1 += p.1 as f32 / n;
            a.2 += p.2 as f32 / n;
            a.3 += p.3 as f32 / n;
        }
    }
    for a in agg {
        println!("{:<34} {:>5.1} {:>6.1} {:>5.1}", a.0, a.1, a.2, a.3);
    }
    if let Some(out) = &args.learn_value {
        value::learn(&cfg, &cal, out);
    }
}

/// One scorecard line: a measured value against the range a source gives.
struct Row {
    name: &'static str,
    value: f32,
    lo: f32,
    hi: f32,
    source: &'static str,
}

impl Row {
    fn new(name: &'static str, value: f32, lo: f32, hi: f32, source: &'static str) -> Self {
        Self { name, value, lo, hi, source }
    }

    /// A source that gives only a mean: ± 10 % of it.
    fn mean(name: &'static str, value: f32, mean: f32, source: &'static str) -> Self {
        let d = 0.1 * mean.abs();
        Self::new(name, value, mean - d, mean + d, source)
    }

    fn verdict(&self) -> &'static str {
        if self.value.is_nan() {
            "n/a"
        } else if self.value < self.lo {
            "low"
        } else if self.value > self.hi {
            "high"
        } else {
            "ok"
        }
    }
}

/// The scorecard (M8) against docs/tactics/principles.md, "Calibration targets". Targets are situation-conditional
/// (shapes in a phase, shares, rates per event): calibrated parameters are tuned against them. References are
/// per-match volumes, real counts × the in-play scale: they depend on the teams' styles, so the engine's two squads
/// need not match them (docs/design.md, "Short-pass losses and conditional targets"). Where a source gives only a mean,
/// the range is ± 10 % of it.
fn scorecard(cal: &Calibration, both: &TeamStats, n: f32, scale: f32) {
    let pct = |a: u32, b: u32| 100.0 * a as f32 / b.max(1) as f32;
    let rate = |v: &[f32; 6], r: std::ops::Range<usize>| v[r.clone()].iter().sum::<f32>() / cal.run_s[r].iter().sum::<f32>().max(1.0);
    let change = |v: &[f32; 6], late: std::ops::Range<usize>, early: std::ops::Range<usize>| 100.0 * (rate(v, late) / rate(v, early) - 1.0);
    let (_, two_three, _) = cal.presser_shares();
    let in_shape: Vec<(f32, f32, f32)> = (0..3).map(|i| cal.in_shape(i)).collect();
    let mut mins = both.sub_minutes.clone();
    mins.sort_by(f32::total_cmp);
    let median = if mins.is_empty() { f32::NAN } else { mins[mins.len() / 2] };
    let targets = [
        Row::new("mid-block length (m)", cal.mid_block_length(), 22.5, 29.1, "FIFA-WC22-MB"),
        Row::new("mid-block width (m)", cal.mid_block_width(), 38.4, 40.75, "FIFA-WC22-MB"),
        Row::new("mid-block line (m)", cal.mid_block_line(), 37.2, 37.9, "SPORMETRE-WC22"),
        Row::new("low block length (m)", cal.low_block_length(), 24.7, 25.5, "SPORMETRE-WC22"),
        Row::new("low block width (m)", cal.low_block_width(), 35.5, 36.0, "SPORMETRE-WC22"),
        Row::new("low block line (m)", cal.low_block_line(), 17.6, 18.1, "SPORMETRE-WC22"),
        Row::new("build-up length (m)", in_shape[0].0, 39.2, 39.5, "SPORMETRE-WC22"),
        Row::new("build-up width (m)", in_shape[0].1, 53.5, 54.5, "SPORMETRE-WC22"),
        Row::new("build-up line (m)", in_shape[0].2, 19.5, 19.7, "SPORMETRE-WC22"),
        Row::new("progression length (m)", in_shape[1].0, 32.8, 33.2, "SPORMETRE-WC22"),
        Row::new("progression width (m)", in_shape[1].1, 55.0, 55.5, "SPORMETRE-WC22"),
        Row::new("progression line (m)", in_shape[1].2, 40.0, 40.6, "SPORMETRE-WC22"),
        Row::new("final-third length (m)", in_shape[2].0, 35.8, 36.6, "SPORMETRE-WC22"),
        Row::new("final-third width (m)", in_shape[2].1, 43.3, 45.2, "SPORMETRE-WC22"),
        Row::new("final-third line (m)", in_shape[2].2, 52.6, 53.7, "SPORMETRE-WC22"),
        Row::mean("GK to goal line (m)", cal.gk_goal_dist(), 13.1, "FIFA-GKH"),
        Row::mean("GK to back line (m)", cal.gk_line_dist(), 20.4, "FIFA-GKH"),
        Row::mean("presses by 2-3 (%)", 100.0 * two_three, 58.0, "UEFA-PI"),
        Row::new("counter-press (s)", cal.counter_press_duration(), 0.0, 5.5, "BDFL-KRUE; UEFA-PI"),
        Row::new("one-touch goals (%)", pct(cal.goals_one_touch, cal.goals), 63.0, 80.0, "FIFA-TSG; UEFA-TO"),
        Row::mean("goals, 1-5 passes (%)", pct(cal.goals_short_moves, cal.goals), 53.0, "UEFA-TO"),
        Row::new("counter-attack (s)", cal.counter_duration(), 9.0, 11.0, "UEFA-PI"),
        Row::mean("counter-attack (m)", cal.counter_distance(), 70.0, "UEFA-PI"),
        Row::new("scans/s before receiving", cal.scans_before_receiving as f32 / (10.0 * cal.receptions.max(1) as f32), 0.14, 0.74, "JORDET20"),
        Row::mean("running, 2nd half (%)", change(&cal.run_m, 3..6, 0..3), -7.0, "FIFA-WC22-RUN"),
        Row::new("HI, last vs first 15 (%)", change(&cal.hi_m, 5..6, 0..1), -45.0, -20.0, "BRADLEY09; MOHR03"),
        Row::mean("fouls, attacking third (%)", 100.0 * both.fouls_by_third[2] as f32 / both.fouls.max(1) as f32, 30.0, "StatsBomb"),
        Row::new("penalties scored (%)", pct(both.penalty_goals, both.penalties), 78.0, 80.0, "ACAD-PK01; OPTA-PK01"),
        Row::new("direct FK xG per shot", both.fk_xg / both.fk_shots.max(1) as f32, 0.04, 0.06, "StatsBomb"),
        Row::mean("shot within 10 s / corner", both.corner_shots as f32 / both.corners.max(1) as f32, 1.61 / 4.97, "StatsBomb"),
        Row::mean("goal kicks short (%)", pct(both.goal_kicks_short, both.goal_kicks), 100.0 * 3.85 / 7.59, "StatsBomb"),
        Row::mean("substitution minute, median", median, 72.0, "StatsBomb"),
    ];
    let pt = |v: u32| v as f32 / (2.0 * n);
    let references = [
        Row::new("goals per match", 2.0 * pt(both.goals), 1.5 * scale, 4.0 * scale, "sanity, plan"),
        Row::new("shots per team", pt(both.shots), 8.0 * scale, 20.0 * scale, "sanity, plan"),
        Row::new("pass completion (%)", 100.0 * both.pass_completion(), 65.0, 90.0, "sanity, plan"),
        Row::mean("fouls per team", pt(both.fouls), 12.11 * scale, "StatsBomb"),
        Row::mean("yellow cards per team", pt(both.yellows), 2.12 * scale, "StatsBomb"),
        Row::mean("offsides per team", pt(both.offsides), 1.49 * scale, "StatsBomb"),
        Row::mean("corners per team", pt(both.corners), 4.97 * scale, "StatsBomb"),
        Row::mean("goal kicks per team", pt(both.goal_kicks), 7.59 * scale, "StatsBomb"),
        Row::mean("throw-ins per team", pt(both.throw_ins), 15.9 * scale, "StatsBomb"),
        Row::mean("substitutions per team", pt(both.subs), 4.39, "StatsBomb"),
    ];
    let print = |title: &str, rows: &[Row]| {
        let met = rows.iter().filter(|r| r.verdict() == "ok").count();
        println!("\n--- scorecard: {title}, {met} of {} in range (± 10 % where the source gives a mean) ---", rows.len());
        for r in rows {
            println!("{:<28} {:>8.2}   [{:.2} to {:.2}]  {:<5} {}", r.name, r.value, r.lo, r.hi, r.verdict(), r.source);
        }
    };
    print("situation-conditional targets", &targets);
    print("per-match references (style-dependent, × in-play scale)", &references);
}
