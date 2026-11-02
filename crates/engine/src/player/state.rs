//! Dynamic per-player state.

use glam::Vec2;

use super::{Attributes, Foot, Kinematics};
use crate::ai::brain::Beliefs;
use crate::ai::movement::Plan;
use crate::ai::scan::Scan;
use crate::ball::KickSpec;
use crate::pitch::Side;
use crate::team::{PlayerInstructions, Role};

/// Index into `MatchState::players` (0..11 home, 11..22 away).
pub type PlayerId = usize;

/// An intended kick. Solved from the actual ball position when executed, then perturbed by execution error.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KickPlan {
    pub spec: KickSpec,
    pub kind: KickKind,
    pub receiver: Option<PlayerId>,
    /// Ground target (world frame).
    pub target: Vec2,
    /// Expected flight time to the target.
    pub arrival_t: f32,
    pub xg: f32,
    /// Planned initial ball speed (m/s): decides how square the body must be to strike it (`execution::setup_angle`).
    pub speed: f32,
    /// The foot chosen for the kick (shots); `None`: whichever the body position gives (`execution::natural_foot`).
    pub foot: Option<Foot>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KickKind {
    GroundPass,
    LoftedPass,
    Shot,
    Clearance,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Body {
    Normal,
    /// Kicking motion / follow-through.
    Kicking { until: f32 },
    Tackling { until: f32 },
    /// Beaten or off balance: can't act, moves sluggishly.
    Recovering { until: f32 },
    /// Goalkeeper dive towards a point.
    Diving { until: f32, dir: Vec2 },
}

/// What the player is currently trying to do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Intent {
    /// Off-ball positioning towards `Player::target`.
    Position,
    /// Going for a loose ball.
    Chase,
    /// Intended receiver of a pass: takes it at `point` at time `t`; `meet`: under pressure he comes to meet it
    /// (ATT-24) instead of waiting where it was aimed.
    Receive { point: Vec2, t: f32, meet: bool },
    /// Ball carrier moving with the ball.
    Carry { dir: Vec2, speed: f32 },
    /// Ball carrier shielding / holding.
    Hold,
    /// Ball carrier setting up a kick: braking and turning his body with the ball at his feet until the kick can be
    /// struck (`execution::set_up`), or struck anyway at `until`; set up since `since` (team-mates read it from then).
    SetUp { plan: KickPlan, until: f32, since: f32 },
    /// Goalkeeper holding the ball in his hands.
    GkHold { release_at: f32 },
}

/// Explainability data for the viewer.
#[derive(Clone, Debug, Default)]
pub struct PlayerDebug {
    /// Principle IDs that produced the current target or decision, e.g. `DEF-02`.
    pub rules: Vec<&'static str>,
    /// Layer-1 anchor (world frame).
    pub anchor: Vec2,
    /// Short description of the current role / decision.
    pub label: String,
    /// On-ball decision breakdown: (description, optimum U, instruction bias, personality bias, probability).
    pub options: Vec<(String, f32, f32, f32, f32)>,
    /// Success probability the on-ball AI gave its chosen option.
    pub chosen_p: f32,
}

#[derive(Clone, Debug)]
pub struct Player {
    pub id: PlayerId,
    pub side: Side,
    pub slot: usize,
    /// His slot's role in possession and out of possession (docs/player-brain.md section 5.1).
    pub role: Role,
    pub out_role: Role,
    pub name: String,
    pub number: u8,
    pub attrs: Attributes,
    pub kin: Kinematics,
    pub instructions: PlayerInstructions,

    pub pos: Vec2,
    pub vel: Vec2,
    /// Unit vector of body orientation.
    pub facing: Vec2,
    pub body: Body,

    /// In-game confidence 0..1 (`player::confidence`): moved by events, returns to `base_confidence`, the pre-match
    /// level.
    pub confidence: f32,
    pub base_confidence: f32,
    /// Familiarity 0..1 with his role in possession and with his role out of possession (section 5.5).
    pub familiarity: [f32; 2],
    /// Remaining energy 0..1 (fatigue, `player::fatigue`): short-term, recovers towards `reserve`.
    pub energy: f32,
    /// Long-term energy reserve 0..1: falls with the running over the match and does not recover in it.
    pub reserve: f32,
    /// How practised his team is in its tactical concept 0..1 (`Team::practice`): he applies the instructions as far
    /// as he understands them (`instruction_familiarity`).
    pub concept: f32,

    /// His reading of the team's situation (player brain).
    pub beliefs: Beliefs,
    /// The off-ball movement he holds (movement model).
    pub plan: Plan,
    pub intent: Intent,
    /// Movement target (world frame).
    pub target: Vec2,
    /// Desired fraction of top speed (0..1).
    pub urgency: f32,
    /// Pacing (M6): in positional play he closes the gap to his target in this time (s), at most at `urgency` and at
    /// `pace_speed` (m/s); 0 = as fast as `urgency` allows.
    pub pace_s: f32,
    pub pace_speed: f32,
    /// Pacing: the floor of a paced movement's speed (m/s, `AiConfig::pace_min_speed`).
    pub pace_min: f32,
    /// Brake to arrive at the target (false = run through it).
    pub brake: bool,
    /// Velocity of a moving target (pursuit, e.g. a contain point goal-side of a running carrier): the target moves
    /// on with it between decisions, and the player arrives at it relative to this velocity.
    pub target_vel: Vec2,
    /// Point the player looks at when not running.
    pub look_at: Vec2,
    pub next_decision: f32,
    /// Layer-3 positional drift (Ornstein–Uhlenbeck state, team frame metres).
    pub drift: Vec2,
    /// Layer 3: the positioning point the player has committed to, as an offset from his anchor (team frame), and
    /// the kind of positioning it belongs to. A new point has to beat it by a switching cost.
    pub commit: Option<(Vec2, &'static str)>,
    pub touch_cooldown_until: f32,
    /// Since when the ball (carried by an opponent) has been nearer to him than to the carrier.
    pub ball_nearer_since: Option<f32>,
    /// Action decided before the ball arrives (ATT-06: first-time play under pressure).
    pub first_time: Option<KickPlan>,
    /// Where he planned to play it (world).
    pub first_time_at: Vec2,
    /// Direction (world) of the controlling first touch when receiving.
    pub receive_dir: Vec2,
    /// ATT-05 scanning (M6, `ai::scan`): his scans and snapshot of the opponents; the scans in the 10 s before he
    /// last gained control (statistics).
    pub scan: Scan,
    pub recv_scans: usize,
    /// Calls (M6, `reading::call_rate`): the calls he has in hand and when they were last topped up.
    pub calls: f32,
    pub calls_at: f32,
    /// Law 12 (M7): cautioned; sent off (he has left the pitch and takes no further part).
    pub cautioned: bool,
    pub sent_off: bool,
    /// Law 3 (M7): came on as a substitute.
    pub substitute: bool,
    /// When the player gained control of the ball (for time-on-ball rules).
    pub control_since: f32,
    /// When the player last made an on-ball decision; before `control_since` he is still settling the ball.
    pub last_decision: f32,
    /// Seed of the layer-3 decision noise for the current spell on the ball (`on_ball::decide`).
    pub decision_seed: u64,
    pub distance_covered: f32,
    pub debug: PlayerDebug,
}

impl Player {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: PlayerId,
        side: Side,
        slot: usize,
        roles: [Role; 2],
        name: String,
        number: u8,
        attrs: Attributes,
        kin: Kinematics,
        instructions: PlayerInstructions,
        confidence: f32,
        familiarity: [f32; 2],
    ) -> Self {
        Self {
            id,
            side,
            slot,
            role: roles[0],
            out_role: roles[1],
            name,
            number,
            attrs,
            kin,
            instructions,
            pos: Vec2::ZERO,
            vel: Vec2::ZERO,
            facing: Vec2::X,
            body: Body::Normal,
            confidence,
            base_confidence: confidence,
            familiarity,
            energy: 1.0,
            reserve: 1.0,
            concept: 1.0,
            beliefs: Beliefs::default(),
            plan: Plan::default(),
            intent: Intent::Position,
            target: Vec2::ZERO,
            urgency: 0.5,
            pace_s: 0.0,
            pace_speed: f32::MAX,
            pace_min: 0.0,
            brake: true,
            target_vel: Vec2::ZERO,
            look_at: Vec2::ZERO,
            next_decision: 0.0,
            drift: Vec2::ZERO,
            commit: None,
            touch_cooldown_until: 0.0,
            ball_nearer_since: None,
            first_time: None,
            first_time_at: Vec2::ZERO,
            receive_dir: Vec2::ZERO,
            // Staggered so that the players do not all scan on the same tick.
            scan: Scan { next: 0.07 * id as f32, ..Scan::default() },
            recv_scans: 0,
            calls: 1.0,
            calls_at: 0.0,
            cautioned: false,
            sent_off: false,
            substitute: false,
            control_since: 0.0,
            last_decision: f32::MIN,
            decision_seed: 0,
            distance_covered: 0.0,
            debug: PlayerDebug::default(),
        }
    }

    pub fn is_gk(&self) -> bool {
        self.role == Role::GK
    }

    /// His role in the phase.
    pub fn role_in(&self, in_possession: bool) -> Role {
        if in_possession { self.role } else { self.out_role }
    }

    /// His familiarity with the role of the phase: a #10 who has never pressed as a second striker is weaker out of
    /// possession (section 5.5).
    pub fn familiarity(&self, in_possession: bool) -> f32 {
        self.familiarity[usize::from(!in_possession)]
    }

    /// How familiar he is with the instructions of his role in the phase (docs/player-brain.md section 7): his role
    /// familiarity × his team's practice of the concept. Adherence to instructions grows with it (`team::adherence`).
    pub fn instruction_familiarity(&self, in_possession: bool) -> f32 {
        self.familiarity(in_possession) * self.concept
    }

    /// Highest ball (bottom above the ground, m) he can still head.
    pub fn header_height(&self) -> f32 {
        self.kin.jump_reach - 0.25
    }

    pub fn can_act(&self, now: f32) -> bool {
        match self.body {
            Body::Normal => true,
            Body::Kicking { until } | Body::Tackling { until } | Body::Recovering { until } | Body::Diving { until, .. } => {
                now >= until
            }
        }
    }

    pub fn speed(&self) -> f32 {
        self.vel.length()
    }
}
