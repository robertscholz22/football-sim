//! Formation roles (docs/player-brain.md section 5): what a slot asks of its player. Each role is a catalog entry
//! (ROLE-01 … ROLE-14) and is defined in `data/roles/*.ron`: its unit and lane, the duties he holds or competes for,
//! his movement priors, the team-mates he reads first and the instructions it accepts.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// The 14 roles (section 5.2). A formation slot names one in possession and one out of possession.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[allow(clippy::upper_case_acronyms)]
pub enum Role {
    /// ROLE-01 goalkeeper.
    GK,
    /// ROLE-02 centre-back in a pair.
    CB,
    /// ROLE-03 central centre-back of a three or five.
    CBC,
    /// ROLE-04 outside centre-back of a three or five.
    CBW,
    /// ROLE-05 full-back in a back four.
    FB,
    /// ROLE-06 wing-back in a back five.
    WB,
    /// ROLE-07 single pivot.
    DM,
    /// ROLE-08 central midfielder in a pair (double pivot or flat two).
    CMP,
    /// ROLE-09 #8 beside or ahead of a single pivot.
    CM8,
    /// ROLE-10 attacking midfielder between the lines.
    AM,
    /// ROLE-11 wide player of a midfield line.
    WM,
    /// ROLE-12 winger or wide forward.
    W,
    /// ROLE-13 lone centre-forward.
    CF,
    /// ROLE-14 striker in a pair.
    ST2,
}

/// The line of the team a role belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Unit {
    Keeper,
    Back,
    Midfield,
    Front,
}

/// The vertical lane of a role (ATT-12 channels: centre, inside, wide). Every formation has centre-lane roles in each
/// unit, whose spots keep the centre occupied (ATT-13).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Lane {
    Centre,
    Half,
    Wide,
}

/// A duty is a claim the role holds or competes for (section 8.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Duty {
    /// Member of the last line out of possession (BLK-02).
    LastLine,
    /// May lead the last line: steps it up or drops it (BLK-02).
    LeadLine,
    /// Candidate for the rest defence while in possession (TR-05).
    RestDefence,
    /// Covers a team-mate who steps out or advances (DEF-02).
    Cover,
    /// Licence to step out of the line to press or follow (BLK-03).
    StepOut,
    /// Marks opponents who enter his zone (DEF-04, IND-04).
    MarkZone,
    /// Gives width on his side in possession (ATT-11, ATT-12).
    Width,
    /// Gives depth: holds or attacks the last line in possession (ATT-11).
    Depth,
    /// Decides when and where the press starts (PRS-01).
    StartPress,
}

/// The movement repertoire (section 6), which includes the ATT-16 movement types (in front: show, check; in between:
/// drop; in to out, out to in; in behind: run in behind). Priors say which movements a role uses and how readily
/// (0 … 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Movement {
    Hold,
    Show,
    Check,
    Drop,
    Pin,
    RunInBehind,
    InToOut,
    OutToIn,
    Overlap,
    Underlap,
    Invert,
    DropIntoBackLine,
    StepIntoMidfield,
    Decoy,
    RestDefence,
    Press,
    Cover,
    Mark,
    Track,
    Recover,
    BoxRun,
}

/// A role's definition (`data/roles/*.ron`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RoleDef {
    pub role: Role,
    /// Catalog ID (`docs/tactics/principles.md`).
    pub id: String,
    pub name: String,
    pub unit: Unit,
    pub lane: Lane,
    pub duties: Vec<Duty>,
    pub movements: Vec<(Movement, f32)>,
    /// Team-mates whose duties his own depend on: he reads them first (section 7).
    pub relations: Vec<Role>,
    /// Role instructions it accepts (layer 2, section 5.4).
    pub instructions: Vec<String>,
}

const ROLE_FILES: [&str; 14] = [
    include_str!("../../../../data/roles/gk.ron"),
    include_str!("../../../../data/roles/cb.ron"),
    include_str!("../../../../data/roles/cbc.ron"),
    include_str!("../../../../data/roles/cbw.ron"),
    include_str!("../../../../data/roles/fb.ron"),
    include_str!("../../../../data/roles/wb.ron"),
    include_str!("../../../../data/roles/dm.ron"),
    include_str!("../../../../data/roles/cmp.ron"),
    include_str!("../../../../data/roles/cm8.ron"),
    include_str!("../../../../data/roles/am.ron"),
    include_str!("../../../../data/roles/wm.ron"),
    include_str!("../../../../data/roles/w.ron"),
    include_str!("../../../../data/roles/cf.ron"),
    include_str!("../../../../data/roles/st2.ron"),
];

/// Every role, in declaration order.
pub const ROLES: [Role; 14] =
    [Role::GK, Role::CB, Role::CBC, Role::CBW, Role::FB, Role::WB, Role::DM, Role::CMP, Role::CM8, Role::AM, Role::WM, Role::W, Role::CF, Role::ST2];

fn defs() -> &'static [RoleDef; 14] {
    static DEFS: OnceLock<[RoleDef; 14]> = OnceLock::new();
    DEFS.get_or_init(|| {
        std::array::from_fn(|i| {
            let d: RoleDef = ron::from_str(ROLE_FILES[i]).expect("builtin role must parse");
            assert_eq!(d.role, ROLES[i], "data/roles: file order must follow `ROLES`");
            d
        })
    })
}

impl Role {
    pub fn def(self) -> &'static RoleDef {
        &defs()[self as usize]
    }

    pub fn unit(self) -> Unit {
        self.def().unit
    }

    pub fn lane(self) -> Lane {
        self.def().lane
    }

    pub fn has_duty(self, duty: Duty) -> bool {
        self.def().duties.contains(&duty)
    }

    /// Prior for a movement (0 if the role does not use it).
    pub fn prior(self, m: Movement) -> f32 {
        self.def().movements.iter().find(|(k, _)| *k == m).map_or(0.0, |&(_, v)| v)
    }

    pub fn is_back_line(self) -> bool {
        self.unit() == Unit::Back
    }

    /// A centre-back of a pair, three or five (IND-06: recovers within the goal width).
    pub fn is_centre_back(self) -> bool {
        self.unit() == Unit::Back && self.lane() != Lane::Wide
    }

    /// A central forward (CF or ST2).
    pub fn is_striker(self) -> bool {
        self.unit() == Unit::Front && self.lane() == Lane::Centre
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_role_parses_with_its_catalog_id() {
        for (i, r) in ROLES.into_iter().enumerate() {
            let d = r.def();
            assert_eq!(d.id, format!("ROLE-{:02}", i + 1));
            assert!(d.movements.iter().all(|&(_, v)| (0.0..=1.0).contains(&v)), "{r:?}");
            assert_eq!(r == Role::GK, d.unit == Unit::Keeper);
        }
    }
}
