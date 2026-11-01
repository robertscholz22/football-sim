//! Shared scenario helpers for the integration tests.
// Each test crate compiles this module and uses only some of the helpers.
#![allow(dead_code)]

use match_engine::Vec2;
use match_engine::ai::shape::anchor;
use match_engine::match_state::PLAYERS_PER_TEAM;
use match_engine::pitch::{Side, TeamFrame};
use match_engine::player::PlayerId;
use match_engine::scenario::Scenario;
use match_engine::team::Role;

pub const AWAY: usize = PLAYERS_PER_TEAM;

pub fn slot_of(sc: &Scenario, side: Side, role: Role) -> PlayerId {
    let slot = sc.sim.state.team(side).formation.slots.iter().position(|s| s.role == role).expect("role in formation");
    side.index() * PLAYERS_PER_TEAM + slot
}

pub fn home_frame(sc: &Scenario) -> TeamFrame {
    sc.sim.state.frame(Side::Home)
}

/// Home player with `role` has the ball at home-frame `ball`; every other outfield player stands on his layer-1 anchor,
/// each keeper where his positioning (GK-01 / BU-01) puts him.
/// The clock is moved past any transition window, so both teams are in their settled phases.
pub fn situation(seed: u64, carrier_role: Role, ball: Vec2) -> (Scenario, PlayerId) {
    situation_for(seed, Side::Home, carrier_role, ball)
}

/// As `situation`, for a carrier of `side` with the ball at `ball` in that side's frame.
pub fn situation_for(seed: u64, side: Side, carrier_role: Role, ball: Vec2) -> (Scenario, PlayerId) {
    settled(Scenario::new(seed), side, carrier_role, ball)
}

/// The bundled squads in formations `home` and `away` (`team::FORMATIONS`), live play.
pub fn with_formations(seed: u64, home: &str, away: &str) -> Scenario {
    with_formations_config(seed, home, away, match_engine::config::SimConfig::default())
}

/// As `with_formations`, with configuration `cfg`.
pub fn with_formations_config(seed: u64, home: &str, away: &str, cfg: match_engine::config::SimConfig) -> Scenario {
    use match_engine::match_state::PlayPhase;
    use match_engine::pitch::PitchConditions;
    use match_engine::team::TeamData;
    let mut h = TeamData::builtin("northbridge").unwrap();
    let mut a = TeamData::builtin("harbour").unwrap();
    (h.formation, a.formation) = (home.to_string(), away.to_string());
    let mut sim = match_engine::Simulation::new(&h, &a, cfg, PitchConditions::default(), seed);
    sim.state.phase = PlayPhase::Live;
    Scenario { sim }
}

/// As `situation_for`, in scenario `sc`.
pub fn settled(mut sc: Scenario, side: Side, carrier_role: Role, ball: Vec2) -> (Scenario, PlayerId) {
    let carrier = slot_of(&sc, side, carrier_role);
    let w = sc.sim.state.frame(side).to_world(ball);
    sc.place(carrier, w).give_ball(carrier).think();
    sc.sim.state.time += 20.0;
    for _ in 0..3 {
        sc.think();
        let st = &sc.sim.state;
        let targets: Vec<Vec2> = (0..2 * PLAYERS_PER_TEAM)
            .map(|i| {
                if st.players[i].is_gk() {
                    match_engine::ai::positioning::decide(st, &sc.sim.cfg, i, &mut match_engine::rng::SimRng::new(0)).target
                } else {
                    st.frame(st.players[i].side).to_world(anchor(st, &sc.sim.cfg, i).0)
                }
            })
            .collect();
        for (i, t) in targets.into_iter().enumerate() {
            if i != carrier {
                sc.place(i, t);
            }
        }
    }
    sc.think();
    (sc, carrier)
}

/// The home centre-back plays an error-free lofted pass 35 m forward to the home striker standing on its target; the
/// away centre-back stands `gap` metres goal-side of the target, everyone else far away. `set` adjusts the scenario
/// before the kick. Returns (scenario, receiver, defender); the brains have reacted to the release.
pub fn high_ball(seed: u64, gap: f32, set: impl Fn(&mut Scenario)) -> (Scenario, PlayerId, PlayerId) {
    use match_engine::ball::KickSpec;
    use match_engine::player::{KickKind, KickPlan};
    let mut sc = Scenario::new(seed);
    let ex = &mut sc.sim.cfg.execution;
    (ex.angle_lofted, ex.elevation, ex.speed_rel, ex.spin_rel, ex.shank_probability) = (0.0, 0.0, 0.0, 0.0, 0.0);
    // The receiver reads the pass at once (section 7), so he goes for it as it is struck.
    sc.sim.cfg.ai.reading_receive_delay_max_s = 0.0;
    let fr = home_frame(&sc);
    let (passer, receiver) = (slot_of(&sc, Side::Home, Role::CB), slot_of(&sc, Side::Home, Role::CF));
    let (defender, keeper) = (slot_of(&sc, Side::Away, Role::CB), slot_of(&sc, Side::Away, Role::GK));
    let (from, target) = (Vec2::new(-20.0, 0.0), Vec2::new(15.0, 0.0));
    for id in 0..2 * PLAYERS_PER_TEAM {
        let side_y = if id < PLAYERS_PER_TEAM { -32.0 } else { 32.0 };
        sc.place(id, fr.to_world(Vec2::new(-40.0 + 4.0 * (id % PLAYERS_PER_TEAM) as f32, side_y)));
    }
    sc.place(passer, fr.to_world(from)).place(receiver, fr.to_world(target));
    sc.place(defender, fr.to_world(target + Vec2::new(gap, 0.0))).place(keeper, fr.to_world(Vec2::new(50.0, 0.0)));
    sc.sim.state.players[passer].facing = fr.to_world(Vec2::X) - fr.to_world(Vec2::ZERO);
    sc.give_ball(passer);
    sc.sim.state.time += 20.0;
    set(&mut sc);
    sc.think();
    let now = sc.sim.state.time;
    let target = fr.to_world(target);
    let plan = KickPlan {
        spec: KickSpec::Lofted { target, launch_angle: 0.5, backspin: 6.0, sidespin: 0.0 },
        kind: KickKind::LoftedPass,
        receiver: Some(receiver),
        target,
        arrival_t: 2.2,
        xg: 0.0,
        speed: 20.0,
        foot: None,
    };
    sc.sim.execute_kick(passer, plan, false, now);
    (sc, receiver, defender)
}
