//! Layer-3 personality in a fixed on-ball situation: carrier near the box, a free teammate better placed.

use match_engine::Vec2;
use match_engine::ai::analysis::gk01_spot;
use match_engine::ai::on_ball::{OptionEval, Situation, decide, evaluate};
use match_engine::ai::personality::OptionKind;
use match_engine::pitch::Side;
use match_engine::player::PlayerId;
use match_engine::scenario::Scenario;
use match_engine::team::Role;

mod common;
use common::{AWAY, home_frame, situation, slot_of};

/// 2 v 1 at the edge of the box: the carrier (a midfielder) 18 m out on the left with one defender goal-side of him,
/// the striker free 12.5 m out on the right; the rest of the defence is caught upfield but one, back wide on the goal
/// line, so the keeper on his GK-01 spot (x = 50) is the second-last opponent and the striker is clearly onside
/// (LAW-01: 10 m, about four times the passer's judgement error).
fn scene() -> (Scenario, PlayerId) {
    let from = Vec2::new(34.5, 9.0);
    let (mut sc, carrier) = situation(21, Role::CM8, from);
    let fr = home_frame(&sc);
    sc.place(slot_of(&sc, Side::Home, Role::CF), fr.to_world(Vec2::new(40.0, -6.0)));
    let cb = slot_of(&sc, Side::Away, Role::CB);
    let back = (AWAY + 1..AWAY + 11).find(|&i| i != cb).unwrap();
    for i in AWAY + 1..AWAY + 11 {
        let p = if i == cb {
            Vec2::new(39.0, 7.0)
        } else if i == back {
            Vec2::new(51.0, -24.0)
        } else {
            Vec2::new(5.0, -25.0 + 5.0 * (i - AWAY) as f32)
        };
        sc.place(i, fr.to_world(p));
    }
    sc.place(slot_of(&sc, Side::Away, Role::GK), fr.to_world(gk01_spot(from, &sc.sim.cfg)));
    sc.think();
    (sc, carrier)
}

fn options(sc: &Scenario, carrier: PlayerId, ego: f32, instinct: f32) -> Vec<OptionEval> {
    let mut st = sc.sim.state.clone();
    let a = &mut st.players[carrier].attrs;
    a.egotism = ego;
    a.instinct = instinct;
    evaluate(&st, &sc.sim.cfg, &sc.sim.plan, Situation::open_play(carrier, st.ball.pos))
}

fn self_share(opts: &[OptionEval]) -> f32 {
    opts.iter().filter(|o| o.kind.is_self()).map(|o| o.prob).sum()
}

#[test]
fn egotism_raises_self_actions() {
    let (sc, carrier) = scene();
    let share = |ego| self_share(&options(&sc, carrier, ego, 47.0));
    let (humble, average, egotist) = (share(0.0), share(50.0), share(100.0));
    assert!(humble < average && average < egotist, "self-action share by egotism 0/50/100: {humble:.2} {average:.2} {egotist:.2}");
    assert!(egotist > 3.0 * humble, "egotism effect too weak: {humble:.2} → {egotist:.2}");
}

#[test]
fn humble_player_with_perfect_instinct_plays_the_optimal_pass() {
    let (sc, carrier) = scene();
    let opts = options(&sc, carrier, 0.0, 100.0);
    let best = opts.iter().max_by(|a, b| a.u_opt.total_cmp(&b.u_opt)).unwrap();
    assert!(matches!(best.kind, OptionKind::Pass { .. }), "optimum in a 2 v 1 should be the pass, got {}", best.label);
    assert!(best.prob > 0.95, "optimal pass chosen with probability {:.2}", best.prob);
}

#[test]
fn decision_noise_follows_the_softmax_and_persists_on_the_ball() {
    let (sc, carrier) = scene();
    let (cfg, plan) = (&sc.sim.cfg, &sc.sim.plan);
    let mut st = sc.sim.state.clone();
    st.players[carrier].control_since = st.time;
    let sit = Situation::open_play(carrier, st.ball.pos);
    let opts = evaluate(&st, cfg, plan, sit);
    let n = 1500;
    let mut counts = vec![0u32; opts.len()];
    let (mut same, mut redrawn) = (0, 0);
    for seed in 0..n {
        st.players[carrier].decision_seed = seed;
        let first = decide(&st, cfg, plan, sit).2;
        counts[first] += 1;
        if seed < 50 {
            // Re-deciding in the same situation a moment later keeps the choice; once the misjudgement is drawn
            // afresh it may change.
            let mut later = st.clone();
            later.time += 0.3;
            same += (decide(&later, cfg, plan, sit).2 == first) as u32;
            later.time += cfg.ai.decision_noise_hold_s;
            redrawn += (decide(&later, cfg, plan, sit).2 != first) as u32;
        }
    }
    for (o, c) in opts.iter().zip(counts) {
        let share = c as f32 / n as f32;
        assert!((share - o.prob).abs() < 0.04, "{}: chosen {share:.3}, softmax {:.3}", o.label, o.prob);
    }
    assert_eq!(same, 50, "the choice changed without the situation changing");
    assert!(redrawn > 0, "the noise is never drawn afresh");
}

#[test]
fn ways_to_pass_to_one_team_mate_share_one_softmax_weight() {
    use match_engine::ai::on_ball::{Action, softmax};
    let (sc, carrier) = scene();
    let opts = options(&sc, carrier, 10.0, 10.0);
    let receiver = |o: &OptionEval| match (o.kind, o.action) {
        (OptionKind::Pass { .. }, Action::Kick(k)) => k.receiver,
        _ => None,
    };
    let a = opts.iter().find(|o| receiver(o).is_some()).unwrap().clone();
    let b = opts.iter().find(|o| receiver(o).is_some() && receiver(o) != receiver(&a)).unwrap().clone();
    let flat = |mut o: OptionEval, u: f32| {
        (o.u_opt, o.instr, o.pers) = (u, 0.0, 0.0);
        o
    };
    // Two equally good ways to reach one team-mate weigh as much as one way to reach another, and a worse second
    // way takes nothing from the team-mate's share.
    for second in [0.01, 0.0] {
        let mut set = vec![flat(a.clone(), 0.01), flat(a.clone(), second), flat(b.clone(), 0.01)];
        softmax(&mut set, 0.002);
        let (to_a, to_b) = (set[0].prob + set[1].prob, set[2].prob);
        assert!((to_a - to_b).abs() < 1e-4, "second way {second}: to a {to_a:.3}, to b {to_b:.3}");
    }
}

#[test]
fn dribble_directions_share_one_softmax_weight() {
    use match_engine::ai::on_ball::softmax;
    let (sc, carrier) = scene();
    let opts = options(&sc, carrier, 10.0, 10.0);
    let mut dribbles = opts.iter().filter(|o| matches!(o.kind, OptionKind::Dribble { .. }));
    let (d1, d2) = (dribbles.next().unwrap().clone(), dribbles.next().unwrap().clone());
    let pass = opts.iter().find(|o| matches!(o.kind, OptionKind::Pass { .. })).unwrap().clone();
    let flat = |mut o: OptionEval, u: f32| {
        (o.u_opt, o.instr, o.pers) = (u, 0.0, 0.0);
        o
    };
    // Carrying in two equally good directions weighs as much as one equally good pass: the number of directions
    // tried does not make carrying more likely.
    for second in [0.01, 0.0] {
        let mut set = vec![flat(d1.clone(), 0.01), flat(d2.clone(), second), flat(pass.clone(), 0.01)];
        softmax(&mut set, 0.002);
        let (carry, to) = (set[0].prob + set[1].prob, set[2].prob);
        assert!((carry - to).abs() < 1e-4, "second direction {second}: carry {carry:.3}, pass {to:.3}");
    }
}
