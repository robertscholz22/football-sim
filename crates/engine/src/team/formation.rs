//! Formations: 11 slots, each with an in-possession and an out-of-possession role (docs/player-brain.md section 5.1)
//! and separate in- / out-of-possession shapes.

use serde::{Deserialize, Serialize};

use super::role::Role;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Slot {
    /// Role in possession.
    pub role: Role,
    /// Role out of possession, if it differs (a 4-2-3-1 #10 who defends as a second striker).
    #[serde(default)]
    pub out_role: Option<Role>,
    pub label: String,
    /// (depth 0..1 from back line to front line, width −1..1 with +1 = left side) out of possession.
    pub out_pos: (f32, f32),
    /// Same, in possession.
    pub in_pos: (f32, f32),
}

impl Slot {
    /// The slot's role in the phase.
    pub fn role(&self, in_possession: bool) -> Role {
        if in_possession { self.role } else { self.out_role.unwrap_or(self.role) }
    }
}

/// Formations shipped with the engine.
pub const FORMATIONS: [&str; 6] = ["4-4-2", "4-3-3", "4-2-3-1", "3-4-2-1", "3-4-3", "3-5-2"];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Formation {
    pub name: String,
    pub slots: Vec<Slot>,
}

impl Formation {
    pub fn from_ron(text: &str) -> Result<Self, ron::error::SpannedError> {
        ron::from_str(text)
    }

    /// Formations shipped with the engine (`data/tactics`).
    pub fn builtin(name: &str) -> Option<Self> {
        let text = match name {
            "4-3-3" => include_str!("../../../../data/tactics/4-3-3.ron"),
            "4-4-2" => include_str!("../../../../data/tactics/4-4-2.ron"),
            "4-2-3-1" => include_str!("../../../../data/tactics/4-2-3-1.ron"),
            "3-4-2-1" => include_str!("../../../../data/tactics/3-4-2-1.ron"),
            "3-4-3" => include_str!("../../../../data/tactics/3-4-3.ron"),
            "3-5-2" => include_str!("../../../../data/tactics/3-5-2.ron"),
            _ => return None,
        };
        Some(Self::from_ron(text).expect("builtin formation must parse"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_have_eleven_slots_with_keeper_first() {
        for name in FORMATIONS {
            let f = Formation::builtin(name).unwrap();
            assert_eq!(f.name, name);
            assert_eq!(f.slots.len(), 11);
            for in_poss in [true, false] {
                assert_eq!(f.slots[0].role(in_poss), Role::GK, "{name}");
                assert_eq!(f.slots.iter().filter(|s| s.role(in_poss) == Role::GK).count(), 1, "{name}");
            }
        }
    }

    #[test]
    fn back_lines_match_the_formation_name() {
        // The first digit of the name is the size of the back line out of possession, except where the wing-backs
        // drop into it (a 3-x-x defends with five).
        for name in FORMATIONS {
            let f = Formation::builtin(name).unwrap();
            let back = f.slots.iter().filter(|s| s.role(false).is_back_line()).count();
            let expected = if name.starts_with('3') { 5 } else { name[..1].parse().unwrap() };
            assert_eq!(back, expected, "{name}");
        }
    }
}
