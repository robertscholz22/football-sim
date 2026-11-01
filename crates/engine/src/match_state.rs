//! The complete dynamic state of a match.

use glam::Vec2;

use crate::ai::brain::Beliefs;
use crate::ball::Ball;
use crate::pitch::{PitchConditions, Side, TeamFrame};
use crate::player::{KickKind, Player, PlayerId};
use crate::team::{Formation, PlayerData, Practice, ResolvedTactics, TeamInstructions};

pub const PLAYERS_PER_TEAM: usize = 11;
pub const HALF_DURATION_DEFAULT: f32 = 2700.0;

pub fn side_of(id: PlayerId) -> Side {
    Side::from_index(id / PLAYERS_PER_TEAM)
}

pub fn team_range(side: Side) -> std::ops::Range<PlayerId> {
    let s = side.index() * PLAYERS_PER_TEAM;
    s..s + PLAYERS_PER_TEAM
}

#[derive(Clone, Debug)]
pub struct Team {
    pub side: Side,
    pub name: String,
    pub short: String,
    pub color: [u8; 3],
    pub formation: Formation,
    pub instructions: TeamInstructions,
    /// Layer 1 resolved with layer 2 (`team::resolve`). Recomputed when instructions change.
    pub tactics: ResolvedTactics,
    pub frame: TeamFrame,
    /// How practised the team is in its formation and concept (docs/player-brain.md section 7).
    pub practice: Practice,
    /// How well the players in slots `i` and `j` know each other (`Partners::between`).
    pub partners: [[f32; PLAYERS_PER_TEAM]; PLAYERS_PER_TEAM],
    /// Law 3: the substitutes still on the bench, the substitutions made and the stoppages used for them.
    pub bench: Vec<PlayerData>,
    pub subs_used: u32,
    pub sub_windows: u32,
}

/// A kicked ball that has not been touched since.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BallFlight {
    pub kicker: PlayerId,
    pub receiver: Option<PlayerId>,
    pub kind: KickKind,
    pub start_t: f32,
    pub start_pos: Vec2,
    pub target: Vec2,
    pub arrival_t: f32,
    pub speed: f32,
    pub xg: f32,
    /// Backward pass in the kicker's team frame (pressing trigger PRS-01).
    pub backward: bool,
    /// Played first time (ATT-06).
    pub first_time: bool,
    pub save_attempted: bool,
    /// The kicked ball was heading into the goal (on target), from the trajectory predicted at release.
    pub on_goal: bool,
    /// Struck with the kicker's weaker foot.
    pub weak_foot: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestartKind {
    Kickoff,
    ThrowIn,
    GoalKick,
    Corner,
    /// Law 13: after a foul (direct) or an offside (indirect).
    FreeKick { direct: bool },
    /// Law 14.
    Penalty,
}

impl RestartKind {
    /// Law 11.3: no offside offence from a goal kick, a throw-in or a corner kick.
    pub fn offside_exempt(self) -> bool {
        matches!(self, Self::GoalKick | Self::ThrowIn | Self::Corner)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PlayPhase {
    Live,
    /// Players move to their restart positions; the taker restarts once at the spot.
    Restart { kind: RestartKind, side: Side, spot: Vec2, taker: PlayerId, earliest: f32, deadline: f32 },
    /// Celebration after a goal, then kickoff.
    GoalScored { resume_at: f32, kickoff_side: Side },
    FullTime,
}

#[derive(Clone, Debug)]
pub struct MatchState {
    /// Engine time in seconds since kick-off: regular and added time of both halves (`clock` gives the match clock).
    pub time: f32,
    pub half: u8,
    pub half_length: f32,
    /// Engine time when the current half kicked off.
    pub half_start: f32,
    /// Law 7.3: the playing time lost in each half that the referee allows for (s) …
    pub lost: [f32; 2],
    /// … and the added time, announced at the end of the half's regular time as the time lost so far in whole minutes
    /// and extended by what is lost after that (s).
    pub added: [Option<f32>; 2],
    pub ball: Ball,
    /// 0..11 home (slot order), 11..22 away.
    pub players: Vec<Player>,
    pub teams: [Team; 2],
    pub score: [u32; 2],
    /// Team that last had the ball under control.
    pub possession: Side,
    /// Player currently in control (dribbling / ball at feet / holding).
    pub control: Option<PlayerId>,
    pub last_touch: Option<PlayerId>,
    pub flight: Option<BallFlight>,
    pub phase: PlayPhase,
    pub conditions: PitchConditions,
    /// Quality (0..1) of the last controlling touch (pressing trigger PRS-01).
    pub last_touch_quality: f32,
    /// Time of the last change of possession.
    pub possession_since: f32,
    /// Each team's situation as observers see it (`team_view`).
    pub views: [Beliefs; 2],
}

impl MatchState {
    pub fn team(&self, side: Side) -> &Team {
        &self.teams[side.index()]
    }

    pub fn frame(&self, side: Side) -> TeamFrame {
        self.teams[side.index()].frame
    }

    pub fn squad(&self, side: Side) -> &[Player] {
        &self.players[team_range(side)]
    }

    /// The team's situation as observers (stats, viewer, benches, tests) report it: the phase and blocks of its first
    /// player's beliefs (perception is exact, so all players of a side agree on them) with every duty as its player
    /// claims it himself: since reading team-mates takes time (phase F), a player's belief about a team-mate's duty
    /// can be wrong. The opponents read the duties here too (`ai::offer`): they see what a defender does.
    pub fn team_view(&self, side: Side) -> &Beliefs {
        &self.views[side.index()]
    }

    /// Ball ground position in the team frame of `side`.
    pub fn ball_in(&self, side: Side) -> Vec2 {
        self.frame(side).to_team(self.ball.ground_pos())
    }

    /// Player position in the team frame of `side`.
    pub fn pos_in(&self, id: PlayerId, side: Side) -> Vec2 {
        self.frame(side).to_team(self.players[id].pos)
    }

    /// Seconds played in the current half.
    pub fn half_elapsed(&self) -> f32 {
        self.time - self.half_start
    }

    /// The match clock (s) as shown: regular time, held at 45:00 / 90:00 during added time …
    pub fn clock(&self) -> f32 {
        (self.half as f32 - 1.0) * self.half_length + self.half_elapsed().min(self.half_length)
    }

    /// … and the added time played so far (s).
    pub fn stoppage(&self) -> f32 {
        (self.half_elapsed() - self.half_length).max(0.0)
    }

    pub fn is_live(&self) -> bool {
        self.phase == PlayPhase::Live
    }

    pub fn carrier(&self) -> Option<&Player> {
        self.control.map(|id| &self.players[id])
    }
}
