//! Continuation value of keeping the ball (look-ahead level 1, docs/player-brain.md 8.4): a carrier who holds or
//! carries is worth the best pass he expects to have at his next decision. Free, he keeps what is on; a defender
//! running into the lane takes it away, so he plays it now.

use match_engine::Vec2;
use match_engine::ai::analysis::possession_value;
use match_engine::ai::on_ball::{OptionEval, Situation, evaluate};
use match_engine::ai::personality::OptionKind;
use match_engine::config::SimConfig;
use match_engine::pitch::Side;
use match_engine::player::PlayerId;
use match_engine::scenario::Scenario;
use match_engine::team::Role;

mod common;
use common::{home_frame, slot_of};

/// The carrier free on the halfway line, the striker free 25 m ahead of him, everybody else out on the touchlines (the
/// opponents' line up to 34 m deep, so the striker is clearly onside, LAW-01).
fn scene() -> (Scenario, PlayerId) {
    let mut sc = Scenario::new(31);
    let fr = home_frame(&sc);
    let carrier = slot_of(&sc, Side::Home, Role::CM8);
    let st = slot_of(&sc, Side::Home, Role::CF);
    for id in 0..22 {
        if id != carrier && id != st && !sc.sim.state.players[id].is_gk() {
            let (x, y) = if id < 11 { (-40.0 + 4.0 * (id % 11) as f32, -31.0) } else { (38.0 - 4.0 * (id % 11) as f32, 31.0) };
            sc.place(id, fr.to_world(Vec2::new(x, y)));
        }
    }
    sc.place(carrier, fr.to_world(Vec2::ZERO)).give_ball(carrier);
    sc.place(st, fr.to_world(Vec2::new(25.0, 0.0)));
    sc.think();
    (sc, carrier)
}

fn options(sc: &Scenario, carrier: PlayerId, cfg: &SimConfig) -> Vec<OptionEval> {
    let st = &sc.sim.state;
    evaluate(st, cfg, &sc.sim.plan, Situation::open_play(carrier, st.ball.pos))
}

fn hold(opts: &[OptionEval]) -> f32 {
    opts.iter().find(|o| o.kind == OptionKind::Hold).unwrap().u_opt
}

fn best_pass(opts: &[OptionEval]) -> f32 {
    opts.iter().filter(|o| matches!(o.kind, OptionKind::Pass { .. })).map(|o| o.u_opt).fold(f32::MIN, f32::max)
}

#[test]
fn holding_keeps_the_best_pass_while_free() {
    let (sc, carrier) = scene();
    let cfg = sc.sim.cfg.clone();
    let opts = options(&sc, carrier, &cfg);
    let best = best_pass(&opts);
    let here = possession_value(home_frame(&sc).to_team(sc.sim.state.ball.ground_pos()), &cfg.ai);
    let premium = best - here;
    assert!(premium > 0.0, "the free striker should be worth more than the ball where it is: premium {premium:+.4}");
    // Free and unchallenged, holding keeps what is on, less the small cost of standing still.
    let lost = best - hold(&opts);
    assert!(lost.abs() < 0.2 * premium, "hold {:+.5} vs best pass {best:+.5} (premium {premium:+.5})", hold(&opts));
}

#[test]
fn a_defender_running_into_the_lane_makes_holding_worth_less_than_passing_now() {
    let (mut sc, carrier) = scene();
    let cfg = sc.sim.cfg.clone();
    let fr = home_frame(&sc);
    let calm = options(&sc, carrier, &cfg);
    // An opponent 6 m beside the lane to the striker, running across it at 7 m/s.
    let d = 11 + 7;
    sc.place(d, fr.to_world(Vec2::new(12.5, 6.0)));
    sc.sim.state.players[d].vel = fr.to_world(Vec2::new(0.0, -7.0));
    let closing = options(&sc, carrier, &cfg);
    let (gap_calm, gap_closing) = (best_pass(&calm) - hold(&calm), best_pass(&closing) - hold(&closing));
    assert!(
        gap_closing > gap_calm + 0.002,
        "best pass − hold: {gap_closing:+.5} with the lane closing, {gap_calm:+.5} without"
    );
}
