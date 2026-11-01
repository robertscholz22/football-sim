//! Layer-1 anchors: formation slot → point in the current block.
//! Out of possession the block comes from DEF-05 (compactness), DEF-06 (ball-side shift), BLK-02 and BLK-05
//! (line height); in possession from ATT-11 (width and depth, BU-02 full-backs wide via the formation data).
//! Layer 2 (instructions) and layer 3 (adherence via discipline and role familiarity) are blended here.

use glam::Vec2;

use super::brain::Block;
use crate::config::SimConfig;
use crate::match_state::MatchState;
use crate::pitch::clamp_to_pitch;
use crate::player::PlayerId;
use crate::team::{Slot, adherence, resolve_player};

/// Team-frame point of a formation slot inside a block.
pub fn slot_point(slot: &Slot, in_possession: bool, block: &Block) -> Vec2 {
    let (d, w) = if in_possession { slot.in_pos } else { slot.out_pos };
    Vec2::new(block.line_x + d * (block.front_x - block.line_x), block.centre_y + w * block.half_width)
}

/// Anchor of a player in the team frame: the layer-1 anchor blended towards the layer-2 anchor by the player's
/// adherence to instructions (layer 3). Returns (blended anchor, pure layer-1 anchor).
pub fn anchor(state: &MatchState, _cfg: &SimConfig, id: PlayerId) -> (Vec2, Vec2) {
    let p = &state.players[id];
    let team = state.team(p.side);
    let b = &p.beliefs;
    let slot = &team.formation.slots[p.slot];
    let in_poss = b.in_possession;
    let (blk2, blk1) = if in_poss { (b.att_block, b.att_block_l1) } else { (b.def_block, b.def_block_l1) };
    let a1 = slot_point(slot, in_poss, &blk1);
    let mut a2 = slot_point(slot, in_poss, &blk2);
    if in_poss {
        // Player instruction: get forward / stay back.
        a2.x += resolve_player(&p.instructions).forward_runs * 6.0;
    }
    let adh = adherence(&p.attrs, p.instruction_familiarity(in_poss), false);
    (clamp_to_pitch(a1.lerp(a2, adh), 1.0), clamp_to_pitch(a1, 1.0))
}
