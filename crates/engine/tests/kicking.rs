//! Body and ball: a kick against the body is only struck once the player has braked and turned with the ball, a
//! normal pass is controlled almost always, and a heavy touch knocks the ball a stride or two away, not across the pitch.

use match_engine::Vec2;
use match_engine::ai::execution::{set_up, setup_angle};
use match_engine::ball::KickSpec;
use match_engine::config::DuelConfig;
use match_engine::duel::control_prob;
use match_engine::events::EventKind;
use match_engine::pitch::Side;
use match_engine::player::{Attributes, Intent, KickKind, KickPlan};
use match_engine::scenario::Scenario;
use match_engine::team::Role;

mod common;
use common::{home_frame, slot_of};

/// Everyone except `keep` stands far away on the touchlines.
fn clear_pitch(sc: &mut Scenario, keep: &[usize]) {
    let fr = home_frame(sc);
    for id in 0..22 {
        if !keep.contains(&id) && !sc.sim.state.players[id].is_gk() {
            let y = if id < 11 { -32.0 } else { 32.0 };
            sc.place(id, fr.to_world(Vec2::new(-40.0 + 4.0 * (id % 11) as f32, y)));
        }
    }
}

#[test]
fn pass_against_the_run_waits_for_the_set_up() {
    for seed in 0..10 {
        let mut sc = Scenario::new(300 + seed);
        let fr = home_frame(&sc);
        let carrier = slot_of(&sc, Side::Home, Role::CM8);
        let mate = slot_of(&sc, Side::Home, Role::CB);
        clear_pitch(&mut sc, &[carrier, mate]);
        sc.place(carrier, fr.to_world(Vec2::ZERO));
        sc.place(mate, fr.to_world(Vec2::new(-20.0, 3.0)));
        sc.give_ball(carrier);
        let fwd = fr.to_world(Vec2::X) - fr.to_world(Vec2::ZERO);
        let target = sc.sim.state.players[mate].pos;
        let now = sc.sim.state.time;
        let p = &mut sc.sim.state.players[carrier];
        // Running away from the pass at 6 m/s, body facing the run.
        p.vel = fwd * 6.0;
        p.facing = fwd;
        p.next_decision = now + 100.0;
        let plan = KickPlan {
            spec: KickSpec::Ground { target, arrival_speed: 6.0 },
            kind: KickKind::GroundPass,
            receiver: Some(mate),
            target,
            arrival_t: 1.5,
            xg: 0.0,
            speed: 15.0,
            foot: None,
        };
        p.intent = Intent::SetUp { plan, until: now + 3.0, since: now };
        let mut struck = None;
        for _ in 0..150 {
            let before = sc.sim.state.players[carrier].clone();
            let n = sc.sim.events.len();
            sc.run(sc.sim.cfg.timing.dt);
            if sc.sim.events[n..].iter().any(|e| matches!(e.kind, EventKind::Pass { .. })) {
                struck = Some((sc.sim.state.time - now, before));
                break;
            }
        }
        let (t, p) = struck.expect("the pass is played");
        let dir = (target - p.pos).normalize();
        let ex = &sc.sim.cfg.execution;
        assert!(t > 0.3, "seed {seed}: kicked after {t:.2} s, without braking and turning first");
        assert!(t < 1.6, "seed {seed}: set-up took {t:.2} s");
        assert!(set_up(&p, dir, 15.0, None, ex), "seed {seed}: struck with the body {:.0}° off the kick (allowed {:.0}°), moving {:.1} m/s against it",
            p.facing.angle_to(dir).abs().to_degrees(), setup_angle(15.0, p.kin.max_kick_speed, ex).to_degrees(), -p.vel.dot(dir));
    }
}

#[test]
fn normal_passes_are_controlled_driven_balls_are_hard() {
    let cfg = DuelConfig::default();
    let avg = Attributes::uniform(47.0);
    let p = |speed: f32, pressure: f32| control_prob(&avg, 0.5, 1.0, speed, 0.0, pressure, &cfg);
    assert!(p(8.0, 0.3) > 0.97, "firm pass, little pressure: {:.3}", p(8.0, 0.3));
    assert!(p(14.0, 0.5) > 0.95, "normal pass under some pressure: {:.3}", p(14.0, 0.5));
    assert!(p(25.0, 0.5) < 0.75, "driven ball: {:.3}", p(25.0, 0.5));
    assert!(p(14.0, 1.0) < p(14.0, 0.0), "pressure makes it harder");
}

#[test]
fn heavy_touch_stays_within_a_few_metres() {
    let mut seps = Vec::new();
    for seed in 0..40 {
        let mut sc = Scenario::new(500 + seed);
        // Force failed touches: every touch is heavy, and how heavy is spread over the whole range.
        sc.sim.cfg.duel.first_touch_offset = -3.0;
        let fr = home_frame(&sc);
        let r = slot_of(&sc, Side::Home, Role::CM8);
        clear_pitch(&mut sc, &[r]);
        sc.sim.state.players[r].attrs.first_touch = 37.0;
        sc.place(r, fr.to_world(Vec2::ZERO));
        // A 20 m/s ground ball straight at him from 12 m.
        let from = fr.to_world(Vec2::new(-12.0, 0.0));
        sc.loose_ball(from);
        let to = (fr.to_world(Vec2::ZERO) - from).normalize();
        sc.sim.state.ball.vel = (to * 20.0).extend(0.0);
        let mut touch_t = None;
        let mut max_sep: f32 = 0.0;
        for _ in 0..200 {
            let n = sc.sim.events.len();
            sc.run(sc.sim.cfg.timing.dt);
            let st = &sc.sim.state;
            if touch_t.is_none() && sc.sim.events[n..].iter().any(|e| matches!(e.kind, EventKind::Miscontrol { by } if by == r)) {
                touch_t = Some(st.time);
            }
            if let Some(t0) = touch_t {
                max_sep = max_sep.max((st.ball.ground_pos() - st.players[r].pos).length());
                if st.time - t0 > 1.5 || st.control.is_some() {
                    break;
                }
            }
        }
        if touch_t.is_some() {
            seps.push(max_sep);
        }
    }
    assert!(seps.len() >= 30, "only {} heavy touches in 40 trials", seps.len());
    seps.sort_by(f32::total_cmp);
    let median = seps[seps.len() / 2];
    let far = seps.iter().filter(|&&s| s > 6.0).count() as f32 / seps.len() as f32;
    assert!(median < 3.0, "median distance after a heavy touch {median:.1} m");
    assert!(far < 0.2, "{:.0} % of heavy touches ran more than 6 m away", 100.0 * far);
}
