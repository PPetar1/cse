use time::Date;

use std::fmt::Display;

#[derive(serde::Deserialize, Debug, serde::Serialize)]
pub struct Unit {
    pub name: String,
    pub toe: String,
    pub faction: String,
    /// 0–100. A unit forced to retreat routs (double attrition) when a roll
    /// beats its morale.
    pub morale: u32,
    /// 0–100. Chance for each of the unit's elements to commit (fire) in a
    /// combat round. Per-element experience needs per-instance state the
    /// ready/damaged count buckets don't hold — future work.
    pub experience: u32,
    pub location: UnitLocation,
    pub elements: Vec<ElementInUnit>,
}

impl Display for Unit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.location {
            UnitLocation::OnMap(coords) => {
                write!(f, "{}\nFaction: {}\nLocation: ({}, {})", self.name, self.faction, coords.x, coords.y)
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
#[derive(serde::Deserialize, Debug, PartialEq, serde::Serialize)]
pub enum UnitLocation {
    OnMap(LocationCoords),
    Offmap(String),
}

#[derive(serde::Deserialize, Debug, serde::Serialize)]
pub struct ElementInUnit {
    pub name: String,
    pub ready: u32,
    pub damaged: u32,
}

#[derive(serde::Deserialize, Debug, serde::Serialize)]
pub struct Toe {
    pub name: String,
    pub size: Size,
    pub start_date: Date,
    pub end_date: Date,
    pub elements: Vec<ElementInToe>,// Tuple holds the name of the element in question,
                                 // number of elements the toe prescribes 
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
    pub accuracy: u32,
    pub range: u32,
    pub v_inf: u32,
    pub v_arm: u32,
}

#[derive(serde::Deserialize, Debug, serde::Serialize)]
pub enum ElementClass {
    Inf,
    LightTank,
    MedTank,
    MotInf,
    LightArt,
    AtGun,
}

#[derive(serde::Deserialize, Debug, PartialEq, serde::Serialize)]
pub struct LocationCoords {
    pub x: u32,
    pub y: u32,
}
