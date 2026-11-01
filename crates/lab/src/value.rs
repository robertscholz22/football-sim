//! Possession value learned by self-play (M8). For every cell of a `ValueGrid`, the share of the possessions that had
//! the ball under control there and ended in a goal: a first-visit Monte Carlo estimate of the value of the policy the
//! matches were played with. Far from goal a cell sees few goals (300 matches: ≈ 8 in an own-half cell, so neighbouring
//! cells differed by a factor of ten), so each cell pools its neighbourhood until it holds `MIN_GOALS`; in the box a
//! cell's own counts suffice and the steep gradient near goal is kept. What is still thin is shrunk towards the field
//! the matches were played with. The result is written as a `SimConfig` override; playing with it and learning again
//! iterates towards the field the engine's own play confirms.

use std::path::Path;

use match_engine::Vec2;
use match_engine::ai::analysis::{parametric_value, possession_value};
use match_engine::config::{SimConfig, ValueGrid};
use match_engine::stats::{Calibration, VALUE_CELL_M};

/// Prior weight (in possessions) of the field the matches were played with.
const SHRINK: f32 = 50.0;
/// Goals a cell's estimate pools at least (relative error ≈ 1 / √40 ≈ 16 %), from a square neighbourhood of up to
/// `MAX_RADIUS` cells around it.
const MIN_GOALS: u32 = 40;
const MAX_RADIUS: i32 = 5;

/// The learned field from the counts in `cal`, shrunk towards the field of `cfg`.
pub fn field(cfg: &SimConfig, cal: &Calibration) -> ValueGrid {
    let mut g = ValueGrid::new(VALUE_CELL_M);
    let (nx, ny) = (g.nx as i32, g.ny as i32);
    let count = |v: &[u32], ix: i32, iy: i32| v.get((ix * ny + iy) as usize).copied().unwrap_or(0);
    for i in 0..g.v.len() {
        let (cx, cy) = ((i / g.ny) as i32, (i % g.ny) as i32);
        let (mut visits, mut goals) = (0, 0);
        for r in 0..=MAX_RADIUS {
            (visits, goals) = (0, 0);
            for ix in (cx - r).max(0)..=(cx + r).min(nx - 1) {
                for iy in cy - r..=cy + r {
                    // The wings are folded: across the centre line the field mirrors.
                    let iy = if iy < 0 { -iy - 1 } else { iy };
                    if iy < ny {
                        visits += count(&cal.value_visits, ix, iy);
                        goals += count(&cal.value_goals, ix, iy);
                    }
                }
            }
            if goals >= MIN_GOALS {
                break;
            }
        }
        let prior = possession_value(g.centre(i), &cfg.ai);
        g.v[i] = (goals as f32 + SHRINK * prior) / (visits as f32 + SHRINK);
    }
    g
}

pub fn learn(cfg: &SimConfig, cal: &Calibration, out: &Path) {
    let g = field(cfg, cal);
    let visits: u32 = cal.value_visits.iter().sum();
    let goals: u32 = cal.value_goals.iter().sum();
    println!("\n--- possession value by self-play ({visits} cell visits, {goals} of them in possessions that scored) ---");
    println!("x from own goal line (m) | central (|y| < 9 m): learned / played with / parametric | wide (|y| 21–30 m)");
    for x0 in (0..105).step_by(15) {
        let mut row = String::new();
        for (y0, y1) in [(0.0, 9.0), (21.0, 30.0)] {
            let pts: Vec<Vec2> = (0..5)
                .flat_map(|i| (0..3).map(move |j| (i, j)))
                .map(|(i, j)| Vec2::new(x0 as f32 + 1.5 + 3.0 * i as f32 - 52.5, y0 + (y1 - y0) * (j as f32 + 0.5) / 3.0))
                .collect();
            let mean = |f: &dyn Fn(Vec2) -> f32| pts.iter().map(|&p| f(p)).sum::<f32>() / pts.len() as f32;
            row += &format!(
                "  {:.4} / {:.4} / {:.4}",
                mean(&|p| g.at(p)),
                mean(&|p| possession_value(p, &cfg.ai)),
                mean(&|p| parametric_value(p, &cfg.ai))
            );
        }
        println!("{:>3}–{:<3}{row}", x0, x0 + 15);
    }
    let body = ron::to_string(&g).expect("serialise the value grid");
    let text = format!(
        "// Possession value learned by self-play (match_lab --learn-value): {visits} cell visits, {goals} scoring.\n\
         // A SimConfig override: match_lab --config <this file>.\n(ai: (value_grid: Some({body})))\n"
    );
    std::fs::write(out, text).unwrap_or_else(|e| panic!("write {}: {e}", out.display()));
    println!("written to {}", out.display());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The written override parses back into the same grid.
    #[test]
    fn learned_field_round_trips_as_a_config_override() {
        let cfg = SimConfig::default();
        let g = field(&cfg, &Calibration::default());
        let text = format!("(ai: (value_grid: Some({})))", ron::to_string(&g).unwrap());
        let back = SimConfig::from_ron(&text).unwrap();
        assert_eq!(back.ai.value_grid.as_ref(), Some(&g));
        // With no matches the field is the prior at the cell centres.
        let c = g.centre(100);
        assert!((g.at(c) - parametric_value(c, &cfg.ai)).abs() < 1e-6);
    }
}
