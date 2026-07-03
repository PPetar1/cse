use time::Date;

use std::fmt::Display;

#[derive(serde::Deserialize, Debug, serde::Serialize)]
pub struct Unit {
    pub name: String,
    pub toe: String,
    pub faction: String,
    pub location: UnitLocation,
    pub elements: Vec<ElementInUnit>,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn unit_with(elements: Vec<ElementInUnit>) -> Unit {
        Unit {
            name: "Test Division".to_string(),
            toe: "test_toe".to_string(),
            faction: "AX".to_string(),
            location: UnitLocation::Offmap("irrelevant".to_string()),
            elements,
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
