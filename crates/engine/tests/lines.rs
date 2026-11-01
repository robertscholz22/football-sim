//! The ball out of play: a carrier whose momentum takes the ball at his feet over his own goal line gives a restart.
//! The held ball is put at his foot each tick before the physics step, so the crossing must be seen over the whole
//! tick, not only within the physics sub-steps (a centre-back once held the ball 5 m behind his line for 35 minutes).

use match_engine::Vec2;
use match_engine::ball::BALL_RADIUS;
use match_engine::events::EventKind;
use match_engine::pitch::{HALF_LENGTH, Side};
use match_engine::player::Intent;
use match_engine::scenario::Scenario;
use match_engine::team::Role;

mod common;
use common::{home_frame, slot_of};

#[test]
fn a_carrier_who_takes_the_ball_over_his_goal_line_gives_a_restart() {
    let mut sc = Scenario::new(7);
    let fr = home_frame(&sc);
    let carrier = slot_of(&sc, Side::Home, Role::CB);
    for id in 0..22 {
        if id != carrier {
            let y = if id < 11 { -30.0 } else { 30.0 };
            sc.place(id, fr.to_world(Vec2::new(10.0 + 2.0 * (id % 11) as f32, y)));
        }
    }
    // He has just won control of the ball 0.2 m inside his goal line, running and facing towards it: his foot, where
    // the held ball goes, is already over the line.
    let at = fr.to_world(Vec2::new(-HALF_LENGTH + 0.2, -12.0));
    sc.place(carrier, at);
    sc.give_ball(carrier);
    sc.sim.state.ball.pos = at.extend(BALL_RADIUS);
    let back = fr.to_world(-Vec2::X) - fr.to_world(Vec2::ZERO);
    let now = sc.sim.state.time;
    let p = &mut sc.sim.state.players[carrier];
    p.vel = back * 2.0;
    p.facing = back;
    p.intent = Intent::Hold;
    p.next_decision = now + 100.0;
    sc.run(2.0);
    let st = &sc.sim.state;
    assert!(
        sc.sim.events.iter().any(|e| matches!(e.kind, EventKind::Restart { .. })),
        "no restart: carrier at {:?} (team frame), ball at {:?}, live {}",
        fr.to_team(st.players[carrier].pos),
        fr.to_team(st.ball.ground_pos()),
        st.is_live()
    );
}
