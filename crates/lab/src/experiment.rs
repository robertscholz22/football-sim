//! Tactical A/B experiments (M8). An experiment file (`data/experiments/*.ron`) lists arms, each a pair of squads with
//! changes (formation, manager options, instructions, attributes) and a configuration. Every arm plays the same seeds;
//! per-match metrics are compared between arms by paired differences (the same seed in both), with 95 % confidence
//! intervals from the spread of the differences.

use std::path::Path;
use std::time::Instant;

use match_engine::config::{SimConfig, TacticsConfig};
use match_engine::pitch::PitchConditions;
use match_engine::team::{PlayerInstructions, TeamData, TeamInstructions, TeamOption};
use rayon::prelude::*;
use serde::Deserialize;

use crate::{Args, Result, load_config, load_team, play};

#[derive(Debug, Deserialize)]
pub struct Experiment {
    pub name: String,
    /// What the experiment asks, printed with the results.
    #[serde(default)]
    pub question: String,
    pub arms: Vec<Arm>,
    /// Weighted sums of the arms reported besides each arm against the first, e.g. the interaction of a 2 × 2 design
    /// with weights [1, -1, -1, 1].
    #[serde(default)]
    pub contrasts: Vec<Contrast>,
}

#[derive(Debug, Deserialize)]
pub struct Arm {
    pub name: String,
    #[serde(default)]
    pub home: TeamSpec,
    #[serde(default)]
    pub away: TeamSpec,
    /// A `SimConfig` override file, relative to the experiment file; default: the lab's `--config`.
    #[serde(default)]
    pub config: Option<String>,
}

/// A squad and the changes made to it for an arm.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct TeamSpec {
    /// A builtin squad or a `TeamData` RON file (relative to the experiment file); default: the lab's `--home` /
    /// `--away`.
    pub base: Option<String>,
    /// Replaces the squad's formation (`team::FORMATIONS`).
    pub formation: Option<String>,
    /// Replaces the squad's team instructions.
    pub instructions: Option<TeamInstructions>,
    /// Manager options (`TeamOption`) applied on top of the instructions.
    pub options: Vec<TeamOption>,
    pub players: Vec<PlayerPatch>,
}

/// Changes to one player of a squad.
#[derive(Debug, Deserialize)]
pub struct PlayerPatch {
    /// Index in the squad's eleven (0 = goalkeeper), as listed in the squad file.
    pub slot: usize,
    /// Ratings by name (`Attributes::rating_mut`), e.g. [("speed", 95.0)].
    #[serde(default)]
    pub attrs: Vec<(String, f32)>,
    /// Replaces the player's instructions.
    #[serde(default)]
    pub instructions: Option<PlayerInstructions>,
}

#[derive(Debug, Deserialize)]
pub struct Contrast {
    pub name: String,
    pub weights: Vec<f64>,
}

impl TeamSpec {
    /// The squad for this arm: the base (`default` when none is named) with the changes applied.
    pub fn build(&self, default: &str, dir: &Path, tc: &TacticsConfig) -> TeamData {
        let mut t = match &self.base {
            Some(b) if TeamData::builtin(b).is_none() => load_team(&dir.join(b).to_string_lossy()),
            Some(b) => load_team(b),
            None => load_team(default),
        };
        if let Some(f) = &self.formation {
            t.formation = f.clone();
        }
        if let Some(i) = &self.instructions {
            t.instructions = i.clone();
        }
        for o in &self.options {
            o.apply(&mut t.instructions, tc);
        }
        for p in &self.players {
            let player = t.players.get_mut(p.slot).unwrap_or_else(|| panic!("{}: no player in slot {}", t.name, p.slot));
            for (name, v) in &p.attrs {
                *player.attrs.rating_mut(name).unwrap_or_else(|| panic!("unknown rating {name}")) = *v;
            }
            if let Some(i) = &p.instructions {
                player.instructions = i.clone();
            }
        }
        t
    }
}

/// Per-match metrics compared between arms (home perspective where it matters: arms usually change the home side).
const METRICS: [&str; 16] = [
    "home points",
    "home goals",
    "away goals",
    "home xG",
    "away xG",
    "home shots",
    "away shots",
    "home possession %",
    "home pass %",
    "away pass %",
    "home offside",
    "away offside",
    "home fouls",
    "away fouls",
    "home corners",
    "away corners",
];

fn metrics(r: &Result) -> [f64; METRICS.len()] {
    let [h, a] = &r.teams;
    let points = match r.score[0].cmp(&r.score[1]) {
        std::cmp::Ordering::Greater => 3.0,
        std::cmp::Ordering::Equal => 1.0,
        std::cmp::Ordering::Less => 0.0,
    };
    [
        points,
        r.score[0] as f64,
        r.score[1] as f64,
        h.xg as f64,
        a.xg as f64,
        h.shots as f64,
        a.shots as f64,
        100.0 * h.possession_s as f64 / (h.possession_s + a.possession_s).max(1.0) as f64,
        100.0 * h.pass_completion() as f64,
        100.0 * a.pass_completion() as f64,
        h.offsides as f64,
        a.offsides as f64,
        h.fouls as f64,
        a.fouls as f64,
        h.corners as f64,
        a.corners as f64,
    ]
}

/// Mean and half-width of the 95 % confidence interval (normal approximation).
fn mean_ci(v: &[f64]) -> (f64, f64) {
    let n = v.len() as f64;
    let mean = v.iter().sum::<f64>() / n;
    let var = v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0);
    (mean, 1.96 * (var / n).sqrt())
}

pub fn load(path: &Path) -> Experiment {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("experiment {}: {e}", path.display()));
    let exp: Experiment = ron::from_str(&text).unwrap_or_else(|e| panic!("experiment {}: {e}", path.display()));
    assert!(!exp.arms.is_empty(), "experiment {}: no arms", path.display());
    for c in &exp.contrasts {
        assert_eq!(c.weights.len(), exp.arms.len(), "contrast {}: one weight per arm", c.name);
    }
    exp
}

pub fn run(args: &Args, path: &Path, cond: PitchConditions) {
    let exp = load(path);
    let dir = path.parent().unwrap_or(Path::new("."));
    let arms: Vec<(TeamData, TeamData, SimConfig)> = exp
        .arms
        .iter()
        .map(|a| {
            let cfg = match &a.config {
                Some(c) => load_config(Some(&dir.join(c)), args.half_minutes),
                None => load_config(args.config.as_deref(), args.half_minutes),
            };
            (a.home.build(&args.home, dir, &cfg.tactics), a.away.build(&args.away, dir, &cfg.tactics), cfg)
        })
        .collect();
    let n = args.matches as usize;
    let t0 = Instant::now();
    let jobs: Vec<(usize, usize)> = (0..arms.len()).flat_map(|k| (0..n).map(move |i| (k, i))).collect();
    let rows: Vec<(usize, usize, [f64; METRICS.len()])> = jobs
        .into_par_iter()
        .map(|(k, i)| {
            let (home, away, cfg) = &arms[k];
            (k, i, metrics(&play(home, away, cfg, cond, args.seed + i as u64, false)))
        })
        .collect();
    // m[k][j][i]: arm k, metric j, match i.
    let mut m = vec![vec![vec![0.0; n]; METRICS.len()]; arms.len()];
    for (k, i, v) in rows {
        for (j, x) in v.into_iter().enumerate() {
            m[k][j][i] = x;
        }
    }

    println!("=== experiment: {} ===", exp.name);
    if !exp.question.is_empty() {
        println!("{}", exp.question);
    }
    println!(
        "{} matches per arm, seeds {}–{}, {:.0} s wall; means with 95 % confidence intervals",
        n,
        args.seed,
        args.seed + n as u64 - 1,
        t0.elapsed().as_secs_f64()
    );
    let label = |k: usize| (b'A' + k as u8) as char;
    for (k, (a, (home, away, _))) in exp.arms.iter().zip(&arms).enumerate() {
        println!("  {}: {} ({} v {})", label(k), a.name, home.name, away.name);
    }
    print!("\n{:<18}", "metric");
    for k in 0..arms.len() {
        print!(" {:>15}", label(k));
    }
    println!();
    for (j, name) in METRICS.iter().enumerate() {
        print!("{name:<18}");
        for arm in &m {
            let (mean, ci) = mean_ci(&arm[j]);
            print!(" {:>15}", format!("{mean:.2} ± {ci:.2}"));
        }
        println!();
    }

    // Contrasts: each arm against the first, then the experiment's own; paired by seed.
    let mut contrasts: Vec<(String, Vec<f64>)> = (1..arms.len())
        .map(|k| {
            let mut w = vec![0.0; arms.len()];
            (w[0], w[k]) = (-1.0, 1.0);
            (format!("{} − A", label(k)), w)
        })
        .collect();
    contrasts.extend(exp.contrasts.iter().map(|c| (c.name.clone(), c.weights.clone())));
    if contrasts.is_empty() {
        return;
    }
    println!("\n--- paired differences (same seeds), 95 % CI; * = the interval excludes 0 ---");
    for (name, w) in &contrasts {
        let terms: Vec<String> =
            w.iter().enumerate().filter(|(_, x)| **x != 0.0).map(|(k, x)| format!("{x:+} {}", label(k))).collect();
        println!("  {name}: {}", terms.join(" "));
    }
    print!("{:<18}", "metric");
    for (name, _) in &contrasts {
        print!(" {:>17}", name.chars().take(17).collect::<String>());
    }
    println!();
    for (j, metric) in METRICS.iter().enumerate() {
        print!("{metric:<18}");
        for (_, w) in &contrasts {
            let d: Vec<f64> = (0..n).map(|i| w.iter().zip(&m).map(|(x, arm)| x * arm[j][i]).sum()).collect();
            let (mean, ci) = mean_ci(&d);
            let star = if mean.abs() > ci { "*" } else { " " };
            print!(" {:>17}", format!("{mean:+.2} ± {ci:.2}{star}"));
        }
        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every shipped experiment parses, builds its squads and reads its configurations.
    #[test]
    fn shipped_experiments_load() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/experiments");
        let mut n = 0;
        for e in std::fs::read_dir(&dir).expect("data/experiments") {
            let path = e.unwrap().path();
            if path.extension().is_some_and(|x| x == "ron") {
                let exp = load(&path);
                let tc = SimConfig::default().tactics;
                for a in &exp.arms {
                    a.home.build("northbridge", &dir, &tc);
                    a.away.build("harbour", &dir, &tc);
                    if let Some(c) = &a.config {
                        load_config(Some(&dir.join(c)), 45.0);
                    }
                }
                n += 1;
            }
        }
        assert!(n > 0, "no experiments in {}", dir.display());
    }

    #[test]
    fn paired_interval() {
        let (mean, ci) = mean_ci(&[1.0, 2.0, 3.0, 4.0]);
        assert!((mean - 2.5).abs() < 1e-12);
        assert!((ci - 1.96 * (5.0f64 / 3.0 / 4.0).sqrt()).abs() < 1e-12);
    }
}
