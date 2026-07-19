//! A simple rule-based opponent, wired in wherever a scenario marks a
//! player's `controller = "Ai"` (invoked from
//! `session::play_pending_ai_turns`; `Command` lives in
//! `terminal/command.rs`). It
//! consumes `Game` exactly the way `command.rs` does — no privileged access,
//! no new pathfinding or combat logic — so a stronger AI can replace the
//! decision-making here later without touching how it's invoked.
//!
//! Per faction stack: attack the best adjacent enemy if `simulate` predicts
//! favorable odds; otherwise step toward the nearest unclaimed objective
//! hex, or the nearest enemy unit if the scenario has none.

use std::collections::BTreeMap;
use std::fmt::Display;

use rand::Rng;

use crate::core::unit::UnitLocation;
use crate::game::Game;

/// A `simulate` prediction of at least this defender-retreat rate is
/// "favorable enough" to attack. Conservative on purpose — a prototype AI
/// that only takes clearly winning fights is easier to reason about (and to
/// trust) than one that gambles.
const ATTACK_RETREAT_THRESHOLD: f32 = 0.6;
const SIMULATION_RUNS: u32 = 20;

/// Play the on-turn faction's turn: decide and execute one action per stack
/// of units, then hand back a report of what happened.
pub(crate) fn take_turn(game: &mut Game, rng: &mut impl Rng) -> AiTurnReport {
    let faction = game.current_faction().to_string();
    let mut log = Vec::new();

    for (from, unit_names) in faction_stacks(game, &faction) {
        if let Some(to) = best_attack(game, from, &faction, rng) {
            if let Ok(report) = game.attack(from, to, rng) {
                log.push(format!("{:?} attacks {:?}:\n{}", from, to, indent(&report.to_string())));
            }
            continue;
        }

        let Some(target) = priority_target(game, &faction, from) else { continue };
        for name in unit_names {
            if let Some(to) = move_toward(game, from, target, &name) {
                log.push(format!("{name} moves {:?} -> {:?}", from, to));
            }
        }
    }

    AiTurnReport { faction, log }
}

/// The faction's on-map units grouped by hex. A `BTreeMap` keeps hex order
/// deterministic, matching the project-wide rule against feeding an RNG from
/// unordered iteration.
fn faction_stacks(game: &Game, faction: &str) -> Vec<((u32, u32), Vec<String>)> {
    let mut stacks: BTreeMap<(u32, u32), Vec<String>> = BTreeMap::new();
    for unit in game.units_of_faction(faction) {
        if let UnitLocation::OnMap(coords) = &unit.location {
            stacks.entry((coords.x, coords.y)).or_default().push(unit.name.clone());
        }
    }
    stacks.into_iter().collect()
}

/// The best adjacent enemy hex to attack from `from`, if any `simulate`
/// prediction clears `ATTACK_RETREAT_THRESHOLD`.
fn best_attack(game: &Game, from: (u32, u32), faction: &str, rng: &mut impl Rng) -> Option<(u32, u32)> {
    let mut best: Option<((u32, u32), f32)> = None;
    for (x, y) in game.adjacent(from.0, from.1) {
        let Some(location) = game.location(x, y) else { continue };
        let defenders = game.units_at_location(location);
        if defenders.is_empty() || defenders.iter().any(|unit| unit.faction == faction) {
            continue;
        }
        let Ok(report) = game.simulate(from, (x, y), SIMULATION_RUNS, rng) else { continue };
        let retreat_rate = report.retreats as f32 / report.runs as f32;
        if retreat_rate < ATTACK_RETREAT_THRESHOLD {
            continue;
        }
        let better = match best {
            Some((_, best_rate)) => retreat_rate > best_rate,
            None => true,
        };
        if better {
            best = Some(((x, y), retreat_rate));
        }
    }
    best.map(|(coords, _)| coords)
}

/// Where a stack at `from` should head: the nearest victory hex this faction
/// doesn't already hold, or — absent any objectives (or with all of them
/// already held) — the nearest enemy unit. None if neither exists (nothing
/// left to do).
fn priority_target(game: &Game, faction: &str, from: (u32, u32)) -> Option<(u32, u32)> {
    let objective = game.victory_hexes().into_iter()
        .filter(|hex| !game.hex_controlled_by(faction, hex.x, hex.y))
        .filter_map(|hex| Some(((hex.x, hex.y), game.distance(from, (hex.x, hex.y))?)))
        .min_by_key(|&(_, distance)| distance);
    if let Some((coords, _)) = objective {
        return Some(coords);
    }

    game.units_not_of_faction(faction).into_iter()
        .filter_map(|unit| match &unit.location {
            UnitLocation::OnMap(coords) => Some(((coords.x, coords.y), game.distance(from, (coords.x, coords.y))?)),
            UnitLocation::Offmap(_) => None,
        })
        .min_by_key(|&(_, distance)| distance)
        .map(|(coords, _)| coords)
}

/// Move the named unit at `from` toward `target`: straight there if
/// reachable this turn (letting `move_unit`'s own cheapest-path costing
/// decide), otherwise the single neighbouring hex that most closes the
/// distance. Returns the hex actually reached, or None if the unit is stuck
/// (no MP, nowhere passable) or no longer at `from`. All validation —
/// terrain, occupancy, MP — is `move_unit`'s; this only picks candidates and
/// trusts its answer.
fn move_toward(game: &mut Game, from: (u32, u32), target: (u32, u32), name: &str) -> Option<(u32, u32)> {
    let unit_i = unit_index_at(game, from, name)?;

    if game.move_unit(from.0, from.1, target.0, target.1, unit_i).is_ok() {
        return Some(target);
    }

    game.location(target.0, target.1)?;
    let mut candidates = game.adjacent(from.0, from.1);
    candidates.sort_by_key(|&(x, y)| (game.distance((x, y), target).unwrap_or(u32::MAX), x, y));

    for (x, y) in candidates {
        if game.move_unit(from.0, from.1, x, y, unit_i).is_ok() {
            return Some((x, y));
        }
    }
    None
}

/// The named unit's index in `units_at_location`'s name-sorted order at
/// `from` — the index `move_unit` expects. Resolved fresh per `move_toward`
/// call: earlier stack members may already have moved away, shifting later
/// units' indices.
fn unit_index_at(game: &Game, from: (u32, u32), name: &str) -> Option<usize> {
    let location = game.location(from.0, from.1)?;
    game.units_at_location(location).iter().position(|unit| unit.name == name)
}

fn indent(text: &str) -> String {
    text.lines().map(|line| format!("  {line}")).collect::<Vec<_>>().join("\n")
}

/// What one faction's AI-controlled turn did — returned through
/// `session.rs`, printed by the terminal and logged by the GUI the same way
/// a human's `attack`/`end_turn` output is, so the player can always see why
/// the AI did what it did (transparency is a project pillar, not just a
/// human-player nicety).
pub(crate) struct AiTurnReport {
    faction: String,
    log: Vec<String>,
}

impl Display for AiTurnReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "=== {} AI turn ===", self.faction)?;
        if self.log.is_empty() {
            write!(f, "(no actions)")
        } else {
            write!(f, "{}", self.log.join("\n"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::unit::LocationCoords;
    use crate::game::Game;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    fn minimal_scenario(players: &str, units: &str, extra: &str) -> String {
        let map_path = concat!(env!("CARGO_MANIFEST_DIR"), "/maps/basic_map.map");
        format!(r#"
name = "ai test scenario"
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
{extra}
"#)
    }

    const AI_VS_HUMAN: &str = r#"
[[factions]]
faction_name = "Axis"
faction_tag = "AX"
[[factions]]
faction_name = "Soviet Union"
faction_tag = "SU"
[[players]]
name = "Axis"
faction = "AX"
controller = "Ai"
"#;

    #[test]
    fn current_player_is_ai_reads_the_controller_with_a_human_default() {
        let units = r#"
[[units]]
name = "Axis Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }
"#;
        let mut game = Game::build(minimal_scenario(AI_VS_HUMAN, units, "")).unwrap();
        assert!(game.current_player_is_ai());

        game.end_turn();
        assert!(!game.current_player_is_ai());
    }

    #[test]
    fn an_ai_stack_attacks_a_weak_adjacent_enemy_at_favorable_odds() {
        // Three Axis divisions (AI) stacked against one weak Soviet division:
        // simulate should read this as heavily favorable and attack.
        let units = r#"
[[units]]
name = "Axis First"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }

[[units]]
name = "Axis Second"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }

[[units]]
name = "Axis Third"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }

[[units]]
name = "Soviet Division"
toe = "test_toe"
faction = "SU"
location = { x = 2, y = 1 }
morale = 0
"#;
        let mut game = Game::build(minimal_scenario(AI_VS_HUMAN, units, "")).unwrap();
        let mut rng = StdRng::seed_from_u64(1);

        let report = take_turn(&mut game, &mut rng);

        assert!(report.log.iter().any(|line| line.contains("attacks")));
        // The Soviet stack lost the hex one way or another: retreated,
        // routed, shattered or surrendered — either way it isn't standing
        // at (2, 1) unmolested, and the winning Axis units hold ground.
        let axis_locations: Vec<_> = game.units_of_faction("AX").iter()
            .map(|unit| unit.location.clone())
            .collect();
        assert!(axis_locations.contains(&UnitLocation::OnMap(LocationCoords { x: 2, y: 1 })));
    }

    #[test]
    fn an_ai_unit_moves_toward_the_nearest_unclaimed_objective() {
        let units = r#"
[[units]]
name = "Axis Division"
toe = "test_toe"
faction = "AX"
location = { x = 0, y = 0 }
"#;
        let victory = r#"
[victory_conditions]
[[victory_conditions.hexes]]
x = 9
y = 7
points = 10
"#;
        let mut game = Game::build(minimal_scenario(AI_VS_HUMAN, units, victory)).unwrap();
        let mut rng = StdRng::seed_from_u64(1);

        let start_distance = game.distance((0, 0), (9, 7)).unwrap();

        let report = take_turn(&mut game, &mut rng);

        let unit = game.unit("Axis Division").unwrap();
        let UnitLocation::OnMap(coords) = &unit.location else {
            panic!("unit left the map");
        };
        let (x, y) = (coords.x, coords.y);
        assert!(game.distance((x, y), (9, 7)).unwrap() < start_distance);
        assert!(report.log.iter().any(|line| line.contains("moves")));
    }

    #[test]
    fn a_stuck_lead_unit_does_not_block_its_stacks_other_units_from_moving() {
        // "Axis A" sorts before "Axis B" (units_at_location's name order),
        // so it's index 0 at the stack's hex — the index move_toward used to
        // always address, regardless of which unit it was actually iterating
        // for. Its TOE has 0 MP, so it can never move; "Axis B" has the
        // normal budget and should still get moved toward the target.
        let units = r#"
[[toe]]
name = "stuck_toe"
size = "Division"
mp = 0
start_date = "1941-01-01"
end_date = "1941-08-01"
[[toe.elements]]
name = "test_element"
amount = 10

[[units]]
name = "Axis A"
toe = "stuck_toe"
faction = "AX"
location = { x = 0, y = 0 }

[[units]]
name = "Axis B"
toe = "test_toe"
faction = "AX"
location = { x = 0, y = 0 }

[[units]]
name = "Soviet Division"
toe = "test_toe"
faction = "SU"
location = { x = 9, y = 7 }
"#;
        let mut game = Game::build(minimal_scenario(AI_VS_HUMAN, units, "")).unwrap();
        let mut rng = StdRng::seed_from_u64(1);

        let report = take_turn(&mut game, &mut rng);

        let a = game.unit("Axis A").unwrap();
        assert_eq!(a.location, UnitLocation::OnMap(LocationCoords { x: 0, y: 0 }));

        let b = game.unit("Axis B").unwrap();
        let UnitLocation::OnMap(coords) = &b.location else {
            panic!("unit left the map");
        };
        assert_ne!((coords.x, coords.y), (0, 0));

        // Every logged move line names the unit that actually moved.
        assert!(report.log.iter().all(|line| !line.starts_with("Axis A moves")));
        assert!(report.log.iter().any(|line| line.starts_with("Axis B moves")));
    }

    #[test]
    fn an_ai_unit_moves_toward_the_nearest_enemy_without_objectives() {
        let units = r#"
[[units]]
name = "Axis Division"
toe = "test_toe"
faction = "AX"
location = { x = 0, y = 0 }

[[units]]
name = "Soviet Division"
toe = "test_toe"
faction = "SU"
location = { x = 9, y = 7 }
"#;
        let mut game = Game::build(minimal_scenario(AI_VS_HUMAN, units, "")).unwrap();
        let mut rng = StdRng::seed_from_u64(1);

        let start_distance = game.distance((0, 0), (9, 7)).unwrap();

        take_turn(&mut game, &mut rng);

        let unit = game.unit("Axis Division").unwrap();
        let UnitLocation::OnMap(coords) = &unit.location else {
            panic!("unit left the map");
        };
        let (x, y) = (coords.x, coords.y);
        assert!(game.distance((x, y), (9, 7)).unwrap() < start_distance);
    }
}
