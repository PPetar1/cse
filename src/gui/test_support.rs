//! Shared test fixtures for the `gui` module family (cf.
//! `game/test_support.rs`): a minimal two-player scenario builder and a
//! `GuiApp` with an empty shared game. Compiled only for tests.

use super::GuiApp;

pub(super) fn minimal_scenario(units: &str) -> String {
    let map_path = concat!(env!("CARGO_MANIFEST_DIR"), "/maps/basic_map.map");
    format!(r#"
name = "gui test scenario"
game_version = "0.1.0"
map = "{map_path}"
start_date = "1941-06-22"
turn_length = 7

[[players]]
faction_name = "Axis"
faction_tag = "AX"
[[players]]
faction_name = "Soviet Union"
faction_tag = "SU"

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

pub(super) fn app() -> GuiApp {
    GuiApp::new(crate::new_shared_game())
}
