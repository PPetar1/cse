use time::Date;

use crate::core::location::Location;

pub struct Unit<'a> {
    name: String,
    toe: &'a Toe<'a>,
    faction: String,
    location: &'a Location,
    pub elements: Vec<(&'a Element, u32, u32)>// Tuple holds the reference to the element in question,
                                          // number of ready elements in unit, number of damaged
                                          // elements in unit (&ele, rdy, dmg)
}

impl<'a> Unit<'a> {
    pub fn new(name: String, toe: &'a Toe<'a>, faction: String, location: &'a Location, elements: Vec<(&'a Element, u32, u32)>) -> Unit<'a> {
        Unit { name, toe, faction, location, elements }
    }
}

pub struct Toe<'a> {
    name: String,
    size: Size,
    start_date: Date,
    end_date: Date,
    elements: Vec<(&'a Element, u32)>,// Tuple holds the reference to the element in question,
                                      // number of elements the toe prescribes 
}

impl<'a> Toe<'a> {
    pub fn new(&self, name: String, size: Size, start_date: Date, end_date: Date, elements: Vec<(&'a Element, u32)>) -> Toe<'a> {
        Toe { name, size, start_date, end_date, elements }
    }
}

pub enum Size {
    Division,
    Brigade,
    Regiment,
    Corps,
}

#[derive(serde::Deserialize)]
pub struct Element {
    name: String,
    class: ElementClass,
    cv: f32,
    accuracy: u32,
    range: u32,
    v_inf: u32,
    v_arm: u32,
}

impl Element {
    pub fn new(name: String, class: ElementClass, cv: f32, accuracy: u32, range: u32, v_inf: u32, v_arm: u32) -> Element {
        Element { name, class, cv, accuracy, range, v_inf, v_arm }
    }
}

#[derive(serde::Deserialize)]
pub enum ElementClass {
    Inf,
    LightTank,
    MedTank,
    MotInf,
    LightArt,
    AtGun,
}
