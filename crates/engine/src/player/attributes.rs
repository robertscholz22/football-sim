//! Player ratings on the 0–100 scale (stored as f32 so fractional values are possible): the 28-rating palette of
//! `docs/attributes.md`, plus the facts height, weight and preferred foot.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Foot {
    #[default]
    Right,
    Left,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Attributes {
    // Body
    /// Top speed.
    pub speed: f32,
    /// Acceleration, deceleration and lateral acceleration.
    pub quickness: f32,
    /// Contact: shielding, the body part of tackles and aerial duels, staying on the feet; kick power.
    pub strength: f32,
    /// Vertical reach with height.
    pub jumping: f32,
    /// Energy drain and recovery over the match (M6).
    pub stamina: f32,
    // Technique
    /// Receiving: the control probability of every reception.
    pub first_touch: f32,
    /// Carrying: touch error and carrying speed, take-ons, turning with the ball, shielding.
    pub dribbling: f32,
    /// Ground passes up to 20 m, blended into long passing up to 35 m.
    pub short_passing: f32,
    /// Ground passes beyond 35 m and every lofted pass that is not a cross; clearances and the keeper's kicks.
    pub long_passing: f32,
    /// Deliveries into the box from the wide channels of the final third (`pitch::is_cross`).
    pub crossing: f32,
    /// Shots up to 12 m, blended into long shots up to 22 m.
    pub finishing: f32,
    /// Shots beyond 22 m: power, dip and curl.
    pub long_shots: f32,
    pub heading: f32,
    /// Tackle reach, timing and success.
    pub tackling: f32,
    pub weak_foot: f32,
    // Mind
    /// Understanding (positional reliability, applying instructions), match reading, vision, learning.
    pub intelligence: f32,
    /// Anticipation (reaction time, touches on passing balls, blocks) and decisions (the choice temperature).
    pub instinct: f32,
    /// Execution, control and decisions under pressure; settling time; focus.
    pub composure: f32,
    /// Calls, organising, lifting team-mates (M6).
    pub leadership: f32,
    // Character
    pub work_rate: f32,
    /// How faithfully the player follows the manager's instructions.
    pub discipline: f32,
    /// From team player (0) to egotist (100): keeping the ball and seeking personal glory instead of passing.
    pub egotism: f32,
    /// Bias to difficult options: take-ons, curled shots, through balls.
    pub flair: f32,
    /// Commitment: tackle intent, going into duels, risk appetite under pressure.
    pub aggression: f32,
    // Goalkeeping
    /// Reaction to the shot and dive reach.
    pub reflexes: f32,
    /// Catch or parry.
    pub handling: f32,
    /// The direct duel: close-range shots and smothering at the attacker's feet.
    pub one_on_ones: f32,
    /// Claiming crosses and high balls (M4).
    pub aerial_command: f32,
    // Facts
    pub height_cm: f32,
    /// Used only in challenges, aerial and on the ground (`duel::challenge_strength`).
    pub weight_kg: f32,
    pub preferred_foot: Foot,
}

impl Default for Attributes {
    fn default() -> Self {
        Self {
            speed: 47.4,
            quickness: 47.4,
            strength: 47.4,
            jumping: 47.4,
            stamina: 47.4,
            first_touch: 47.4,
            dribbling: 47.4,
            short_passing: 47.4,
            long_passing: 47.4,
            crossing: 47.4,
            finishing: 47.4,
            long_shots: 47.4,
            heading: 47.4,
            tackling: 47.4,
            weak_foot: 36.8,
            intelligence: 47.4,
            instinct: 47.4,
            composure: 47.4,
            leadership: 47.4,
            work_rate: 47.4,
            discipline: 57.9,
            egotism: 26.3,
            flair: 47.4,
            aggression: 47.4,
            reflexes: 10.5,
            handling: 10.5,
            one_on_ones: 10.5,
            aerial_command: 10.5,
            height_cm: 180.0,
            weight_kg: 75.0,
            preferred_foot: Foot::Right,
        }
    }
}

/// Map a 0–100 attribute to [0, 1].
pub fn norm(v: f32) -> f32 {
    (v / 100.0).clamp(0.0, 1.0)
}

/// The ratings by group, in display order.
pub const GROUPS: [(&str, &[&str]); 5] = [
    ("Body", &["speed", "quickness", "strength", "jumping", "stamina"]),
    (
        "Technique",
        &[
            "first_touch",
            "dribbling",
            "short_passing",
            "long_passing",
            "crossing",
            "finishing",
            "long_shots",
            "heading",
            "tackling",
            "weak_foot",
        ],
    ),
    ("Mind", &["intelligence", "instinct", "composure", "leadership"]),
    ("Character", &["work_rate", "discipline", "egotism", "flair", "aggression"]),
    ("Goalkeeping", &["reflexes", "handling", "one_on_ones", "aerial_command"]),
];

impl Attributes {
    /// Every rating set to `v` (facts unchanged); useful for tests.
    pub fn uniform(v: f32) -> Self {
        let mut a = Self::default();
        for (_, names) in GROUPS {
            for name in names {
                *a.rating_mut(name).expect("rating in GROUPS") = v;
            }
        }
        a
    }
}

/// `rating` / `rating_mut`: a rating by its field name, as listed in `GROUPS`.
macro_rules! by_name {
    ($($f:ident),+ $(,)?) => {
        impl Attributes {
            pub fn rating(&self, name: &str) -> Option<f32> {
                match name {
                    $(stringify!($f) => Some(self.$f),)+
                    _ => None,
                }
            }

            pub fn rating_mut(&mut self, name: &str) -> Option<&mut f32> {
                match name {
                    $(stringify!($f) => Some(&mut self.$f),)+
                    _ => None,
                }
            }
        }
    };
}

by_name!(
    speed, quickness, strength, jumping, stamina, first_touch, dribbling, short_passing, long_passing, crossing, finishing,
    long_shots, heading, tackling, weak_foot, intelligence, instinct, composure, leadership, work_rate, discipline, egotism,
    flair, aggression, reflexes, handling, one_on_ones, aerial_command,
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_name_every_rating_once() {
        let names: Vec<&str> = GROUPS.iter().flat_map(|(_, n)| n.iter().copied()).collect();
        assert_eq!(names.len(), 28);
        let mut a = Attributes::default();
        for (i, name) in names.iter().enumerate() {
            *a.rating_mut(name).unwrap() = 1000.0 + i as f32;
        }
        for (i, name) in names.iter().enumerate() {
            assert_eq!(a.rating(name), Some(1000.0 + i as f32), "{name}");
        }
    }
}
