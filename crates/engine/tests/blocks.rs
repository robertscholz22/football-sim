//! Shot blocks: a shot straight at an outfield defender hits his body, one passing within a stretched leg's reach
//! (`shot_block_reach`) is blocked about `shot_leg_block_p` of the time, and one further out gets through.

use match_engine::Vec2;
use match_engine::ai::analysis::gk01_spot;
use match_engine::ball::KickSpec;
use match_engine::pitch::{HALF_LENGTH, Side};
use match_engine::player::{KickKind, KickPlan};
use match_engine::scenario::Scenario;
use match_engine::team::Role;

mod common;
use common::{home_frame, slot_of};

/// Share of `trials` error-free low shots from 22.5 m at the goal centre that the away centre-back, standing 3 m in
/// front of the shooter and `offset` metres beside the shot's path, gets a touch to.
fn blocked_share(offset: f32, trials: u64) -> f64 {
    let mut blocked = 0;
    for seed in 0..trials {
        let mut sc = Scenario::new(900 + seed);
        let ex = &mut sc.sim.cfg.execution;
        (ex.angle_shot, ex.elevation_shot, ex.speed_rel, ex.spin_rel, ex.shank_probability) = (0.0, 0.0, 0.0, 0.0, 0.0);
        let fr = home_frame(&sc);
        let shooter = slot_of(&sc, Side::Home, Role::CF);
        let gk = slot_of(&sc, Side::Away, Role::GK);
        let def = slot_of(&sc, Side::Away, Role::CB);
        let from = Vec2::new(HALF_LENGTH - 22.5, 0.0);
        for id in 0..22 {
            if id != shooter && id != gk && id != def {
                let side_y = if id < 11 { -32.0 } else { 32.0 };
                sc.place(id, fr.to_world(Vec2::new(-40.0 + 4.0 * (id % 11) as f32, side_y)));
            }
        }
        sc.place(shooter, fr.to_world(from));
        sc.place(gk, fr.to_world(gk01_spot(from, &sc.sim.cfg)));
        sc.place(def, fr.to_world(from + Vec2::new(3.0, offset)));
        sc.sim.state.players[shooter].facing = fr.to_world(Vec2::X);
        sc.give_ball(shooter);
        let now = sc.sim.state.time;
        let target = fr.to_world(Vec2::new(HALF_LENGTH, 0.0));
        let speed = 0.8 * sc.sim.state.players[shooter].kin.max_kick_speed;
        let plan = KickPlan {
            spec: KickSpec::Through { target: target.extend(0.3), speed, sidespin: 0.0, topspin: 0.0 },
            kind: KickKind::Shot,
            receiver: None,
            target,
            arrival_t: (target - fr.to_world(from)).length() / speed,
            xg: 0.05,
            speed,
            foot: None,
        };
        sc.sim.execute_kick(shooter, plan, false, now);
        for _ in 0..50 {
            sc.sim.step();
            if sc.sim.state.last_touch == Some(def) {
                blocked += 1;
                break;
            }
            if sc.sim.state.flight.is_none() || !sc.sim.state.is_live() {
                break;
            }
        }
    }
    blocked as f64 / trials as f64
}

#[test]
fn a_shot_at_the_body_is_blocked_a_leg_reaches_some_and_further_out_none() {
    let body = blocked_share(0.2, 100);
    let leg = blocked_share(1.3, 200);
    let clear = blocked_share(2.4, 100);
    assert!(body >= 0.8, "body {body:.2}");
    assert!((0.25..=0.7).contains(&leg), "leg {leg:.2}");
    assert_eq!(clear, 0.0, "2.4 m beside the path");
}
