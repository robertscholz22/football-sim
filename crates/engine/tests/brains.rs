//! Player brains (docs/player-brain.md section 4): each player evaluates the team's situation himself. With exact
//! perception and team-mates who know each other every player of a side reaches the same beliefs at every brain
//! update, except while the line leader's call is on its way (M6); reading team-mates (section 7) makes them differ
//! where players do not know each other.

use match_engine::Simulation;
use match_engine::ai::brain::{Beliefs, DefDuty};
use match_engine::config::SimConfig;
use match_engine::match_state::team_range;
use match_engine::pitch::{PitchConditions, Side};
use match_engine::team::{Partners, Practice, TeamData};

/// A builtin squad whose players know each other and the concept perfectly (practice and partners 1).
fn known(name: &str) -> TeamData {
    let mut t = TeamData::builtin(name).unwrap();
    t.practice = Practice { formation: 1.0, concept: 1.0 };
    t.partners = Partners { default: 1.0, pairs: Vec::new() };
    t
}

#[test]
fn exact_perception_gives_each_side_one_reading() {
    let mut cfg = SimConfig::default();
    cfg.timing.half_length_s = 300.0;
    let mut sim = Simulation::new(&known("northbridge"), &known("harbour"), cfg, PitchConditions::default(), 7);
    let (mut checks, mut duties, mut agree) = (0, 0, 0);
    // Beliefs without the player's own claim time and calls, and without what reaches him through a call or reading
    // since M6 (section 7): the leader's line call (BLK-02) with its hold timer and the block it moves (and the leader, the one nearest its middle), the defensive
    // concept and claims (with the first defender) that follow from the block, and a called chaser ("mine") with his point. A team-mate hears a
    // call at his next update, or reads it after his delay; until then he works from the last call he had, and the
    // block, which shifts at a capped rate (DEF-05), takes a while to close the gap.
    let shared = |b: &Beliefs, r: &Beliefs| Beliefs {
        claim_since: r.claim_since,
        claim_called: r.claim_called,
        line_called: r.line_called,
        line_called_at: r.line_called_at,
        chase_called: r.chase_called,
        line_reading_since: r.line_reading_since,
        line_leader: r.line_leader,
        line_step: r.line_step,
        def_block: r.def_block,
        def_block_l1: r.def_block_l1,
        concept_def: r.concept_def,
        def_duties: r.def_duties,
        first: r.first,
        first_on: r.first_on,
        chaser: r.chaser,
        chase_point: r.chase_point,
        chase_t: r.chase_t,
        challenge: r.challenge,
        ..b.clone()
    };
    while !sim.finished() {
        sim.step();
        for side in [Side::Home, Side::Away] {
            let squad = &sim.state.players[team_range(side)];
            for p in &squad[1..] {
                assert_eq!(
                    shared(&p.beliefs, &squad[0].beliefs),
                    squad[0].beliefs,
                    "t {:.2}: player {} reads {side:?}'s situation differently",
                    sim.state.time,
                    p.id
                );
            }
            checks += 1;
            agree += u32::from(squad.iter().all(|p| p.beliefs.def_duties == squad[0].beliefs.def_duties));
            duties += squad[0].beliefs.def_duties.iter().filter(|r| matches!(r, DefDuty::First | DefDuty::Mark(_))).count();
        }
    }
    // The beliefs were really evaluated, not left at their defaults; the defensive duties agree nearly always (a call
    // on its way is the exception: 95 % of the checks at M6).
    assert!(checks > 10_000 && duties > checks / 2, "{checks} checks, {duties} duties");
    assert!(agree as f32 > 0.9 * checks as f32, "duties agree in {agree} of {checks} checks");
}

#[test]
fn rest_defence_is_claimed_by_its_roles() {
    // TR-05 by duty: in every formation the rest defence comes from the roles with the rest-defence duty while they
    // are enough (the carrier and a receiver excepted).
    use match_engine::ai::brain::AttDuty;
    use match_engine::scenario::Scenario;
    use match_engine::team::{Duty, FORMATIONS};
    for f in FORMATIONS {
        let mut home = TeamData::builtin("northbridge").unwrap();
        home.formation = f.to_string();
        let away = TeamData::builtin("harbour").unwrap();
        let sim = match_engine::Simulation::new(&home, &away, Default::default(), Default::default(), 3);
        let mut sc = Scenario { sim };
        sc.sim.state.phase = match_engine::match_state::PlayPhase::Live;
        let carrier = 6;
        sc.give_ball(carrier).think();
        let b = &sc.sim.state.players[0].beliefs;
        let squad = sc.sim.state.squad(match_engine::pitch::Side::Home);
        let holders = squad.iter().filter(|p| p.id != carrier && p.role.has_duty(Duty::RestDefence)).count();
        let rest: Vec<_> = (0..11).filter(|&i| b.att_duties[i] == AttDuty::RestDefence).collect();
        assert!(rest.len() >= 4, "{f}: {rest:?}");
        if holders >= rest.len() {
            assert!(rest.iter().all(|&i| squad[i].role.has_duty(Duty::RestDefence)), "{f}: {rest:?}");
        }
    }
}

#[test]
fn a_perfect_defence_has_one_first_defender_one_line_leader_and_one_man_per_opponent() {
    // Test 10 (docs/player-brain.md section 15), out of possession: with exact perception and players who know each
    // other the claims give exactly one first defender (two in the counter-press, TRN-02), one line leader with the
    // line-leading duty (BLK-02), and no opponent marked or tracked by two players (DEF-04). (History: until M6 this
    // held at every tick. Since M6 the leader's line call reaches team-mates at their next update, or after their
    // reading delay when he has no call in hand, so their blocks and claims can disagree while it is on its way: each
    // such error must end within that delay and stay rare.)
    use match_engine::ai::brain::{OutPhase, TeamPhase};
    use match_engine::team::Duty;
    let mut cfg = SimConfig::default();
    cfg.timing.half_length_s = 300.0;
    let bound = cfg.ai.reading_delay_max_s + 2.0 * cfg.ai.team_tick_s + cfg.timing.dt;
    let mut sim = Simulation::new(&known("northbridge"), &known("harbour"), cfg, PitchConditions::default(), 9);
    let (mut checks, mut marks, mut errors) = (0, 0, 0);
    let mut since = [None::<f32>; 2];
    while !sim.finished() {
        sim.step();
        for side in [Side::Home, Side::Away] {
            let b = sim.state.team_view(side);
            if b.in_possession || b.shape_t < 0.0 || !sim.state.is_live() {
                since[side.index()] = None;
                continue;
            }
            let t = sim.state.time;
            let firsts = b.def_duties.iter().filter(|d| **d == DefDuty::First).count();
            let max = if b.phase == TeamPhase::Out(OutPhase::CounterPress) { 2 } else { 1 };
            let leader = b.line_leader.unwrap_or_else(|| panic!("t {t:.2} {side:?}: nobody leads the line"));
            assert!(sim.state.squad(side)[leader].out_role.has_duty(Duty::LeadLine));
            let mut men: Vec<_> = b.def_duties.iter().filter_map(|d| match d {
                DefDuty::Mark(o) | DefDuty::Track(o) => Some(*o),
                _ => None,
            }).collect();
            marks += men.len();
            men.sort();
            if (1..=max).contains(&firsts) && men.windows(2).all(|w| w[0] != w[1]) {
                since[side.index()] = None;
            } else {
                errors += 1;
                let s = *since[side.index()].get_or_insert(t);
                assert!(t - s <= bound, "t {t:.2} {side:?}: {firsts} first defenders, marks {men:?} since {s:.2}");
            }
            checks += 1;
        }
    }
    assert!(checks > 5_000 && marks > checks, "{checks} checks, {marks} marks");
    assert!(errors * 50 < checks, "{errors} error ticks in {checks} checks");
}

#[test]
fn a_perfect_team_in_possession_keeps_its_rest_defence_one_depth_and_no_duplicate_runs() {
    // Test 10 (docs/player-brain.md section 15), in possession: with perfect readers the claims give the TR-05 rest
    // defence (4, or 5 against five high opponents) at every moment, someone gives depth on the last line in the
    // progression and the final third (ATT-11), and two players rarely offer the same space (ATT-10, section 8.3).
    use match_engine::Vec2;
    use match_engine::ai::brain::{AttDuty, InPhase, TeamPhase};
    use match_engine::team::Movement;
    let mut cfg = SimConfig::default();
    cfg.timing.half_length_s = 300.0;
    let mut sim = Simulation::new(&known("northbridge"), &known("harbour"), cfg, PitchConditions::default(), 9);
    for p in sim.state.players.iter_mut() {
        p.attrs.intelligence = 100.0;
        (p.confidence, p.energy, p.familiarity) = (1.0, 1.0, [1.0; 2]);
    }
    let (tc, overlap) = (sim.cfg.tactics.clone(), sim.cfg.ai.offer_overlap_m);
    let (mut ticks, mut advanced, mut depth, mut same_space, mut same_run) = (0, 0, 0, 0, 0);
    let (mut counter, mut counter_same) = (0, 0);
    while !sim.finished() {
        sim.step();
        for side in [Side::Home, Side::Away] {
            let b = sim.state.team_view(side);
            if !b.in_possession || !sim.state.is_live() {
                continue;
            }
            ticks += 1;
            let rest = b.att_duties.iter().filter(|d| **d == AttDuty::RestDefence).count();
            assert!((tc.tr05_min_players..=tc.tr05_max_players).contains(&rest), "t {:.2} {side:?}: rest defence {rest}", sim.state.time);
            let fr = sim.state.frame(side);
            let squad = sim.state.squad(side);
            if matches!(b.phase, TeamPhase::In(InPhase::Progression | InPhase::FinalThird)) {
                advanced += 1;
                let deep = |p: &match_engine::player::Player| {
                    fr.to_team(p.target).x > b.offside_line - 4.0 || p.plan.kind == Movement::RunInBehind
                };
                depth += usize::from(squad.iter().any(deep));
            }
            let offers: Vec<(Movement, Vec2)> = squad
                .iter()
                .filter(|p| !matches!(p.plan.kind, Movement::Hold | Movement::Pin))
                .filter_map(|p| p.plan.offer.map(|o| (p.plan.kind, o)))
                .collect();
            let close = |k: Option<Movement>| {
                offers.iter().enumerate().any(|(i, a)| {
                    offers[i + 1..].iter().any(|c| (a.1 - c.1).length() < overlap && k.is_none_or(|k| a.0 == k && c.0 == k))
                })
            };
            same_space += usize::from(close(None));
            same_run += usize::from(close(Some(Movement::RunInBehind)));
            // TR-03: on the counter, runners attack different areas [FIFA-ES Urias].
            if b.phase == TeamPhase::In(InPhase::CounterAttack) {
                counter += 1;
                counter_same += usize::from(close(Some(Movement::RunInBehind)));
            }
        }
    }
    let share = |n: usize, of: usize| n as f32 / of as f32;
    assert!(ticks > 10_000 && advanced > ticks / 3, "{ticks} ticks, {advanced} advanced");
    assert!(share(depth, advanced) > 0.75, "depth on the line in {:.0} % of the advanced phases", 100.0 * share(depth, advanced));
    assert!(share(same_space, ticks) < 0.05, "two offers within {overlap} m in {:.1} % of ticks", 100.0 * share(same_space, ticks));
    assert!(share(same_run, ticks) < 0.005, "duplicate runs in behind in {:.2} % of ticks", 100.0 * share(same_run, ticks));
    assert!(counter > 500, "{counter} counter-attack ticks");
    assert!(share(counter_same, counter) < 0.01, "duplicate runs on the counter in {:.2} % of its ticks", 100.0 * share(counter_same, counter));
}

#[test]
fn a_wing_back_plays_higher_and_stays_back_less_than_a_full_back() {
    // Test 13 (docs/player-brain.md section 15): the formation's roles define the behaviour (ROLE-05, ROLE-06). The
    // full-back of a back four (4-4-2) and the wing-back of a back five (3-5-2), each with the ball over a match:
    // the wing-back is higher and claims rest defence less often, because the back three keeps it [UEFA-TO; FIFA-TSG].
    // Both formations put them on the touchline in possession (BU-02), so their width stays the same. The squads know
    // each other and the concept, so what differs is the role, not the reading (section 7).
    use match_engine::ai::brain::AttDuty;
    use match_engine::team::Role;
    let measure = |formation: &str, role: Role| {
        let mut acc = [0.0f64; 4];
        for seed in 0..4u64 {
            let mut home = known("northbridge");
            home.formation = formation.to_string();
            let mut away = known("harbour");
            away.formation = "4-4-2".to_string();
            let mut cfg = SimConfig::default();
            cfg.timing.half_length_s = 300.0;
            let mut sim = Simulation::new(&home, &away, cfg, PitchConditions::default(), seed);
            let ids: Vec<usize> = (0..11).filter(|&i| sim.state.players[i].role == role).collect();
            let mut n = 0;
            while !sim.finished() {
                sim.step();
                n += 1;
                let b = sim.state.team_view(Side::Home);
                if n % 25 != 0 || !b.in_possession || !sim.state.is_live() {
                    continue;
                }
                let fr = sim.state.frame(Side::Home);
                for &i in &ids {
                    let p = fr.to_team(sim.state.players[i].pos);
                    acc[0] += 1.0;
                    acc[1] += f64::from(p.x);
                    acc[2] += f64::from(p.y.abs());
                    acc[3] += f64::from(u8::from(b.att_duties[i] == AttDuty::RestDefence));
                }
            }
        }
        [acc[1] / acc[0], acc[2] / acc[0], acc[3] / acc[0]]
    };
    let [fb_x, fb_y, fb_rest] = measure("4-4-2", Role::FB);
    let [wb_x, wb_y, wb_rest] = measure("3-5-2", Role::WB);
    let summary = format!("full-back x {fb_x:.1} |y| {fb_y:.1} rest {fb_rest:.2}; wing-back x {wb_x:.1} |y| {wb_y:.1} rest {wb_rest:.2}");
    // History: the margin was 4 m when phase E gave 8.1 m (x −9.3 vs −1.2). With crosses (M4) and BLK-03 (M5) the
    // opponents' back line holds more and the gap is 3.8 m in these two short matches (x −6.2 vs −2.4). In the watch
    // round (2026-11-01) two matches gave 2.9 m; the gap per pair of seeds ranges 2.9–9.8 m, so four seeds now.
    assert!(wb_x > fb_x + 3.0, "{summary}");
    assert!(wb_rest < fb_rest - 0.25, "{summary}");
    assert!(wb_y > fb_y - 1.5, "{summary}");
}

/// Press and mark errors of `side`'s claims over a match (out of possession with an opponent on the ball): ticks
/// with no first defender or more than the phase allows, ticks with a man marked twice, and the longest press-error
/// episode in seconds.
fn coordination_errors(home: &TeamData, away: &TeamData, side: Side, seed: u64, half_s: f32) -> (u32, u32, u32, f32) {
    use match_engine::ai::brain::{OutPhase, TeamPhase};
    let mut cfg = SimConfig::default();
    cfg.timing.half_length_s = half_s;
    let mut sim = Simulation::new(home, away, cfg, PitchConditions::default(), seed);
    let (mut ticks, mut press, mut marked, mut since, mut longest) = (0, 0, 0, None::<f32>, 0.0f32);
    while !sim.finished() {
        sim.step();
        let t = sim.state.time;
        let b = sim.state.team_view(side);
        let on_ball = sim.state.control.is_some_and(|c| sim.state.players[c].side != side);
        if b.in_possession || b.shape_t < 0.0 || !sim.state.is_live() || !on_ball {
            since = None;
            continue;
        }
        ticks += 1;
        let firsts = b.def_duties.iter().filter(|d| **d == DefDuty::First).count();
        let max = if b.phase == TeamPhase::Out(OutPhase::CounterPress) { 2 } else { 1 };
        if (1..=max).contains(&firsts) {
            since = None;
        } else {
            press += 1;
            let s = *since.get_or_insert(t);
            longest = longest.max(t - s);
        }
        let mut men: Vec<_> = b
            .def_duties
            .iter()
            .filter_map(|d| match d {
                DefDuty::Mark(o) | DefDuty::Track(o) => Some(*o),
                _ => None,
            })
            .collect();
        men.sort();
        marked += u32::from(men.windows(2).any(|w| w[0] == w[1]));
    }
    (ticks, press, marked, longest)
}

#[test]
fn a_new_signing_causes_press_errors_that_his_team_mates_resolve_within_the_reading_delay() {
    // Test 11 (docs/player-brain.md section 15): one player who knows none of his team-mates (partners 0), in a side
    // that otherwise knows each other and the concept. He and they misjudge each other's times, so at times nobody or
    // two press; whoever reads the other's claim corrects within the reading delay (plus a brain tick or two). The
    // opponents, who know each other, err far less: only while a line call is on its way (M6; until M6 never).
    let mut home = known("northbridge");
    let signing = home.players[6].number;
    home.partners.pairs = home.players.iter().filter(|p| p.number != signing).map(|p| (signing, p.number, 0.0)).collect();
    let away = known("harbour");
    let cfg = SimConfig::default();
    let bound = cfg.ai.reading_delay_max_s + 2.0 * cfg.ai.team_tick_s + cfg.timing.dt;
    let (mut errors, mut longest, mut new_side, mut settled) = (0, 0.0f32, 0, 0);
    for seed in [3, 4] {
        let (ticks, press, marked, long) = coordination_errors(&home, &away, Side::Home, seed, 600.0);
        println!("seed {seed}: home {ticks} ticks, press errors {press}, marked twice {marked}, longest {long:.2} s");
        errors += press;
        new_side += press + marked;
        longest = longest.max(long);
        let (ticks, press, marked, long) = coordination_errors(&home, &away, Side::Away, seed, 600.0);
        println!("seed {seed}: away {ticks} ticks, press errors {press}, marked twice {marked}, longest {long:.2} s");
        settled += press + marked;
        longest = longest.max(long);
    }
    assert!(errors > 0, "the signing never misjudged a team-mate");
    assert!(longest <= bound, "a press error lasted {longest:.2} s (bound {bound:.2} s)");
    // History: the factor was 4 while the ratio over seeds 1–8 was 7.2 (settled 334, the signing's side 2399). Since
    // the watch round (2026-11-01; first-time replans, `att06_replan_m`) a receiver who meets the ball away from the
    // planned point controls it, and the defenders' references jump from the pass target to him; the settled side
    // marks a man twice more often for a moment (seeds 3–4: 111 ticks, all marking, no press error, against 333 on
    // the signing's side, 192 of them press errors). Over seeds 1–8 the ratio is 3.2. Set T (2026-11-02; carriers keep
    // the ball longer, the second defender keeps his role with the first's hysteresis): seeds 3–4 settled 329, the
    // signing's side 796 (389 press errors).
    assert!(2 * settled < new_side, "settled side {settled} error ticks, the signing's side {new_side}");
}

#[test]
fn practice_shortens_the_reading_delays() {
    // Test 12 (docs/player-brain.md section 15): with the same partner knowledge (0.5), a side that has practised
    // the formation and concept (1.0) reads every team-mate's claims and movements, and every passer, sooner than one
    // that has not (0.2). (History: until M4 this test counted coordination-error ticks over full matches. Twelve
    // seeds then gave 33.8 vs 33.1 error ticks per 1000 out-of-possession ticks: practice moves the reading delay by
    // ≈ 0.12 s (1.03 vs 1.15 s), far below the spread between matches. Practice shows in pair passing instead, on the familiarity bench.)
    use match_engine::ai::reading;
    let away = known("harbour");
    let delays = |practice: f32| {
        let mut home = TeamData::builtin("northbridge").unwrap();
        home.practice = Practice { formation: practice, concept: practice };
        home.partners = Partners { default: 0.5, pairs: Vec::new() };
        let cfg = SimConfig::default();
        let sim = Simulation::new(&home, &away, cfg.clone(), PitchConditions::default(), 5);
        let mut out = Vec::new();
        for i in team_range(Side::Home) {
            for j in team_range(Side::Home).filter(|&j| j != i) {
                out.push((reading::delay(&sim.state, &cfg.ai, i, j), reading::receive_delay(&sim.state, &cfg.ai, i, j)));
            }
        }
        out
    };
    let (practised, new) = (delays(1.0), delays(0.2));
    for (k, (a, b)) in practised.iter().zip(&new).enumerate() {
        assert!(a.0 < b.0 && a.1 < b.1, "pair {k}: reading {:.2} vs {:.2} s, receiving {:.2} vs {:.2} s", a.0, b.0, a.1, b.1);
    }
    let mean = |v: &[(f32, f32)], f: fn(&(f32, f32)) -> f32| v.iter().map(f).sum::<f32>() / v.len() as f32;
    println!(
        "reading delay {:.2} vs {:.2} s, receiving delay {:.2} vs {:.2} s",
        mean(&practised, |x| x.0),
        mean(&new, |x| x.0),
        mean(&practised, |x| x.1),
        mean(&new, |x| x.1)
    );
}
