//! The match loop. One `step()` advances the match by `TimingConfig::dt`:
//! phase handling → player brains → off-ball decisions → ball carrier → goalkeeper dives → locomotion →
//! ball physics (sub-stepped) → contacts (saves, tackles, receptions, interceptions, headers) → referee → stats.

use glam::{Vec2, Vec3};

use crate::ai::analysis::pressure_at;
use crate::ai::deviation::{self, decision_interval, drift_sigma, update_drift};
use crate::ai::execution::{ExecInput, natural_foot, perturb, set_up, setup_time, setup_turn};
use crate::ai::on_ball::{self, Action, Situation, receive_dir, receiver_pressured};
use crate::ai::{movement, positioning, reading, scan};
use crate::ai::reach::{intercept, pass_reaction, reception_point};
use crate::ai::brain::{self, Beliefs, DefDuty};
use crate::ai::world::World;
use crate::ball::{
    BALL_RADIUS, Ball, BallEvent, BallMode, BallParams, KickSpec, SurfaceField, Trajectory, physics::SurfaceInput, predict,
    check_lines, solve_kick, step_ball,
};
use crate::config::SimConfig;
use crate::duel::{TackleOutcome, aerial_win_prob, catch_prob, control_prob, resolve_tackle, pass_touch_prob, save_prob, tackle_win_prob};
use crate::events::{Card, EventKind, MatchEvent};
use crate::match_state::{BallFlight, MatchState, PLAYERS_PER_TEAM, PlayPhase, RestartKind, Team, side_of};
use crate::pitch::{
    GOAL_HALF_WIDTH, HALF_LENGTH, HALF_WIDTH, PENALTY_AREA_DEPTH, PENALTY_AREA_HALF_WIDTH, PitchConditions, Side, TeamFrame, clamp_to_pitch,
    is_cross,
};
use crate::player::{Body, Intent, KickKind, KickPlan, Kinematics, Player, PlayerId, confidence, fatigue, locomotion, norm};
use crate::rng::SimRng;
use crate::rules::referee::{self, Misconduct};
use crate::stats::Stats;
use crate::team::{Formation, TeamData, applied_defending, resolve};

mod laws;

/// Prediction horizon (s) of the cached ball trajectory.
const TRAJ_HORIZON: f32 = 6.0;
const TRAJ_SAMPLE_DT: f32 = 0.05;
/// A diving keeper aims to have the ball this far inside his standing reach.
const GK_DIVE_MARGIN: f32 = 0.3;

/// An on-ball decision for diagnostics: (time, situation, options, chosen index).
pub type LoggedDecision = (f32, Situation, Vec<on_ball::OptionEval>, usize);

/// Law 5.3: a foul the referee lets play go on after; penalised if the fouled team loses the ball before `until`.
#[derive(Clone, Copy, Debug)]
struct Advantage {
    by: PlayerId,
    on: PlayerId,
    spot: Vec2,
    misconduct: Misconduct,
    until: f32,
}

pub struct Simulation {
    pub state: MatchState,
    pub cfg: SimConfig,
    pub rng: SimRng,
    pub surface: SurfaceField,
    /// Ball parameters for the live simulation (sub-step = dt / substeps) and for planning.
    pub live: BallParams,
    pub plan: BallParams,
    /// Predicted path of the ball from its current state (planning physics).
    pub traj: Trajectory,
    pub events: Vec<MatchEvent>,
    pub stats: Stats,
    /// Diagnostics: when `Some`, every on-ball decision is appended.
    pub decision_log: Option<Vec<LoggedDecision>>,
    next_brain: f32,
    /// Team that kicked off the first half.
    first_kickoff: Side,
    /// Kicker and receiver of the last pass awaiting its first touch (pass completion).
    pending_pass: Option<PlayerId>,
    /// The intended receiver of the pass in flight and when he starts to take it (`reading::receive_delay`).
    pending_receiver: Option<(PlayerId, f32)>,
    ball_events: Vec<BallEvent>,
    /// When each side last scored and conceded (confidence statistics).
    last_scored: [f32; 2],
    last_conceded: [f32; 2],
    /// Law 11: the players marked in an offside position when a team-mate last played the ball.
    offside: [bool; 2 * PLAYERS_PER_TEAM],
    /// The restart being taken, until its kick (no offside from a goal kick, throw-in or corner, Law 11.3).
    restart_kick: Option<RestartKind>,
    /// The shot in flight was struck from this restart (a direct free kick or a penalty kick).
    set_shot: Option<RestartKind>,
    /// SP-04: the keeper who guessed the side of the penalty in flight.
    gk_anticipates: Option<PlayerId>,
    /// The last parry: shooter, time, the restart the shot was struck from. A parry that goes in is the shooter's goal.
    parry: Option<(PlayerId, f32, Option<RestartKind>)>,
    /// Law 5.3: a foul the referee is playing advantage after, and cards for fouls after which advantage ensued, to
    /// be shown at the next stoppage.
    advantage: Option<Advantage>,
    pending_cards: Vec<(PlayerId, Card)>,
    /// When each side last took a corner (set-piece statistics).
    corner_at: [f32; 2],
}

impl Simulation {
    pub fn new(home: &TeamData, away: &TeamData, cfg: SimConfig, conditions: PitchConditions, seed: u64) -> Self {
        let mut rng = SimRng::new(seed);
        let surface = SurfaceField::new(rng.next_seed());
        let mut players = Vec::with_capacity(2 * PLAYERS_PER_TEAM);
        let mut teams = Vec::new();
        for (side, data) in [(Side::Home, home), (Side::Away, away)] {
            let formation = Formation::builtin(&data.formation).unwrap_or_else(|| Formation::builtin("4-4-2").unwrap());
            for (slot, pd) in data.players.iter().take(PLAYERS_PER_TEAM).enumerate() {
                let id = side.index() * PLAYERS_PER_TEAM + slot;
                let kin = Kinematics::new(&pd.attrs, &cfg.player, &conditions, 1.0);
                let roles = [formation.slots[slot].role(true), formation.slots[slot].role(false)];
                players.push(Player::new(
                    id,
                    side,
                    slot,
                    roles,
                    pd.name.clone(),
                    pd.number,
                    pd.attrs.clone(),
                    kin,
                    pd.instructions.clone(),
                    pd.confidence,
                    roles.map(|r| pd.familiarity_with(r)),
                ));
                players.last_mut().unwrap().concept = data.practice.concept;
            }
            let numbers: Vec<u8> = data.players.iter().take(PLAYERS_PER_TEAM).map(|p| p.number).collect();
            let partners = std::array::from_fn(|i| std::array::from_fn(|j| data.partners.between(numbers[i], numbers[j])));
            let dir = if side == Side::Home { 1.0 } else { -1.0 };
            teams.push(Team {
                side,
                name: data.name.clone(),
                short: data.short.clone(),
                color: [data.color.0, data.color.1, data.color.2],
                formation,
                instructions: data.instructions.clone(),
                tactics: resolve(&data.instructions, &cfg.tactics),
                frame: TeamFrame::new(dir),
                practice: data.practice,
                partners,
                bench: data.bench.clone(),
                subs_used: 0,
                sub_windows: 0,
            });
        }
        assert_eq!(players.len(), 2 * PLAYERS_PER_TEAM, "each team needs eleven players");
        let away_team = teams.pop().unwrap();
        let home_team = teams.pop().unwrap();
        let live = BallParams::new(&cfg.ball, &conditions, cfg.timing.dt / cfg.timing.ball_substeps as f32);
        let plan = BallParams::new(&cfg.ball, &conditions, cfg.ai.planning_dt);
        let first_kickoff = if rng.chance(0.5) { Side::Home } else { Side::Away };
        let state = MatchState {
            time: 0.0,
            half: 1,
            half_length: cfg.timing.half_length_s,
            half_start: 0.0,
            lost: [cfg.rules.added_base_s; 2],
            added: [None; 2],
            ball: Ball::at_rest(Vec2::ZERO),
            players,
            teams: [home_team, away_team],
            score: [0, 0],
            possession: first_kickoff,
            control: None,
            last_touch: None,
            flight: None,
            phase: PlayPhase::FullTime,
            conditions,
            last_touch_quality: 1.0,
            possession_since: 0.0,
            views: [Beliefs::default(), Beliefs::default()],
        };
        let mut sim = Self {
            state,
            cfg,
            rng,
            surface,
            live,
            plan,
            traj: Trajectory::default(),
            events: Vec::new(),
            stats: Stats::default(),
            next_brain: 0.0,
            first_kickoff,
            pending_pass: None,
            pending_receiver: None,
            ball_events: Vec::new(),
            decision_log: None,
            last_scored: [f32::MIN; 2],
            last_conceded: [f32::MIN; 2],
            offside: [false; 2 * PLAYERS_PER_TEAM],
            restart_kick: None,
            set_shot: None,
            gk_anticipates: None,
            parry: None,
            advantage: None,
            pending_cards: Vec::new(),
            corner_at: [f32::MIN; 2],
        };
        sim.setup_kickoff(first_kickoff);
        sim
    }

    /// Match between two squads shipped with the engine.
    pub fn from_builtin(home: &str, away: &str, seed: u64) -> Self {
        let h = TeamData::builtin(home).expect("unknown home team");
        let a = TeamData::builtin(away).expect("unknown away team");
        Self::new(&h, &a, SimConfig::default(), PitchConditions::default(), seed)
    }

    pub fn finished(&self) -> bool {
        self.state.phase == PlayPhase::FullTime
    }

    /// Change pitch conditions (recomputes ball and movement parameters).
    pub fn set_conditions(&mut self, c: PitchConditions) {
        self.state.conditions = c;
        self.live = BallParams::new(&self.cfg.ball, &c, self.cfg.timing.dt / self.cfg.timing.ball_substeps as f32);
        self.plan = BallParams::new(&self.cfg.ball, &c, self.cfg.ai.planning_dt);
        for p in &mut self.state.players {
            p.kin = Kinematics::new(&p.attrs, &self.cfg.player, &c, p.energy);
        }
    }

    /// Re-resolve team tactics after the instructions changed (layer 2).
    pub fn refresh_tactics(&mut self) {
        for t in &mut self.state.teams {
            t.tactics = resolve(&t.instructions, &self.cfg.tactics);
        }
        self.next_brain = self.state.time;
    }

    /// Run until full time.
    pub fn run_to_end(&mut self) {
        while !self.finished() {
            self.step();
        }
    }

    fn push(&mut self, kind: EventKind) {
        if let EventKind::Shot { by, .. } = kind {
            self.stats.cal.shots_by_period[crate::stats::period(&self.state)] += 1;
            if self.state.time - self.corner_at[side_of(by).index()] < 10.0 {
                self.stats.teams[side_of(by).index()].corner_shots += 1;
            }
        }
        self.event_confidence(&kind);
        let (clock, stoppage) = (self.state.clock(), self.state.stoppage());
        self.events.push(MatchEvent { t: self.state.time, clock, stoppage, kind });
    }

    fn nudge(&mut self, id: PlayerId, delta: f32) {
        confidence::nudge(&mut self.state.players[id], &self.cfg.player, delta);
    }

    /// Confidence (M6): the step each event gives the players concerned (`PlayerConfig::conf_*`).
    fn event_confidence(&mut self, kind: &EventKind) {
        let pc = self.cfg.player.clone();
        match *kind {
            EventKind::Tackle { by, on, outcome } => match outcome {
                TackleOutcome::Won => {
                    self.nudge(by, pc.conf_duel_won);
                    self.nudge(on, pc.conf_duel_lost);
                }
                TackleOutcome::Beaten => {
                    self.nudge(by, pc.conf_duel_lost);
                    self.nudge(on, pc.conf_duel_won);
                }
                TackleOutcome::Deflected => {}
            },
            EventKind::Interception { by } => self.nudge(by, pc.conf_duel_won),
            EventKind::Aerial { won, lost } => {
                self.nudge(won, pc.conf_duel_won);
                self.nudge(lost, pc.conf_duel_lost);
            }
            EventKind::Miscontrol { by } => self.nudge(by, pc.conf_miscontrol),
            EventKind::Save { gk, shooter, .. } => {
                self.nudge(gk, pc.conf_save);
                if let Some(s) = shooter {
                    self.nudge(s, pc.conf_shot_on_target);
                }
            }
            EventKind::Goal { side, scorer, own_goal, .. } => {
                if let Some(s) = scorer.filter(|_| !own_goal) {
                    self.nudge(s, pc.conf_shot_on_target + pc.conf_goal);
                }
                let conceding = side.opponent();
                for i in 0..self.state.players.len() {
                    if self.state.players[i].side == conceding {
                        let gk = self.state.players[i].is_gk();
                        self.nudge(i, if gk { 2.0 * pc.conf_conceded } else { pc.conf_conceded });
                    }
                }
            }
            _ => {}
        }
    }

    /// A pass by `kicker` completed or lost: his confidence (M6) and the passing after conceding.
    fn pass_outcome(&mut self, kicker: PlayerId, completed: bool) {
        let pc = &self.cfg.player;
        let delta = if completed { pc.conf_pass_completed } else { pc.conf_pass_lost };
        self.nudge(kicker, delta);
        // ATT-05: completion by the scans in the 10 s before he received the ball.
        let g = match self.state.players[kicker].recv_scans {
            0..=2 => 0,
            3..=4 => 1,
            _ => 2,
        };
        let c = &mut self.stats.cal.passes_by_scans[g];
        c[0] += 1;
        c[1] += completed as u32;
        let side = side_of(kicker).index();
        if self.state.time - self.last_conceded[side] < crate::stats::AFTER_GOAL_S {
            let a = &mut self.stats.cal.passes_after_conceding[side];
            a[0] += 1;
            a[1] += completed as u32;
        }
    }

    /// Advance the match by one tick.
    pub fn step(&mut self) {
        if self.finished() {
            return;
        }
        let now = self.state.time;
        let dt = self.cfg.timing.dt;
        self.handle_phase(now);
        scan::update(&mut self.state, &self.cfg.ai, now);
        // Section 7: the intended receiver starts to take the pass once he has read it.
        if let Some((r, t)) = self.pending_receiver
            && now >= t
        {
            self.pending_receiver = None;
            if self.state.flight.is_some_and(|f| f.receiver == Some(r)) {
                self.prepare_receiver(r, now);
            }
        }
        if now >= self.next_brain {
            // Fatigue (M6): the physical capabilities follow the energy. Confidence returns to its pre-match level,
            // faster with a leader near.
            let tick = self.cfg.ai.team_tick_s;
            let pc = &self.cfg.player;
            let lead: Vec<f32> = (0..self.state.players.len())
                .map(|i| {
                    let p = &self.state.players[i];
                    let near = |q: &&Player| q.id != p.id && (q.pos - p.pos).length() < pc.conf_lift_radius;
                    self.state.squad(p.side).iter().filter(near).map(|q| q.attrs.leadership).fold(0.0, f32::max)
                })
                .collect();
            for (p, lead) in self.state.players.iter_mut().zip(lead) {
                p.kin = Kinematics::new(&p.attrs, pc, &self.state.conditions, p.energy);
                confidence::step(p, pc, lead, tick);
            }
            for side in 0..2 {
                if now - self.last_scored[side] < crate::stats::AFTER_GOAL_S {
                    self.stats.cal.after_scoring_s[side] += tick;
                }
            }
            self.stats.cal.team_s += tick;
            self.update_brains(now);
            self.next_brain = now + self.cfg.ai.team_tick_s;
            if self.state.is_live() {
                self.stats.sample(&self.state, &self.cfg, self.cfg.ai.team_tick_s);
            }
            self.stats.sample_running(&self.state, &self.cfg, self.cfg.ai.team_tick_s);
        }
        self.off_ball(now);
        self.carrier(now);
        self.goalkeepers(now);
        for p in &mut self.state.players {
            locomotion::step(p, dt, now);
            fatigue::step(p, &self.cfg.player, dt);
        }
        locomotion::separate(&mut self.state.players, self.cfg.player.body_radius);
        let before = self.state.ball.pos;
        self.attach_ball();
        self.step_ball(dt);
        // A carried ball is put at the carrier's foot before the physics step, which sees only the crossings within
        // its own sub-steps: check the whole tick's movement too, or a carrier can take the ball over a line unseen.
        if self.state.ball.mode != BallMode::Held
            && !self.ball_events.iter().any(|e| matches!(e, BallEvent::Goal { .. } | BallEvent::Out { .. }))
        {
            check_lines(&self.state.ball, before, &mut self.ball_events);
        }
        if self.state.is_live() {
            self.contacts(now);
            self.referee(now);
            self.advantage_check(now);
        }
        self.state.time += dt;
        self.clock();
    }

    // ------------------------------------------------------------------ phases, kick-off, restarts

    fn setup_kickoff(&mut self, side: Side) {
        self.stoppage_laws();
        self.substitutions();
        let taker = referee::restart_taker(&self.state, RestartKind::Kickoff, side, Vec2::ZERO);
        let pos = referee::kickoff_positions(&self.state, &self.cfg, side, taker);
        for (p, x) in self.state.players.iter_mut().zip(pos) {
            if p.sent_off {
                continue;
            }
            p.pos = x;
            p.vel = Vec2::ZERO;
            p.target = x;
            p.target_vel = Vec2::ZERO;
            p.body = Body::Normal;
            p.intent = Intent::Position;
            p.first_time = None;
            p.drift = Vec2::ZERO;
            p.facing = self.state.teams[p.side.index()].frame.to_world(Vec2::X);
        }
        self.state.ball = Ball::at_rest(Vec2::ZERO);
        self.state.control = None;
        self.state.flight = None;
        self.state.possession = side;
        self.state.possession_since = self.state.time;
        self.reset_brains();
        let now = self.state.time;
        self.state.phase =
            PlayPhase::Restart { kind: RestartKind::Kickoff, side, spot: Vec2::ZERO, taker, earliest: now + 1.0, deadline: now + 6.0 };
        self.push(EventKind::Kickoff { side });
    }

    /// No transition logic across a stoppage: brains start from the current possession.
    fn reset_brains(&mut self) {
        let poss = self.state.possession;
        for p in &mut self.state.players {
            p.beliefs = Beliefs { in_possession: p.side == poss, ..Beliefs::default() };
        }
        for side in [Side::Home, Side::Away] {
            self.state.views[side.index()] = Beliefs { in_possession: side == poss, ..Beliefs::default() };
        }
        self.next_brain = self.state.time;
    }

    fn start_restart(&mut self, kind: RestartKind, side: Side, spot: Vec2) {
        let now = self.state.time;
        self.stoppage_laws();
        self.substitutions();
        let spot = clamp_to_pitch(spot, 0.3);
        let taker = referee::restart_taker(&self.state, kind, side, spot);
        self.state.ball = Ball::at_rest(spot);
        self.clear_flight();
        self.state.control = None;
        if self.state.possession != side {
            self.state.possession = side;
            self.state.possession_since = now;
            self.stats.passes_in_possession[side.index()] = 0;
        }
        self.reset_brains();
        // Set pieces near goal wait for the players to take up their positions (SP-01..04).
        let set = crate::ai::set_piece::organised(&self.state, kind, side, spot);
        let tc = &self.cfg.tactics;
        let (earliest, deadline) = if set { (now + tc.sp01_setup_min_s, now + tc.sp01_setup_max_s) } else { (now + 1.2, now + 8.0) };
        self.state.phase = PlayPhase::Restart { kind, side, spot, taker, earliest, deadline };
        let ts = &mut self.stats.teams[side.index()];
        match kind {
            RestartKind::Corner => ts.corners += 1,
            RestartKind::ThrowIn => ts.throw_ins += 1,
            RestartKind::GoalKick => ts.goal_kicks += 1,
            RestartKind::FreeKick { .. } => ts.free_kicks += 1,
            RestartKind::Penalty | RestartKind::Kickoff => {}
        }
        self.push(EventKind::Restart { kind, side });
    }

    fn handle_phase(&mut self, now: f32) {
        match self.state.phase {
            PlayPhase::GoalScored { resume_at, kickoff_side } if now >= resume_at => self.setup_kickoff(kickoff_side),
            PlayPhase::Restart { kind, side, spot, taker, earliest, deadline } => {
                self.state.ball = Ball::at_rest(spot);
                let tp = &mut self.state.players[taker];
                let inward = -Vec2::new(
                    if spot.x.abs() > HALF_LENGTH - 1.0 { spot.x.signum() } else { 0.0 },
                    if spot.y.abs() > HALF_WIDTH - 1.0 { spot.y.signum() } else { 0.0 },
                )
                .normalize_or(self.state.teams[side.index()].frame.to_world(Vec2::X));
                tp.target = spot - inward * 0.4;
                tp.urgency = 0.8;
                tp.pace_s = 0.0;
                tp.brake = true;
                tp.target_vel = Vec2::ZERO;
                tp.look_at = spot + inward * 10.0;
                tp.intent = Intent::Position;
                tp.debug.label = format!("{kind:?}");
                if now >= deadline && (tp.pos - spot).length() > 1.0 {
                    tp.pos = spot - inward * 0.4;
                    tp.vel = Vec2::ZERO;
                }
                let ready = (tp.pos - spot).length() < 1.0 && tp.vel.length() < 1.0 && tp.can_act(now);
                // SP-01..04: an organised set piece waits until everybody is in position (up to the deadline).
                let organised = crate::ai::set_piece::organised(&self.state, kind, side, spot);
                let placed = !organised
                    || now >= deadline
                    || self.state.players.iter().all(|p| p.sent_off || p.id == taker || (p.pos - p.target).length() < 2.5);
                if now >= earliest && ready && placed {
                    self.take_restart(kind, side, spot, taker, now);
                }
            }
            _ => {}
        }
    }

    fn take_restart(&mut self, kind: RestartKind, side: Side, spot: Vec2, taker: PlayerId, now: f32) {
        self.state.phase = PlayPhase::Live;
        self.state.control = Some(taker);
        self.state.possession = side;
        let seed = self.rng.next_seed();
        let p = &mut self.state.players[taker];
        p.control_since = now;
        p.decision_seed = seed;
        if kind == RestartKind::Penalty {
            self.take_penalty(taker, spot, now);
            return;
        }
        let mut sit = Situation::open_play(taker, spot.extend(BALL_RADIUS));
        sit.restart = true;
        sit.offside_exempt = kind.offside_exempt();
        // SP-03: a direct free kick may be shot at goal.
        sit.direct = kind == RestartKind::FreeKick { direct: true };
        if kind == RestartKind::ThrowIn {
            sit.max_pass = 25.0;
        }
        self.restart_kick = Some(kind);
        self.on_ball_decision(taker, sit, now);
        if self.state.control == Some(taker) {
            // No pass chosen (restarts are passes only): play it to the nearest teammate.
            let to = self
                .state
                .squad(side)
                .iter()
                .filter(|p| p.id != taker)
                .min_by(|a, b| (a.pos - spot).length().total_cmp(&(b.pos - spot).length()).then(a.id.cmp(&b.id)))
                .map(|p| (p.id, p.pos))
                .unwrap();
            let plan = KickPlan {
                spec: KickSpec::Ground { target: to.1, arrival_speed: 5.0 },
                kind: KickKind::GroundPass,
                receiver: Some(to.0),
                target: to.1,
                arrival_t: 1.0,
                xg: 0.0,
                speed: 8.0,
                foot: None,
            };
            self.execute_kick(taker, plan, false, now);
        }
    }

    /// LAW-03, Law 7: each half lasts its regular time plus the added time. At the end of regular time the minimum added
    /// time is shown, the time lost so far in whole minutes; what is lost after that extends it.
    fn clock(&mut self) {
        let h = self.state.half as usize - 1;
        let (elapsed, regular) = (self.state.half_elapsed(), self.state.half_length);
        if elapsed >= regular && self.state.added[h].is_none() {
            let minutes = (self.state.lost[h] / 60.0).round();
            self.state.added[h] = Some(minutes * 60.0);
            self.stats.cal.added_shown_s[h] = minutes * 60.0;
            self.push(EventKind::AddedTime { minutes: minutes as u32 });
        }
        if !self.state.added[h].is_some_and(|a| elapsed >= regular + a) {
            return;
        }
        self.stats.cal.stoppage_played_s += elapsed - regular;
        if self.state.half == 1 {
            self.push(EventKind::HalfTime);
            self.state.half = 2;
            self.state.half_start = self.state.time;
            for team in &mut self.state.teams {
                team.frame = TeamFrame::new(-team.frame.dir);
            }
            self.setup_kickoff(self.first_kickoff.opponent());
        } else {
            self.push(EventKind::FullTime);
            self.state.phase = PlayPhase::FullTime;
        }
    }

    /// Law 7.3: the referee allows for `s` seconds lost (onto the added time once it has been shown).
    fn allow(&mut self, s: f32) {
        let h = self.state.half as usize - 1;
        match &mut self.state.added[h] {
            Some(a) => *a += s,
            None => self.state.lost[h] += s,
        }
    }

    // ------------------------------------------------------------------ player brains and off-ball decisions

    /// Every player updates his beliefs from the same world cache.
    pub fn update_brains(&mut self, now: f32) {
        self.traj = predict(&self.state.ball, &self.plan, TRAJ_HORIZON, TRAJ_SAMPLE_DT);
        let mut world = World::new(&self.state, &self.cfg, &self.traj, now);
        let mut beliefs: Vec<Beliefs> =
            (0..self.state.players.len()).map(|id| brain::update(&self.state, &mut world, id, &self.cfg, now)).collect();
        brain::read_team_mates(&self.state, &mut world, &self.cfg, &mut beliefs, now);
        // Calls (M6, section 7): a player voices a new claim he must make known, the line leader a change of his call
        // and the chaser "mine", while he has calls in hand.
        let rates: Vec<f32> = self.state.players.iter().map(|p| reading::call_rate(&self.state, &self.cfg.ai, p)).collect();
        for ((p, b), rate) in self.state.players.iter_mut().zip(beliefs.iter_mut()).zip(rates) {
            p.calls = (p.calls + rate * (now - p.calls_at)).min(self.cfg.ai.call_max);
            p.calls_at = now;
            let (prev, slot) = (&p.beliefs, p.slot);
            let mut calls = p.calls;
            let mut voice = |k: usize| {
                let ok = calls >= 1.0;
                if ok {
                    calls -= 1.0;
                    self.stats.cal.calls[k] += 1;
                }
                ok
            };
            // "Mine" and the line call before claims: they are the calls that cannot wait.
            b.chase_called = b.chaser == Some(slot) && if prev.chaser == Some(slot) { prev.chase_called } else { voice(2) };
            if b.line_leader == Some(slot) && b.line_step != prev.line_step {
                b.line_called_at = now;
                b.line_called = voice(1);
            }
            if b.claim_since == now {
                b.claim_called = brain::voiced_claim(b, slot) && voice(0);
            }
            p.calls = calls;
        }
        self.state.views = [Side::Home, Side::Away].map(|side| brain::view(&beliefs, side));
        for (p, b) in self.state.players.iter_mut().zip(beliefs) {
            p.beliefs = b;
        }
    }

    fn off_ball(&mut self, now: f32) {
        let restart = match self.state.phase {
            PlayPhase::Restart { kind, side, spot, taker, .. } => Some((kind, side, spot, taker)),
            _ => None,
        };
        for id in 0..2 * PLAYERS_PER_TEAM {
            let p = &self.state.players[id];
            if self.state.control == Some(id) || restart.is_some_and(|r| r.3 == id) {
                continue;
            }
            if now < p.next_decision || !p.can_act(now) {
                continue;
            }
            if let Intent::Receive { .. } = p.intent
                && self.state.flight.is_some_and(|f| f.receiver == Some(id))
            {
                // ATT-06 / ATT-22 / ATT-24: track the actual ball path.
                let pc = &self.cfg.player;
                if let Some((t, pt, meet)) = self.reception(id) {
                    let pm = &mut self.state.players[id];
                    pm.intent = Intent::Receive { point: pt, t: now + t, meet };
                    // ATT-05: he looks again while the ball comes and re-decides how to take it, up to his reaction
                    // time before it arrives.
                    if t > pm.kin.reaction {
                        self.decide_reception(id, t, pt);
                    }
                } else if intercept(p, &self.traj, pc.control_height, pc.control_radius, 0.0).is_none() {
                    // He can no longer get to it (it ran past him): a loose ball for whoever is nearest, him included,
                    // instead of a pass that nobody else may go for.
                    if let Some(f) = self.state.flight.as_mut() {
                        f.receiver = None;
                    }
                    let pm = &mut self.state.players[id];
                    pm.intent = Intent::Position;
                    pm.first_time = None;
                }
            }
            let mv = positioning::decide(&self.state, &self.cfg, id, &mut self.rng);
            // Movement model: he follows his movement's path, re-reading its references at his reaction time.
            let mut interval = if mv.tracks || mv.moving {
                self.state.players[id].kin.reaction
            } else {
                decision_interval(&self.state.players[id], &self.cfg.ai, &mut self.rng)
            };
            // Concentration (M6): a lapse; he switches off and does not re-read the game for `lapse_s`, holding this
            // movement and its target. Not the first defender on the ball, a receiver or the keeper.
            let p = &self.state.players[id];
            let on_ball = matches!(p.intent, Intent::Receive { .. })
                || (!p.beliefs.in_possession && p.beliefs.def_duties[p.slot] == DefDuty::First);
            if self.state.is_live() && !p.is_gk() && !on_ball {
                let chance = deviation::lapse_rate(p, &self.cfg.ai) * interval;
                if self.rng.chance(chance) {
                    interval = interval.max(self.cfg.ai.lapse_s);
                    self.stats.cal.lapses += 1;
                }
            }
            if mv.moving {
                let d = movement::Decided {
                    kind: mv.kind,
                    chose: mv.chose,
                    offer: mv.offer,
                    projected: mv.projected,
                    noise: mv.noise,
                    concept: mv.concept,
                };
                let plan = movement::plan_after(&self.state, &self.cfg, &self.state.players[id], d);
                self.state.players[id].plan = plan;
            }
            // SP-01..04: set-piece positions while the restart is prepared.
            let set = restart.and_then(|(kind, side, spot, taker)| {
                crate::ai::set_piece::target(&self.state, &self.cfg.tactics, id, kind, side, spot, taker, mv.target)
            });
            let sigma = drift_sigma(&self.state.players[id], &self.cfg.ai);
            let drift = update_drift(self.state.players[id].drift, interval, sigma, self.cfg.ai.drift_theta, &mut self.rng);
            let fr = self.state.frame(self.state.players[id].side);
            let p = &mut self.state.players[id];
            p.drift = drift;
            let mut target = mv.target;
            if mv.drifts {
                target += fr.to_world(drift);
            }
            // Layer 3: in positional play a player only adjusts once his point has moved more than `settle_radius`
            // from where he is heading; he does not chase every small correction.
            if mv.settles && (target - p.target).length() < self.cfg.ai.settle_radius {
                target = p.target;
            }
            if let Some(sp) = set {
                target = sp.pos;
            }
            if let Some((kind, side, spot, _)) = restart {
                if p.side != side {
                    target = referee::keep_out(target, spot, referee::keep_out_radius(kind));
                    // Law 16.1: at a goal kick the opponents stay outside the penalty area.
                    let mut t = fr.to_team(target);
                    if kind == RestartKind::GoalKick && t.y.abs() < PENALTY_AREA_HALF_WIDTH + 0.5 {
                        t.x = t.x.min(HALF_LENGTH - PENALTY_AREA_DEPTH - 0.5);
                        target = fr.to_world(t);
                    }
                }
                if kind == RestartKind::Kickoff {
                    let mut t = fr.to_team(target);
                    t.x = t.x.min(-0.5);
                    target = fr.to_world(t);
                }
            }
            p.target = target;
            p.commit = mv.commit;
            p.urgency = mv.urgency;
            p.pace_s = positioning::pace_time(p, &self.cfg.ai, mv.pace_s);
            p.pace_speed = if mv.pace_s > 0.0 { self.cfg.ai.pace_run_speed } else { f32::MAX };
            p.brake = mv.brake;
            p.target_vel = mv.target_vel;
            p.look_at = mv.look_at;
            if !matches!(p.intent, Intent::Receive { .. }) || mv.intent != Intent::Position {
                p.intent = mv.intent;
            }
            if matches!(p.intent, Intent::Receive { .. })
                && let Some(l) = open_body(p, self.state.ball.ground_pos())
            {
                p.look_at = l;
            }
            p.next_decision = now + interval;
            p.debug.rules = mv.rules;
            p.debug.label = mv.label.to_string();
            p.debug.anchor = mv.anchor;
            if let Some(sp) = set {
                p.urgency = p.urgency.max(0.7);
                p.brake = true;
                p.target_vel = Vec2::ZERO;
                p.look_at = self.state.ball.ground_pos();
                p.debug.rules = vec![sp.rule];
                p.debug.label = sp.label.to_string();
            }
        }
    }

    // ------------------------------------------------------------------ ball carrier

    fn carrier(&mut self, now: f32) {
        if !self.state.is_live() {
            return;
        }
        let Some(c) = self.state.control else { return };
        let ball = self.state.ball;
        let p = &self.state.players[c];
        let (p_pos, p_side) = (p.pos, p.side);
        let dist = (ball.ground_pos() - p.pos).length();
        let at_feet = dist <= self.cfg.player.control_radius && ball.height() <= self.cfg.player.control_height;
        match p.intent {
            Intent::GkHold { release_at } => {
                if now >= release_at {
                    let mut sit = Situation::open_play(c, p_pos.extend(BALL_RADIUS));
                    sit.gk_hands = true;
                    self.on_ball_decision(c, sit, now);
                    if self.state.control == Some(c) {
                        // Nothing better: kick it long.
                        let fr = self.state.frame(p_side);
                        let target = fr.to_world(Vec2::new(10.0, 0.0));
                        let plan = KickPlan {
                            spec: KickSpec::Lofted { target, launch_angle: 0.6, backspin: 4.0, sidespin: 0.0 },
                            kind: KickKind::Clearance,
                            receiver: None,
                            target,
                            arrival_t: 3.0,
                            xg: 0.0,
                            speed: 25.0,
                            foot: None,
                        };
                        self.execute_kick(c, plan, false, now);
                    }
                }
            }
            Intent::SetUp { plan, until, .. } => {
                let dir = (plan.target - ball.ground_pos()).normalize_or(p.facing);
                if p.can_act(now) && (set_up(p, dir, plan.speed, plan.foot, &self.cfg.execution) || now >= until) {
                    self.execute_kick(c, plan, false, now);
                    return;
                }
                // Plant and turn: brake and step towards the kick direction.
                let p = &mut self.state.players[c];
                p.target = p.pos + dir * 0.5;
                p.urgency = 0.3;
                p.brake = true;
                p.target_vel = Vec2::ZERO;
                p.look_at = p.pos + dir * 10.0;
            }
            Intent::Carry { dir, speed } => {
                if dist > self.cfg.player.dribble_leash + 2.0 {
                    // Heavy touch: the ball is loose.
                    self.state.control = None;
                    let p = &mut self.state.players[c];
                    p.intent = Intent::Position;
                    p.next_decision = now;
                    return;
                }
                // A running player plays a ball ahead of him with his next step: he can decide (and pass) then, but
                // the next dribble touch waits until it is at his feet.
                let towards = p.vel.dot((ball.ground_pos() - p.pos).normalize_or_zero()).max(0.0);
                let reach = self.cfg.player.control_radius + self.cfg.player.run_reach_s * towards;
                let playable = dist <= reach && ball.height() <= self.cfg.player.control_height;
                if playable && now >= p.touch_cooldown_until && p.can_act(now) {
                    if now >= p.next_decision {
                        self.on_ball_decision(c, Situation::open_play(c, ball.pos), now);
                    } else if at_feet && p.next_decision - now > self.cfg.player.retouch_cooldown {
                        // No knock-on when a decision comes due before he could touch the ball again (below): he
                        // keeps the ball at his feet for it instead of chasing it then.
                        let pressure = pressure_at(&self.state, p_side, ball.ground_pos());
                        let lead = if speed > 0.0 { on_ball::dribble_lead(pressure) } else { 0.6 };
                        self.dribble_touch(c, dir, lead, now);
                    }
                }
                let ball = self.state.ball;
                let (control_radius, catch_s) = (self.cfg.player.control_radius, self.cfg.player.settle_catch_s);
                let p = &mut self.state.players[c];
                if let Intent::Carry { speed, .. } = p.intent {
                    p.target = ball.ground_pos() + ball.ground_vel() * 0.3;
                    p.urgency = (speed / p.kin.top_speed + 0.1).min(1.0);
                    // He goes to the ball to play it, closing the gap the controlling touch or a knock-on left within
                    // `settle_catch_s`, not at dribbling pace.
                    let gap = ((ball.ground_pos() - p.pos).length() - 0.5 * control_radius).max(0.0);
                    let need = ball.ground_vel().length() + gap / catch_s;
                    p.urgency = p.urgency.max(need / p.kin.top_speed).min(1.0);
                    p.brake = false;
                    p.target_vel = Vec2::ZERO;
                    p.look_at = ball.ground_pos();
                }
            }
            _ => {
                // Holding / shielding: ball at feet (see `attach_ball`).
                if now >= p.next_decision && p.can_act(now) && at_feet {
                    self.on_ball_decision(c, Situation::open_play(c, ball.pos), now);
                }
                if self.state.control == Some(c) && matches!(self.state.players[c].intent, Intent::Hold) {
                    let fr = self.state.frame(self.state.players[c].side);
                    let p = &mut self.state.players[c];
                    p.target = p.pos;
                    p.urgency = 0.3;
                    p.brake = true;
                    p.target_vel = Vec2::ZERO;
                    p.look_at = fr.to_world(TeamFrame::opp_goal());
                }
            }
        }
    }

    /// Ask the on-ball AI and carry out its choice.
    fn on_ball_decision(&mut self, c: PlayerId, sit: Situation, now: f32) {
        let (action, opts, chosen) = on_ball::decide(&self.state, &self.cfg, &self.plan, sit);
        if let Some(log) = &mut self.decision_log {
            log.push((self.state.time, sit, opts.clone(), chosen));
        }
        let p = &mut self.state.players[c];
        p.debug.options = opts.iter().map(|o| (o.label.clone(), o.u_opt, o.instr, o.pers, o.prob)).collect();
        if let Some(o) = opts.get(chosen) {
            p.debug.label = o.label.clone();
            p.debug.rules = o.rules.clone();
            p.debug.chosen_p = o.p;
        }
        p.last_decision = now;
        p.next_decision = now + self.cfg.ai.onball_interval;
        match action {
            Action::Kick(plan) => {
                let p = &self.state.players[c];
                let dir = (plan.target - sit.ball.truncate()).normalize_or(p.facing);
                let ex = &self.cfg.execution;
                if sit.restart {
                    // The ball was dead: the taker has lined up with the kick.
                    self.state.players[c].facing = dir;
                    self.execute_kick(c, plan, false, now);
                } else if sit.first_time || sit.gk_hands || set_up(p, dir, plan.speed, plan.foot, ex) {
                    self.execute_kick(c, plan, sit.first_time, now);
                } else {
                    // Body not square to the kick, or running away from it: brake and turn with the ball first.
                    let until = now + setup_time(p, dir, plan.speed, plan.foot, ex) + ex.setup_timeout_s;
                    let p = &mut self.state.players[c];
                    p.intent = Intent::SetUp { plan, until, since: now };
                    p.debug.label = format!("set up → {}", p.debug.label);
                    // ATT-24: the receiver reads the set-up and moves to receive before the ball is struck, once he
                    // has read the passer (section 7).
                    if let Some(r) = plan.receiver {
                        let read = reading::receive_delay(&self.state, &self.cfg.ai, r, c);
                        let q = &mut self.state.players[r];
                        q.next_decision = q.next_decision.min(now + read + q.kin.reaction);
                    }
                }
            }
            Action::Dribble { dir, lead } => {
                let drib = norm(self.state.players[c].attrs.dribbling);
                let p = &mut self.state.players[c];
                let speed = 0.8 * p.kin.top_speed * (0.6 + 0.4 * drib);
                p.intent = Intent::Carry { dir, speed };
                self.dribble_touch(c, dir, lead, now);
            }
            Action::Hold => {
                if !sit.restart && !sit.gk_hands {
                    self.state.players[c].intent = Intent::Hold;
                }
            }
        }
    }

    /// Push the ball ahead while dribbling: it runs `lead` metres ahead of a player moving at the carry speed.
    pub fn dribble_touch(&mut self, c: PlayerId, dir: Vec2, lead: f32, now: f32) {
        let pressure = pressure_at(&self.state, self.state.players[c].side, self.state.ball.ground_pos());
        let p = &self.state.players[c];
        let drib = norm(p.attrs.dribbling);
        let speed = match p.intent {
            Intent::Carry { speed, .. } => speed,
            _ => 0.6 * p.kin.top_speed,
        };
        let a = self.live.rolling_decel;
        let ex = &self.cfg.execution;
        let err = (ex.error_scale_worst + (ex.error_scale_best - ex.error_scale_worst) * drib) * (1.0 + 0.6 * pressure * (1.0 - norm(p.attrs.composure)));
        let yaw = self.rng.gauss(0.0, 0.06 * err);
        // The touch is weighted to what the carrier can follow: his current speed along the new direction plus the
        // lead. A sharp change of direction is a cut or drag-back that keeps the ball close while he turns.
        let v_along = p.vel.dot(dir).clamp(0.0, speed);
        let straight = (1.0 + p.vel.normalize_or(dir).dot(dir)) / 2.0;
        let lead = (lead * straight).max(0.3);
        let u = (v_along + (2.0 * a * lead).sqrt()) * (1.0 + self.rng.gauss(0.0, 0.06 * err)).clamp(0.6, 1.5);
        let mut d = Vec2::from_angle(yaw).rotate(dir);
        let bp = self.state.ball.ground_pos();
        if (bp.y + d.y * 4.0).abs() > HALF_WIDTH - 0.5 {
            d.y = -d.y;
        }
        if (bp.x + d.x * 4.0).abs() > HALF_LENGTH - 0.5 {
            d.x = -d.x;
        }
        let mut ball = self.state.ball;
        ball.pos.z = BALL_RADIUS;
        ball.kick((d * u).extend(0.0), Vec3::ZERO);
        self.state.ball = ball;
        self.state.last_touch = Some(c);
        self.clear_flight();
        self.played(c);
        let p = &mut self.state.players[c];
        p.touch_cooldown_until = now + 0.2;
        if let Intent::Carry { speed, .. } = p.intent {
            p.intent = Intent::Carry { dir: d, speed };
        }
        self.traj = predict(&self.state.ball, &self.plan, TRAJ_HORIZON, TRAJ_SAMPLE_DT);
    }

    /// Solve the planned kick from the actual ball position, apply execution error and release the ball.
    pub fn execute_kick(&mut self, c: PlayerId, plan: KickPlan, first_time: bool, now: f32) {
        let side = self.state.players[c].side;
        // A set-up kick is legible to the receiver from the start of the set-up (section 7).
        let legible = match self.state.players[c].intent {
            Intent::SetUp { since, .. } => since,
            _ => now,
        };
        let fr = self.state.frame(side);
        let mut from = self.state.ball.pos;
        if self.state.ball.mode == BallMode::Held {
            let p = &self.state.players[c];
            from = (p.pos + p.facing * 0.5).extend(BALL_RADIUS);
        }
        from.z = from.z.max(BALL_RADIUS);
        let p = &self.state.players[c];
        let Some(sol) = solve_kick(from, &plan.spec, &self.plan, p.kin.max_kick_speed) else {
            // Not executable from here: keep the ball.
            self.state.players[c].intent = Intent::Hold;
            return;
        };
        let pressure = pressure_at(&self.state, side, from.truncate());
        // The skill the kick uses depends on its length (shots: distance to goal) and on whether it is a cross.
        let (from_t, target_t) = (fr.to_team(from.truncate()), fr.to_team(plan.target));
        let range = if plan.kind == KickKind::Shot { (TeamFrame::opp_goal() - from_t).length() } else { (target_t - from_t).length() };
        let cross = matches!(plan.kind, KickKind::GroundPass | KickKind::LoftedPass)
            && is_cross(from_t, target_t, plan.kind == KickKind::LoftedPass);
        let dir = sol.vel.truncate().normalize_or(p.facing);
        let foot = plan.foot.unwrap_or_else(|| {
            natural_foot(p.attrs.preferred_foot, p.facing, dir, sol.vel.length(), p.kin.max_kick_speed, &self.cfg.execution)
        });
        let mut inp = ExecInput::for_player(
            &p.attrs,
            plan.kind,
            range,
            cross,
            foot,
            first_time,
            pressure,
            p.confidence,
            p.energy,
            &self.cfg.execution,
        );
        if !p.is_gk() || self.state.ball.mode != BallMode::Held {
            inp.body_excess = setup_turn(p.facing, dir, plan.foot, sol.vel.length(), p.kin.max_kick_speed, &self.cfg.execution);
        }
        inp.speed_frac = sol.vel.length() / p.kin.max_kick_speed.max(1.0);
        let (vel, spin, _) = perturb(sol.vel, sol.spin, &inp, &self.cfg.execution, &mut self.rng);
        let mut ball = self.state.ball;
        ball.pos = from;
        ball.mode = BallMode::Air;
        ball.kick(vel, spin);
        self.state.ball = ball;
        self.clear_flight();
        self.state.control = None;
        self.state.last_touch = Some(c);
        self.traj = predict(&self.state.ball, &self.plan, TRAJ_HORIZON, TRAJ_SAMPLE_DT);
        let start = from.truncate();
        let backward = fr.to_team(plan.target).x < fr.to_team(start).x - 2.0;
        self.state.flight = Some(BallFlight {
            kicker: c,
            receiver: plan.receiver,
            kind: plan.kind,
            start_t: now,
            start_pos: start,
            target: plan.target,
            arrival_t: sol.time,
            speed: vel.length(),
            xg: plan.xg,
            backward,
            first_time,
            save_attempted: false,
            on_goal: self.traj.on_goal,
            weak_foot: inp.weak_foot,
        });
        // Laws 11, 13–14: the offside marks from this kick; set-piece statistics.
        let restart = self.restart_kick;
        self.played(c);
        let ts = &mut self.stats.teams[side.index()];
        match restart {
            Some(RestartKind::Penalty) => {
                self.set_shot = restart;
                self.keeper_guess(c, now);
            }
            Some(RestartKind::FreeKick { direct: true }) if plan.kind == KickKind::Shot => {
                ts.fk_shots += 1;
                ts.fk_xg += plan.xg;
                self.set_shot = restart;
            }
            // Short: under 32 yd (StatsBomb).
            Some(RestartKind::GoalKick) if (plan.target - start).length() < 29.3 => ts.goal_kicks_short += 1,
            Some(RestartKind::Corner) => {
                if !crate::rules::referee::in_penalty_area(&self.state, side.opponent(), plan.target) {
                    ts.corners_short += 1;
                }
                self.corner_at[side.index()] = now;
            }
            _ => {}
        }
        let windup = self.cfg.ai.kick_windup_s;
        let cooldown = self.cfg.player.retouch_cooldown;
        let p = &mut self.state.players[c];
        p.body = Body::Kicking { until: now + windup };
        p.touch_cooldown_until = now + cooldown;
        p.intent = Intent::Position;
        p.first_time = None;
        p.next_decision = now + windup;

        let ts = &mut self.stats.teams[side.index()];
        if plan.kind == KickKind::Shot {
            // SP-04: a penalty kick's xG is the penalty conversion rate.
            let xg = if restart == Some(RestartKind::Penalty) { self.cfg.tactics.sp04_xg } else { plan.xg };
            ts.shots += 1;
            ts.xg += xg;
            self.pending_pass = None;
            self.push(EventKind::Shot { by: c, xg, first_time, weak_foot: inp.weak_foot });
        } else {
            ts.passes += 1;
            self.stats.passes_in_possession[side.index()] += 1;
            self.pending_pass = Some(c);
            let length = (plan.target - start).length();
            self.push(EventKind::Pass { by: c, to: plan.receiver, kind: plan.kind, length, first_time, headed: false });
        }

        if let Some(r) = plan.receiver {
            self.receive_after(r, c, legible, now);
        }
        // ATT-22 / PRS-02: everybody reacts to the kick after his reaction time, to a pass less what he read before it
        // was struck (IND-08); brains update at release.
        let pc = &self.cfg.player;
        for q in &mut self.state.players {
            let reaction = if plan.kind == KickKind::Shot { q.kin.reaction } else { pass_reaction(q, 0.0, pc) };
            q.next_decision = q.next_decision.min(now + reaction);
        }
        self.update_brains(now);
        self.next_brain = now + self.cfg.ai.team_tick_s;
    }

    /// ATT-24: where the intended receiver `r` takes the pass in flight (time from now, point), and whether he comes
    /// to meet it. Free where it was aimed, he takes it there; under pressure there (ATT-06), or unable to get there
    /// in time, he meets it at the earliest point he reaches before the ball. Under pressure he attacks a high ball up
    /// to his header height ("just before your opponent" [USS-D p.73]) rather than waiting for it to drop to him.
    fn reception(&self, r: PlayerId) -> Option<(f32, Vec2, bool)> {
        let p = &self.state.players[r];
        let pc = &self.cfg.player;
        let target = self.state.flight?.target;
        // Planned from the ball as it is now, so a sample's time is the time left (`self.traj` may be up to a team
        // tick old).
        let traj = predict(&self.state.ball, &self.plan, TRAJ_HORIZON, TRAJ_SAMPLE_DT);
        if let Some((t, pt)) = reception_point(p, &traj, target, pc.control_height, self.cfg.tactics.att24_set_margin_s)
            && !receiver_pressured(&self.state, &self.cfg, r, pt, t)
        {
            return Some((t, pt, false));
        }
        intercept(p, &traj, p.header_height().max(pc.control_height), 0.3, 0.0).map(|(t, pt)| (t, pt, true))
    }

    /// ATT-06: under pressure at his reception point (in `t` s) the receiver decides before the ball arrives whether
    /// to play first time; free, he will control it and turn. Sets `first_time` and the direction of the controlling
    /// touch; returns whether he is pressed there.
    fn decide_reception(&mut self, r: PlayerId, t: f32, point: Vec2) -> bool {
        let pressured = receiver_pressured(&self.state, &self.cfg, r, point, t) || self.man_on(r, t, point);
        let mut first_time = None;
        if pressured {
            let height = self.traj.at(t).pos.z.max(BALL_RADIUS);
            let mut sit = Situation::open_play(r, point.extend(height));
            sit.first_time = true;
            let (action, opts, chosen) = on_ball::decide(&self.state, &self.cfg, &self.plan, sit);
            if let Some(log) = &mut self.decision_log {
                log.push((self.state.time, sit, opts.clone(), chosen));
            }
            if let Action::Kick(k) = action {
                first_time = Some(k);
            }
            let p = &mut self.state.players[r];
            p.debug.options = opts.iter().map(|o| (o.label.clone(), o.u_opt, o.instr, o.pers, o.prob)).collect();
            if let Some(o) = opts.get(chosen) {
                p.debug.label = format!("receive → {}", o.label);
                p.debug.chosen_p = o.p;
            }
        }
        let dir = receive_dir(&self.state, &self.cfg, r, point, pressured);
        let p = &mut self.state.players[r];
        p.first_time = first_time;
        p.receive_dir = dir;
        pressured
    }

    /// Calls (M6): a team-mate within `call_range` of the receiver `r`'s reception point who sees pressure on him that `r`
    /// does not (outside his view since his last scan) shouts "man on" if he has a call in hand: `r`'s picture is
    /// refreshed. Returns whether `r` is pressured after the call.
    fn man_on(&mut self, r: PlayerId, t: f32, point: Vec2) -> bool {
        let saved = self.state.players[r].scan.clone();
        let now = self.state.time;
        scan::refresh(&mut self.state, r, now);
        let side = self.state.players[r].side;
        let caller = self
            .state
            .squad(side)
            .iter()
            .filter(|q| q.id != r && q.calls >= 1.0 && (q.pos - point).length() < self.cfg.ai.call_range)
            .min_by(|a, b| (a.pos - point).length().total_cmp(&(b.pos - point).length()).then(a.id.cmp(&b.id)))
            .map(|q| q.id);
        match caller {
            Some(c) if receiver_pressured(&self.state, &self.cfg, r, point, t) => {
                self.state.players[c].calls -= 1.0;
                self.stats.cal.calls[3] += 1;
                true
            }
            _ => {
                self.state.players[r].scan = saved;
                false
            }
        }
    }

    /// The intended receiver `r` of `passer`'s pass, legible since `legible` (the strike, or the start of its set-up),
    /// starts to take it once he has read it (section 7).
    fn receive_after(&mut self, r: PlayerId, passer: PlayerId, legible: f32, now: f32) {
        let ready = legible + reading::receive_delay(&self.state, &self.cfg.ai, r, passer);
        if ready > now {
            self.pending_receiver = Some((r, ready));
        } else {
            self.pending_receiver = None;
            self.prepare_receiver(r, now);
        }
    }

    /// The intended receiver moves to the reception point and decides (ATT-06) whether to play first time.
    fn prepare_receiver(&mut self, r: PlayerId, now: f32) {
        let rp = &self.state.players[r];
        let (t, point, meet) = self.reception(r).unwrap_or_else(|| {
            (self.state.flight.map_or(1.0, |f| f.arrival_t), self.state.flight.map_or(rp.pos, |f| f.target), false)
        });
        // One noise draw per reception: the later looks (ATT-05) re-decide with it.
        self.state.players[r].decision_seed = self.rng.next_seed();
        self.decide_reception(r, t, point);
        let steer = positioning::receive_steering(&self.state, &self.state.players[r], point, now + t);
        let p = &mut self.state.players[r];
        p.intent = Intent::Receive { point, t: now + t, meet };
        (p.target, p.target_vel, p.brake) = steer;
        p.urgency = 1.0;
        if let Some(l) = open_body(p, self.state.ball.ground_pos()) {
            p.look_at = l;
        }
        p.debug.rules = vec!["ATT-06", "ATT-22", "ATT-24"];
    }

    fn clear_flight(&mut self) {
        self.set_shot = None;
        self.gk_anticipates = None;
        if let Some(f) = self.state.flight.take()
            && let Some(r) = f.receiver
        {
            let p = &mut self.state.players[r];
            if matches!(p.intent, Intent::Receive { .. }) {
                p.intent = Intent::Position;
                p.next_decision = self.state.time;
            }
            p.first_time = None;
        }
    }

    // ------------------------------------------------------------------ goalkeepers

    /// Goalkeeper reaction to a shot: dive towards the point on the ball's path he can reach with the most margin,
    /// just far enough to have it within standing reach and only as fast as the time left demands.
    fn goalkeepers(&mut self, now: f32) {
        let Some(f) = self.state.flight else { return };
        if f.kind != KickKind::Shot && self.state.ball.speed() < 12.0 {
            return;
        }
        // GK-01: the keeper commits to shots that are going in; wide ones he lets go (he may still touch them).
        if !self.traj.on_goal {
            return;
        }
        let defending = side_of(f.kicker).opponent();
        let Some(gk) = self.state.squad(defending).iter().find(|p| p.is_gk()).map(|p| p.id) else { return };
        let g = &self.state.players[gk];
        let pc = &self.cfg.player;
        let reaction = if self.gk_anticipates == Some(gk) {
            0.0
        } else {
            pc.gk_reaction_max + (pc.gk_reaction_min - pc.gk_reaction_max) * norm(g.attrs.reflexes)
        };
        if g.body != Body::Normal || now < f.start_t + reaction {
            return;
        }
        let fr = self.state.frame(defending);
        let reach = g.kin.gk_dive_reach;
        let standing = self.cfg.player.gk_standing_reach;
        let dive_speed = self.cfg.player.gk_dive_speed;
        // Planned from the ball as it is now, so a sample's time is the time the keeper has left (`self.traj` may be
        // up to a team tick old).
        let traj = predict(&self.state.ball, &self.plan, TRAJ_HORIZON, TRAJ_SAMPLE_DT);
        let mut best: Option<(f32, Vec2, f32)> = None;
        for s in &traj.samples {
            if traj.exit_time.is_some_and(|e| s.t > e + 0.05) {
                break;
            }
            let gpt = s.pos.truncate();
            // Only balls heading for the goal mouth area.
            let tp = fr.to_team(gpt);
            if tp.x > -HALF_LENGTH + PENALTY_AREA_DEPTH {
                continue;
            }
            if s.pos.z - BALL_RADIUS > self.cfg.player.gk_max_height {
                continue;
            }
            let need = ((gpt - g.pos).length() - (standing - GK_DIVE_MARGIN)).max(0.0);
            let slack = (dive_speed * s.t).min(reach) - need;
            if best.is_none_or(|(b, _, _)| slack > b) {
                best = Some((slack, gpt, s.t));
            }
        }
        let Some((_, point, t)) = best else { return };
        let to = point - g.pos;
        if to.length() <= standing {
            return;
        }
        let dur = t.max(0.15);
        let need = to.length() - (standing - GK_DIVE_MARGIN);
        let v = (to.normalize() * need / dur).clamp_length_max(dive_speed);
        let p = &mut self.state.players[gk];
        p.body = Body::Diving { until: now + dur.min(0.9), dir: v };
        p.debug.label = "dive".into();
        p.debug.rules = vec!["GK-01"];
    }

    // ------------------------------------------------------------------ ball

    /// Keep a held ball in the keeper's hands and a shielded ball at the carrier's feet.
    fn attach_ball(&mut self) {
        let Some(c) = self.state.control else { return };
        let p = &self.state.players[c];
        match p.intent {
            Intent::GkHold { .. } => {
                self.state.ball.mode = BallMode::Held;
                self.state.ball.pos = p.pos.extend(1.0);
                self.state.ball.vel = Vec3::ZERO;
                self.state.ball.spin = Vec3::ZERO;
            }
            Intent::SetUp { .. } => {
                // The ball is drawn to the kicking foot and turns with the body.
                let foot = p.pos + p.facing * 0.4;
                let bp = self.state.ball.ground_pos();
                let pos = bp + (foot - bp).clamp_length_max(4.0 * self.cfg.timing.dt);
                self.state.ball.mode = BallMode::Rolling;
                self.state.ball.pos = pos.extend(BALL_RADIUS);
                self.state.ball.vel = p.vel.extend(0.0);
                self.state.ball.spin = Vec3::ZERO;
            }
            Intent::Hold => {
                let foot = p.pos + p.facing * 0.4;
                self.state.ball.mode = BallMode::Rolling;
                self.state.ball.pos = foot.extend(BALL_RADIUS);
                self.state.ball.vel = p.vel.extend(0.0);
                self.state.ball.spin = Vec3::ZERO;
            }
            _ => {}
        }
    }

    fn step_ball(&mut self, dt: f32) {
        self.ball_events.clear();
        let n = self.cfg.timing.ball_substeps.max(1);
        let sub = dt / n as f32;
        for _ in 0..n {
            let surface = SurfaceInput { field: &self.surface, rng: &mut self.rng };
            step_ball(&mut self.state.ball, sub, &self.live, Some(surface), &mut self.ball_events);
        }
    }

    // ------------------------------------------------------------------ contacts

    fn contacts(&mut self, now: f32) {
        if self.state.ball.mode == BallMode::Held {
            return;
        }
        if self.tackles(now) {
            return;
        }
        let ball = self.state.ball;
        let b0 = ball.pos - ball.vel * self.cfg.timing.dt;
        let seg = ball.pos - b0;
        let shooter = self.state.flight.filter(|f| f.kind == KickKind::Shot).map(|f| f.kicker);
        // Closest eligible player to the ball's path during this tick.
        let mut best: Option<(f32, PlayerId, Vec3)> = None;
        for p in &self.state.players {
            if self.state.control == Some(p.id) || now < p.touch_cooldown_until || !p.can_act(now) && !matches!(p.body, Body::Diving { .. }) {
                continue;
            }
            // Teammates leave a dribbling carrier's ball alone; opponents can only win it in a tackle duel
            // (`tackles`, IND-03), which already favours a defender when a touch runs closer to him.
            if self.state.control.is_some() {
                continue;
            }
            let u = if seg.length_squared() > 1e-8 {
                (p.pos.extend(0.0) - b0).truncate().dot(seg.truncate()) / seg.truncate().length_squared().max(1e-8)
            } else {
                1.0
            };
            // An opponent meets a shot where it passes closest to him: one roll there, not a leg's reach early (a
            // missed stretch would leave him on touch cooldown when the ball reaches his body).
            if u > 1.0 && shooter.is_some_and(|k| side_of(k) != p.side) {
                continue;
            }
            let u = u.clamp(0.0, 1.0);
            let bp = b0 + seg * u;
            let d = (bp.truncate() - p.pos).length();
            let pc = &self.cfg.player;
            let radius = if p.is_gk() && self.gk_can_handle(p.id, bp.truncate()) {
                pc.gk_standing_reach
            } else if shooter.is_some_and(|k| side_of(k) != p.side) && !p.is_gk() && bp.z - BALL_RADIUS <= pc.shot_block_height {
                pc.shot_block_reach.max(pc.control_radius)
            } else {
                pc.control_radius
            };
            if d > radius {
                continue;
            }
            if best.is_none_or(|(bd, _, _)| d < bd) {
                best = Some((d, p.id, bp));
            }
        }
        let Some((d, id, bp)) = best else { return };
        // Law 11.2: a player marked offside who plays the ball (or challenges for it, below) is penalised.
        if self.offside_offence(id) {
            return;
        }
        let height = bp.z - BALL_RADIUS;
        let p = &self.state.players[id];
        // Slow high balls are taken down on the chest / thigh; only fast ones are headed.
        let chest_height = self.cfg.player.control_height + 0.4;
        if p.is_gk() && self.gk_can_handle(id, bp.truncate()) && height <= self.cfg.player.gk_max_height {
            self.gk_contact(id, now);
            return;
        }
        if height > p.header_height() {
            return;
        }
        // A high ball with an opponent close enough to jump for it is an aerial duel.
        let challenger = if height > self.cfg.player.control_height { self.aerial_challenger(id, bp, height, now) } else { None };
        if let Some(c) = challenger {
            if self.offside_offence(c) {
                return;
            }
            let pw = aerial_win_prob(&self.state.players[id], &self.state.players[c], &self.cfg.duel);
            let (won, lost) = if self.rng.chance(pw) { (id, c) } else { (c, id) };
            self.state.players[lost].touch_cooldown_until = now + 0.4;
            if self.aerial_foul(lost, won, now) {
                return;
            }
            self.push(EventKind::Aerial { won, lost });
            self.header(won, true, now);
            return;
        }
        let p = &self.state.players[id];
        if height <= self.cfg.player.control_height || (height <= chest_height && ball.speed() < 9.0) {
            self.foot_contact(id, d, now);
            return;
        }
        // Uncontested, the intended receiver lets a pass he planned to take lower drop to him rather than heading it
        // as it comes down through head height near him.
        let waits = self.state.flight.is_some_and(|f| f.receiver == Some(id) && side_of(f.kicker) == p.side)
            && matches!(p.intent, Intent::Receive { t, .. } if now < t)
            && ball.vel.z < 0.0;
        if !waits {
            self.header(id, false, now);
        }
    }

    /// The opponent of `id` best placed to challenge him for a high ball at `bp` (`height` above the ground): an
    /// outfield player within `aerial_contest_radius` who can reach that height and is free to play it.
    fn aerial_challenger(&self, id: PlayerId, bp: Vec3, height: f32, now: f32) -> Option<PlayerId> {
        let side = side_of(id);
        let r = self.cfg.duel.aerial_contest_radius;
        self.state
            .squad(side.opponent())
            .iter()
            .filter(|o| !o.is_gk() && o.can_act(now) && now >= o.touch_cooldown_until && height <= o.header_height())
            .map(|o| (o.id, (o.pos - bp.truncate()).length()))
            .filter(|&(_, d)| d <= r)
            .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
            .map(|(o, _)| o)
    }

    /// Goalkeepers may handle the ball in their own penalty area, except from a teammate's deliberate pass.
    fn gk_can_handle(&self, id: PlayerId, at: Vec2) -> bool {
        let p = &self.state.players[id];
        let t = self.state.frame(p.side).to_team(at);
        let in_box = t.x < -HALF_LENGTH + PENALTY_AREA_DEPTH && t.y.abs() < PENALTY_AREA_HALF_WIDTH;
        // Back-pass rule: not from a teammate's deliberate pass.
        let back_pass = self.state.flight.is_some_and(|f| side_of(f.kicker) == p.side && f.kind != KickKind::Shot)
            || self.state.control.is_some_and(|c| side_of(c) == p.side);
        in_box && !back_pass
    }

    fn gain_control(&mut self, id: PlayerId, quality: f32, now: f32) {
        let side = self.state.players[id].side;
        if let Some(k) = self.pending_pass.take() {
            if side_of(k) == side {
                self.stats.teams[side.index()].passes_completed += 1;
            }
            self.pass_outcome(k, side_of(k) == side);
        }
        if self.state.possession != side {
            self.state.possession = side;
            self.state.possession_since = now;
            self.stats.passes_in_possession[side.index()] = 0;
        }
        self.clear_flight();
        self.played(id);
        self.state.control = Some(id);
        self.state.last_touch = Some(id);
        self.state.last_touch_quality = quality;
        let seed = self.rng.next_seed();
        let p = &mut self.state.players[id];
        p.recv_scans = p.scan.within(now, 10.0);
        let cal = &mut self.stats.cal;
        cal.scans_before_receiving += p.recv_scans as u32;
        cal.receptions += 1;
        p.control_since = now;
        p.decision_seed = seed;
        p.first_time = None;
    }

    /// The first touch by anyone after a pass decides pass completion; touches by opponents count as interceptions.
    fn note_touch(&mut self, id: PlayerId) {
        if let Some(k) = self.pending_pass.take() {
            let side = side_of(id);
            if side_of(k) == side {
                self.stats.teams[side.index()].passes_completed += 1;
            } else {
                self.stats.teams[side.index()].interceptions += 1;
                self.push(EventKind::Interception { by: id });
            }
            self.pass_outcome(k, side_of(k) == side);
        }
    }

    /// `d`: horizontal distance (m) from the player to the ball's path this tick.
    fn foot_contact(&mut self, id: PlayerId, d: f32, now: f32) {
        let ball = self.state.ball;
        let p = &self.state.players[id];
        let side = p.side;
        let intended = self.state.flight.is_some_and(|f| f.receiver == Some(id));
        let opponents_ball = self.state.flight.is_some_and(|f| side_of(f.kicker) != side)
            || self.state.control.is_some_and(|c| side_of(c) != side);
        // A dropping ball is harder to cushion than one along the ground at the same speed (as `evaluate_pass`).
        let rel_speed = (ball.vel - p.vel.extend(0.0)).length();
        let eff_height = ball.height() + 0.6 * ball.vel.z.abs() / 5.0;

        // Contested ball: an interceptor only gets a touch on it with some probability, but a shot straight at an
        // outfield player hits his body. A kick that reaches him within his reaction time of being struck is blocked
        // the same way: he has no time to shape a touch (StatsBomb: 19.8 blocks per team-match, most of them passes
        // and crosses; a fifth of all corners come from one).
        if opponents_ball {
            let shot = self.state.flight.is_some_and(|f| f.kind == KickKind::Shot);
            let blocked = shot || self.state.flight.is_some_and(|f| now - f.start_t < p.kin.reaction);
            let pc = &self.cfg.player;
            let p_touch = if blocked && !p.is_gk() && d < pc.body_radius {
                pc.shot_body_block_p
            } else if blocked && !p.is_gk() {
                pc.shot_leg_block_p * (0.75 + 0.25 * norm(p.attrs.instinct))
            } else {
                pass_touch_prob(ball.speed(), p.attrs.instinct, pc)
            };
            if !self.rng.chance(p_touch) {
                self.state.players[id].touch_cooldown_until = now + 0.5;
                return;
            }
            // Law 11: an attempt to play a pass is a deliberate play; a blocked kick is a deflection.
            if !blocked || (p.is_gk() && !shot) {
                self.played(id);
            } else if !p.is_gk() {
                // A kick hitting the body square on rebounds; one clipping a leg is deflected and keeps most of its
                // speed (often out of play for a corner or throw-in).
                let d_in = ball.vel.truncate().normalize_or(Vec2::X);
                let speed = ball.vel.truncate().length();
                let (dir, f) = if d < pc.body_radius {
                    (Vec2::from_angle(self.rng.range(-0.8, 0.8)).rotate(-d_in), self.rng.range(0.15, 0.35))
                } else {
                    let turn = self.rng.range(0.25, 1.2) * if self.rng.chance(0.5) { 1.0 } else { -1.0 };
                    (Vec2::from_angle(turn).rotate(d_in), self.rng.range(0.4, 0.75))
                };
                let mut b = self.state.ball;
                b.pos.z = b.pos.z.max(BALL_RADIUS);
                b.kick((dir * speed * f).extend(self.rng.range(0.0, 3.0)), Vec3::ZERO);
                self.state.ball = b;
                self.pending_pass = None;
                self.clear_flight();
                self.state.last_touch = Some(id);
                self.state.players[id].touch_cooldown_until = now + 0.4;
                self.push(EventKind::Block { by: id });
                self.update_brains(now);
                return;
            }
        }
        // ATT-06: first-time play decided when the pass was released.
        if intended && let Some(plan) = self.state.players[id].first_time {
            self.note_touch(id);
            self.gain_control(id, 1.0, now);
            self.execute_kick(id, plan, true, now);
            return;
        }
        let pressure = pressure_at(&self.state, side, ball.ground_pos());
        let p = &self.state.players[id];
        let p_ctrl = control_prob(&p.attrs, p.confidence, p.energy, rel_speed, eff_height, pressure, &self.cfg.duel);
        let p_vel = p.vel;
        let dir = if intended { p.receive_dir } else { receive_dir(&self.state, &self.cfg, id, ball.ground_pos(), pressure > 0.4) };
        let dir = touch_cone(dir, p_vel, self.state.players[id].kin.lateral_accel * self.cfg.player.touch_follow_s);
        self.note_touch(id);
        let draw = self.rng.uniform();
        if draw < p_ctrl {
            self.gain_control(id, p_ctrl, now);
            let drib = norm(self.state.players[id].attrs.dribbling);
            let p = &mut self.state.players[id];
            // The controlling touch goes with him at his own speed along it, so a player arriving at speed does not
            // run past it.
            let speed = (0.5 * p.kin.top_speed * (0.6 + 0.4 * drib)).max(p_vel.dot(dir));
            p.intent = Intent::Carry { dir, speed };
            // Settle, then decide (decision interval scales with composure; ATT-05: faster after a recent scan with
            // nobody close, slower on a stale picture).
            let pc = &self.cfg.player;
            let scanned = scan::settle_factor(p, pressure > 0.4, now, &self.cfg.ai);
            p.next_decision = now + scanned * (pc.settle_decision_s + pc.settle_composure_s * (1.0 - norm(p.attrs.composure)));
            self.dribble_touch(id, dir.normalize_or(Vec2::X), 0.8, now);
            self.state.players[id].touch_cooldown_until = now + 0.15;
        } else {
            // Heavy touch: the ball comes off the foot, back off the shin or squirming past, at a speed that grows with
            // how badly the touch failed. Most stay within a stride or two; only a really poor touch runs away.
            let cfg = &self.cfg.duel;
            let rel = ball.ground_vel() - p_vel;
            let badness = ((draw - p_ctrl) / (1.0 - p_ctrl).max(1e-3)).clamp(0.0, 1.0);
            let skill = norm(self.state.players[id].attrs.first_touch);
            let k = (cfg.heavy_touch_rebound_min + cfg.heavy_touch_rebound_range * badness.powi(3)) * (1.3 - 0.6 * skill);
            let d_in = rel.normalize_or(Vec2::X);
            // A fast opponents' ball he only stuck a foot out to is deflected rather than miscontrolled: it keeps
            // `deflect_keep` of its speed and turns off its line (often out of play near the lines).
            let v = if opponents_ball && !intended && rel.length() > cfg.deflect_min_speed {
                let turn = self.rng.range(0.15, 1.0) * if self.rng.chance(0.5) { 1.0 } else { -1.0 };
                p_vel + Vec2::from_angle(turn).rotate(d_in) * rel.length() * self.rng.range(cfg.deflect_keep[0], cfg.deflect_keep[1])
            } else {
                let out = if self.rng.chance(0.5) {
                    Vec2::from_angle(self.rng.range(-1.0, 1.0)).rotate(-d_in)
                } else {
                    Vec2::from_angle(self.rng.range(-0.8, 0.8)).rotate(d_in)
                };
                p_vel + out * rel.length() * k
            };
            let mut b = self.state.ball;
            b.pos.z = b.pos.z.max(BALL_RADIUS);
            b.kick(v.extend(if ball.height() > 0.3 { 1.0 } else { 0.0 }), Vec3::ZERO);
            self.state.ball = b;
            self.clear_flight();
            if self.state.control.is_some_and(|c| side_of(c) != side) {
                self.state.control = None;
            }
            self.state.last_touch = Some(id);
            self.state.players[id].touch_cooldown_until = now + 0.4;
            self.push(EventKind::Miscontrol { by: id });
            self.update_brains(now);
        }
    }

    /// `contested`: won in an aerial duel just now.
    fn header(&mut self, id: PlayerId, contested: bool, now: f32) {
        let p = &self.state.players[id];
        let (side, p_pos) = (p.side, p.pos);
        let fr = self.state.frame(side);
        let pos_t = fr.to_team(p.pos);
        let skill = norm(p.attrs.heading);
        // A header won in a duel is often not a controlled one: a share `contested_header_loose × (1 − heading)` is
        // headed away loose instead of to a team-mate (StatsBomb: the winner's first action is a clearance 35 % of
        // the time against a pass; a headed clearance is next controlled by the clearing side 20 % of the time).
        let loose = contested && self.rng.chance((self.cfg.duel.contested_header_loose * (1.0 - skill)).clamp(0.0, 1.0));
        // A header at goal needs a delivered ball (a teammate's pass or cross), otherwise it is a clearance / knock-on.
        let delivered = self.state.flight.is_some_and(|f| side_of(f.kicker) == side && f.kind != KickKind::Shot);
        let attacking = delivered && pos_t.x > HALF_LENGTH - PENALTY_AREA_DEPTH && pos_t.y.abs() < PENALTY_AREA_HALF_WIDTH;
        // A header that is not at goal is a headed pass to the best-placed teammate within its range: one with a
        // few metres of space, the freer and the nearer to 15 m the better, forward preferred (a knock-down or
        // flick-on; a defender heads to a team-mate when he has one). With nobody to head to it is a clearance, long
        // and wide. Heading power: a 35° lob of `v² sin 2θ / g` metres, `v` up to 12–20 m/s by skill.
        let v_max = 12.0 + 8.0 * skill;
        let range = v_max * v_max * 0.94 / 9.81;
        let knock = if attacking || loose {
            None
        } else {
            self.state
                .squad(side)
                .iter()
                .filter(|m| m.id != id && !m.is_gk())
                .filter_map(|m| {
                    let d = (m.pos - p_pos).length();
                    if !(5.0..=range).contains(&d) {
                        return None;
                    }
                    let free = self.state.squad(side.opponent()).iter().map(|o| (o.pos - m.pos).length()).fold(f32::MAX, f32::min);
                    (free >= 4.0).then(|| (free.min(10.0) + 0.1 * (fr.to_team(m.pos).x - pos_t.x) - 0.1 * (d - 15.0).abs(), m.id, fr.to_team(m.pos), d))
                })
                .max_by(|a, b| a.0.total_cmp(&b.0))
        };
        let (aim_t, speed, vz, receiver) = if attacking {
            (Vec2::new(HALF_LENGTH, self.rng.range(-3.0, 3.0)), 9.0 + 7.0 * skill, 0.3, None)
        } else if let Some((_, r, at, d)) = knock {
            let v = (10.4 * d).sqrt().min(v_max);
            (at, 0.82 * v, 0.57 * v, Some(r))
        } else if pos_t.x < 0.0 {
            (Vec2::new(pos_t.x + 30.0, pos_t.y.signum() * 20.0), 9.0 + 6.0 * skill, 5.5, None)
        } else {
            (Vec2::new(pos_t.x + 15.0, pos_t.y * 0.5), 9.0 + 6.0 * skill, 5.5, None)
        };
        let err = self.rng.gauss(0.0, 0.25 * (1.4 - skill) * if contested { 1.5 } else { 1.0 });
        let dir = Vec2::from_angle(err).rotate((fr.to_world(aim_t) - p_pos).normalize_or(Vec2::X));
        let mut b = self.state.ball;
        b.kick((dir * speed).extend(vz), Vec3::ZERO);
        self.state.ball = b;
        self.note_touch(id);
        self.clear_flight();
        self.played(id);
        self.state.control = None;
        self.state.last_touch = Some(id);
        self.state.players[id].touch_cooldown_until = now + 0.4;
        self.push(EventKind::Header { by: id });
        if let Some(r) = receiver {
            // A headed pass: its receiver goes for it and the opponents read it like any other pass.
            self.traj = predict(&self.state.ball, &self.plan, TRAJ_HORIZON, TRAJ_SAMPLE_DT);
            let target = fr.to_world(aim_t);
            let arrival_t = self.traj.samples.iter().find(|s| s.t > 0.3 && s.pos.z - BALL_RADIUS <= self.cfg.player.control_height).map_or(1.0, |s| s.t);
            self.state.flight = Some(BallFlight {
                kicker: id,
                receiver: Some(r),
                kind: KickKind::LoftedPass,
                start_t: now,
                start_pos: p_pos,
                target,
                arrival_t,
                speed,
                xg: 0.0,
                backward: fr.to_team(target).x < pos_t.x - 2.0,
                first_time: true,
                save_attempted: false,
                on_goal: false,
                weak_foot: false,
            });
            self.stats.teams[side.index()].passes += 1;
            self.stats.passes_in_possession[side.index()] += 1;
            self.pending_pass = Some(id);
            self.push(EventKind::Pass { by: id, to: Some(r), kind: KickKind::LoftedPass, length: (target - p_pos).length(), first_time: true, headed: true });
            self.receive_after(r, id, now, now);
            let pc = &self.cfg.player;
            for q in &mut self.state.players {
                q.next_decision = q.next_decision.min(now + pass_reaction(q, 0.0, pc));
            }
            self.update_brains(now);
            self.next_brain = now + self.cfg.ai.team_tick_s;
            return;
        }
        if attacking {
            let xg = 0.08 + 0.12 * skill;
            self.stats.teams[side.index()].shots += 1;
            self.stats.teams[side.index()].xg += xg;
            self.state.flight = Some(BallFlight {
                kicker: id,
                receiver: None,
                kind: KickKind::Shot,
                start_t: now,
                start_pos: p_pos,
                target: fr.to_world(aim_t),
                arrival_t: 0.5,
                speed,
                xg,
                backward: false,
                first_time: true,
                save_attempted: false,
                on_goal: false,
                weak_foot: false,
            });
            self.push(EventKind::Shot { by: id, xg, first_time: true, weak_foot: false });
        }
        self.traj = predict(&self.state.ball, &self.plan, TRAJ_HORIZON, TRAJ_SAMPLE_DT);
        if let Some(f) = self.state.flight.as_mut() {
            f.on_goal = self.traj.on_goal;
        }
    }

    fn gk_contact(&mut self, gk: PlayerId, now: f32) {
        let ball = self.state.ball;
        let speed = ball.speed();
        let f = self.state.flight;
        let shot = f.is_some_and(|f| f.kind == KickKind::Shot) || speed > 15.0;
        let g = &self.state.players[gk];
        if shot {
            if f.is_some_and(|f| f.save_attempted) {
                return;
            }
            let reaction = f.map_or(1.0, |f| now - f.start_t);
            let reach_frac = match g.body {
                Body::Diving { dir, .. } => (dir.length() / self.cfg.player.gk_dive_speed).clamp(0.0, 1.0),
                _ => 0.0,
            };
            let origin = f.map_or(ball.pos.truncate(), |f| f.start_pos);
            let shot_dist = (self.state.frame(g.side).to_team(origin) - TeamFrame::own_goal()).length();
            // SP-04: a keeper who guessed the side of a penalty kick saves `sp04_save_right` × (0.5 + reflexes).
            let p_save = if self.set_shot == Some(RestartKind::Penalty) && self.gk_anticipates == Some(gk) {
                self.cfg.tactics.sp04_save_right * (0.5 + norm(g.attrs.reflexes))
            } else {
                save_prob(g, shot_dist, speed, reach_frac, reaction, &self.cfg.duel)
            };
            if let Some(fl) = self.state.flight.as_mut() {
                fl.save_attempted = true;
            }
            if !self.rng.chance(p_save) {
                self.state.players[gk].touch_cooldown_until = now + 1.0;
                return;
            }
            let held = self.rng.chance(catch_prob(g, speed));
            let shooter = f.map(|f| f.kicker);
            let side = g.side;
            self.stats.teams[side.index()].saves += 1;
            if let Some(f) = f.filter(|f| f.on_goal) {
                let ts = &mut self.stats.teams[side_of(f.kicker).index()];
                ts.on_target += 1;
                if matches!(self.set_shot, Some(RestartKind::FreeKick { .. })) {
                    ts.fk_on_target += 1;
                }
            }
            self.push(EventKind::Save { gk, shooter, held });
            if held {
                self.collect(gk, now);
            } else {
                // Parry: out in front and to the side, or behind the goal line round the post (over the bar if high).
                let fr = self.state.frame(side);
                let mut b = self.state.ball;
                let v = if self.rng.chance(self.cfg.duel.parry_behind) {
                    let (bt, gt) = (fr.to_team(b.pos.truncate()), fr.to_team(self.state.players[gk].pos));
                    let s = if bt.y >= gt.y { 1.0 } else { -1.0 };
                    // Crossing the goal line outside the post.
                    let aim = Vec2::new(-HALF_LENGTH, s * (GOAL_HALF_WIDTH + self.rng.range(0.5, 3.0)));
                    let dir = fr.to_world(aim - bt).normalize_or(Vec2::X);
                    let up = if b.pos.z > 1.8 { self.rng.range(3.0, 6.0) } else { self.rng.range(0.5, 2.5) };
                    (dir * speed * self.rng.range(0.3, 0.5)).extend(up)
                } else {
                    let away = fr.to_world(Vec2::new(1.0, self.rng.range(-1.5, 1.5))).normalize_or(Vec2::X);
                    (away * speed * self.rng.range(0.2, 0.4)).extend(self.rng.range(0.5, 3.0))
                };
                b.kick(v, Vec3::ZERO);
                self.state.ball = b;
                self.pending_pass = None;
                self.parry = shooter.map(|sh| (sh, now, self.set_shot));
                self.clear_flight();
                self.state.last_touch = Some(gk);
                self.state.players[gk].touch_cooldown_until = now + 0.6;
                self.update_brains(now);
            }
        } else {
            let p_hold = (0.75 + 0.25 * norm(g.attrs.handling)).min(0.98);
            if self.rng.chance(p_hold) {
                self.note_touch(gk);
                self.collect(gk, now);
            } else {
                self.foot_contact(gk, f32::MAX, now);
            }
        }
    }

    fn collect(&mut self, gk: PlayerId, now: f32) {
        self.gain_control(gk, 1.0, now);
        let hold = self.cfg.ai.gk_hold_s;
        let p = &mut self.state.players[gk];
        p.intent = Intent::GkHold { release_at: now + hold };
        p.body = Body::Normal;
        p.target = p.pos;
        self.state.ball.mode = BallMode::Held;
        self.update_brains(now);
    }

    /// IND-03: a defender within reach of the ball tackles when the odds are good (don't dive in); with a small
    /// aggression-driven hazard otherwise. Returns true if a tackle happened this tick.
    /// BOX-03: is the carrier `c` in shooting range of defender `d`'s goal, with a teammate of `d` goal-side of him
    /// within `cover_radius`?
    fn covered_near_goal(&self, d: PlayerId, c: PlayerId, cover_radius: f32) -> bool {
        let car = self.state.frame(self.state.players[d].side).to_team(self.state.players[c].pos);
        positioning::in_shooting_range(car, &self.cfg.tactics) && self.covered(d, c, cover_radius)
    }

    /// DEF-02: does a teammate of defender `d` cover him against carrier `c`: goal-side of the carrier and within
    /// `cover_radius` of him?
    fn covered(&self, d: PlayerId, c: PlayerId, cover_radius: f32) -> bool {
        let side = self.state.players[d].side;
        let fr = self.state.frame(side);
        let car = fr.to_team(self.state.players[c].pos);
        let goal = TeamFrame::own_goal();
        self.state.squad(side).iter().any(|m| {
            let q = fr.to_team(m.pos);
            m.id != d && !m.is_gk() && (q - car).length() < cover_radius && (goal - q).length() < (goal - car).length()
        })
    }

    fn tackles(&mut self, now: f32) -> bool {
        let Some(c) = self.state.control else { return false };
        if matches!(self.state.players[c].intent, Intent::GkHold { .. }) {
            return false;
        }
        let ball = self.state.ball;
        if ball.height() > 1.0 {
            return false;
        }
        let cside = self.state.players[c].side;
        // When the ball came nearer to each defender than to the carrier (seen only after his reaction time, below).
        let (bp, car_pos) = (ball.ground_pos(), self.state.players[c].pos);
        for p in &mut self.state.players[crate::match_state::team_range(cside.opponent())] {
            if (bp - p.pos).length() < (bp - car_pos).length() {
                p.ball_nearer_since.get_or_insert(now);
            } else {
                p.ball_nearer_since = None;
            }
        }
        let mut cand: Vec<(f32, PlayerId)> = self
            .state
            .squad(cside.opponent())
            .iter()
            .filter(|d| d.can_act(now) && d.body == Body::Normal || matches!(d.body, Body::Kicking { .. }) && d.can_act(now))
            .map(|d| ((d.pos - ball.ground_pos()).length(), d.id))
            .filter(|&(dist, id)| dist <= self.state.players[id].kin.tackle_reach)
            .collect();
        cand.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let dt = self.cfg.timing.dt;
        for (_, d) in cand {
            let def = &self.state.players[d];
            let car = &self.state.players[c];
            let mv = car.vel.normalize_or(car.facing);
            let from_behind = (def.pos - car.pos).dot(mv) < -0.3;
            let mut p_win = tackle_win_prob(def, car, from_behind, &self.cfg.duel);
            // Between dribble touches the ball may be nearer the defender than the carrier: easy to nick, and a ball
            // clearly nearer to him (by `tackle_loose_margin`) is contested whatever the duel odds, unless he comes
            // from behind (a lunge through the carrier). He acts on it once he has seen it, after his reaction time.
            let (d_def, d_car) = ((bp - def.pos).length(), (bp - car.pos).length());
            let seen = def.ball_nearer_since.is_some_and(|s| now - s >= self.cfg.duel.tackle_cue_reaction * def.kin.reaction);
            if d_def < d_car && seen {
                p_win = (p_win + 0.3).min(0.95);
            }
            let loose = !from_behind && d_def + self.cfg.duel.tackle_loose_margin < d_car && seen;
            // Aggressive defenders sometimes dive in against the odds; not once cautioned (Law 12.3).
            let hazard = if def.cautioned { 0.0 } else { 0.2 * norm(def.attrs.aggression) * dt };
            // BOX-01: a carrier setting up a shot is challenged whatever the odds, to stop the shot being got off.
            let shooting = matches!(car.intent, Intent::SetUp { plan, .. } if plan.kind == KickKind::Shot);
            // BOX-03: near goal, a covered defender engages in the duel (2v1) instead of containing.
            let box03 = applied_defending(&self.state.team(def.side).tactics, &def.attrs, def.instruction_familiarity(false), &self.cfg.tactics);
            let min_p = if self.covered_near_goal(d, c, box03.cover_radius) {
                box03.tackle_min_p
            } else {
                self.cfg.tactics.ind03_tackle_min_p
            } + if def.cautioned { self.cfg.rules.caution_margin } else { 0.0 };
            // IND-03: containing only works while he stays between the carrier and his goal. A carrier about to get
            // past him (goal-side of him within `ind03_last_ditch_s`) is challenged whatever the odds, unless a
            // teammate covers behind him (DEF-02: the cover takes the carrier if the first defender is beaten).
            let fr = self.state.frame(def.side);
            let (car_t, vel_t, def_t) = (fr.to_team(car.pos), fr.to_team(car.vel), fr.to_team(def.pos));
            let last_ditch = !from_behind
                && vel_t.x < -1.0
                && car_t.x >= def_t.x - 0.3
                && car_t.x + vel_t.x * self.cfg.tactics.ind03_last_ditch_s < def_t.x - 0.3
                && !self.covered(d, c, self.cfg.tactics.box03_cover_radius);
            let commit = shooting || last_ditch || loose || p_win >= min_p || self.rng.chance(hazard);
            if !commit {
                continue;
            }
            let outcome = resolve_tackle(p_win, &self.cfg.duel, &mut self.rng);
            let dur = self.cfg.duel.tackle_duration_s;
            self.state.players[d].body = Body::Tackling { until: now + dur };
            // Law 12: a foul stops play, unless the referee plays advantage (the carrier got past: the outcome
            // stands). A foul is not a tackle in the statistics.
            let foul = self.challenge_foul(d, c, outcome, from_behind, now);
            if foul == Some(true) {
                return true;
            }
            let side = self.state.players[d].side;
            if foul.is_some() {
                let rec = self.cfg.duel.beaten_recovery_s;
                self.state.players[d].body = Body::Recovering { until: now + rec };
                self.update_brains(now);
                return true;
            }
            self.stats.teams[side.index()].tackles += 1;
            let reason = if shooting {
                0
            } else if last_ditch {
                1
            } else if loose {
                2
            } else if p_win >= min_p {
                if d_def < d_car { 3 } else { 4 }
            } else {
                5
            };
            self.stats.teams[side.index()].tackle_reasons[reason] += 1;
            self.push(EventKind::Tackle { by: d, on: c, outcome });
            match outcome {
                TackleOutcome::Won => {
                    self.stats.teams[side.index()].tackles_won += 1;
                    self.state.players[c].body = Body::Recovering { until: now + 0.5 };
                    self.state.players[c].intent = Intent::Position;
                    self.gain_control(d, 0.8, now);
                    let p = &mut self.state.players[d];
                    p.intent = Intent::Hold;
                    p.next_decision = now + dur;
                }
                TackleOutcome::Deflected => {
                    self.stats.teams[side.index()].tackles_won += 1;
                    let a = self.rng.range(0.0, std::f32::consts::TAU);
                    let mut b = self.state.ball;
                    b.pos.z = BALL_RADIUS;
                    b.kick((Vec2::from_angle(a) * self.rng.range(3.0, 8.0)).extend(0.0), Vec3::ZERO);
                    self.state.ball = b;
                    self.state.control = None;
                    self.state.last_touch = Some(d);
                    self.state.players[c].intent = Intent::Position;
                    self.state.players[c].touch_cooldown_until = now + 0.3;
                    self.state.players[d].touch_cooldown_until = now + 0.3;
                }
                TackleOutcome::Beaten => {
                    let rec = self.cfg.duel.beaten_recovery_s;
                    self.state.players[d].body = Body::Recovering { until: now + rec };
                }
            }
            self.update_brains(now);
            return true;
        }
        false
    }

    // ------------------------------------------------------------------ referee

    fn referee(&mut self, now: f32) {
        // Play was stopped this tick (an offside or a foul): what the ball did after does not count.
        if !self.state.is_live() {
            self.ball_events.clear();
            return;
        }
        let events = std::mem::take(&mut self.ball_events);
        for e in &events {
            match *e {
                BallEvent::Goal { world_x_sign } => {
                    let conceding = referee::defending_side(&self.state, world_x_sign);
                    let scoring = conceding.opponent();
                    self.state.score[scoring.index()] += 1;
                    let mut last = self.state.last_touch;
                    // A shot the keeper parried into his own goal is the shooter's.
                    let parried = self.parry.filter(|&(sh, t, _)| {
                        side_of(sh) == scoring && now - t < 3.0 && last.is_some_and(|l| side_of(l) == conceding && self.state.players[l].is_gk())
                    });
                    if let Some((sh, ..)) = parried {
                        last = Some(sh);
                    }
                    let own_goal = last.is_some_and(|l| side_of(l) == conceding);
                    let first_time = self.state.flight.is_some_and(|f| f.first_time);
                    let passes = self.stats.passes_in_possession[scoring.index()];
                    let ts = &mut self.stats.teams[scoring.index()];
                    ts.goals += 1;
                    let shot = self.state.flight.is_some_and(|f| f.kind == KickKind::Shot && side_of(f.kicker) == scoring);
                    if shot || parried.is_some() {
                        if shot {
                            ts.on_target += 1;
                        }
                        match if shot { self.set_shot } else { parried.and_then(|p| p.2) } {
                            Some(RestartKind::Penalty) => ts.penalty_goals += 1,
                            Some(RestartKind::FreeKick { .. }) => {
                                ts.fk_on_target += u32::from(shot);
                                ts.fk_goals += 1;
                            }
                            _ => {}
                        }
                    }
                    if now - self.corner_at[scoring.index()] < 10.0 {
                        ts.corner_goals += 1;
                    }
                    let cal = &mut self.stats.cal;
                    cal.goals += 1;
                    cal.goals_by_period[crate::stats::period(&self.state)] += 1;
                    if first_time {
                        cal.goals_one_touch += 1;
                    }
                    if (1..=5).contains(&passes) {
                        cal.goals_short_moves += 1;
                    }
                    if now - self.last_scored[scoring.index()] < crate::stats::AFTER_GOAL_S {
                        cal.goals_after_scoring[scoring.index()] += 1;
                    }
                    self.stats.value_goal(scoring);
                    self.last_scored[scoring.index()] = now;
                    self.last_conceded[conceding.index()] = now;
                    self.pending_pass = None;
                    self.clear_flight();
                    self.state.control = None;
                    self.state.phase = PlayPhase::GoalScored { resume_at: now + 4.0, kickoff_side: conceding };
                    self.push(EventKind::Goal { side: scoring, scorer: last, own_goal, first_time, passes_in_move: passes });
                    self.allow(self.cfg.rules.added_goal_s);
                    break;
                }
                BallEvent::Out { exit, over_goal_line } => {
                    if let Some(k) = self.pending_pass.take() {
                        self.pass_outcome(k, false);
                    }
                    let (kind, side, spot) = referee::restart_for_out(&self.state, exit, over_goal_line);
                    self.start_restart(kind, side, spot);
                    break;
                }
                _ => {}
            }
        }
        self.ball_events = events;
    }
}

/// ATT-05: a receiver who will play first time opens his body half-way between the incoming ball and the kick.
fn open_body(p: &Player, ball: Vec2) -> Option<Vec2> {
    let k = p.first_time?;
    let to_ball = (ball - p.pos).normalize_or_zero();
    let to_kick = (k.target - p.pos).normalize_or_zero();
    Some(p.pos + (to_ball + to_kick).normalize_or(to_ball) * 10.0)
}

/// The controlling touch goes only where the player can turn his run to: at most the angle whose sideways velocity
/// change `turn` (lateral acceleration × `touch_follow_s`) covers, any direction when he is slower than that. A wanted
/// direction outside is taken to the edge: he takes the ball in his stride and turns with it afterwards.
fn touch_cone(dir: Vec2, vel: Vec2, turn: f32) -> Vec2 {
    let speed = vel.length();
    if speed <= turn {
        return dir;
    }
    let max = (turn / speed).asin();
    let run = vel / speed;
    let a = run.angle_to(dir);
    if a.abs() <= max { dir } else { Vec2::from_angle(max.copysign(a)).rotate(run) }
}
