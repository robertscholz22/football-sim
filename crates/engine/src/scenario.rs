//! Builders for tests and experiments: single players and fixed match situations.

use glam::Vec2;

use crate::ball::Ball;
use crate::config::SimConfig;
use crate::match_state::{PLAYERS_PER_TEAM, PlayPhase};
use crate::pitch::{PitchConditions, Side};
use crate::player::{Attributes, Intent, Kinematics, Player, PlayerId};
use crate::sim::Simulation;
use crate::team::{PlayerInstructions, Role, TeamData};

/// A stand-alone player with the given attributes, at rest at the origin.
pub fn make_player(id: PlayerId, side: Side, attrs: Attributes, cfg: &SimConfig) -> Player {
    let kin = Kinematics::new(&attrs, &cfg.player, &Default::default(), 1.0);
    Player::new(id, side, 0, [Role::CMP; 2], format!("P{id}"), id as u8 + 1, attrs, kin, PlayerInstructions::default(), 0.5, [0.8; 2])
}

/// A live match situation built from the bundled squads: every player placed at a team-frame position, the ball at
/// `ball` (world) and possession / control set. Brains are updated once, so roles and blocks are ready.
pub struct Scenario {
    pub sim: Simulation,
}

impl Scenario {
    /// Bundled squads, live play, both teams in their kick-off positions.
    pub fn new(seed: u64) -> Self {
        Self::with_config(seed, SimConfig::default())
    }

    /// As `new`, with configuration `cfg` (the players start where it puts them).
    pub fn with_config(seed: u64, cfg: SimConfig) -> Self {
        let home = TeamData::builtin("northbridge").expect("bundled squad");
        let away = TeamData::builtin("harbour").expect("bundled squad");
        let mut sim = Simulation::new(&home, &away, cfg, PitchConditions::default(), seed);
        sim.state.phase = PlayPhase::Live;
        Self { sim }
    }

    /// Place player `id` at world position `pos` (standing).
    pub fn place(&mut self, id: PlayerId, pos: Vec2) -> &mut Self {
        let p = &mut self.sim.state.players[id];
        p.pos = pos;
        p.vel = Vec2::ZERO;
        p.target = pos;
        self
    }

    /// Place every player of `side` at team-frame positions (slot order).
    pub fn place_team(&mut self, side: Side, team_pos: &[Vec2; PLAYERS_PER_TEAM]) -> &mut Self {
        let fr = self.sim.state.frame(side);
        let base = side.index() * PLAYERS_PER_TEAM;
        for (i, &q) in team_pos.iter().enumerate() {
            self.place(base + i, fr.to_world(q));
        }
        self
    }

    /// Ball at the feet of `id`, who is in control.
    pub fn give_ball(&mut self, id: PlayerId) -> &mut Self {
        let st = &mut self.sim.state;
        let p = &mut st.players[id];
        p.intent = Intent::Hold;
        st.ball = Ball::at_rest(p.pos + p.facing * 0.4);
        st.control = Some(id);
        st.last_touch = Some(id);
        st.flight = None;
        let side = p.side;
        if st.possession != side {
            st.possession = side;
            st.possession_since = st.time;
        }
        self
    }

    /// Loose ball at rest at `pos` (world).
    pub fn loose_ball(&mut self, pos: Vec2) -> &mut Self {
        let st = &mut self.sim.state;
        st.ball = Ball::at_rest(pos);
        st.control = None;
        st.flight = None;
        self
    }

    /// Update every player's brain for the current situation (twice, so transitions settle).
    pub fn think(&mut self) -> &mut Self {
        let now = self.sim.state.time;
        self.sim.update_brains(now);
        self.sim.update_brains(now);
        self
    }

    /// Run the simulation for `seconds`.
    pub fn run(&mut self, seconds: f32) -> &mut Self {
        let n = (seconds / self.sim.cfg.timing.dt).round() as usize;
        for _ in 0..n {
            self.sim.step();
        }
        self
    }
}
