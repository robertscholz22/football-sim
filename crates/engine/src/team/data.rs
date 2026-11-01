//! Squad data files (`data/teams/*.ron`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::Role;
use super::instructions::{PlayerInstructions, TeamInstructions};
use crate::player::Attributes;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlayerData {
    pub name: String,
    pub number: u8,
    #[serde(default)]
    pub attrs: Attributes,
    /// Starting confidence 0..1.
    #[serde(default = "half")]
    pub confidence: f32,
    /// Familiarity with a role 0..1 (docs/player-brain.md section 5.5): `role_familiarity` for the roles listed there,
    /// this for any other.
    #[serde(default = "familiar")]
    pub familiarity: f32,
    #[serde(default)]
    pub role_familiarity: BTreeMap<Role, f32>,
    #[serde(default)]
    pub instructions: PlayerInstructions,
}

fn half() -> f32 {
    0.5
}

fn familiar() -> f32 {
    0.8
}

impl PlayerData {
    /// His familiarity with `role`.
    pub fn familiarity_with(&self, role: Role) -> f32 {
        self.role_familiarity.get(&role).copied().unwrap_or(self.familiarity)
    }

    /// How well he suits `role` as a substitute: his familiarity with it, or with the closest role he knows (× 0.85
    /// in the same unit, × 0.6 otherwise).
    pub fn suitability(&self, role: Role) -> f32 {
        self.role_familiarity
            .iter()
            .map(|(&r, &f)| f * if r == role { 1.0 } else if r.unit() == role.unit() { 0.85 } else { 0.6 })
            .fold(self.familiarity, f32::max)
    }

    /// A goalkeeper by his attributes (outfield players' goalkeeping attributes are low).
    pub fn is_goalkeeper(&self) -> bool {
        self.attrs.reflexes.max(self.attrs.handling) >= 40.0
    }
}

/// How practised a team is, 0..1 (docs/player-brain.md sections 7 and 14): in its formation and roles, and in its
/// tactical concept (the instructions). In the manager game it comes from training.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Practice {
    pub formation: f32,
    pub concept: f32,
}

impl Default for Practice {
    fn default() -> Self {
        Self { formation: 0.9, concept: 0.9 }
    }
}

/// How much the players have played together, 0..1 (section 7): a squad default and overrides per pair of shirt
/// numbers. In the manager game it comes from minutes played together.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Partners {
    pub default: f32,
    #[serde(default)]
    pub pairs: Vec<(u8, u8, f32)>,
}

impl Default for Partners {
    fn default() -> Self {
        Self { default: 0.9, pairs: Vec::new() }
    }
}

impl Partners {
    /// How well the players with shirt numbers `a` and `b` know each other (1 for a player and himself).
    pub fn between(&self, a: u8, b: u8) -> f32 {
        if a == b {
            return 1.0;
        }
        self.pairs.iter().find(|&&(x, y, _)| (x, y) == (a, b) || (x, y) == (b, a)).map_or(self.default, |p| p.2)
    }
}

/// Familiarity levels of the familiarity bench (docs/player-brain.md section 15), from a settled side to one just
/// assembled: role familiarity, partners and practice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Settledness {
    Settled,
    NewSystem,
    Signings,
    Assembled,
}

impl Settledness {
    pub const ALL: [Settledness; 4] = [Settledness::Settled, Settledness::NewSystem, Settledness::Signings, Settledness::Assembled];

    /// (role familiarity, partners, practice).
    pub fn levels(self) -> (f32, f32, f32) {
        match self {
            Settledness::Settled => (0.9, 0.9, 0.9),
            Settledness::NewSystem => (0.9, 0.9, 0.2),
            Settledness::Signings => (0.8, 0.5, 0.6),
            Settledness::Assembled => (0.6, 0.1, 0.2),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TeamData {
    pub name: String,
    pub short: String,
    pub color: (u8, u8, u8),
    /// Name of a formation in `data/tactics`.
    pub formation: String,
    #[serde(default)]
    pub instructions: TeamInstructions,
    #[serde(default)]
    pub practice: Practice,
    #[serde(default)]
    pub partners: Partners,
    /// Eleven players in formation-slot order (goalkeeper first).
    pub players: Vec<PlayerData>,
    /// Substitutes (Law 3), goalkeeper first if there is one.
    #[serde(default)]
    pub bench: Vec<PlayerData>,
}

impl TeamData {
    pub fn from_ron(text: &str) -> Result<Self, ron::error::SpannedError> {
        ron::from_str(text)
    }

    /// Squads shipped with the engine (`data/teams`).
    pub fn builtin(name: &str) -> Option<Self> {
        let text = match name {
            "northbridge" => include_str!("../../../../data/teams/northbridge.ron"),
            "harbour" => include_str!("../../../../data/teams/harbour.ron"),
            _ => return None,
        };
        Some(Self::from_ron(text).expect("builtin team must parse"))
    }

    /// The same squad at a familiarity level: every player's role familiarity, the partners and both practices.
    pub fn settled(mut self, level: Settledness) -> Self {
        let (role, partners, practice) = level.levels();
        for p in &mut self.players {
            p.familiarity = role;
            p.role_familiarity.clear();
        }
        self.partners = Partners { default: partners, pairs: Vec::new() };
        self.practice = Practice { formation: practice, concept: practice };
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_squads_have_eleven_players() {
        for name in ["northbridge", "harbour"] {
            let t = TeamData::builtin(name).unwrap();
            assert_eq!(t.players.len(), 11, "{name}");
            assert!(super::super::Formation::builtin(&t.formation).is_some());
        }
    }
}
