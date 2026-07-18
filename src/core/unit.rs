use time::Date;

use std::fmt::Display;

#[derive(serde::Deserialize, Debug, serde::Serialize)]
pub struct Unit {
    pub name: String,
    pub toe: String,
    pub faction: String,
    pub location: UnitLocation,
    /// Movement points left this turn; refilled to the TOE's `mp` when the
    /// unit's faction comes on turn.
    pub mp_left: u32,
    pub elements: Vec<ElementInUnit>,
    /// How dug in this unit is at its current hex, 0 (none) up to a fixed
    /// cap — gains one level per turn spent stationary, resets to 0 the
    /// moment it relocates (move, retreat, or advance into a vacated hex).
    /// Boosts its defensive CV in combat; see "Entrenchment" in
    /// docs/manual.md.
    pub fort_level: u32,
    /// The name of the leader currently commanding this unit, if any — see
    /// `core::leader::Leader`. `None` until assigned, either by the
    /// scenario or the `reassign_leader` command.
    pub leader: Option<String>,
}

impl Unit {
    /// Strength-weighted (ready + damaged) average element morale, 0 for an
    /// empty unit. A unit forced to retreat routs (double attrition) when a
    /// roll beats this.
    pub fn average_morale(&self) -> u32 {
        self.average_stat(|element| element.morale)
    }

    /// Strength-weighted average element experience, for display.
    pub fn average_experience(&self) -> u32 {
        self.average_stat(|element| element.experience)
    }

    fn average_stat(&self, stat: impl Fn(&ElementInUnit) -> u32) -> u32 {
        let strength: u32 = self.elements.iter().map(|e| e.ready + e.damaged).sum();
        if strength == 0 {
            return 0;
        }
        let total: u32 = self.elements.iter().map(|e| (e.ready + e.damaged) * stat(e)).sum();
        total / strength
    }
}

impl Display for Unit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.location {
            UnitLocation::OnMap(coords) => {
                write!(f, "{}\nFaction: {}\nLocation: ({}, {})\nMovement points: {}\nEntrenchment: level {}",
                    self.name, self.faction, coords.x, coords.y, self.mp_left, self.fort_level)
            }
            UnitLocation::Offmap(name) => {
                write!(f, "{}\nFaction: {}\nLocation: {}(offmap)", self.name, self.faction, name)
            }
        }
    }
}

/// Where a unit currently is: a hex on the map, or a named offmap box.
///
/// Externally tagged (serde default) on purpose — postcard save files cannot
/// handle `#[serde(untagged)]`. The scenario TOML uses the friendlier untagged
/// `UnitLocationConfig` instead.
#[derive(serde::Deserialize, Debug, Clone, PartialEq, serde::Serialize)]
pub enum UnitLocation {
    OnMap(LocationCoords),
    Offmap(String),
}

#[derive(serde::Deserialize, Debug, serde::Serialize)]
pub struct ElementInUnit {
    pub name: String,
    pub ready: u32,
    pub damaged: u32,
    /// 0–100. Scales the element's CV in combat; the unit's average gates
    /// routs. Set in the scenario per element, per unit, or per faction —
    /// the most specific setting wins.
    pub morale: u32,
    /// 0–100. Chance for the element to commit (fire) each combat round;
    /// also scales its CV. Same scenario inheritance as morale.
    pub experience: u32,
}

#[derive(serde::Deserialize, Debug, serde::Serialize)]
pub struct Toe {
    pub name: String,
    pub size: Size,
    /// Movement points per turn for units on this TOE — leg formations low,
    /// motorized/armored high (era data, not code).
    pub mp: u32,
    /// Hex-distance limit for air missions (`air_support`/`interdict`)
    /// launched from a unit's current on-map location — the "airfields"
    /// range limit. `None` = unlimited (every TOE before this field existed,
    /// and every ground TOE, which never sets it). Meaningless for a unit
    /// still parked in an offmap reserve box, since there's no coordinate to
    /// measure a distance from — see `Game::check_mission_range`.
    #[serde(default)]
    pub range: Option<u32>,
    pub start_date: Date,
    pub end_date: Date,
    /// The element types this TOE prescribes, and how many of each.
    pub elements: Vec<ElementInToe>,
}

#[derive(serde::Deserialize, Debug, serde::Serialize)]
pub struct ElementInToe {
    pub name: String,
    pub amount: u32,
}

#[derive(serde::Deserialize, Debug, serde::Serialize)]
pub enum Size {
    Division,
    Brigade,
    Regiment,
    Corps,
}

#[derive(serde::Deserialize, Debug, serde::Serialize)]
pub struct Element {
    pub name: String,
    pub class: ElementClass,
    pub cv: f32,
    /// 0–100. How easily fire takes effect on this element: armor for
    /// vehicles, exposure for everything else. Targets are always engaged
    /// with the fire value matching their hardness, so one stat suffices.
    pub vulnerability: u32,
    /// Ground elements can't target air-domain elements unless flagged —
    /// dual-purpose flak, historically. Meaningless (and harmless) on an
    /// already-air-domain element (`ElementClass::is_air_domain`), which can
    /// always engage air regardless of this flag. See the derived
    /// `can_target_air` in `procedures::combat::combat_elements` and
    /// docs/manual.md.
    #[serde(default)]
    pub anti_air: bool,
    /// The weapons this element fights with; every in-range device fires
    /// each combat round. Must not be empty (State::build validates).
    pub devices: Vec<Device>,
}

/// One weapon carried by an element — a rifle/LMG volley, a tank's main gun,
/// its hull machine guns…
#[derive(serde::Deserialize, Debug, Clone, serde::Serialize)]
pub struct Device {
    pub name: String,
    /// 0–100. Chance for a single shot to hit its target.
    pub accuracy: u32,
    /// Meters. The device fires in a combat round iff this covers the
    /// round's range band.
    pub range: u32,
    /// Shots per combat round.
    pub rate_of_fire: u32,
    /// 0–100. How devastating a hit is against unarmored targets (small
    /// arms, HE) — the chance it takes effect on a fully vulnerable target.
    pub soft_attack: u32,
    /// 0–100. How devastating a hit is against armored targets (AP).
    pub hard_attack: u32,
    /// 0–100. How devastating a hit is against air-domain targets — the
    /// counterpart to soft/hard attack for the air domain. Only relevant on
    /// a device belonging to a firer whose derived `can_target_air` is set
    /// (see `procedures::combat::combat_elements`).
    #[serde(default)]
    pub air_attack: u32,
}

#[derive(serde::Deserialize, Debug, PartialEq, serde::Serialize)]
pub enum ElementClass {
    Inf,
    LightTank,
    MedTank,
    MotInf,
    LightArt,
    AtGun,
    /// A CAS aircraft ("bomber") — joins a ground attack as an extra firer
    /// via `Game::air_support` (see docs/manual.md). Air-domain: can
    /// engage ground targets normally and air targets via `air_attack`.
    GroundAttack,
    /// An air-superiority aircraft. Air-domain, but unlike `GroundAttack`
    /// can *only* engage other air-domain targets.
    Fighter,
}

impl ElementClass {
    /// Armored elements are engaged with hard (AP) fire; everything else
    /// receives soft fire.
    pub fn is_armored(&self) -> bool {
        matches!(self, ElementClass::LightTank | ElementClass::MedTank)
    }

    /// Air-domain elements (aircraft) are valid targets for anything whose
    /// derived `can_target_air` is set (see
    /// `procedures::combat::combat_elements`), and never for ground-only
    /// firers.
    pub fn is_air_domain(&self) -> bool {
        matches!(self, ElementClass::GroundAttack | ElementClass::Fighter)
    }

    /// Whether an element of this class can engage ground-domain targets.
    /// True for every class except `Fighter`, which only engages air.
    pub fn can_target_ground(&self) -> bool {
        !matches!(self, ElementClass::Fighter)
    }
}

#[derive(serde::Deserialize, Debug, Clone, PartialEq, serde::Serialize)]
pub struct LocationCoords {
    pub x: u32,
    pub y: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit_with(elements: Vec<ElementInUnit>) -> Unit {
        Unit {
            name: "Test Division".to_string(),
            toe: "test_toe".to_string(),
            faction: "AX".to_string(),
            location: UnitLocation::Offmap("irrelevant".to_string()),
            mp_left: 0,
            elements,
            fort_level: 0,
            leader: None,
        }
    }

    fn bucket(ready: u32, damaged: u32, morale: u32, experience: u32) -> ElementInUnit {
        ElementInUnit { name: "squad".to_string(), ready, damaged, morale, experience }
    }

    #[test]
    fn average_stats_are_strength_weighted() {
        // 30 elements at morale 100, 10 at morale 0 (of which 5 damaged
        // still count — they are present and retreat with the unit).
        let unit = unit_with(vec![bucket(30, 0, 100, 80), bucket(5, 5, 0, 40)]);

        assert_eq!(unit.average_morale(), 75);
        assert_eq!(unit.average_experience(), 70);
    }

    #[test]
    fn averages_of_an_empty_unit_are_zero() {
        let unit = unit_with(vec![bucket(0, 0, 100, 100)]);

        assert_eq!(unit.average_morale(), 0);
        assert_eq!(unit.average_experience(), 0);
    }
}
