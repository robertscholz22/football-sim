//! Match event log. The FNV-1a hash of the log is the determinism check: the same seed must give the same hash.

use crate::duel::TackleOutcome;
use crate::match_state::RestartKind;
use crate::pitch::Side;
use crate::player::{KickKind, PlayerId};

#[derive(Clone, Debug, PartialEq)]
pub enum EventKind {
    Kickoff { side: Side },
    /// `headed`: a headed pass (a knock-down or flick-on), not a kick the on-ball AI chose.
    Pass { by: PlayerId, to: Option<PlayerId>, kind: KickKind, length: f32, first_time: bool, headed: bool },
    /// `weak_foot`: struck with the shooter's weaker foot (always false for headers).
    Shot { by: PlayerId, xg: f32, first_time: bool, weak_foot: bool },
    Save { gk: PlayerId, shooter: Option<PlayerId>, held: bool },
    Goal { side: Side, scorer: Option<PlayerId>, own_goal: bool, first_time: bool, passes_in_move: u32 },
    Tackle { by: PlayerId, on: PlayerId, outcome: TackleOutcome },
    Interception { by: PlayerId },
    Miscontrol { by: PlayerId },
    /// An outfield player blocked a shot.
    Block { by: PlayerId },
    Header { by: PlayerId },
    /// Contested high ball: `won` plays it, `lost` is beaten to it.
    Aerial { won: PlayerId, lost: PlayerId },
    Restart { kind: RestartKind, side: Side },
    /// Law 11.
    Offside { by: PlayerId },
    /// Law 12: `advantage`: the referee let play go on (Law 5.3).
    Foul { by: PlayerId, on: PlayerId, penalty: bool, advantage: bool },
    Card { player: PlayerId, card: Card },
    /// Law 3: `off` leaves, `on` takes his place in the same slot (names: the slot's id passes to `on`).
    Substitution { side: Side, off: String, on: String },
    /// Law 7.3: minimum added time shown at the end of a half's regular time (whole minutes).
    AddedTime { minutes: u32 },
    HalfTime,
    FullTime,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Card {
    Yellow,
    /// A second caution: sent off.
    SecondYellow,
    Red,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MatchEvent {
    /// Engine time (`MatchState::time`).
    pub t: f32,
    /// Match clock and added time played when it happened (`MatchState::clock`, `stoppage`).
    pub clock: f32,
    pub stoppage: f32,
    pub kind: EventKind,
}

impl MatchEvent {
    /// One-line description for logs and the viewer.
    pub fn describe(&self, name: impl Fn(PlayerId) -> String) -> String {
        let mm = (self.clock / 60.0) as u32;
        let ss = (self.clock % 60.0) as u32;
        let extra = if self.stoppage > 0.0 { format!("+{}", (self.stoppage / 60.0) as u32 + 1) } else { String::new() };
        let text = match &self.kind {
            EventKind::Kickoff { side } => format!("kick-off {side:?}"),
            EventKind::Pass { by, to, kind, length, .. } => {
                format!("{kind:?} {} → {} ({length:.0} m)", name(*by), to.map(&name).unwrap_or_else(|| "space".into()))
            }
            EventKind::Shot { by, xg, first_time, weak_foot } => {
                let first = if *first_time { ", first time" } else { "" };
                format!("shot {} (xG {xg:.2}{first}{})", name(*by), if *weak_foot { ", weak foot" } else { "" })
            }
            EventKind::Save { gk, held, .. } => format!("save {} ({})", name(*gk), if *held { "held" } else { "parried" }),
            EventKind::Goal { side, scorer, own_goal, .. } => format!(
                "GOAL {side:?} {}{}",
                scorer.map(&name).unwrap_or_default(),
                if *own_goal { " (own goal)" } else { "" }
            ),
            EventKind::Tackle { by, on, outcome } => format!("tackle {} on {}: {outcome:?}", name(*by), name(*on)),
            EventKind::Interception { by } => format!("interception {}", name(*by)),
            EventKind::Miscontrol { by } => format!("heavy touch {}", name(*by)),
            EventKind::Block { by } => format!("block {}", name(*by)),
            EventKind::Header { by } => format!("header {}", name(*by)),
            EventKind::Aerial { won, lost } => format!("aerial duel {} beats {}", name(*won), name(*lost)),
            EventKind::Restart { kind, side } => format!("{kind:?} {side:?}"),
            EventKind::Offside { by } => format!("offside {}", name(*by)),
            EventKind::Foul { by, on, penalty, advantage } => format!(
                "foul {} on {}{}",
                name(*by),
                name(*on),
                if *penalty { " — PENALTY" } else if *advantage { " (advantage)" } else { "" }
            ),
            EventKind::Card { player, card } => format!("{card:?} card {}", name(*player)),
            EventKind::Substitution { side, off, on } => format!("substitution {side:?}: {on} for {off}"),
            EventKind::AddedTime { minutes } => format!("{minutes} minutes added"),
            EventKind::HalfTime => "half time".into(),
            EventKind::FullTime => "full time".into(),
        };
        format!("{mm:02}:{ss:02}{extra} {text}")
    }
}

/// FNV-1a hash of the event log (via the `Debug` representation, which is deterministic).
pub fn hash_events(events: &[MatchEvent]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for e in events {
        for b in format!("{e:?}").bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    }
    h
}
