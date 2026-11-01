//! Physical dribbling: an unchallenged carrier keeps the ball through changes of direction. A touch is weighted to
//! what he can follow, so a sharp turn is a cut or drag-back rather than a ball that runs away from him.

use match_engine::Vec2;
use match_engine::pitch::Side;
use match_engine::player::{Intent, PlayerId};
use match_engine::scenario::Scenario;
use match_engine::team::Role;

mod common;
use common::{home_frame, slot_of};

/// Carrier running at 6 m/s along +x (team frame) in the centre circle, everyone else at least 35 m away.
fn running_carrier(seed: u64) -> (Scenario, PlayerId) {
    let mut sc = Scenario::new(seed);
    let fr = home_frame(&sc);
    let carrier = slot_of(&sc, Side::Home, Role::CM8);
    for id in 0..22 {
        if id != carrier && !sc.sim.state.players[id].is_gk() {
            let y = if id < 11 { -30.0 } else { 30.0 };
            sc.place(id, fr.to_world(Vec2::new(-40.0 + 2.0 * (id % 11) as f32, y)));
        }
    }
    sc.place(carrier, fr.to_world(Vec2::ZERO));
    sc.give_ball(carrier);
    let fwd = fr.to_world(Vec2::X) - fr.to_world(Vec2::ZERO);
    let p = &mut sc.sim.state.players[carrier];
    p.vel = fwd * 6.0;
    p.facing = fwd;
    p.intent = Intent::Carry { dir: fwd, speed: 6.0 };
    (sc, carrier)
}

#[test]
fn carrier_keeps_the_ball_through_turns() {
    for deg in [0.0f32, 45.0, 90.0, 135.0, 180.0] {
        let mut kept = 0;
        let trials = 20;
        for seed in 0..trials {
            let (mut sc, c) = running_carrier(100 + seed);
            let fr = home_frame(&sc);
            let dir = fr.to_world(Vec2::from_angle(deg.to_radians())) - fr.to_world(Vec2::ZERO);
            let now = sc.sim.state.time;
            // No on-ball decisions during the test: only the carry and its touches.
            sc.sim.state.players[c].next_decision = now + 100.0;
            sc.sim.state.players[c].intent = Intent::Carry { dir, speed: 6.0 };
            sc.sim.dribble_touch(c, dir, 2.5, now);
            sc.run(2.5);
            let st = &sc.sim.state;
            let gap = (st.ball.ground_pos() - st.players[c].pos).length();
            if st.control == Some(c) && gap <= sc.sim.cfg.player.dribble_leash {
                kept += 1;
            }
        }
        assert_eq!(kept, trials, "turn of {deg}°: carrier kept the ball in {kept}/{trials} trials");
    }
}
