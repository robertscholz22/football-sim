//! Teams: formations and roles, manager instructions (layer 2), tactic resolution and squad data.

pub mod data;
pub mod formation;
pub mod instructions;
pub mod options;
pub mod role;
pub mod tactics;

pub use data::{Partners, PlayerData, Practice, Settledness, TeamData};
pub use formation::{FORMATIONS, Formation, Slot};
pub use role::{Duty, Lane, Movement, ROLES, Role, RoleDef, Unit};
pub use instructions::{
    AttackingInstructions, BlockHeight, Channel, DefendingInstructions, PlayerInstructions, SetPieceInstructions, TeamInstructions,
};
pub use options::{PlayerPreset, TeamOption};
pub use tactics::{
    AttackingTactics, DefendingTactics, ResolvedPlayer, ResolvedTactics, SetPieceTactics, adherence, applied_attacking, applied_defending, resolve,
    resolve_player,
};
