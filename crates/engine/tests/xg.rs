//! xG: a long shot against a set keeper is worth little; a keeper caught out of position opens the goal.

use match_engine::Vec2;
use match_engine::ai::analysis::{ShotHow, XgParts, gk01_spot, shot_xg_parts};
use match_engine::pitch::{HALF_LENGTH, Side};
use match_engine::player::Foot;
use match_engine::scenario::Scenario;
use match_engine::team::Role;

mod common;
use common::{home_frame, slot_of};

/// Home striker shoots from team-frame `from`; the away keeper stands at `keeper`; everyone else is far away.
fn shot(from: Vec2, keeper: Option<Vec2>) -> XgParts {
    shot_how(from, keeper, |_| ShotHow::default())
}

/// As `shot`, struck as `how(preferred foot)` says.
fn shot_how(from: Vec2, keeper: Option<Vec2>, how: impl Fn(Foot) -> ShotHow) -> XgParts {
    let mut sc = Scenario::new(7);
    let fr = home_frame(&sc);
    let shooter = slot_of(&sc, Side::Home, Role::CF);
    let gk = slot_of(&sc, Side::Away, Role::GK);
    for id in 0..22 {
        let y = if id < 11 { -32.0 } else { 32.0 };
        sc.place(id, fr.to_world(Vec2::new(-40.0 + 4.0 * (id % 11) as f32, y)));
    }
    sc.place(shooter, fr.to_world(from));
    sc.place(gk, fr.to_world(keeper.unwrap_or_else(|| gk01_spot(from, &sc.sim.cfg))));
    sc.give_ball(shooter);
    let pref = sc.sim.state.players[shooter].attrs.preferred_foot;
    shot_xg_parts(&sc.sim.state, &sc.sim.cfg, shooter, from, how(pref))
}

#[test]
fn long_shot_against_a_set_keeper_is_worth_little() {
    for from in [Vec2::new(HALF_LENGTH - 30.0, 0.0), Vec2::new(HALF_LENGTH - 25.0, 12.0), Vec2::new(HALF_LENGTH - 35.0, -8.0)] {
        let xg = shot(from, None);
        assert!(xg.total < 0.04, "{from}: xG {:.3}", xg.total);
        assert!(xg.after_keeper - xg.after_blockers < 0.005, "{from}: a set keeper adds {:.3}", xg.after_keeper - xg.after_blockers);
    }
}

#[test]
fn keeper_out_of_position_opens_the_goal() {
    let from = Vec2::new(HALF_LENGTH - 30.0, 0.0);
    let set = shot(from, None).total;
    // Caught 14 m off his line and well to the side, e.g. after sweeping.
    let caught = shot(from, Some(Vec2::new(HALF_LENGTH - 14.0, 9.0))).total;
    assert!(caught > set + 0.1, "set keeper {set:.3}, caught out {caught:.3}");
    let near = shot(Vec2::new(HALF_LENGTH - 11.0, 0.0), None).total;
    assert!((0.12..0.4).contains(&near), "penalty-spot shot against a set keeper: {near:.3}");
}

#[test]
fn weak_foot_and_wide_positions_lower_xg() {
    let other = |f: Foot| if f == Foot::Right { Foot::Left } else { Foot::Right };
    for from in [Vec2::new(HALF_LENGTH - 9.0, 0.0), Vec2::new(HALF_LENGTH - 15.0, 4.0), Vec2::new(HALF_LENGTH - 24.0, 0.0)] {
        let strong = shot_how(from, None, |f| ShotHow { foot: Some(f), ..Default::default() });
        let weak = shot_how(from, None, |f| ShotHow { foot: Some(other(f)), ..Default::default() });
        // StatsBomb: goals per xG of weak-foot shots ≈ 0.85 of strong-foot ones.
        let r = weak.total / strong.total;
        assert!((0.7..0.95).contains(&r), "{from}: weak/strong {r:.2}");
        assert!(weak.weak_foot && !strong.weak_foot);
    }
    // Same distance to the goal centre, 0 vs 12 m wide.
    for d in [12.0f32, 20.0] {
        let central = shot(Vec2::new(HALF_LENGTH - d, 0.0), None).total;
        let x = (d * d - 144.0).sqrt();
        let wide = shot(Vec2::new(HALF_LENGTH - x, 12.0), None).total;
        assert!(wide < 0.75 * central, "{d} m: central {central:.3}, wide {wide:.3}");
    }
}
