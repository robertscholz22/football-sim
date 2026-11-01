//! Whole-match properties: determinism, numerical sanity, both teams play.

use match_engine::Simulation;
use match_engine::config::SimConfig;
use match_engine::events::{EventKind, hash_events};
use match_engine::pitch::{HALF_LENGTH, HALF_WIDTH, PitchConditions};
use match_engine::team::TeamData;

fn sim(seed: u64, half_s: f32) -> Simulation {
    let mut cfg = SimConfig::default();
    cfg.timing.half_length_s = half_s;
    let home = TeamData::builtin("northbridge").unwrap();
    let away = TeamData::builtin("harbour").unwrap();
    Simulation::new(&home, &away, cfg, PitchConditions::default(), seed)
}

#[test]
fn same_seed_same_match_different_seed_different_match() {
    let run = |seed| {
        let mut s = sim(seed, 300.0);
        s.run_to_end();
        (hash_events(&s.events), s.state.players.iter().map(|p| p.pos).collect::<Vec<_>>())
    };
    let (a, pa) = run(11);
    let (b, pb) = run(11);
    let (c, _) = run(12);
    assert_eq!(a, b, "same seed must give the same event log");
    assert_eq!(pa, pb, "same seed must give the same final positions");
    assert_ne!(a, c, "different seeds should give different matches");
}

#[test]
fn full_match_is_sane() {
    let mut s = sim(5, 45.0 * 60.0);
    let mut checks = 0;
    while !s.finished() {
        s.step();
        checks += 1;
        if checks % 50 == 0 {
            for p in &s.state.players {
                assert!(p.pos.is_finite() && p.vel.is_finite(), "non-finite player state at t={}", s.state.time);
                assert!(p.pos.x.abs() < HALF_LENGTH + 8.0 && p.pos.y.abs() < HALF_WIDTH + 8.0, "player off the map: {:?}", p.pos);
            }
            assert!(s.state.ball.pos.is_finite(), "non-finite ball at t={}", s.state.time);
        }
    }
    assert!(s.events.iter().any(|e| matches!(e.kind, EventKind::FullTime)));
    let t = &s.stats.teams;
    assert!(t[0].shots >= 1 && t[1].shots >= 1, "both teams shoot: {} / {}", t[0].shots, t[1].shots);
    let poss = t[0].possession_s / (t[0].possession_s + t[1].possession_s);
    assert!((0.2..=0.8).contains(&poss), "possession {poss}");
    assert!(t[0].passes > 50 && t[1].passes > 50, "both teams pass: {} / {}", t[0].passes, t[1].passes);
    let still = s.stats.cal.longest_still_s;
    assert!(still < 30.0, "the ball stood still {still:.0} s in live play: a stuck state");
}

#[test]
fn every_formation_plays() {
    // Each formation shipped with the engine, against the next one: sane positions and both teams pass.
    for (i, f) in match_engine::team::FORMATIONS.iter().enumerate() {
        let mut cfg = SimConfig::default();
        cfg.timing.half_length_s = 600.0;
        let mut home = TeamData::builtin("northbridge").unwrap();
        let mut away = TeamData::builtin("harbour").unwrap();
        home.formation = f.to_string();
        away.formation = match_engine::team::FORMATIONS[(i + 1) % match_engine::team::FORMATIONS.len()].to_string();
        let mut s = Simulation::new(&home, &away, cfg, PitchConditions::default(), 40 + i as u64);
        assert_eq!(s.state.team(match_engine::pitch::Side::Home).formation.name, *f);
        s.run_to_end();
        for p in &s.state.players {
            assert!(p.pos.is_finite() && p.pos.x.abs() < HALF_LENGTH + 8.0 && p.pos.y.abs() < HALF_WIDTH + 8.0, "{f}: {:?}", p.pos);
        }
        let t = &s.stats.teams;
        assert!(t[0].passes > 20 && t[1].passes > 20, "{f}: both teams pass: {} / {}", t[0].passes, t[1].passes);
    }
}
