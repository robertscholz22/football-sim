//! Ball state, physics and trajectory prediction.

pub mod physics;
pub mod surface;
pub mod trajectory;

use glam::{Vec2, Vec3};
use serde::{Deserialize, Serialize};

pub use physics::{BallEvent, BallParams, check_lines, step_ball};
pub use surface::SurfaceField;
pub use trajectory::{KickSolution, KickSpec, Trajectory, predict, solve_kick};

pub const BALL_RADIUS: f32 = 0.11;
pub const BALL_MASS: f32 = 0.43;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BallMode {
    /// In flight or bouncing.
    Air,
    /// Rolling on the ground (z = radius).
    Rolling,
    /// Held in a goalkeeper's hands; physics paused, position follows the keeper.
    Held,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ball {
    /// Centre of the ball (z = BALL_RADIUS when on the ground).
    pub pos: Vec3,
    pub vel: Vec3,
    /// Angular velocity in rad/s. +z spin curves the ball to the left of its direction of travel.
    pub spin: Vec3,
    pub mode: BallMode,
}

impl Ball {
    pub fn at_rest(p: Vec2) -> Self {
        Self { pos: p.extend(BALL_RADIUS), vel: Vec3::ZERO, spin: Vec3::ZERO, mode: BallMode::Rolling }
    }

    pub fn ground_pos(&self) -> Vec2 {
        self.pos.truncate()
    }

    pub fn ground_vel(&self) -> Vec2 {
        self.vel.truncate()
    }

    /// Height of the ball's bottom above the ground.
    pub fn height(&self) -> f32 {
        self.pos.z - BALL_RADIUS
    }

    pub fn speed(&self) -> f32 {
        self.vel.length()
    }

    /// Apply a kick: replaces velocity and spin.
    pub fn kick(&mut self, vel: Vec3, spin: Vec3) {
        self.vel = vel;
        self.spin = spin;
        self.mode = if vel.z > 0.05 || self.pos.z > BALL_RADIUS + 0.01 { BallMode::Air } else { BallMode::Rolling };
        if self.mode == BallMode::Rolling {
            self.vel.z = 0.0;
            self.pos.z = BALL_RADIUS;
        }
    }
}
