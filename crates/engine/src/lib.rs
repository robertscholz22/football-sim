//! Headless, deterministic 11v11 football match engine.
//!
//! Layer-1 (tactically optimal) behaviour is derived from the sourced principle catalog in
//! `docs/tactics/principles.md`; rule IDs such as `DEF-02` are referenced where they are implemented.
//! Layer 2 (manager instructions) lives in `team`, layer 3 (individual deviation) in `ai::deviation`,
//! `ai::personality` and `ai::execution`.

pub mod ai;
pub mod ball;
pub mod config;
pub mod duel;
pub mod events;
pub mod match_state;
pub mod pitch;
pub mod player;
pub mod rng;
pub mod rules;
pub mod scenario;
pub mod sim;
pub mod stats;
pub mod team;

pub use glam::{Vec2, Vec3};
pub use sim::Simulation;
