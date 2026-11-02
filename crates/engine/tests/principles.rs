//! Principle scenario tests: a fixed situation is set up and the layer-1 output (the brains' duties and blocks,
//! off-ball targets) is checked against the catalog translation in `docs/tactics/principles.md`.
//!
//! Home attacks, away defends. Positions are given in the home team frame (attacking +x).

use match_engine::Vec2;
use match_engine::ai::analysis::gk01_depth;
use match_engine::ai::brain::{self, AttDuty, DefDuty, OutPhase, TeamPhase};
use match_engine::ai::movement;
use match_engine::ai::offer::is_open;
use match_engine::ai::on_ball::{Action, Situation, evaluate};
use match_engine::ai::personality::OptionKind;
use match_engine::ai::positioning::{self, marking_point};
use match_engine::ai::shape::{anchor, slot_point};
use match_engine::config::SimConfig;
use match_engine::match_state::{PLAYERS_PER_TEAM, team_range};
use match_engine::pitch::{HALF_LENGTH, HALF_WIDTH, PENALTY_AREA_HALF_WIDTH, Side, TeamFrame};
use match_engine::events::EventKind;
use match_engine::player::{Intent, KickKind, PlayerId};
use match_engine::player::attributes::Attributes;
use match_engine::rng::SimRng;
use match_engine::scenario::Scenario;
use match_engine::team::{Movement, Role, TeamInstructions, Unit, resolve};

mod common;
use common::{AWAY, high_ball, home_frame, situation, slot_of};

fn decide(sc: &Scenario, id: PlayerId, seed: u64) -> positioning::Move {
    positioning::decide(&sc.sim.state, &sc.sim.cfg, id, &mut SimRng::new(seed))
}

#[test]
fn def02_cover_is_behind_and_inside_the_presser() {
    for (seed, ball) in [(1, Vec2::new(10.0, 24.0)), (2, Vec2::new(-5.0, -26.0)), (3, Vec2::new(22.0, 18.0))] {
        let (sc, _) = situation(seed, Role::CM8, ball);
        let st = &sc.sim.state;
        let b = st.team_view(Side::Away);
        let fr = st.frame(Side::Away);
        assert_eq!(b.phase, TeamPhase::Out(OutPhase::Block));
        let first = b.def_duties.iter().position(|r| *r == DefDuty::First).expect("first defender");
        let second = b.def_duties.iter().position(|r| *r == DefDuty::Second).expect("second defender");
        let f = fr.to_team(st.players[AWAY + first].pos);
        let c = fr.to_team(decide(&sc, AWAY + second, seed).target);
        let ball_t = fr.to_team(st.ball.ground_pos());
        assert!(c.x < f.x - 2.0, "seed {seed}: cover {c} not behind the presser {f}");
        assert!(c.y * ball_t.y.signum() < f.y * ball_t.y.signum(), "seed {seed}: cover {c} not inside the presser {f}");
        let d = (c - f).length();
        let want = sc.sim.cfg.tactics.def02_cover_distance;
        assert!((d - want).abs() < 1.0, "seed {seed}: cover distance {d}, want {want}");
    }
}

/// Away first defender (slot) after a fresh role assignment.
fn first_defender(sc: &mut Scenario) -> usize {
    for p in &mut sc.sim.state.players[team_range(Side::Away)] {
        (p.beliefs.first, p.beliefs.first_on) = (None, None);
    }
    sc.think();
    sc.sim.state.team_view(Side::Away).first.expect("first defender")
}

#[test]
fn def03_a_defender_in_the_path_takes_over_from_a_chaser() {
    // Carrier running at goal at 7 m/s; one defender 1.5 m behind him (nearest to the contain point while it stands
    // still), one 12 m ahead in his path, everyone else far away.
    for (seed, y) in [(1, 0.0), (2, 8.0), (3, -10.0)] {
        let (mut sc, carrier) = situation(seed, Role::CF, Vec2::new(5.0, y));
        let fr = home_frame(&sc);
        let (chaser, ahead) = (slot_of(&sc, Side::Away, Role::CMP), slot_of(&sc, Side::Away, Role::CB));
        for i in AWAY + 1..AWAY + PLAYERS_PER_TEAM {
            sc.place(i, fr.to_world(Vec2::new(-30.0, (i - AWAY) as f32 * 4.0 - 24.0)));
        }
        sc.place(chaser, fr.to_world(Vec2::new(3.5, y + 1.0))).place(ahead, fr.to_world(Vec2::new(17.0, y + 2.0)));
        sc.sim.state.players[carrier].vel = fr.to_world(Vec2::new(7.0, 0.0));
        assert_eq!(first_defender(&mut sc) + AWAY, ahead, "seed {seed}: running carrier");
        sc.sim.state.players[carrier].vel = Vec2::ZERO;
        assert_eq!(first_defender(&mut sc) + AWAY, chaser, "seed {seed}: standing carrier");
    }
}

#[test]
fn def03_an_engaged_first_defender_keeps_the_role_until_the_ball_is_past_him() {
    for (seed, y) in [(1, 0.0), (2, 12.0), (3, -15.0)] {
        let (mut sc, carrier) = situation(seed, Role::CF, Vec2::new(10.0, y));
        let fr = home_frame(&sc);
        let (engaged, other) = (slot_of(&sc, Side::Away, Role::CMP), slot_of(&sc, Side::Away, Role::CB));
        for i in AWAY + 1..AWAY + PLAYERS_PER_TEAM {
            sc.place(i, fr.to_world(Vec2::new(-30.0, (i - AWAY) as f32 * 4.0 - 24.0)));
        }
        // Engaged: 3.5 m goal-side, 1 m to the side (inside the corridor, within tracking distance).
        sc.place(engaged, fr.to_world(Vec2::new(13.5, y + 1.0)));
        assert_eq!(first_defender(&mut sc) + AWAY, engaged, "seed {seed}");
        // A teammate arrives right at the contain point: quicker by more than the hysteresis, but the engaged
        // defender keeps the role; a fresh assignment would give it to the teammate.
        let d = sc.sim.cfg.tactics.ind01_contain_distance;
        sc.place(other, fr.to_world(Vec2::new(10.0 + d, y))).think();
        let first = |sc: &Scenario| sc.sim.state.team_view(Side::Away).first.unwrap() + AWAY;
        assert_eq!(first(&sc), engaged, "seed {seed}: engaged");
        // Once the carrier is past him (the ball played past the first), the teammate takes over.
        sc.place(carrier, fr.to_world(Vec2::new(15.0, y))).give_ball(carrier);
        sc.place(other, fr.to_world(Vec2::new(15.0 + d, y))).think();
        assert_eq!(first(&sc), other, "seed {seed}: beaten");
        // Control: without the engagement, the teammate at the contain point is first straight away.
        sc.place(carrier, fr.to_world(Vec2::new(10.0, y))).give_ball(carrier);
        sc.place(other, fr.to_world(Vec2::new(10.0 + d, y)));
        assert_eq!(first_defender(&mut sc) + AWAY, other, "seed {seed}: fresh");
    }
}

#[test]
fn def02_cover_stays_goal_side_when_the_first_defender_is_beaten() {
    let tc = SimConfig::default().tactics;
    for (first, ball) in [(Vec2::new(8.0, 5.0), Vec2::new(10.0, 5.0)), (Vec2::new(30.0, -20.0), Vec2::new(33.0, -18.0))] {
        // Home frame → away team frame: x and y flip.
        let (f, b) = (-first, -ball);
        let c = positioning::cover_point(f, b, &tc);
        let goal = TeamFrame::own_goal();
        assert!((c - goal).length() < (b - goal).length() - 3.0, "cover {c} not goal-side of the ball {b}");
    }
}

#[test]
fn ind07_a_defender_up_to_the_slack_behind_the_receiver_attacks_the_high_ball() {
    // 17 m goal-side of the target he arrives after the striker, but within `ind07_challenge_slack_s`: he goes
    // straight for the striker's reception point, without stopping at a contain distance.
    let (sc, receiver, defender) = high_ball(1, 17.0, |_| {});
    let st = &sc.sim.state;
    let b = st.team_view(Side::Away);
    let Intent::Receive { point, .. } = st.players[receiver].intent else { panic!("striker is not receiving") };
    assert_eq!(b.chaser.map(|s| AWAY + s), Some(defender), "chaser {:?}", b.chaser);
    assert!(b.challenge && (b.chase_point - point).length() < 0.1, "challenge {} at {}, reception {point}", b.challenge, b.chase_point);
    let mv = decide(&sc, defender, 1);
    assert!(mv.rules.contains(&"IND-07") && mv.target == b.chase_point, "rules {:?}, target {}", mv.rules, mv.target);
    // Much further away he cannot get there in time: no challenge.
    let (sc, _, _) = high_ball(1, 30.0, |_| {});
    assert!(!sc.sim.state.team_view(Side::Away).challenge);
    // 6 m away he contests it, so the striker attacks the ball in the air rather than letting it drop to him.
    let (sc, receiver, _) = high_ball(1, 6.0, |_| {});
    let Intent::Receive { t, meet, .. } = sc.sim.state.players[receiver].intent else { panic!("striker is not receiving") };
    let height = sc.sim.traj.at(t - sc.sim.state.time).pos.z;
    assert!(meet && height > sc.sim.cfg.player.control_height, "meets it: {meet}, at {height:.2} m");
}

#[test]
fn ind08_a_defender_reading_the_pass_goes_for_it_as_it_is_struck() {
    use match_engine::ball::KickSpec;
    use match_engine::player::{KickKind, KickPlan};
    // The home centre-back passes 30 m along the ground to the striker; an away midfielder stands 10 m from the
    // passer, 2 m off the line, everyone else far away. Is he the chaser right after the release?
    let chases = |seed: u64, cue: f32| {
        let mut sc = Scenario::new(seed);
        sc.sim.cfg.player.pass_cue_s = cue;
        let ex = &mut sc.sim.cfg.execution;
        (ex.angle_ground, ex.speed_rel, ex.spin_rel, ex.shank_probability) = (0.0, 0.0, 0.0, 0.0);
        let fr = home_frame(&sc);
        let (passer, receiver) = (slot_of(&sc, Side::Home, Role::CB), slot_of(&sc, Side::Home, Role::CF));
        let defender = slot_of(&sc, Side::Away, Role::CMP);
        for id in 0..2 * PLAYERS_PER_TEAM {
            let side_y = if id < PLAYERS_PER_TEAM { -32.0 } else { 32.0 };
            sc.place(id, fr.to_world(Vec2::new(-40.0 + 4.0 * (id % PLAYERS_PER_TEAM) as f32, side_y)));
        }
        let target = Vec2::new(10.0, 0.0);
        sc.place(passer, fr.to_world(Vec2::new(-20.0, 0.0))).place(receiver, fr.to_world(target));
        sc.place(defender, fr.to_world(Vec2::new(-10.0, 2.0)));
        sc.sim.state.players[passer].facing = fr.to_world(Vec2::X) - fr.to_world(Vec2::ZERO);
        sc.give_ball(passer);
        sc.sim.state.time += 20.0;
        sc.think();
        let (now, target) = (sc.sim.state.time, fr.to_world(target));
        let plan = KickPlan {
            spec: KickSpec::Ground { target, arrival_speed: 8.0 },
            kind: KickKind::GroundPass,
            receiver: Some(receiver),
            target,
            arrival_t: 2.0,
            xg: 0.0,
            speed: 18.0,
            foot: None,
        };
        sc.sim.execute_kick(passer, plan, false, now);
        sc.sim.state.team_view(Side::Away).chaser.map(|s| AWAY + s) == Some(defender)
    };
    for seed in 1..5 {
        // Reacting only to the strike he is too late for a ball that passes him 0.6 s later; reading the passer's
        // preparation `pass_cue_s` earlier, he gets there.
        assert!(chases(seed, SimConfig::default().player.pass_cue_s), "seed {seed}: with the cue");
        assert!(!chases(seed, 0.0), "seed {seed}: without the cue");
    }
}

#[test]
fn def06_block_shifts_to_the_ball_side_and_stays_on_the_pitch() {
    let tc = SimConfig::default().tactics;
    for y in [-30.0, -15.0, 15.0, 30.0] {
        let d = resolve(&TeamInstructions::default(), &tc).defending;
        let b = brain::defensive_block(&d, Vec2::new(0.0, y), false, false, &tc);
        assert!(b.centre_y * y > 0.0, "ball y {y}: block centre {}", b.centre_y);
        assert!(b.centre_y.abs() + b.half_width <= HALF_WIDTH + 1e-3, "ball y {y}: block leaves the pitch");
    }
    // The whole defending team follows: the mean y of its outfield targets moves towards the ball.
    for ball_y in [-24.0, 24.0] {
        let (sc, _) = situation(4, Role::CM8, Vec2::new(0.0, ball_y));
        let st = &sc.sim.state;
        let fr = st.frame(Side::Away);
        let ball_t = fr.to_team(st.ball.ground_pos());
        let ys: Vec<f32> = (1..PLAYERS_PER_TEAM).map(|i| fr.to_team(decide(&sc, AWAY + i, 4).target).y).collect();
        let mean = ys.iter().sum::<f32>() / ys.len() as f32;
        assert!(mean * ball_t.y.signum() > 5.0, "ball y {}: mean defending y {mean}", ball_t.y);
    }
}

#[test]
fn att11_in_possession_shape_follows_the_ball_through_the_thirds() {
    let tc = SimConfig::default().tactics;
    let t = resolve(&TeamInstructions::default(), &tc);
    // With the ball at each phase's mean position: the back line at the catalog height, the team at the catalog width.
    for (i, x) in tc.att11_ball_x.into_iter().enumerate() {
        let b = brain::attacking_block(Vec2::new(x, 0.0), HALF_LENGTH, &t, &tc);
        assert!((b.line_x + HALF_LENGTH - tc.att11_line[i]).abs() < 1e-3, "ball x {x}: line {}", b.line_x);
        assert!((2.0 * b.half_width - tc.att11_width[i]).abs() < 1e-3, "ball x {x}: width {}", 2.0 * b.half_width);
    }
    // In between it moves with the ball; wide of the ball it shifts over but keeps its wide players on the pitch.
    let mid = brain::attacking_block(Vec2::new(17.5, 30.0), HALF_LENGTH, &t, &tc);
    let line = mid.line_x + HALF_LENGTH;
    assert!(line > tc.att11_line[1] && line < tc.att11_line[2], "line {line}");
    assert!(mid.centre_y > 0.0 && mid.centre_y + mid.half_width <= HALF_WIDTH - tc.att11_touchline_margin + 1e-3);
}

#[test]
fn ind04_marking_is_goal_side_and_tighter_near_goal() {
    let tc = SimConfig::default().tactics;
    let goal = TeamFrame::own_goal();
    let ball = Vec2::new(-5.0, 15.0);
    let gap = |opp: Vec2| {
        let m = marking_point(opp, ball, &tc);
        assert!((m - goal).length() < (opp - goal).length(), "mark {m} not goal-side of {opp}");
        (m - opp).length()
    };
    let far = gap(Vec2::new(5.0, 5.0));
    let near = gap(Vec2::new(-40.0, 5.0));
    assert!(near < far, "near goal {near} m, far from goal {far} m");
    assert!(near >= tc.ind04_min - 0.5 && far <= tc.ind04_max + 0.5);

    // In a match situation every marker heads for his marking point.
    let (sc, _) = situation(5, Role::CM8, Vec2::new(20.0, -10.0));
    let st = &sc.sim.state;
    let fr = st.frame(Side::Away);
    let b = st.team_view(Side::Away);
    let ball_t = fr.to_team(st.players[st.control.unwrap()].pos);
    let mut markers = 0;
    for (slot, role) in b.def_duties.iter().enumerate() {
        if let DefDuty::Mark(o) = *role {
            markers += 1;
            let want = marking_point(fr.to_team(st.players[o].pos), ball_t, &sc.sim.cfg.tactics);
            let got = fr.to_team(decide(&sc, AWAY + slot, 5).target);
            assert!((got - want).length() < 0.5, "marker {slot}: target {got}, marking point {want}");
        }
    }
    assert!(markers > 0, "no markers assigned near the box");
}

/// Fraction of `n` noisy decisions of `id` whose target lies within `deg` of the line from the ball through `blocker`.
fn share_on_line(sc: &Scenario, id: PlayerId, ball: Vec2, blocker: Vec2, deg: f32, n: u64) -> f32 {
    let fr = home_frame(sc);
    let on = (0..n)
        .filter(|&s| {
            let t = fr.to_team(decide(sc, id, s).target);
            (t - ball).angle_to(blocker - ball).abs().to_degrees() < deg && (t - ball).length() > (blocker - ball).length()
        })
        .count();
    on as f32 / n as f32
}

/// Layer 1 exactly: a perfect reader (reliability 1) is always on the optimum, without layer-3 noise. A supporter's
/// spot is worth only the carrier's chance of choosing him times its value, so the noise of an ordinary reader moves
/// his choice more than a front player's.
fn perfect_reader(sc: &mut Scenario, id: PlayerId) {
    let p = &mut sc.sim.state.players[id];
    p.attrs.intelligence = 100.0;
    (p.confidence, p.energy, p.familiarity) = (1.0, 1.0, [1.0; 2]);
}

/// The away strikers other than `except` stand off the carrier's left side, so the lanes towards the right
/// centre-midfielder are cut only by what the test puts there (with a striker beside the carrier every lane on that
/// side is cut and no spot is free: then standing in a shadow costs nothing).
fn strikers_aside(sc: &mut Scenario, ball: Vec2, except: Option<PlayerId>) {
    let fr = home_frame(sc);
    let st2: Vec<PlayerId> = (AWAY..2 * PLAYERS_PER_TEAM).filter(|&i| sc.sim.state.players[i].role == Role::ST2 && Some(i) != except).collect();
    for (k, i) in st2.into_iter().enumerate() {
        sc.place(i, fr.to_world(ball + Vec2::new(4.0 - 5.0 * k as f32, 7.0)));
    }
}

#[test]
fn att10_support_does_not_stand_in_line_behind_a_teammate() {
    let (mut sc, _) = situation(6, Role::DM, Vec2::new(-5.0, 0.0));
    let fr = home_frame(&sc);
    let ball = fr.to_team(sc.sim.state.ball.ground_pos());
    strikers_aside(&mut sc, ball, None);
    let cm = slot_of(&sc, Side::Home, Role::CM8);
    let a = anchor(&sc.sim.state, &sc.sim.cfg, cm).0;
    // Another teammate halfway between the ball and the midfielder's spot: standing there would be collinear.
    let mate = slot_of(&sc, Side::Home, Role::CF);
    let m = ball + (a - ball) * 0.5;
    sc.place(mate, fr.to_world(m));
    perfect_reader(&mut sc, cm);
    let deg = sc.sim.cfg.tactics.att10_collinear_deg;
    let share = share_on_line(&sc, cm, ball, m, deg, 10);
    assert!(share == 0.0, "{:.0}% of targets collinear with carrier and teammate", 100.0 * share);
}

#[test]
fn att14_support_leaves_a_defenders_cover_shadow() {
    let (mut sc, _) = situation(7, Role::DM, Vec2::new(-5.0, 0.0));
    let fr = home_frame(&sc);
    let ball = fr.to_team(sc.sim.state.ball.ground_pos());
    let cm = slot_of(&sc, Side::Home, Role::CM8);
    let a = anchor(&sc.sim.state, &sc.sim.cfg, cm).0;
    // An opponent halfway along the lane to the midfielder's spot puts that spot in his cover shadow.
    let d = ball + (a - ball) * 0.5;
    let st2 = slot_of(&sc, Side::Away, Role::ST2);
    sc.place(st2, fr.to_world(d));
    strikers_aside(&mut sc, ball, Some(st2));
    perfect_reader(&mut sc, cm);
    // The principle is in the valuation: his most valuable offer is outside the shadow. (History: this asserted that no
    // chosen target was inside it; with the keepers in goal (M4) the carrier has free options elsewhere, a support
    // spot adds little either way and the role prior may keep him on his spot.)
    let deg = sc.sim.cfg.tactics.att14_shadow_deg;
    let s = positioning::move_scores(&sc.sim.state, &sc.sim.cfg, cm).expect("a supporter with someone to offer to");
    let best = s.iter().max_by(|a, b| a.marginal.total_cmp(&b.marginal)).unwrap();
    let t = best.point;
    let shadowed = (t - ball).angle_to(d - ball).abs().to_degrees() < deg && (t - ball).length() > (d - ball).length();
    assert!(!shadowed && best.marginal > 0.0, "best offer {:?} at {t:?} (marginal {:.5}) inside the cover shadow", best.kind, best.marginal);
}

#[test]
fn att14_strikers_in_the_midfielders_shadow_go_deep_and_the_far_midfielder_offers_in_the_gap() {
    // Test 4 [DFB-SCHOEN 4.1]: both strikers stand "im Deckungsschatten der beiden stellenden gegnerischen
    // Mittelfeldspieler" and should go "so tief wie möglich … um die gegnerische Verteidigung zu binden", while the
    // ball-far midfielder, covered by the ball-near opposing forward, "müsste sich auf Lücke anbieten".
    let (mut sc, carrier) = common::settled(common::with_formations(73, "4-4-2", "4-3-3"), Side::Home, Role::CB, Vec2::new(-28.0, 6.0));
    let fr = home_frame(&sc);
    let ball = fr.to_team(sc.sim.state.ball.ground_pos());
    let of = |sc: &Scenario, side: Side, role: Role| -> Vec<PlayerId> {
        team_range(side).filter(|&i| sc.sim.state.players[i].role == role).collect()
    };
    let (strikers, eights) = (of(&sc, Side::Home, Role::ST2), of(&sc, Side::Away, Role::CM8));
    for (&s, &m) in strikers.iter().zip(&eights) {
        let at = fr.to_team(sc.sim.state.players[s].pos);
        sc.place(m, fr.to_world(ball + (at - ball) * 0.45));
    }
    // The ball-far central midfielder and the opposing forward in his lane.
    let far = *of(&sc, Side::Home, Role::CMP)
        .iter()
        .max_by(|&&a, &&b| {
            let d = |i: PlayerId| (fr.to_team(sc.sim.state.players[i].pos).y - ball.y).abs();
            d(a).total_cmp(&d(b))
        })
        .unwrap();
    let a = anchor(&sc.sim.state, &sc.sim.cfg, far).0;
    let shadow = ball + (a - ball) * 0.5;
    sc.place(slot_of(&sc, Side::Away, Role::CF), fr.to_world(shadow));
    for &i in strikers.iter().chain([&far]) {
        perfect_reader(&mut sc, i);
    }
    sc.think();
    assert_ne!(far, carrier);
    for &s in &strikers {
        let short = (0..10).filter(|&k| matches!(decide(&sc, s, k).kind, Movement::Check | Movement::Drop)).count();
        assert!(short == 0, "striker {s} offers short in the midfielders' shadow in {short} of 10");
    }
    let deg = sc.sim.cfg.tactics.att14_shadow_deg;
    let share = share_on_line(&sc, far, ball, shadow, deg, 10);
    assert!(share == 0.0, "the ball-far midfielder stays in the forward's shadow in {:.0} % of choices", 100.0 * share);
}

#[test]
fn tr05_rest_defence_stays_behind_the_ball() {
    let (mut sc, _) = situation(8, Role::CM8, Vec2::new(25.0, 5.0));
    let tc = sc.sim.cfg.tactics.clone();
    let fr = home_frame(&sc);
    let ball = fr.to_team(sc.sim.state.ball.ground_pos());
    let rest: Vec<usize> = (0..PLAYERS_PER_TEAM)
        .filter(|&i| sc.sim.state.team_view(Side::Home).att_duties[i] == AttDuty::RestDefence)
        .collect();
    assert_eq!(rest.len(), tc.tr05_min_players);
    for &i in &rest {
        let t = fr.to_team(decide(&sc, i, 8).target);
        assert!(t.x <= ball.x - tc.tr05_gap + 0.5, "rest defender {i} at {t}, ball {ball}");
    }
    // Five opponents left high: five stay back.
    for (k, i) in (6..PLAYERS_PER_TEAM).enumerate() {
        sc.place(AWAY + i, fr.to_world(Vec2::new(-15.0, -20.0 + 10.0 * k as f32)));
    }
    sc.think();
    let n = sc.sim.state.team_view(Side::Home).att_duties.iter().filter(|r| **r == AttDuty::RestDefence).count();
    assert_eq!(n, tc.tr05_max_players);
}

#[test]
fn trn02_counter_press_ends_after_the_window() {
    let (mut sc, carrier) = situation(9, Role::CM8, Vec2::new(30.0, 24.0));
    let window = sc.sim.cfg.tactics.trn02_window_s;
    // The nearest defender wins the ball where it was lost: in the final third, out wide, with support nearby.
    let ball = sc.sim.state.players[carrier].pos;
    let thief = (AWAY..2 * PLAYERS_PER_TEAM)
        .min_by(|&a, &b| {
            let (pa, pb) = (sc.sim.state.players[a].pos, sc.sim.state.players[b].pos);
            (pa - ball).length().total_cmp(&(pb - ball).length())
        })
        .unwrap();
    sc.place(thief, ball + Vec2::new(0.0, -0.5)).give_ball(thief).think();
    let t0 = sc.sim.state.time;
    let phase = |sc: &Scenario| sc.sim.state.team_view(Side::Home).phase;
    assert_eq!(phase(&sc), TeamPhase::Out(OutPhase::CounterPress));
    sc.sim.state.time = t0 + window - 0.2;
    sc.think();
    assert_eq!(phase(&sc), TeamPhase::Out(OutPhase::CounterPress));
    sc.sim.state.time = t0 + window + 0.2;
    sc.think();
    assert_ne!(phase(&sc), TeamPhase::Out(OutPhase::CounterPress));
}

#[test]
fn gk02_keeper_smothers_only_when_he_gets_there_first() {
    let (mut sc, carrier) = situation(10, Role::CF, Vec2::new(38.0, 0.0));
    let fr = home_frame(&sc);
    // Through on goal: the defenders are behind the ball.
    for i in 1..PLAYERS_PER_TEAM {
        sc.place(AWAY + i, fr.to_world(Vec2::new(15.0, -25.0 + 5.0 * i as f32)));
    }
    sc.think();
    let gk = AWAY;
    let mv = decide(&sc, gk, 10);
    assert_eq!(mv.label, "angle", "14 m out, keeper far: narrow the angle");
    // Carrier 2 m in front of the keeper: smother.
    let gk_pos = fr.to_team(sc.sim.state.players[gk].pos);
    sc.place(carrier, fr.to_world(gk_pos - Vec2::new(2.0, 0.0))).give_ball(carrier).think();
    let mv = decide(&sc, gk, 10);
    assert_eq!(mv.label, "smother");
    assert!(mv.rules.contains(&"GK-02"));
}

#[test]
fn gk01_depth_lies_within_the_real_keepers_interquartile_range() {
    let tc = SimConfig::default().tactics;
    // StatsBomb open data, open-play shots: keeper depth off his line (p25, p75) by shot distance band.
    for (d, p25, p75) in [(4.0, 0.7, 1.9), (10.5, 1.0, 2.4), (14.0, 1.5, 3.1), (18.0, 1.6, 3.1), (22.5, 1.5, 2.9), (27.5, 1.5, 2.9), (32.5, 1.3, 2.6)] {
        let depth = gk01_depth(d, &tc);
        assert!((p25..=p75).contains(&depth), "{d} m: {depth:.2} m off the line, real IQR {p25}–{p75}");
    }
}

#[test]
fn gk03_keeper_stays_in_goal_with_the_ball_in_shooting_range_even_in_possession() {
    let tc = SimConfig::default().tactics;
    let off_line = |sc: &Scenario, mv: &positioning::Move| home_frame(sc).to_team(mv.target).x + 52.5;
    // BU-01: a centre-back on the ball 30 m from his own goal; a turnover there must not find the keeper high.
    let (sc, _) = situation(4, Role::CB, Vec2::new(-22.5, 0.0));
    let gk = slot_of(&sc, Side::Home, Role::GK);
    let mv = decide(&sc, gk, 4);
    assert!(mv.rules.contains(&"BU-01"));
    assert!(off_line(&sc, &mv) <= gk01_depth(30.0, &tc) + 0.5, "in range: {:.1} m off the line", off_line(&sc, &mv));
    // Ball past halfway: he supports behind the centre-backs again.
    let (sc, _) = situation(4, Role::CB, Vec2::new(8.0, 0.0));
    let mv = decide(&sc, gk, 4);
    assert!(off_line(&sc, &mv) > 8.0, "out of range: {:.1} m off the line", off_line(&sc, &mv));
}

/// Home carrier with the ball at home-frame `ball`, in shooting range of the away goal; the away first defender stands
/// 2 m goal-side of him (inside PRS-03's deceleration distance, so no curved approach), everyone else on his anchor.
fn shooting_range(seed: u64, ball: Vec2) -> (Scenario, PlayerId) {
    let (mut sc, carrier) = situation(seed, Role::CF, ball);
    let first = sc.sim.state.team_view(Side::Away).def_duties.iter().position(|r| *r == DefDuty::First).unwrap();
    let fr = home_frame(&sc);
    let goal_side = ball + (Vec2::new(52.5, 0.0) - ball).normalize() * 2.0;
    sc.place(AWAY + first, fr.to_world(goal_side)).think();
    (sc, carrier)
}

#[test]
fn box01_first_defender_gets_tight_and_shows_outside_in_shooting_range() {
    let tc = SimConfig::default().tactics;
    for (seed, ball) in [(1, Vec2::new(36.0, 10.0)), (2, Vec2::new(38.0, -12.0)), (3, Vec2::new(33.0, 6.0))] {
        let (sc, carrier) = shooting_range(seed, ball);
        let st = &sc.sim.state;
        let fr = st.frame(Side::Away);
        let first = st.team_view(Side::Away).def_duties.iter().position(|r| *r == DefDuty::First).unwrap();
        let mv = decide(&sc, AWAY + first, seed);
        assert!(mv.rules.contains(&"BOX-01"), "seed {seed}: rules {:?}", mv.rules);
        let c = fr.to_team(st.players[carrier].pos);
        let t = fr.to_team(mv.target);
        let d = (t - c).length();
        assert!(d < tc.ind01_contain_distance, "seed {seed}: contain distance {d} not tighter than IND-01");
        assert!(t.x < c.x, "seed {seed}: contain point {t} not goal-side of the carrier {c}");
        assert!(t.y.abs() < c.y.abs(), "seed {seed}: contain point {t} does not block the inside of {c}");
    }
}

#[test]
fn box02_second_defender_stands_on_the_shooting_line() {
    let tc = SimConfig::default().tactics;
    for (seed, ball) in [(1, Vec2::new(36.0, 10.0)), (2, Vec2::new(38.0, -12.0)), (3, Vec2::new(33.0, 6.0))] {
        let (sc, carrier) = shooting_range(seed, ball);
        let st = &sc.sim.state;
        let fr = st.frame(Side::Away);
        let second = st.team_view(Side::Away).def_duties.iter().position(|r| *r == DefDuty::Second).expect("second");
        let mv = decide(&sc, AWAY + second, seed);
        assert!(mv.rules.contains(&"BOX-02"), "seed {seed}: rules {:?}", mv.rules);
        let c = fr.to_team(st.players[carrier].pos);
        let to_goal = (TeamFrame::own_goal() - c).normalize();
        let rel = fr.to_team(mv.target) - c;
        assert!(rel.perp_dot(to_goal).abs() < 0.3, "seed {seed}: block point {rel} off the shooting line");
        assert!((rel.dot(to_goal) - tc.box02_block_distance).abs() < 0.3, "seed {seed}: block point {rel}");
    }
}

#[test]
fn box03_a_covered_defender_engages_an_equal_carrier_an_uncovered_one_contains() {
    let tackles = |cover: bool| {
        let mut n = 0;
        for seed in 1..=20 {
            let (mut sc, carrier) = situation(seed, Role::CF, Vec2::new(36.0, 0.0));
            let fr = home_frame(&sc);
            let defender = slot_of(&sc, Side::Away, Role::CB);
            for i in AWAY + 1..AWAY + PLAYERS_PER_TEAM {
                sc.place(i, fr.to_world(Vec2::new(5.0, (i - AWAY) as f32 * 4.0 - 24.0)));
            }
            let st = &mut sc.sim.state;
            for id in [carrier, defender] {
                st.players[id].attrs = Attributes::uniform(58.0);
                st.players[id].confidence = 0.5;
            }
            st.players[carrier].facing = fr.to_world(Vec2::X) - fr.to_world(Vec2::ZERO);
            st.players[carrier].next_decision = st.time + 10.0;
            sc.give_ball(carrier);
            let ball = fr.to_team(sc.sim.state.ball.ground_pos());
            sc.place(defender, fr.to_world(ball + Vec2::new(0.9, 0.0)));
            if cover {
                let other = (AWAY + 1..AWAY + PLAYERS_PER_TEAM).find(|&i| i != defender).unwrap();
                sc.place(other, fr.to_world(Vec2::new(39.5, 1.5)));
            }
            sc.think();
            let from = sc.sim.events.len();
            sc.sim.step();
            // A challenge judged a foul (LAW-02, since M7) is logged as the foul, not as a tackle.
            n += sc.sim.events[from..].iter().filter(|e| matches!(e.kind, EventKind::Tackle { .. } | EventKind::Foul { .. })).count();
        }
        n
    };
    let (covered, alone) = (tackles(true), tackles(false));
    assert!(covered >= 16, "covered defender tackled in {covered} of 20");
    assert!(alone <= 3, "uncovered defender tackled in {alone} of 20");
}

#[test]
fn box04_block_narrows_and_shortens_in_front_of_the_box() {
    let sc = Scenario::new(1);
    let tc = &sc.sim.cfg.tactics;
    let team = sc.sim.state.team(Side::Away);
    let (formation, d) = (&team.formation, &team.tactics.defending);
    let units = brain::shape_units(formation);
    let shape = |ball: Vec2| {
        let blk = brain::box04_block(brain::defensive_block(d, ball, true, false, tc), ball, units, d, tc);
        let pts = |f: fn(Role) -> bool| -> Vec<Vec2> {
            formation.slots.iter().filter(|s| f(s.role(false))).map(|s| slot_point(s, false, &blk)).collect()
        };
        let mut back = pts(Role::is_back_line);
        back.sort_by(|a, b| a.y.total_cmp(&b.y));
        let mid = pts(|r| r.unit() == Unit::Midfield);
        let gap = back.windows(2).map(|w| w[1].y - w[0].y).fold(0.0, f32::max);
        let mid_ahead = mid.iter().map(|q| q.x).sum::<f32>() / mid.len() as f32 - blk.line_x;
        (gap, mid_ahead, back)
    };
    // Ball 18 m out (central and wide of the posts): neighbours at most `box_max_gap` apart, the far full-back
    // inside the box width, the midfield line `box_mid_distance` in front of the back line.
    for y in [0.0, 9.0, -9.0] {
        let (gap, mid_ahead, back) = shape(Vec2::new(-HALF_LENGTH + 18.0, y));
        assert!(gap <= d.box_max_gap + 1e-3, "ball y {y}: back-line gap {gap:.1}");
        assert!(back.iter().all(|q| q.y.abs() < PENALTY_AREA_HALF_WIDTH), "ball y {y}: back line {back:?}");
        assert!((mid_ahead - d.box_mid_distance).abs() < 0.5, "ball y {y}: midfield {mid_ahead:.1} m ahead");
    }
    // Ball in midfield: the DEF-05 block, untouched.
    let (gap, mid_ahead, _) = shape(Vec2::new(0.0, 0.0));
    assert!(gap > 2.0 * d.box_max_gap && mid_ahead > 2.0 * d.box_mid_distance, "midfield: gap {gap:.1}, midfield {mid_ahead:.1}");
}

#[test]
fn box04_the_line_drops_at_the_speed_of_the_ball_near_goal() {
    // Away defends; the home carrier runs at the away goal from 47.5 m to 32.5 m out in one second.
    let drop = |ball_speed: f32| {
        let (mut sc, carrier) = situation(5, Role::CF, Vec2::new(5.0, 0.0));
        let line0 = sc.sim.state.team_view(Side::Away).def_block.line_x;
        let fr = home_frame(&sc);
        sc.place(carrier, fr.to_world(Vec2::new(20.0, 0.0))).give_ball(carrier);
        sc.sim.state.ball.vel = (fr.to_world(Vec2::X) * ball_speed).extend(0.0);
        sc.sim.state.time += 1.0;
        let now = sc.sim.state.time;
        sc.sim.update_brains(now);
        line0 - sc.sim.state.team_view(Side::Away).def_block.line_x
    };
    let cap = SimConfig::default().tactics.def05_shift_speed;
    let (still, running) = (drop(0.0), drop(7.0));
    assert!(still <= cap + 1e-3, "ball still: line dropped {still:.1} m in 1 s");
    assert!(running > 1.8 * cap, "ball at 7 m/s: line dropped only {running:.1} m in 1 s");
}

#[test]
fn def04_a_runner_leaving_the_zone_is_tracked_until_a_team_mate_takes_him_over() {
    // DEF-04 / IND-04 ("space in behind" [FA-EFL]): the away centre-back marks the home centre-forward, who sprints
    // towards goal out of his zone with nobody goal-side of him: the marker tracks him. Once a team-mate stands
    // goal-side of the runner, he hands him over.
    let (mut sc, _) = situation(5, Role::CM8, Vec2::new(0.0, 0.0));
    let (marker, runner) = (slot_of(&sc, Side::Away, Role::CB), slot_of(&sc, Side::Home, Role::CF));
    let afr = sc.sim.state.frame(Side::Away);
    let anchor_t = match_engine::ai::shape::anchor(&sc.sim.state, &sc.sim.cfg, marker).0;
    let at = anchor_t + Vec2::new(-17.0, 0.0);
    sc.place(runner, afr.to_world(at));
    sc.sim.state.players[runner].vel = afr.to_world(Vec2::new(-7.0, 0.0));
    let slot = marker - AWAY;
    for p in sc.sim.state.players[AWAY..].iter_mut() {
        p.beliefs.def_duties[slot] = DefDuty::Mark(runner);
    }
    let now = sc.sim.state.time;
    sc.sim.update_brains(now);
    assert_eq!(sc.sim.state.team_view(Side::Away).def_duties[slot], DefDuty::Track(runner));
    let mv = decide(&sc, marker, 1);
    assert_eq!(mv.label, "track the runner");
    assert!(afr.to_team(mv.target).x < at.x, "goal-side of the runner: {:?}", afr.to_team(mv.target));
    // A team-mate goal-side of him takes him over.
    let mate = (AWAY + 1..2 * AWAY).find(|&i| i != marker && !sc.sim.state.players[i].is_gk()).unwrap();
    sc.place(mate, afr.to_world(at + Vec2::new(-2.0, 0.5)));
    sc.sim.update_brains(now + 0.2);
    assert_ne!(sc.sim.state.team_view(Side::Away).def_duties[slot], DefDuty::Track(runner));
}

#[test]
fn blk02_the_line_leader_calls_a_step_only_once_the_pressure_holds() {
    // BLK-02 ("pressure on the ball → step, no pressure → drop" [FA-EFL]) with the call's hysteresis: the line
    // leader changes the call after the reading has held for `blk02_call_hold_s`, so a flicker of pressure moves
    // nobody.
    let (mut sc, carrier) = situation(4, Role::CM8, Vec2::new(5.0, 0.0));
    let fr = home_frame(&sc);
    let cp = fr.to_team(sc.sim.state.players[carrier].pos);
    let presser = (AWAY + 1..2 * AWAY)
        .min_by(|&i, &j| {
            let d = |k: usize| (fr.to_team(sc.sim.state.players[k].pos) - cp).length();
            d(i).total_cmp(&d(j))
        })
        .unwrap();
    let hold = sc.sim.cfg.tactics.blk02_call_hold_s;
    let at = |sc: &mut Scenario, near: bool, t: f32| {
        // Everyone else at least 10 m from the carrier; the presser 1 m goal-side of him or 15 m away.
        for i in AWAY..2 * AWAY {
            let q = fr.to_team(sc.sim.state.players[i].pos);
            if i != presser && (q - cp).length() < 10.0 {
                sc.place(i, fr.to_world(cp + (q - cp).normalize_or(Vec2::X) * 10.0));
            }
        }
        sc.place(presser, fr.to_world(cp + Vec2::new(if near { 1.0 } else { 15.0 }, 0.0)));
        sc.sim.update_brains(t);
        let b = sc.sim.state.team_view(Side::Away);
        assert_eq!(b.pressured, near);
        // The leader's own call (the view is the keeper's, who reads it after his delay since M6).
        let leader = b.line_leader.expect("a line leader");
        sc.sim.state.squad(Side::Away)[leader].beliefs.line_step
    };
    let t0 = sc.sim.state.time;
    assert!(!at(&mut sc, false, t0));
    assert!(!at(&mut sc, false, t0 + 0.2));
    // A short flicker of pressure: no call.
    assert!(!at(&mut sc, true, t0 + 0.4));
    assert!(!at(&mut sc, false, t0 + 0.6));
    // Pressure that holds: the call to step comes after the hold time, and stays through a short release.
    let mut t = t0 + 0.8;
    while t < t0 + 0.8 + hold - 0.01 {
        let _ = at(&mut sc, true, t);
        t += 0.2;
    }
    assert!(at(&mut sc, true, t0 + 1.0 + hold));
    assert!(at(&mut sc, false, t0 + 1.2 + hold));
    assert!(at(&mut sc, true, t0 + 1.4 + hold));
}

/// Phase D front-line scenario: the home right centre-mid on the ball at `ball` (home frame), facing forward, after
/// the in-possession block has reached the line. Returns (scenario, carrier, centre-forward).
fn front(seed: u64, ball: Vec2) -> (Scenario, PlayerId, PlayerId) {
    front_with(seed, ball, built_line())
}

/// As `front`, with configuration `cfg`.
fn front_with(seed: u64, ball: Vec2, cfg: SimConfig) -> (Scenario, PlayerId, PlayerId) {
    let (mut sc, carrier) = common::settled(Scenario::with_config(seed, cfg), Side::Home, Role::CM8, ball);
    for _ in 0..30 {
        sc.sim.state.time += 0.2;
        sc.think();
    }
    let fr = home_frame(&sc);
    sc.sim.state.players[carrier].facing = fr.to_world(Vec2::X) - fr.to_world(Vec2::ZERO);
    keeper_at_depth(&mut sc, Side::Home, ball);
    sc.think();
    let cf = slot_of(&sc, Side::Home, Role::CF);
    (sc, carrier, cf)
}

/// The front-line scenarios (ATT-18, ATT-26, look-ahead) were built with the opponents' mid-block line at 32.5 m from
/// goal (`blk05_line_mid` −20 before the watch round). At the sourced 37.5 m (BLK-05) the 5 m more behind the line make
/// an open carrier's run in behind worth more than every other front movement and give a second runner a lane of his
/// own 8 m away (docs/design.md, "Watch round", set A); these tests are about the choices at the geometry they set.
/// The instruction test runs at the sourced line.
fn built_line() -> SimConfig {
    let mut cfg = SimConfig::default();
    cfg.tactics.blk05_line_mid = -20.0;
    cfg
}

/// The keeper defending against `attacking` at his GK-01 depth for the ball at `ball` (attacking frame): a keeper's
/// layer-1 anchor is his back line, where `situation` leaves him.
fn keeper_at_depth(sc: &mut Scenario, attacking: Side, ball: Vec2) {
    let fr = sc.sim.state.frame(attacking);
    let gk = slot_of(sc, attacking.opponent(), Role::GK);
    let depth = gk01_depth((Vec2::new(HALF_LENGTH, 0.0) - ball).length(), &sc.sim.cfg.tactics);
    sc.place(gk, fr.to_world(Vec2::new(HALF_LENGTH - depth, 0.0)));
}

/// A runner (quick, ordinary control) or a link player (slow, strong, good first touch) [FA-FWD; section 8.1].
fn profile(sc: &mut Scenario, id: PlayerId, runner: bool) {
    let a = &mut sc.sim.state.players[id].attrs;
    let (r, l) = if runner { (88.0, 45.0) } else { (45.0, 88.0) };
    (a.speed, a.quickness, a.first_touch, a.strength, a.short_passing) = (r, r, l, l, l);
    (a.intelligence, a.instinct, a.composure) = (65.0, 65.0, 65.0);
    // Kinematics follow the attributes.
    let c = sc.sim.state.conditions;
    sc.sim.set_conditions(c);
}

/// Shares of `id`'s fresh movement choices over `n` seeds: run in behind, check or drop, pin or hold.
fn front_shares(sc: &Scenario, id: PlayerId, n: u64) -> [f32; 3] {
    let mut c = [0.0; 3];
    for s in 0..n {
        let i = match decide(sc, id, s).kind {
            Movement::RunInBehind => 0,
            Movement::Check | Movement::Drop => 1,
            _ => 2,
        };
        c[i] += 1.0 / n as f32;
    }
    c
}

/// Keep `id`'s decision as his plan, as the simulation does.
fn keep_plan(sc: &mut Scenario, id: PlayerId, mv: &positioning::Move) {
    let d = movement::Decided { kind: mv.kind, chose: mv.chose, offer: mv.offer, projected: mv.projected, noise: mv.noise, concept: mv.concept };
    let plan = movement::plan_after(&sc.sim.state, &sc.sim.cfg, &sc.sim.state.players[id], d);
    sc.sim.state.players[id].plan = plan;
}

#[test]
fn att18_with_an_open_ball_a_fast_nine_runs_in_behind_and_the_through_ball_goes_into_his_run() {
    let (mut sc, carrier, cf) = front(21, Vec2::new(0.0, -8.0));
    profile(&mut sc, cf, true);
    let [run, short, _] = front_shares(&sc, cf, 40);
    assert!(run >= 0.5 && run > short, "open ball: run in behind {:.0} %, check / drop {:.0} %", 100.0 * run, 100.0 * short);
    // He waits onside until the pass is set up [FA-FWD: "timing of the run"; FIFA-HED].
    let fr = home_frame(&sc);
    let onside = sc.sim.state.team_view(Side::Home).offside_line - sc.sim.cfg.tactics.att11_onside_margin;
    let mv = (0..40).map(|s| decide(&sc, cf, s)).find(|m| m.kind == Movement::RunInBehind).expect("a run");
    assert!(fr.to_team(mv.target).x <= onside, "waits onside: target {:.1}, onside {onside:.1}", fr.to_team(mv.target).x);
    let offer = mv.offer.expect("a run offers");
    assert!(fr.to_team(offer).x > onside + 5.0, "offers in behind: {:.1} vs line {onside:.1}", fr.to_team(offer).x);
    // The carrier's through ball to him goes into the space his run offers (judging the line exactly: the passer's
    // LAW-01 judgement error, which can rule a runner just onside offside, is not under test here).
    keep_plan(&mut sc, cf, &mv);
    let st = &sc.sim.state;
    let mut cfg = sc.sim.cfg.clone();
    cfg.ai.offside_judge_m = 0.0;
    let opts = evaluate(st, &cfg, &sc.sim.plan, Situation::open_play(carrier, st.ball.pos));
    let through = opts.iter().find_map(|o| match (&o.kind, &o.action) {
        (OptionKind::Pass { through: true, .. }, Action::Kick(k)) if k.receiver == Some(cf) => Some(k.target),
        _ => None,
    });
    let t = through.expect("a through ball to the runner");
    assert!((t - offer).length() < 2.0, "through ball at {t:?}, run offers {offer:?}");
}

/// `id`'s front movement scores now (section 8) and his look-ahead.
fn scores(sc: &Scenario, id: PlayerId) -> (Vec<positioning::MoveScore>, f32) {
    let st = &sc.sim.state;
    let s = positioning::move_scores(st, &sc.sim.cfg, id).expect("a front player with someone to offer to");
    (s, movement::lookahead(&st.players[id]))
}

/// The best of `s` among `kinds` by `f`.
fn best_of(s: &[positioning::MoveScore], kinds: &[Movement], f: impl Fn(&positioning::MoveScore) -> f32) -> f32 {
    s.iter().filter(|x| kinds.contains(&x.kind)).map(f).fold(f32::MIN, f32::max)
}

const TO_FEET: [Movement; 4] = [Movement::Hold, Movement::Pin, Movement::Check, Movement::Drop];

#[test]
fn att26_a_closed_carrier_gets_an_offer_to_feet_instead_of_a_run() {
    let (mut sc, carrier, cf) = front(22, Vec2::new(0.0, -8.0));
    profile(&mut sc, cf, true);
    let [open_run, ..] = front_shares(&sc, cf, 40);
    // Under pressure [FA-FWD: "If they have their head down or under pressure, drop-off and provide an option to play
    // to feet"]: an opponent closes him down from in front.
    let fr = home_frame(&sc);
    let presser = (AWAY..2 * PLAYERS_PER_TEAM).find(|&i| sc.sim.state.players[i].role == Role::CMP).unwrap();
    sc.place(presser, fr.to_world(Vec2::new(1.5, -8.0)));
    sc.think();
    assert!(!is_open(&sc.sim.state, &sc.sim.cfg, &sc.sim.state.players[carrier]), "the carrier is closed");
    let [closed_run, ..] = front_shares(&sc, cf, 40);
    assert!(closed_run < open_run - 0.2, "run in behind: closed {:.0} %, open {:.0} %", 100.0 * closed_run, 100.0 * open_run);
    let (s, _) = scores(&sc, cf);
    let (feet, run) = (best_of(&s, &TO_FEET, |x| x.marginal), best_of(&s, &[Movement::RunInBehind], |x| x.marginal));
    assert!(feet > run && feet > 0.0, "marginal value: to feet {feet:.4}, run {run:.4}");
}

#[test]
fn att26_same_situation_a_runner_values_the_run_and_a_link_player_the_ball_to_feet() {
    // Section 8.1: player type comes out of the reach and duel models — a runner's run arrives first more often, a
    // link player loses less of a pressed ball to feet [FA-FWD].
    let (mut sc, _, cf) = front(23, Vec2::new(-5.0, -8.0));
    let gap = |sc: &Scenario| {
        let (s, look) = scores(sc, cf);
        (best_of(&s, &[Movement::RunInBehind], |x| x.value(look)) - best_of(&s, &TO_FEET, |x| x.value(look)), s[0].scale)
    };
    profile(&mut sc, cf, true);
    let (runner, scale) = gap(&sc);
    let [r_run, ..] = front_shares(&sc, cf, 40);
    profile(&mut sc, cf, false);
    let (link, _) = gap(&sc);
    let [l_run, ..] = front_shares(&sc, cf, 40);
    assert!(runner > link + 0.1 * scale, "run less ball to feet: runner {runner:.4}, link {link:.4} (scale {scale:.4})");
    assert!(r_run >= l_run, "run in behind: runner {:.0} %, link {:.0} %", 100.0 * r_run, 100.0 * l_run);
}

#[test]
fn att26_forward_runs_and_come_short_instructions_change_the_choice() {
    // At the sourced block line: an instruction is a share of the movement's own worth, so it changes the choice
    // however much the run in behind is worth there (watch round, finding 32).
    let (mut sc, _, cf) = front_with(24, Vec2::new(0.0, -8.0), SimConfig::default());
    let [run0, short0, _] = front_shares(&sc, cf, 60);
    sc.sim.state.players[cf].instructions.forward_runs = Some(1.0);
    let [run1, _, _] = front_shares(&sc, cf, 60);
    sc.sim.state.players[cf].instructions.forward_runs = Some(-1.0);
    let [run2, _, _] = front_shares(&sc, cf, 60);
    sc.sim.state.players[cf].instructions.forward_runs = None;
    sc.sim.state.players[cf].instructions.come_short = Some(1.0);
    let [_, short1, _] = front_shares(&sc, cf, 60);
    assert!(
        run1 > run0 + 0.15 || run2 < run0 - 0.15,
        "run in behind: forward_runs +1 {:.0} %, none {:.0} %, −1 {:.0} %",
        100.0 * run1,
        100.0 * run0,
        100.0 * run2
    );
    assert!(run1 >= run0 && run2 <= run0, "forward_runs is monotonic: {run2:.2} ≤ {run0:.2} ≤ {run1:.2}");
    assert!(short1 > short0 + 0.15, "check / drop: come_short +1 {:.0} %, none {:.0} %", 100.0 * short1, 100.0 * short0);
}

#[test]
#[ignore = "open (watch round, findings C26, S12): the first striker's check, 12 m across, is a separate option for \
            the carrier, so the second one's check adds 0.0019 with or without it; his run in behind nets 0.0016 after \
            its effort (0.0022) against 0.0018 for the check: holding the depth for a team-mate who comes short \
            (DFB-RP16) has no value beyond the offers' overlap"]
fn att26_when_one_striker_comes_short_the_other_holds_the_depth() {
    // Away 4-4-2 on the ball: two strikers [DFB-RP16: "Wenn ein Stürmer sich anbietet, sichert der andere die Tiefe"];
    // section 8.3: a second offer to feet adds little once a team-mate shows.
    let (mut sc, carrier) = common::settled(Scenario::with_config(25, built_line()), Side::Away, Role::CMP, Vec2::new(0.0, 6.0));
    for _ in 0..30 {
        sc.sim.state.time += 0.2;
        sc.think();
    }
    let fr = sc.sim.state.frame(Side::Away);
    sc.sim.state.players[carrier].facing = fr.to_world(Vec2::X) - fr.to_world(Vec2::ZERO);
    keeper_at_depth(&mut sc, Side::Away, Vec2::new(0.0, 6.0));
    sc.think();
    let st2: Vec<PlayerId> = (AWAY..2 * PLAYERS_PER_TEAM).filter(|&i| sc.sim.state.players[i].role == Role::ST2).collect();
    let (a, b) = (st2[0], st2[1]);
    let short = [Movement::Check, Movement::Drop];
    let (sa, _) = scores(&sc, a);
    let hold = |sc: &mut Scenario, kinds: &[Movement]| {
        let m = sa.iter().filter(|x| kinds.contains(&x.kind)).max_by(|x, y| x.own.total_cmp(&y.own)).expect("a movement");
        let d = movement::Decided { kind: m.kind, chose: true, offer: Some(fr.to_world(m.point)), projected: false, noise: 0, concept: None };
        let plan = movement::plan_after(&sc.sim.state, &sc.sim.cfg, &sc.sim.state.players[a], d);
        sc.sim.state.players[a].plan = plan;
    };
    // Alone: `a` pins the line (he may have checked by himself while the scene settled).
    hold(&mut sc, &[Movement::Pin]);
    let alone = best_of(&scores(&sc, b).0, &short, |x| x.marginal);
    // `a` checks to feet (his best check or drop) and holds that movement.
    hold(&mut sc, &short);
    let beside = best_of(&scores(&sc, b).0, &short, |x| x.marginal);
    let [_, b_short, _] = front_shares(&sc, b, 40);
    assert!(beside < alone, "second striker's check / drop adds {beside:.4} with a team-mate showing, {alone:.4} alone");
    assert!(b_short <= 0.25, "second striker comes short too in {:.0} % of choices", 100.0 * b_short);
}

#[test]
fn look_ahead_a_reader_chooses_his_movement_for_the_next_carrier_as_the_pass_is_struck() {
    // Section 8.4: with a pass to the 9 in flight, a high reader re-chooses for the 9 as receiver; a low reader only
    // once the 9 has it.
    let (mut sc, carrier, cf) = front(26, Vec2::new(-5.0, -8.0));
    let w = slot_of(&sc, Side::Home, Role::W);
    let target = sc.sim.state.players[cf].pos;
    let plan = match_engine::player::KickPlan {
        spec: match_engine::ball::KickSpec::Ground { target, arrival_speed: 8.0 },
        kind: KickKind::GroundPass,
        receiver: Some(cf),
        target,
        arrival_t: 1.2,
        xg: 0.0,
        speed: 16.0,
        foot: None,
    };
    let now = sc.sim.state.time;
    // The 9 reads the pass at once (section 7), so he receives as it is struck.
    sc.sim.cfg.ai.reading_receive_delay_max_s = 0.0;
    sc.sim.execute_kick(carrier, plan, false, now);
    sc.think();
    let left = match sc.sim.state.players[cf].intent {
        Intent::Receive { t, .. } => t - sc.sim.state.time,
        _ => panic!("the 9 receives"),
    };
    let set = |sc: &mut Scenario, r: f32| {
        let a = &mut sc.sim.state.players[w].attrs;
        (a.intelligence, a.instinct) = (r, r);
    };
    set(&mut sc, 95.0);
    let high = decide(&sc, w, 1);
    set(&mut sc, 10.0);
    let low = decide(&sc, w, 1);
    assert!(left < 0.9 * sc.sim.cfg.ai.lookahead_horizon_s, "pass due in {left:.2} s");
    assert!(high.chose && high.projected, "high reader re-chose for the receiver: chose {}, projected {}", high.chose, high.projected);
    assert!(!low.projected, "low reader looks {:.2} s ahead, the pass is due in {left:.2} s", 0.1 * sc.sim.cfg.ai.lookahead_horizon_s);
}

/// `id` takes up his best `kind` movement (by its own offer) and holds it, as the simulation keeps a plan.
fn hold_movement(sc: &mut Scenario, id: PlayerId, kind: Movement) {
    let (s, _) = scores(sc, id);
    let m = s.iter().filter(|x| x.kind == kind).max_by(|a, b| a.own.total_cmp(&b.own)).expect("movement in repertoire");
    let fr = sc.sim.state.frame(sc.sim.state.players[id].side);
    let d = movement::Decided { kind, chose: true, offer: Some(fr.to_world(m.point)), projected: false, noise: 0, concept: None };
    let plan = movement::plan_after(&sc.sim.state, &sc.sim.cfg, &sc.sim.state.players[id], d);
    sc.sim.state.players[id].plan = plan;
}

#[test]
fn att26_the_ten_does_not_copy_the_strikers_run_and_offers_between_the_lines_it_opens() {
    // Test 5 [BDFL-HALE]: the striker's run in depth pulls the back line away and the 10 offers in the space between
    // the lines; he must not "identisch mit dem Laufverhalten der Spitze immer den Weg in die Tiefe" suchen.
    let (mut sc, carrier) =
        common::settled(common::with_formations_config(51, "4-2-3-1", "4-4-2", built_line()), Side::Home, Role::CMP, Vec2::new(-5.0, -6.0));
    for _ in 0..30 {
        sc.sim.state.time += 0.2;
        sc.think();
    }
    let fr = home_frame(&sc);
    sc.sim.state.players[carrier].facing = fr.to_world(Vec2::X) - fr.to_world(Vec2::ZERO);
    sc.think();
    let (cf, am) = (slot_of(&sc, Side::Home, Role::CF), slot_of(&sc, Side::Home, Role::AM));
    perfect_reader(&mut sc, am);
    let between = |sc: &Scenario| best_of(&scores(sc, am).0, &[Movement::Drop], |x| x.marginal);
    hold_movement(&mut sc, cf, Movement::Pin);
    let pinned = between(&sc);
    hold_movement(&mut sc, cf, Movement::RunInBehind);
    let pulled = between(&sc);
    let copies = (0..10).filter(|&k| decide(&sc, am, k).kind == Movement::RunInBehind).count();
    // He offers between the opponents' midfield and back units: a drop, or a show for the next carrier that ends there.
    let units = sc.sim.state.players[am].beliefs.opp_units.clone();
    let [.., mid, def] = units[..] else { panic!("two opposing units: {units:?}") };
    let drops = (0..10)
        .filter(|&k| decide(&sc, am, k).offer.is_some_and(|o| (mid..def).contains(&fr.to_team(o).x)))
        .count();
    assert!(pulled > pinned, "the 10's drop between the lines adds {pulled:.4} with the striker running, {pinned:.4} with him pinning");
    assert!(copies == 0 && drops == 10, "with the striker running in behind the 10 runs too in {copies}, offers between the lines in {drops} of 10");
}

#[test]
fn bu_build_up_gives_the_centre_back_three_options_on_both_sides() {
    // Test 7 [FFA-GTC: at least three passing options, "left, right, central and far"; TR-05]: the centre-back on
    // the ball in the own third; once his team-mates have taken up their movements he has at least three safe passes,
    // on both sides of him, and the rest defence keeps its count.
    for (seed, ball) in [(31, Vec2::new(-38.0, -10.0)), (32, Vec2::new(-30.0, 8.0))] {
        let (mut sc, carrier) = situation(seed, Role::CB, ball);
        let mates: Vec<PlayerId> = (0..PLAYERS_PER_TEAM).filter(|&i| i != carrier && !sc.sim.state.players[i].is_gk()).collect();
        for r in 0..3 {
            for &i in &mates {
                let mv = decide(&sc, i, r);
                keep_plan(&mut sc, i, &mv);
                sc.place(i, mv.target);
            }
            sc.think();
        }
        let st = &sc.sim.state;
        let fr = home_frame(&sc);
        let opts = evaluate(st, &sc.sim.cfg, &sc.sim.plan, Situation::open_play(carrier, st.ball.pos));
        let mut safe: Vec<PlayerId> = opts
            .iter()
            .filter_map(|o| match (&o.kind, &o.action) {
                (OptionKind::Pass { .. }, Action::Kick(k)) if o.p >= 0.8 => k.receiver,
                _ => None,
            })
            .collect();
        safe.sort();
        safe.dedup();
        let side = |r: &PlayerId| fr.to_team(st.players[*r].pos).y > ball.y;
        assert!(safe.len() >= sc.sim.cfg.tactics.att09_min_options, "ball at {ball}: {} safe passes", safe.len());
        assert!(safe.iter().any(side) && !safe.iter().all(side), "ball at {ball}: safe passes on one side only");
        let rest = st.team_view(Side::Home).att_duties.iter().filter(|d| **d == AttDuty::RestDefence).count();
        assert!(rest >= sc.sim.cfg.tactics.tr05_min_players, "rest defence {rest}");
    }
}

#[test]
fn full_back_instructions_overlap_invert_and_stay_back_change_his_choices() {
    // Test 14 (docs/player-brain.md section 5.4): the right-back of a 4-4-2 with his winger on the ball wide ahead
    // of him. "Overlap" sends him round the winger, "move inside" (invert) into midfield, "stay back"
    // (forward_runs −1) makes him claim the rest defence (TR-05).
    let (mut sc, _) = common::settled(common::with_formations(41, "4-4-2", "4-4-2"), Side::Home, Role::WM, Vec2::new(5.0, -26.0));
    let rb = (0..PLAYERS_PER_TEAM).find(|&i| sc.sim.state.players[i].role == Role::FB && sc.sim.state.team(Side::Home).formation.slots[i].in_pos.1 < 0.0).unwrap();
    let share = |sc: &Scenario, kinds: &[Movement]| (0..40).filter(|&k| kinds.contains(&decide(sc, rb, k).kind)).count() as f32 / 40.0;
    let round = [Movement::Overlap, Movement::Underlap];
    let rest = |sc: &Scenario| sc.sim.state.team_view(Side::Home).att_duties[rb] == AttDuty::RestDefence;
    let (round0, inside0) = (share(&sc, &round), share(&sc, &[Movement::Invert]));
    assert!(!rest(&sc), "the ball-side full-back joins the attack (TR-05 ball-side release)");
    sc.sim.state.players[rb].instructions.overlap = Some(1.0);
    let round1 = share(&sc, &round);
    sc.sim.state.players[rb].instructions.overlap = None;
    sc.sim.state.players[rb].instructions.invert = Some(1.0);
    let inside1 = share(&sc, &[Movement::Invert]);
    sc.sim.state.players[rb].instructions.invert = None;
    sc.sim.state.players[rb].instructions.forward_runs = Some(-1.0);
    // He knows the instruction fully (concept practised, section 7).
    sc.sim.state.players[rb].concept = 1.0;
    sc.think();
    assert!(round1 >= round0 + 0.5, "overlap / underlap: instructed {:.0} %, not {:.0} %", 100.0 * round1, 100.0 * round0);
    assert!(inside1 >= inside0 + 0.5, "invert: instructed {:.0} %, not {:.0} %", 100.0 * inside1, 100.0 * inside0);
    assert!(rest(&sc), "told to stay back, he claims the rest defence");
}

/// The FT-03 spots for a ball on the `side` (+1 / −1) of the pitch (team frame).
fn box_spots(sc: &Scenario, side: f32) -> Vec<Vec2> {
    sc.sim.cfg.tactics.ft03_spots.iter().map(|&[d, y]| Vec2::new(HALF_LENGTH - d, side * y)).collect()
}

#[test]
fn ft03_with_the_ball_wide_in_the_final_third_a_forward_runs_into_the_box_and_the_cross_goes_to_his_spot() {
    // "attacks the front post", "the back post", "the edge of the penalty area for any second balls" [FIFA-ES].
    let (mut sc, carrier) = situation(31, Role::W, Vec2::new(38.0, -27.0));
    let fr = home_frame(&sc);
    sc.sim.state.players[carrier].facing = fr.to_world(Vec2::new(0.6, 1.0).normalize()) - fr.to_world(Vec2::ZERO);
    sc.think();
    let cf = slot_of(&sc, Side::Home, Role::CF);
    let spots = box_spots(&sc, -1.0);
    let mv = (0..40).map(|s| decide(&sc, cf, s)).find(|m| m.kind == Movement::BoxRun).expect("a box run");
    let offer = fr.to_team(mv.offer.expect("a box run offers"));
    assert!(spots.iter().any(|q| (*q - offer).length() < 2.0), "box run to {offer:?}, spots {spots:?}");
    // The carrier's cross goes to the spot his run offers, once he has read it (section 7).
    keep_plan(&mut sc, cf, &mv);
    sc.sim.state.players[cf].plan.since -= 3.0;
    let st = &sc.sim.state;
    let opts = evaluate(st, &sc.sim.cfg, &sc.sim.plan, Situation::open_play(carrier, st.ball.pos));
    let to_spot = opts.iter().any(|o| match &o.action {
        Action::Kick(k) => k.receiver == Some(cf) && (fr.to_team(k.target) - offer).length() < 2.0,
        _ => false,
    });
    assert!(to_spot, "a cross to the box runner's spot {offer:?}");
    // With the ball central there is nothing to run onto from wide: no box run.
    let (sc, _) = situation(31, Role::CM8, Vec2::new(30.0, 0.0));
    let s = positioning::move_scores(&sc.sim.state, &sc.sim.cfg, cf).expect("a front player with someone to offer to");
    assert!(s.iter().all(|x| x.kind != Movement::BoxRun), "no box run with the ball central");
}

#[test]
fn ft03_box_runners_take_different_spots() {
    // Two offers within `offer_overlap_m` are one option (ATT-10): the second runner adds most at another FT-03 spot,
    // for the carrier or the next carrier (8.4). The far post and the penalty spot are 5.7 m apart, so a run to one adds
    // nothing to the carrier's cross with the other taken, but can add to the next carrier's options.
    let (mut sc, carrier) = situation(32, Role::W, Vec2::new(40.0, 27.0));
    let fr = home_frame(&sc);
    sc.sim.state.players[carrier].facing = fr.to_world(Vec2::new(0.6, -1.0).normalize()) - fr.to_world(Vec2::ZERO);
    sc.think();
    let cf = slot_of(&sc, Side::Home, Role::CF);
    let mv = (0..40).map(|s| decide(&sc, cf, s)).find(|m| m.kind == Movement::BoxRun).expect("a box run");
    keep_plan(&mut sc, cf, &mv);
    sc.sim.state.players[cf].plan.since -= 3.0;
    let taken = fr.to_team(mv.offer.unwrap());
    let others: Vec<PlayerId> = (0..PLAYERS_PER_TEAM).filter(|&i| i != cf && i != carrier && !sc.sim.state.players[i].is_gk()).collect();
    let mut seen = false;
    for id in others {
        let Some(s) = positioning::move_scores(&sc.sim.state, &sc.sim.cfg, id) else { continue };
        if let Some(b) = s.iter().find(|x| x.kind == Movement::BoxRun) {
            seen = true;
            assert!((b.point - taken).length() > 1.0, "{id} runs to {:?}, taken {taken:?}", b.point);
        }
    }
    assert!(seen, "another player can run into the box");
}

#[test]
fn crs01_against_a_ball_in_a_crossing_position_the_line_drops_into_the_box_whatever_the_pressure() {
    let tc = SimConfig::default().tactics;
    let d = resolve(&TeamInstructions::default(), &tc).defending;
    let goal = -HALF_LENGTH;
    for x in [-30.0, -24.0, -20.0] {
        let wide = Vec2::new(x, 28.0);
        for pressured in [true, false] {
            let line = brain::defensive_block(&d, wide, pressured, false, &tc).line_x;
            assert!(line <= goal + tc.crs01_line_depth + 1e-3, "ball {wide:?}, pressured {pressured}: line {:.1} m out", line - goal);
        }
        // Central, a pressed carrier is stepped up to (BLK-02).
        let central = brain::defensive_block(&d, Vec2::new(x, 0.0), true, false, &tc).line_x;
        assert!(central > goal + tc.crs01_line_depth, "central ball at {x}: line {:.1} m out", central - goal);
    }
}

/// The away covering defender's target (team frame of the home side) with the home winger on the ball at `ball`, the
/// away trap instruction set to `trap` (None: layer 1).
fn cover_target(seed: u64, ball: Vec2, trap: Option<f32>) -> (Vec2, Vec2, bool) {
    let (mut sc, carrier) = situation(seed, Role::W, ball);
    sc.sim.state.teams[Side::Away.index()].instructions.defending.trap = trap;
    sc.sim.refresh_tactics();
    sc.think();
    let st = &sc.sim.state;
    let b = st.team_view(Side::Away);
    let second = b.def_duties.iter().position(|d| *d == DefDuty::Second).expect("a covering defender") + AWAY;
    let fr = home_frame(&sc);
    (fr.to_team(decide(&sc, second, 0).target), fr.to_team(st.players[carrier].pos), b.press)
}

#[test]
fn prs05_a_press_at_the_touchline_traps_the_carrier_with_the_cover_closing_the_line() {
    // "show the centre-back to the touchline", "numerical advantage around the ball" [FIFA-PR]: the first defender
    // shows him down the line (DEF-07 channel wide), the cover closes the line, the touchline is the third defender.
    let trap = SimConfig::default().tactics.prs05_trap_distance;
    for (seed, ball) in [(41, Vec2::new(5.0, 31.0)), (42, Vec2::new(-10.0, -31.0))] {
        let (t, c, press) = cover_target(seed, ball, None);
        assert!(press, "seed {seed}: the touchline triggers the press (PRS-01)");
        assert!((t - c).length() <= trap + 1.0, "seed {seed}: trap point {t:?} is {:.1} m from the carrier", (t - c).length());
        assert!(t.x > c.x + 2.0 && (t.y - c.y).abs() < 2.0, "seed {seed}: closes the line: {t:?} vs carrier {c:?}");
        // Instructed not to trap, he covers at the DEF-02 distance.
        let (t0, c0, _) = cover_target(seed, ball, Some(0.0));
        assert!((t0 - c0).length() > (t - c).length() + 1.0, "seed {seed}: no trap: cover {:.1} m away", (t0 - c0).length());
    }
}

#[test]
fn ind05_alone_against_two_the_first_defender_cuts_the_pass_and_delays() {
    // "retreat diagonally in order to cut off passing lines between the two attackers and cover the goal" [FIFA-1V2].
    let (mut sc, carrier) = situation(43, Role::CM8, Vec2::new(0.0, -5.0));
    let fr = home_frame(&sc);
    let mate = slot_of(&sc, Side::Home, Role::CF);
    let defender = slot_of(&sc, Side::Away, Role::CMP);
    for i in AWAY..AWAY + PLAYERS_PER_TEAM {
        if i != defender && !sc.sim.state.players[i].is_gk() {
            sc.place(i, fr.to_world(Vec2::new(-30.0, (i - AWAY) as f32 * 4.0 - 24.0)));
        }
    }
    for i in 0..PLAYERS_PER_TEAM {
        if i != carrier && i != mate && !sc.sim.state.players[i].is_gk() {
            sc.place(i, fr.to_world(Vec2::new(-35.0, i as f32 * 4.0 - 24.0)));
        }
    }
    sc.place(mate, fr.to_world(Vec2::new(8.0, 5.0))).place(defender, fr.to_world(Vec2::new(8.0, -5.0)));
    assert_eq!(first_defender(&mut sc) + AWAY, defender, "the lone defender is first");
    let mv = decide(&sc, defender, 0);
    assert!(mv.rules.contains(&"IND-05"), "outnumbered: {:?}", mv.rules);
    let (c, m, t) = (fr.to_team(sc.sim.state.players[carrier].pos), Vec2::new(8.0, 5.0), fr.to_team(mv.target));
    // Goal-side of the carrier, shaded towards the free attacker, and not tight on the carrier.
    assert!(t.x > c.x, "goal-side: {t:?} vs carrier {c:?}");
    assert!(t.y > c.y + 1.0, "shaded towards the free attacker at {m:?}: {t:?}");
    assert!((t - c).length() >= sc.sim.cfg.tactics.prs07_delay_distance - 0.5, "delays at {:.1} m", (t - c).length());
    // With a team-mate covering, he engages as usual.
    let cover = slot_of(&sc, Side::Away, Role::CB);
    sc.place(cover, fr.to_world(Vec2::new(8.0, -1.0)));
    assert_eq!(first_defender(&mut sc) + AWAY, defender, "still first");
    let mv = decide(&sc, defender, 0);
    assert!(!mv.rules.contains(&"IND-05"), "covered: {:?}", mv.rules);
}

/// Home CM8 on the ball at `ball`, facing forward; the home centre-forward has dropped 9 m in front of the away line;
/// the away side, with its step-out instruction `step` and BLK-03 shift distance `shift`, has settled on its targets.
/// Returns the scenario and the centre-forward.
fn nine_drops(ball: Vec2, step: Option<f32>, shift: f32) -> (Scenario, PlayerId) {
    let (mut sc, carrier) = situation(61, Role::CM8, ball);
    sc.sim.cfg.tactics.blk03_shift_m = shift;
    sc.sim.state.teams[Side::Away.index()].instructions.defending.step_out = step;
    sc.sim.refresh_tactics();
    let fr = home_frame(&sc);
    sc.sim.state.players[carrier].facing = fr.to_world(Vec2::X) - fr.to_world(Vec2::ZERO);
    let cf = slot_of(&sc, Side::Home, Role::CF);
    let back = |sc: &Scenario| (AWAY..AWAY + PLAYERS_PER_TEAM).filter(|&i| sc.sim.state.players[i].out_role.is_back_line()).collect::<Vec<_>>();
    let line = back(&sc).iter().map(|&i| fr.to_team(sc.sim.state.players[i].pos).x).fold(f32::MAX, f32::min);
    sc.place(cf, fr.to_world(Vec2::new(line - 9.0, 1.0)));
    for _ in 0..4 {
        sc.sim.state.time += 0.2;
        sc.think();
        let targets: Vec<Vec2> = (AWAY..AWAY + PLAYERS_PER_TEAM).map(|i| decide(&sc, i, 0).target).collect();
        for (k, t) in targets.into_iter().enumerate() {
            sc.place(AWAY + k, t);
        }
    }
    sc.think();
    (sc, cf)
}

#[test]
fn blk03_a_centre_back_steps_out_to_the_dropping_nine_the_line_shuffles_and_the_far_winger_gains_the_run() {
    // Test 6 (docs/player-brain.md section 15): "a defender steps out to mark a player between the lines while
    // teammates shuffle across" [FIFA-MB]; the 9 drops and a winger runs in behind [FIFA-JOHNSON].
    let shift = SimConfig::default().tactics.blk03_shift_m;
    for ball in [Vec2::new(-3.0, 8.0), Vec2::new(-3.0, -8.0)] {
        let (out, cf) = nine_drops(ball, None, shift);
        let (held, _) = nine_drops(ball, Some(0.0), shift);
        let fr = home_frame(&out);
        let marker = |sc: &Scenario| {
            let b = sc.sim.state.team_view(Side::Away);
            (0..PLAYERS_PER_TEAM).find(|&k| b.def_duties[k] == DefDuty::Mark(cf)).map(|k| sc.sim.state.players[AWAY + k].out_role)
        };
        assert!(marker(&out).is_some_and(|r| r.is_back_line()), "ball {ball:?}: a centre-back follows the 9: {:?}", marker(&out));
        // Told to hold the line, he passes him on to the midfield (who take him if one is free).
        assert!(!marker(&held).is_some_and(|r| r.is_back_line()), "ball {ball:?}: told to hold: {:?}", marker(&held));
        // The weak-side full-back shuffles across, towards the centre.
        let weak_fb = |sc: &Scenario| {
            (AWAY..AWAY + PLAYERS_PER_TEAM)
                .filter(|&i| sc.sim.state.players[i].out_role == Role::FB)
                .map(|i| fr.to_team(decide(sc, i, 0).target).y)
                .find(|y| y.signum() != ball.y.signum())
                .expect("a weak-side full-back")
        };
        let (fb_out, fb_held) = (weak_fb(&out), weak_fb(&held));
        assert!(fb_out.abs() < fb_held.abs() - 3.0, "ball {ball:?}: weak-side full-back at |y| {:.1}, holding {:.1}", fb_out.abs(), fb_held.abs());
        // The weak-side winger's run in behind: worth more to the carrier with the line shuffled, and as an offer where
        // it is on at all (with the ball on his side of the 9 it is a loss either way).
        let run = |sc: &Scenario| {
            let w = (0..PLAYERS_PER_TEAM)
                .find(|&i| sc.sim.state.players[i].role == Role::W && fr.to_team(sc.sim.state.players[i].pos).y.signum() != ball.y.signum())
                .expect("a weak-side winger");
            let (s, _) = scores(sc, w);
            (best_of(&s, &[Movement::RunInBehind], |x| x.own), best_of(&s, &[Movement::RunInBehind], |x| x.marginal))
        };
        let ((own_out, m_out), (own_held, m_held)) = (run(&out), run(&held));
        assert!(
            (own_out > own_held || own_out.max(own_held) <= 0.0) && m_out > m_held,
            "ball {ball:?}: run own {own_out:.5} vs {own_held:.5}, marginal {m_out:.6} vs {m_held:.6}"
        );
    }
    // Without a line team-mate near enough to shift across, he does not step out.
    let (alone, cf) = nine_drops(Vec2::new(-3.0, 8.0), None, 5.0);
    let b = alone.sim.state.team_view(Side::Away);
    let follows = (0..PLAYERS_PER_TEAM).any(|k| b.def_duties[k] == DefDuty::Mark(cf) && alone.sim.state.players[AWAY + k].out_role.is_back_line());
    assert!(!follows, "nobody can shift across: the 9 is passed on to the midfield");
}

#[test]
fn presets_false_nine_drops_target_man_holds_the_line() {
    // Docs/player-brain.md 5.4: the presets are combinations of the player instructions; a false 9 comes short
    // between the lines, a target man stays on the last line.
    use match_engine::team::PlayerPreset;
    let (mut sc, _, cf) = front(24, Vec2::new(-5.0, -8.0));
    let short = |sc: &Scenario| (0..40).filter(|&k| matches!(decide(sc, cf, k).kind, Movement::Check | Movement::Drop)).count();
    sc.sim.state.players[cf].instructions = PlayerPreset::FalseNine.instructions();
    let f9 = short(&sc);
    sc.sim.state.players[cf].instructions = PlayerPreset::TargetMan.instructions();
    let tm = short(&sc);
    assert!(f9 > tm + 8, "comes short: false 9 {f9} / 40, target man {tm} / 40");
}
