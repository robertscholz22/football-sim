//! Continuation value of keeping the ball: a free carrier who holds or carries can still play the best pass on offer
//! afterwards, so those options are worth part of that pass's lead over the value of his location.

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

#[test]
fn holding_keeps_part_of_the_best_pass() {
    let (sc, carrier) = scene();
    let cfg = sc.sim.cfg.clone();
    let mut myopic = cfg.clone();
    myopic.ai.continuation_halflife_s = 1e-4;
    let (with, without) = (options(&sc, carrier, &cfg), options(&sc, carrier, &myopic));
    let best_pass = with.iter().filter(|o| matches!(o.kind, OptionKind::Pass { .. })).map(|o| o.u_opt).fold(f32::MIN, f32::max);
    let here = possession_value(home_frame(&sc).to_team(sc.sim.state.ball.ground_pos()), &cfg.ai);
    let premium = best_pass - here;
    assert!(premium > 0.0, "the free striker should be worth more than the ball where it is: premium {premium:+.4}");
    // Free and unchallenged, holding loses nothing, so it gains exactly the faded premium.
    let expected = premium * 0.5f32.powf(cfg.ai.onball_interval / cfg.ai.continuation_halflife_s);
    let gained = hold(&with) - hold(&without);
    assert!((gained - expected).abs() < 0.05 * expected, "hold gained {gained:+.5}, expected {expected:+.5}");
    // Keeping the pass available is worth less than playing it now.
    assert!(hold(&with) < best_pass, "hold {:+.4} vs best pass {best_pass:+.4}", hold(&with));
}
