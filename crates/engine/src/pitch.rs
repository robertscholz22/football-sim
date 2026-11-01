//! Pitch geometry, team-relative frames and playing-surface conditions.
//!
//! World frame: origin at the centre spot, x along the length (home attacks +x in the first half),
//! y across the width, z up. Units are metres.

use glam::{Vec2, Vec3};
use serde::{Deserialize, Serialize};

pub const LENGTH: f32 = 105.0;
pub const WIDTH: f32 = 68.0;
pub const HALF_LENGTH: f32 = LENGTH / 2.0;
pub const HALF_WIDTH: f32 = WIDTH / 2.0;

/// Inner edge of the posts (goal width 7.32 m).
pub const GOAL_HALF_WIDTH: f32 = 3.66;
/// Underside of the crossbar.
pub const GOAL_HEIGHT: f32 = 2.44;
pub const POST_RADIUS: f32 = 0.06;
pub const GOAL_DEPTH: f32 = 2.0;

pub const PENALTY_AREA_DEPTH: f32 = 16.5;
pub const PENALTY_AREA_HALF_WIDTH: f32 = 20.16;
pub const GOAL_AREA_DEPTH: f32 = 5.5;
pub const GOAL_AREA_HALF_WIDTH: f32 = 9.16;
pub const PENALTY_SPOT_DISTANCE: f32 = 11.0;
pub const CENTRE_CIRCLE_RADIUS: f32 = 9.15;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Side {
    Home,
    Away,
}

impl Side {
    pub fn index(self) -> usize {
        match self {
            Side::Home => 0,
            Side::Away => 1,
        }
    }

    pub fn opponent(self) -> Side {
        match self {
            Side::Home => Side::Away,
            Side::Away => Side::Home,
        }
    }

    pub fn from_index(i: usize) -> Side {
        if i == 0 { Side::Home } else { Side::Away }
    }
}

/// Converts between the world frame and a team frame in which the team attacks towards +x.
/// All tactical code works in team frames.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TeamFrame {
    /// +1 if the team attacks towards world +x, −1 otherwise.
    pub dir: f32,
}

impl TeamFrame {
    pub fn new(dir: f32) -> Self {
        Self { dir }
    }

    pub fn to_team(&self, p: Vec2) -> Vec2 {
        p * self.dir
    }

    pub fn to_world(&self, p: Vec2) -> Vec2 {
        p * self.dir
    }

    pub fn to_team3(&self, p: Vec3) -> Vec3 {
        Vec3::new(p.x * self.dir, p.y * self.dir, p.z)
    }

    /// Own goal centre in the team frame.
    pub fn own_goal() -> Vec2 {
        Vec2::new(-HALF_LENGTH, 0.0)
    }

    /// Opponent goal centre in the team frame.
    pub fn opp_goal() -> Vec2 {
        Vec2::new(HALF_LENGTH, 0.0)
    }
}

/// Which third of the pitch a team-frame x coordinate lies in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Third {
    Defensive,
    Middle,
    Attacking,
}

pub fn third_of(team_x: f32) -> Third {
    let t = LENGTH / 3.0;
    if team_x < -HALF_LENGTH + t {
        Third::Defensive
    } else if team_x < HALF_LENGTH - t {
        Third::Middle
    } else {
        Third::Attacking
    }
}

pub fn in_bounds(p: Vec2) -> bool {
    p.x.abs() <= HALF_LENGTH && p.y.abs() <= HALF_WIDTH
}

pub fn clamp_to_pitch(p: Vec2, margin: f32) -> Vec2 {
    Vec2::new(
        p.x.clamp(-HALF_LENGTH + margin, HALF_LENGTH - margin),
        p.y.clamp(-HALF_WIDTH + margin, HALF_WIDTH - margin),
    )
}

/// Is a team-frame point inside the opponent's (attacking) penalty area?
pub fn in_attacking_box(team_p: Vec2) -> bool {
    team_p.x >= HALF_LENGTH - PENALTY_AREA_DEPTH && team_p.y.abs() <= PENALTY_AREA_HALF_WIDTH
}

/// Is a kick from team-frame `from` to `to` a cross: from a wide channel (outside the penalty-area lines, ATT-12) of
/// the attacking third into the box? A ground ball played back from there is a cut-back, not a cross.
pub fn is_cross(from: Vec2, to: Vec2, lofted: bool) -> bool {
    third_of(from.x) == Third::Attacking
        && from.y.abs() > PENALTY_AREA_HALF_WIDTH
        && in_attacking_box(to)
        && (lofted || to.x >= from.x - 2.0)
}

/// Is a team-frame point inside the own penalty area?
pub fn in_own_box(team_p: Vec2) -> bool {
    team_p.x <= -HALF_LENGTH + PENALTY_AREA_DEPTH && team_p.y.abs() <= PENALTY_AREA_HALF_WIDTH
}

/// Playing-surface state. All values in [0, 1].
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PitchConditions {
    /// 0 = bone dry, 1 = waterlogged. Wet balls skid and run faster, bounce lower; players lose traction.
    pub wetness: f32,
    /// 0 = soft/muddy, 1 = hard. Hard pitches give higher bounces.
    pub firmness: f32,
    /// 0 = very short grass, 1 = long grass. Long grass slows rolling balls.
    pub grass_length: f32,
    /// 0 = perfectly flat, 1 = very bumpy. Bumps deflect bounces and rolling balls.
    pub unevenness: f32,
}

impl Default for PitchConditions {
    fn default() -> Self {
        Self { wetness: 0.2, firmness: 0.6, grass_length: 0.4, unevenness: 0.2 }
    }
}

impl PitchConditions {
    pub fn dry_flat() -> Self {
        Self { wetness: 0.0, firmness: 0.6, grass_length: 0.4, unevenness: 0.0 }
    }

    pub fn wet_flat() -> Self {
        Self { wetness: 1.0, firmness: 0.6, grass_length: 0.4, unevenness: 0.0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn team_frame_round_trip() {
        let f = TeamFrame::new(-1.0);
        let p = Vec2::new(10.0, -3.0);
        assert_eq!(f.to_world(f.to_team(p)), p);
        assert_eq!(f.to_team(Vec2::new(-52.5, 0.0)), TeamFrame::opp_goal());
    }

    #[test]
    fn thirds() {
        assert_eq!(third_of(-40.0), Third::Defensive);
        assert_eq!(third_of(0.0), Third::Middle);
        assert_eq!(third_of(40.0), Third::Attacking);
    }
}
