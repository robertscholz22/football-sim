//! Layer 2 and 3 of the defending parameters: a team's `DefendingInstructions` change only that team's layer-2
//! values, and each player applies them as far as he adheres to instructions (discipline, role familiarity).
//!
//! Home attacks, away defends. Positions are given in the home team frame (attacking +x).

use match_engine::Vec2;
use match_engine::ai::brain::DefDuty;
use match_engine::ai::positioning;
use match_engine::ai::shape::anchor;
use match_engine::pitch::Side;
use match_engine::rng::SimRng;
use match_engine::scenario::Scenario;
use match_engine::team::{Channel, Role};

mod common;
use common::{AWAY, high_ball, home_frame, situation, slot_of};

/// `situation` with the away team's instructions changed by `set`, after the blocks have had time to follow.
fn instructed(seed: u64, ball: Vec2, set: impl Fn(&mut match_engine::team::DefendingInstructions)) -> Scenario {
    let (mut sc, _) = situation(seed, Role::CF, ball);
    set(&mut sc.sim.state.teams[Side::Away.index()].instructions.defending);
    sc.sim.refresh_tactics();
    sc.sim.state.time += 20.0;
    sc.think();
    sc
}

#[test]
fn a_teams_defending_instructions_change_only_its_layer2_block() {
    let width = |sc: &Scenario, side: Side| {
        let b = sc.sim.state.team_view(side);
        (2.0 * b.def_block.half_width, 2.0 * b.def_block_l1.half_width)
    };
    let tc = Scenario::new(1).sim.cfg.tactics.clone();
    let sc = instructed(1, Vec2::ZERO, |d| d.width = Some(30.0));
    let (l2, l1) = width(&sc, Side::Away);
    assert!((l2 - 30.0).abs() < 1e-3 && (l1 - tc.def05_width).abs() < 1e-3, "away: layer 2 {l2:.1}, layer 1 {l1:.1}");
    let (l2, l1) = width(&sc, Side::Home);
    assert!((l2 - tc.def05_width).abs() < 1e-3 && (l1 - tc.def05_width).abs() < 1e-3, "home: layer 2 {l2:.1}, layer 1 {l1:.1}");
}

#[test]
fn players_take_up_the_instructed_shape_as_far_as_they_adhere() {
    let mut sc = instructed(2, Vec2::ZERO, |d| d.width = Some(30.0));
    let catalog_width = sc.sim.cfg.tactics.def05_width;
    let fb = slot_of(&sc, Side::Away, Role::FB);
    // How far his anchor moved from the layer-1 anchor towards the layer-2 one.
    let mut moved = |discipline: f32, familiarity: f32| {
        let p = &mut sc.sim.state.players[fb];
        (p.attrs.discipline, p.familiarity, p.concept) = (discipline, [familiarity; 2], familiarity);
        let (a, a1) = anchor(&sc.sim.state, &sc.sim.cfg, fb);
        (a - a1).length()
    };
    let (obedient, wilful) = (moved(100.0, 1.0), moved(0.0, 0.0));
    let full = (catalog_width - 30.0) / 2.0;
    assert!((obedient - full).abs() < 0.2, "obedient full-back moved {obedient:.2} m of {full:.2}");
    assert!(wilful > 0.1 * full && wilful < 0.5 * full, "wilful full-back moved {wilful:.2} m of {full:.2}");
}

#[test]
fn box01_first_defender_contains_at_the_instructed_distance() {
    let ball = Vec2::new(36.0, 6.0);
    let tc = Scenario::new(3).sim.cfg.tactics.clone();
    // Offset of his target from the BOX-01 contain point at `dist` (away team frame).
    let miss = |instructed_m: Option<f32>, dist: f32| {
        let mut sc = instructed(3, ball, |d| d.contain_distance = instructed_m);
        let first = sc.sim.state.team_view(Side::Away).def_duties.iter().position(|r| *r == DefDuty::First).unwrap();
        let id = AWAY + first;
        let fr = home_frame(&sc);
        let carrier = sc.sim.state.control.unwrap();
        let c = fr.to_team(sc.sim.state.players[carrier].pos);
        let p = &mut sc.sim.state.players[id];
        (p.attrs.discipline, p.familiarity, p.concept) = (100.0, [1.0; 2], 1.0);
        sc.place(id, fr.to_world(c + (Vec2::new(52.5, 0.0) - c).normalize() * 2.0)).think();
        let mv = positioning::decide(&sc.sim.state, &sc.sim.cfg, id, &mut SimRng::new(3));
        assert!(mv.rules.contains(&"BOX-01"), "rules {:?}", mv.rules);
        let af = sc.sim.state.frame(Side::Away);
        let expect = positioning::contain_point(af.to_team(fr.to_world(c)), dist, Channel::Wide, &tc);
        (af.to_team(mv.target) - expect).length()
    };
    let (default, wider) = (miss(None, tc.box01_contain_distance), miss(Some(3.0), 3.0));
    assert!(default < 0.05, "catalog distance: target {default:.2} m off");
    assert!(wider < 0.05, "instructed 3 m: target {wider:.2} m off");
}

#[test]
fn ind07_a_negative_challenge_slack_keeps_an_obedient_defender_off_a_high_ball_he_cannot_win() {
    let challenged = |slack: Option<f32>| {
        let (sc, _, _) = high_ball(4, 17.0, |sc| {
            sc.sim.state.teams[Side::Away.index()].instructions.defending.challenge_slack = slack;
            sc.sim.refresh_tactics();
            for p in sc.sim.state.players.iter_mut().filter(|p| p.side == Side::Away) {
                (p.attrs.discipline, p.familiarity) = (100.0, [1.0; 2]);
            }
        });
        sc.sim.state.team_view(Side::Away).challenge
    };
    assert!(challenged(None), "layer 1 attacks the ball");
    assert!(!challenged(Some(-1.0)), "only when first to it");
}
