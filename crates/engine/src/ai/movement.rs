//! Movement executor (docs/player-brain.md sections 6, 9 and 10). A player chooses a movement at a trigger and holds it:
//! between triggers he follows its path, re-reading its references (the carrier, the man he marks, the block) at his
//! reaction time, and only re-checks the choice every `AiConfig::movement_check_s`. A trigger is any change of the
//! situation he chose it in (`Cue`): possession, his duty, pressing, the phase, a pass struck, a new carrier, the
//! carrier turning open or closed.

use glam::Vec2;

use super::brain::{AttDuty, DefDuty, TeamPhase};
use super::deviation::reliability;
use super::offer::is_open;
use crate::config::SimConfig;
use crate::match_state::{MatchState, side_of};
use crate::player::{Player, PlayerId, norm};
use crate::team::Movement;

/// The situation a movement was chosen in; a change is a trigger.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cue {
    pub in_possession: bool,
    pub duty: DefDuty,
    pub att: AttDuty,
    pub press: bool,
    pub phase: TeamPhase,
    pub carrier: Option<PlayerId>,
    /// Start time of the pass in flight.
    pub pass: Option<f32>,
    /// In possession: the team-mate on the ball is open (`offer::is_open`).
    pub open: bool,
}

/// The movement a player holds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plan {
    pub kind: Movement,
    pub cue: Cue,
    pub since: f32,
    /// When he next re-checks it without a trigger.
    pub check_at: f32,
    /// In possession: the point he offers the ball at (world frame); a run's offer is where the through ball goes.
    pub offer: Option<Vec2>,
    /// Chosen for the receiver of the pass in flight (look-ahead), so its arrival is no trigger.
    pub projected: bool,
    /// The offside line (team frame x) when he chose it; a shift beyond a band is a trigger.
    pub line: f32,
    /// Layer 3: the seed of his misjudgement of the movements' values, kept until the next trigger (drift, not
    /// jitter: a periodic re-check does not redraw it).
    pub noise: u64,
    /// In possession, when his movement is not the one the concept prescribes (a perfect reader's choice, without his
    /// misjudgement): that movement and its offer (world frame), which team-mates believe until they read his
    /// (`reading::seen`, docs/player-brain.md section 7).
    pub concept: Option<(Movement, Vec2)>,
}

impl Default for Plan {
    fn default() -> Self {
        Self {
            kind: Movement::Hold,
            cue: Cue {
                in_possession: false,
                duty: DefDuty::Shape,
                att: AttDuty::Support,
                press: false,
                phase: TeamPhase::Out(super::brain::OutPhase::Block),
                carrier: None,
                pass: None,
                open: false,
            },
            since: 0.0,
            check_at: 0.0,
            offer: None,
            projected: false,
            line: 0.0,
            noise: 0,
            concept: None,
        }
    }
}

/// Does player `p` follow the movement model out of possession (an outfield player)? In possession every outfield
/// player does, through `positioning::possession_movement`.
pub fn applies(p: &Player) -> bool {
    !p.beliefs.in_possession && !p.is_gk()
}

/// How far ahead player `p` looks (0..1, section 8.4): instinct (projecting physical events) and intelligence (how
/// many decisions ahead he thinks) equally.
pub fn lookahead(p: &Player) -> f32 {
    0.5 * (norm(p.attrs.instinct) + norm(p.attrs.intelligence))
}

/// The situation player `p` reads now.
pub fn cue(state: &MatchState, cfg: &SimConfig, p: &Player) -> Cue {
    let b = &p.beliefs;
    let open = b.in_possession
        && state.control.filter(|&c| side_of(c) == p.side && c != p.id).is_some_and(|c| is_open(state, cfg, &state.players[c]));
    Cue {
        in_possession: b.in_possession,
        duty: b.def_duties[p.slot],
        att: b.att_duties[p.slot],
        press: b.press,
        phase: b.phase,
        carrier: state.control,
        pass: state.flight.map(|f| f.start_t),
        open,
    }
}

/// Must player `p` choose his movement afresh now: a trigger, or the periodic check is due?
pub fn rechoose(state: &MatchState, cfg: &SimConfig, p: &Player) -> bool {
    cue(state, cfg, p) != p.plan.cue || state.time >= p.plan.check_at
}

/// What a decision leaves for the plan.
#[derive(Clone, Copy, Debug)]
pub struct Decided {
    pub kind: Movement,
    /// Chosen afresh (a trigger or the periodic check), not followed.
    pub chose: bool,
    pub offer: Option<Vec2>,
    pub projected: bool,
    pub noise: u64,
    pub concept: Option<(Movement, Vec2)>,
}

/// The plan after a decision: a fresh choice starts a new periodic check, which comes sooner for a more reliable
/// reader of the game (layer 3: 0.75–1.25 × `movement_check_s`).
pub fn plan_after(state: &MatchState, cfg: &SimConfig, p: &Player, d: Decided) -> Plan {
    let now = state.time;
    let ai = &cfg.ai;
    let cue = cue(state, cfg, p);
    let since = if d.kind == p.plan.kind { p.plan.since } else { now };
    let (check_at, line, projected) = if d.chose {
        (now + ai.movement_check_s * (1.25 - 0.5 * reliability(p, ai)), p.beliefs.offside_line, d.projected)
    } else {
        (p.plan.check_at, p.plan.line, p.plan.projected)
    };
    Plan { kind: d.kind, cue, since, check_at, offer: d.offer, projected, line, noise: d.noise, concept: d.concept }
}
