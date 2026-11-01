//! Off-ball movement is calm: players away from the ball mostly walk and jog and do not keep changing direction
//! (commitment, settle radius, rate-limited block shift, positional urgency by distance). Without those mechanisms
//! the same measure gives ≈ 4.1 m/s, 20 % walking and 82°/s of turning. The mean speed also rises with the number of
//! possession changes (each sends both teams running): 3.1–3.35 m/s over seeds since the sourced ATT-11 shape
//! (≈ 3.0–3.3 before); 3.4–3.5 since the attribute-slope round of 2026-10-29, whose flatter duel slopes, last-ditch
//! tackles and contested loose touches raised possession changes by ≈ 5 % (3.3–3.5 m/s, walking 30–34 % over seeds);
//! ≈ 3.75 since the front-line movements of phase D (2026-10-31: checks, pins at the line's speed, runs in behind and
//! the riskier balls into them raised interceptions by ≈ 12 %; the `movement` bench went 3.50 → 3.73 m/s).
//! Walking fell to 23–25 % over seeds 21–24 with the possession movements of phase E (2026-10-31: shows that arrive
//! by the carrier's next decision; the bench 25.9 → 24.9 %, 3.73 → 3.79 m/s), so the floor is 22 %. Real players
//! cover less (≈ 2–2.5 m/s off the ball): the block and marking (3.7 / 4.8 m/s) and pacing over the match are M6
//! (fatigue drain); these limits guard against zigzagging and runaway urgency, not the level.

use match_engine::Simulation;
use match_engine::match_state::PlayPhase;

#[test]
fn off_ball_players_walk_and_jog_without_zigzagging() {
    let mut sim = Simulation::from_builtin("northbridge", "harbour", 21);
    sim.state.half_length = 600.0;
    let dt = sim.cfg.timing.dt as f64;
    let mut prev: Vec<_> = sim.state.players.iter().map(|p| p.vel).collect();
    let (mut t, mut dist, mut walk_t) = (0.0f64, 0.0f64, 0.0f64);
    let (mut turn, mut moving_t) = (0.0f64, 0.0f64);
    while !sim.finished() {
        sim.step();
        let live = sim.state.phase == PlayPhase::Live;
        for (i, p) in sim.state.players.iter().enumerate() {
            if live && !p.is_gk() && sim.state.control != Some(i) {
                let s = p.vel.length() as f64;
                t += dt;
                dist += s * dt;
                if s < 2.0 {
                    walk_t += dt;
                }
                if s > 1.0 && prev[i].length() > 1.0 {
                    turn += prev[i].angle_to(p.vel).abs().to_degrees() as f64;
                    moving_t += dt;
                }
            }
            prev[i] = p.vel;
        }
    }
    let speed = dist / t;
    let walk = walk_t / t;
    let turn_rate = turn / moving_t;
    assert!(speed < 3.9, "mean off-ball speed {speed:.2} m/s (≈ {:.1} km per 90 min)", speed * 5.4);
    assert!(walk > 0.22, "walking only {:.0} % of the time", 100.0 * walk);
    assert!(turn_rate < 60.0, "running direction turns {turn_rate:.0}°/s");
}
