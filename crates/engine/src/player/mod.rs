//! Players: attributes, derived capabilities, dynamic state and movement.

pub mod attributes;
pub mod confidence;
pub mod effective;
pub mod fatigue;
pub mod locomotion;
pub mod state;

pub use attributes::{Attributes, Foot, GROUPS, norm};
pub use effective::{AttrClass, Kinematics, effective};
pub use state::{Body, Intent, KickKind, KickPlan, Player, PlayerDebug, PlayerId};
