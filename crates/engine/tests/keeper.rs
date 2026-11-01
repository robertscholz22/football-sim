//! Goalkeeper: the dive is planned with the time actually left and is only as hard as the shot demands, so a long
//! shot within reach is saved almost always, while a close-range shot into the corner beats him.

use match_engine::Vec2;
use match_engine::ai::analysis::gk01_spot;
use match_engine::ball::KickSpec;
use match_engine::events::EventKind;
use match_engine::pitch::{GOAL_HALF_WIDTH, HALF_LENGTH, Side};
use match_engine::player::{KickKind, KickPlan};
use match_engine::scenario::Scenario;
use match_engine::team::Role;

mod common;
use common::{home_frame, slot_of};

/// The home striker strikes a shot without execution error from team-frame `from` at the goal-line point
/// (`y`, `z`) with `speed_frac` of his maximum kick speed; the away keeper starts on his GK-01 spot. Returns
/// (on target, saved, goal) over `trials` seeds.
fn shots(from: Vec2, y: f32, z: f32, speed_frac: f32, trials: u64) -> (u32, u32, u32) {
    let (mut on, mut saved, mut goals) = (0, 0, 0);
    for seed in 0..trials {
        let mut sc = Scenario::new(500 + seed);
        let ex = &mut sc.sim.cfg.execution;
        (ex.angle_shot, ex.elevation_shot, ex.speed_rel, ex.spin_rel, ex.shank_probability) = (0.0, 0.0, 0.0, 0.0, 0.0);
        let fr = home_frame(&sc);
        let shooter = slot_of(&sc, Side::Home, Role::CF);
        let gk = slot_of(&sc, Side::Away, Role::GK);
        for id in 0..22 {
            if id != shooter && id != gk {
                let side_y = if id < 11 { -32.0 } else { 32.0 };
                sc.place(id, fr.to_world(Vec2::new(-40.0 + 4.0 * (id % 11) as f32, side_y)));
            }
        }
        sc.place(shooter, fr.to_world(from));
        sc.place(gk, fr.to_world(gk01_spot(from, &sc.sim.cfg)));
        sc.sim.state.players[shooter].facing = fr.to_world(Vec2::X);
        sc.give_ball(shooter);
        let now = sc.sim.state.time;
        let target = fr.to_world(Vec2::new(HALF_LENGTH, y));
        let speed = speed_frac * sc.sim.state.players[shooter].kin.max_kick_speed;
        let plan = KickPlan {
            spec: KickSpec::Through { target: target.extend(z), speed, sidespin: 0.0, topspin: 0.0 },
            kind: KickKind::Shot,
            receiver: None,
            target,
            arrival_t: (target - fr.to_world(from)).length() / speed,
            xg: 0.1,
            speed,
            foot: None,
        };
        let n0 = sc.sim.events.len();
        sc.sim.execute_kick(shooter, plan, false, now);
        on += sc.sim.state.flight.is_some_and(|f| f.on_goal) as u32;
        for _ in 0..150 {
            sc.sim.step();
            if sc.sim.state.flight.is_none() || !sc.sim.state.is_live() {
                break;
            }
        }
        let ev = &sc.sim.events[n0..];
        goals += ev.iter().any(|e| matches!(e.kind, EventKind::Goal { .. })) as u32;
        saved += ev.iter().any(|e| matches!(e.kind, EventKind::Save { .. })) as u32;
    }
    (on, saved, goals)
}

#[test]
fn long_shot_within_reach_is_saved() {
    // 25 m out, 2 m to the keeper's side at knee height: a dive he has a second for.
    // The save itself is a roll (≈ 90 % here), so enough trials that the stream of random draws doesn't decide it.
    let (on, saved, goals) = shots(Vec2::new(HALF_LENGTH - 25.0, 0.0), 2.0, 0.5, 0.75, 120);
    assert_eq!(on, 120, "the error-free shot is on target");
    assert!(saved >= 102, "saved {saved} / 120, goals {goals}");
}

#[test]
fn close_range_shot_into_the_corner_beats_the_keeper() {
    let (on, saved, goals) = shots(Vec2::new(HALF_LENGTH - 9.0, 0.0), GOAL_HALF_WIDTH - 0.4, 0.3, 0.85, 40);
    assert_eq!(on, 40, "the error-free shot is on target");
    assert!(goals >= 30, "goals {goals} / 40, saved {saved}");
}

/// The home winger crosses without execution error from the wide final third towards the near post, `gap` m in
/// front of the home striker; the away keeper (aerial command `command`) starts on his line at the near post. Returns
/// whether the keeper, after the release, comes for it (his team's loose-ball chaser).
fn comes_for_cross(command: f32, gap: f32) -> bool {
    use match_engine::match_state::PLAYERS_PER_TEAM;
    let mut sc = Scenario::new(77);
    let ex = &mut sc.sim.cfg.execution;
    (ex.angle_lofted, ex.elevation, ex.speed_rel, ex.spin_rel, ex.shank_probability) = (0.0, 0.0, 0.0, 0.0, 0.0);
    sc.sim.cfg.ai.reading_receive_delay_max_s = 0.0;
    let fr = home_frame(&sc);
    let (winger, striker) = (slot_of(&sc, Side::Home, Role::W), slot_of(&sc, Side::Home, Role::CF));
    let gk = slot_of(&sc, Side::Away, Role::GK);
    for id in 0..2 * PLAYERS_PER_TEAM {
        let side_y = if id < PLAYERS_PER_TEAM { -32.0 } else { 32.0 };
        sc.place(id, fr.to_world(Vec2::new(-40.0 + 4.0 * (id % PLAYERS_PER_TEAM) as f32, side_y)));
    }
    let (from, target) = (Vec2::new(40.0, -28.0), Vec2::new(HALF_LENGTH - 4.5, -2.0));
    sc.place(winger, fr.to_world(from)).place(striker, fr.to_world(target + Vec2::new(-gap, 0.0)));
    sc.place(gk, fr.to_world(Vec2::new(HALF_LENGTH - 1.0, -3.0)));
    sc.sim.state.players[gk].attrs.aerial_command = command;
    sc.sim.state.players[winger].facing = fr.to_world(Vec2::new(0.3, 1.0).normalize()) - fr.to_world(Vec2::ZERO);
    sc.give_ball(winger);
    sc.sim.state.time += 20.0;
    sc.think();
    let now = sc.sim.state.time;
    let target = fr.to_world(target);
    let plan = KickPlan {
        spec: KickSpec::Lofted { target, launch_angle: sc.sim.cfg.ai.cross_launch_angle, backspin: 6.0, sidespin: 0.0 },
        kind: KickKind::LoftedPass,
        receiver: Some(striker),
        target,
        arrival_t: 1.5,
        xg: 0.0,
        speed: 20.0,
        foot: None,
    };
    sc.sim.execute_kick(winger, plan, false, now);
    sc.run(0.4);
    let b = sc.sim.state.team_view(Side::Away);
    b.chaser.is_some_and(|c| c + PLAYERS_PER_TEAM == gk)
}

#[test]
fn gk04_a_commanding_keeper_comes_for_a_cross_he_reaches_first_a_hesitant_one_only_when_clearly_first() {
    // "Don't step out too early and before the cross is played"; "Command the space" [FIFA-GK].
    assert!(comes_for_cross(100.0, 7.0), "a commanding keeper comes for a cross he reaches first");
    assert!(!comes_for_cross(0.0, 7.0), "a hesitant keeper stays when he is first only narrowly");
    assert!(comes_for_cross(0.0, 16.0), "a hesitant keeper still comes when he is clearly first");
}
