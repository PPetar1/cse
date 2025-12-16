use time::Date;
use either::Either;

use std::fmt::Display;

#[derive(serde::Deserialize, Debug)]
pub struct Unit {
    pub name: String,
    pub toe: String,
    pub faction: String,
    pub location: Either<LocationCoords, OffmapLocationName>,
    pub elements: Vec<ElementInUnit>// Tuple holds the name of the element in question,
                                          // number of ready elements in unit, number of damaged
                                          // elements in unit (ele, rdy, dmg)
}

impl Unit {
    pub fn new(name: String, toe: String, faction: String, location: Either<LocationCoords, OffmapLocationName>, elements: Vec<ElementInUnit>) -> Unit {
        Unit { name, toe, faction, location, elements }
    }
}

impl Display for Unit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.location.is_left() {
            write!(f, "{}\nFaction: {}\nLocation: ({}, {})", self.name, self.faction, self.location.as_ref().left().unwrap().x, self.location.as_ref().left().unwrap().y)
        }
        else {
            write!(f, "{}\nFaction: {}\nLocation: {}(offmap)", self.name, self.faction, self.location.as_ref().right().unwrap().name)
        }
    }
}

#[derive(serde::Deserialize, Debug)]
pub struct ElementInUnit {
    pub name: String,
    pub ready: u32,
    pub damaged: u32,
}

#[derive(serde::Deserialize, Debug)]
pub struct Toe {
    pub name: String,
    pub size: Size,
    pub start_date: Date,
    pub end_date: Date,
    pub elements: Vec<ElementInToe>,// Tuple holds the name of the element in question,
                                 // number of elements the toe prescribes 
}

#[derive(serde::Deserialize, Debug)]
pub struct ElementInToe {
    pub name: String,
    pub amount: u32,
}

impl Toe {
    pub fn new(&self, name: String, size: Size, start_date: Date, end_date: Date, elements: Vec<ElementInToe>) -> Toe {
        Toe { name, size, start_date, end_date, elements }
    }
}

#[derive(serde::Deserialize, Debug)]
pub enum Size {
    Division,
    Brigade,
    Regiment,
    Corps,
}

#[derive(serde::Deserialize, Debug)]
pub struct Element {
    pub name: String,
    pub class: ElementClass,
    pub cv: f32,
    pub accuracy: u32,
    pub range: u32,
    pub v_inf: u32,
    pub v_arm: u32,
}

impl Element {
    pub fn new(name: String, class: ElementClass, cv: f32, accuracy: u32, range: u32, v_inf: u32, v_arm: u32) -> Element {
        Element { name, class, cv, accuracy, range, v_inf, v_arm }
    }
}

#[derive(serde::Deserialize, Debug)]
pub enum ElementClass {
    Inf,
    LightTank,
    MedTank,
    MotInf,
    LightArt,
    AtGun,
}

#[derive(serde::Deserialize, Debug)]
pub struct LocationCoords {
    pub x: u32,
    pub y: u32,
}

#[derive(serde::Deserialize, Debug)]
pub struct OffmapLocationName {
    pub name: String
}
