//! Pure A* pathfinding over a `Map`. Deliberately pure like `combat.rs`/
//! `supply.rs` — the game layer supplies terrain costs and blocking through
//! `enter_cost` and hands in plain coordinates; this module never touches
//! `Game`/`State`.

use crate::core::location::{hex_to_coords, Location};
use crate::core::map::Map;

/// Total cost of the cheapest path between two on-map hexes, where
/// `enter_cost` prices entering a hex (None = impassable/blocked). Returns
/// None when no route exists. The start hex is never "entered", so it is
/// always free — a unit can path out of terrain it could not path into.
pub fn cheapest_path_cost(
    map: &Map,
    from: (u32, u32),
    to: (u32, u32),
    enter_cost: impl Fn((u32, u32), &Location) -> Option<u32>,
) -> Option<u32> {
    let start = map.get_location(from.0, from.1)?.hex()?;
    let goal = map.get_location(to.0, to.1)?.hex()?;
    let cost_to_enter = |hex| {
        if hex == start {
            return Some(0);
        }
        let coords = hex_to_coords(hex)?;
        let location = map.get_location(coords.0, coords.1)?;
        enter_cost(coords, location)
    };
    let path = hexx::algorithms::a_star(start, goal, |_, entered| cost_to_enter(entered))?;
    // The path includes the start hex, which is not entered.
    path.iter().skip(1).map(|&hex| cost_to_enter(hex)).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::location::{Terrain, TerrainCosts};

    // The shipped map (maps/basic_map.map): a 10x8 grid, mostly open Plains
    // with a Hills/Forest/Swamp/Mountain scatter and two Water hexes at
    // (1, 3) and (6, 0), the only terrain impassable by default.
    fn test_map() -> Map {
        let contents = std::fs::read_to_string(
            concat!(env!("CARGO_MANIFEST_DIR"), "/maps/basic_map.map"),
        ).unwrap();
        Map::map_from_string(&contents).unwrap()
    }

    fn default_cost(_coords: (u32, u32), location: &Location) -> Option<u32> {
        TerrainCosts::new(Default::default()).cost(location.terrain)
    }

    #[test]
    fn the_start_hex_is_free() {
        let map = test_map();

        // A single step onto adjacent Plains costs exactly the destination's
        // terrain cost — the start hex itself is never charged.
        let cost = cheapest_path_cost(&map, (0, 0), (1, 0), default_cost);

        assert_eq!(cost, Some(1));
    }

    #[test]
    fn cost_sums_over_a_multi_hex_route() {
        let map = test_map();

        // (0, 0) to (2, 0): two Plains steps via (1, 0), 1 MP each.
        let cost = cheapest_path_cost(&map, (0, 0), (2, 0), default_cost);

        assert_eq!(cost, Some(2));
    }

    #[test]
    fn no_route_through_impassable_terrain_returns_none() {
        let map = test_map();
        let terrain_costs = TerrainCosts::new([(Terrain::Plains, 0)].into_iter().collect());

        // Plains impassable, and (3, 3) is reachable only through Plains
        // from (1, 2) — no route survives.
        let cost = cheapest_path_cost(&map, (1, 2), (3, 3), |_, location| terrain_costs.cost(location.terrain));

        assert_eq!(cost, None);
    }

    #[test]
    fn enter_cost_can_block_specific_hexes_regardless_of_terrain() {
        let map = test_map();
        let blocked = (1, 0);

        // Otherwise-passable (1, 0) is blocked directly by enter_cost (not
        // by terrain), forcing a detour that costs more than the 1 MP a
        // straight line would.
        let cost = cheapest_path_cost(&map, (0, 0), (2, 0), |coords, location| {
            if coords == blocked {
                return None;
            }
            default_cost(coords, location)
        });

        assert!(cost.unwrap() > 2);
    }

    #[test]
    fn an_invalid_start_or_destination_returns_none() {
        let map = test_map();

        assert_eq!(cheapest_path_cost(&map, (99, 99), (1, 0), default_cost), None);
        assert_eq!(cheapest_path_cost(&map, (0, 0), (99, 99), default_cost), None);
    }
}
