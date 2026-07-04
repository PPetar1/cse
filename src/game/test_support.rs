//! Shared test fixtures for the `game` module family: a minimal scenario
//! builder plus the player/unit TOML snippets the per-module test suites
//! compose it from. Compiled only for tests (`#[cfg(test)]` on the module
//! declaration in `game/mod.rs`).

use super::Game;

pub(super) const ONE_PLAYER: &str = r#"
[[players]]
faction_name = "Axis"
faction_tag = "AX"
"#;

pub(super) const TWO_PLAYERS: &str = r#"
[[players]]
faction_name = "Axis"
faction_tag = "AX"
[[players]]
faction_name = "Soviet Union"
faction_tag = "SU"
"#;

pub(super) const OPPOSING_UNITS: &str = r#"
[[units]]
name = "Axis Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }

[[units]]
name = "Soviet Division"
toe = "test_toe"
faction = "SU"
location = { x = 2, y = 1 }
"#;

pub(super) const ONMAP_UNIT: &str = r#"
[[units]]
name = "1st Test Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }
"#;

pub(super) const OFFMAP_UNIT: &str = r#"
[[units]]
name = "Reserve Division"
toe = "test_toe"
faction = "AX"
location = "GE Reserve"
"#;

pub(super) fn minimal_scenario(players: &str, units: &str) -> String {
    let map_path = concat!(env!("CARGO_MANIFEST_DIR"), "/maps/basic_map.map");
    format!(r#"
name = "test scenario"
game_version = "0.1.0"
map = "{map_path}"
start_date = "1941-06-22"
turn_length = 7
{players}

[[toe]]
name = "test_toe"
size = "Division"
mp = 16
start_date = "1941-01-01"
end_date = "1941-08-01"
[[toe.elements]]
name = "test_element"
amount = 10

[[elements]]
name = "test_element"
class = "Inf"
cv = 4.0
vulnerability = 100
[[elements.devices]]
name = "test_rifles"
accuracy = 20
range = 100
rate_of_fire = 1
soft_attack = 100
hard_attack = 3

{units}
"#)
}

pub(super) fn one_unit_game() -> Game {
    Game::build(minimal_scenario(ONE_PLAYER, ONMAP_UNIT)).unwrap()
}

/// Three stacked attacking divisions at (1, 1) vs one defender at (2, 1)
/// with the given morale — the standard "defender surely loses" setup.
pub(super) fn three_vs_one(defender_morale: u32) -> String {
    format!(r#"
[[units]]
name = "Axis First"
toe = "test_toe"
faction = "AX"
location = {{ x = 1, y = 1 }}

[[units]]
name = "Axis Second"
toe = "test_toe"
faction = "AX"
location = {{ x = 1, y = 1 }}

[[units]]
name = "Axis Third"
toe = "test_toe"
faction = "AX"
location = {{ x = 1, y = 1 }}

[[units]]
name = "Soviet Division"
toe = "test_toe"
faction = "SU"
location = {{ x = 2, y = 1 }}
morale = {defender_morale}
"#)
}
