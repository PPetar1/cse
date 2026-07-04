//! Pure supply-connectivity tracing: which hexes a faction can reach from
//! its supply sources without crossing impassable terrain or enemy-held
//! ground. Deliberately pure like `combat.rs` — the game layer looks up the
//! map/terrain costs and the enemy-occupied hex set itself and hands in
//! plain coordinates; this module never touches `Game`/`State`.

use std::collections::{HashSet, VecDeque};

use crate::core::location::TerrainCosts;
use crate::core::map::Map;

/// Every hex reachable from `sources` by crossing only passable terrain and
/// hexes not in `blocked` (typically enemy-occupied ground). A source hex
/// that is itself blocked is excluded from the seed set — a captured supply
/// depot supplies no one. Multi-source flood fill; MP cost doesn't matter
/// here, only topological connectivity.
pub fn reachable_hexes(
    map: &Map,
    terrain_costs: &TerrainCosts,
    sources: impl IntoIterator<Item = (u32, u32)>,
    blocked: &HashSet<(u32, u32)>,
) -> HashSet<(u32, u32)> {
    let mut visited = HashSet::new();
    let mut frontier = VecDeque::new();

    for source in sources {
        if blocked.contains(&source) || map.get_location(source.0, source.1).is_none() {
            continue;
        }
        if visited.insert(source) {
            frontier.push_back(source);
        }
    }

    while let Some((x, y)) = frontier.pop_front() {
        let Some(location) = map.get_location(x, y) else { continue };
        for neighbour_coords in location.neighbour_coords() {
            if visited.contains(&neighbour_coords) || blocked.contains(&neighbour_coords) {
                continue;
            }
            let Some(neighbour) = map.get_location(neighbour_coords.0, neighbour_coords.1) else {
                continue;
            };
            if terrain_costs.cost(neighbour.terrain).is_none() {
                continue; // impassable
            }
            visited.insert(neighbour_coords);
            frontier.push_back(neighbour_coords);
        }
    }

    visited
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::map::Map;

    // The shipped map (maps/basic_map.map): a 10x8 grid, mostly open Plains
    // with a Hills/Forest/Swamp/Mountain scatter and two Water hexes at
    // (1, 3) and (6, 0), the only terrain impassable by default.
    fn test_map() -> Map {
        let contents = std::fs::read_to_string(
            concat!(env!("CARGO_MANIFEST_DIR"), "/maps/basic_map.map"),
        ).unwrap();
        Map::map_from_string(&contents).unwrap()
    }

    #[test]
    fn a_source_hex_is_always_reachable() {
        let map = test_map();
        let terrain_costs = TerrainCosts::new(Default::default());

        let reachable = reachable_hexes(&map, &terrain_costs, [(0, 0)], &HashSet::new());

        assert!(reachable.contains(&(0, 0)));
    }

    #[test]
    fn reachability_spreads_across_open_terrain() {
        let map = test_map();
        let terrain_costs = TerrainCosts::new(Default::default());

        let reachable = reachable_hexes(&map, &terrain_costs, [(0, 0)], &HashSet::new());

        // Every non-Water hex is reachable with no blockers — Hills, Forest,
        // Swamp and Mountain all cost MP but aren't impassable.
        let passable_hexes = map.all_locations().len() - 2; // minus the two Water hexes
        assert_eq!(reachable.len(), passable_hexes);
        assert!(reachable.contains(&(9, 7))); // the far corner
    }

    #[test]
    fn impassable_terrain_blocks_the_flood() {
        let map = test_map();
        let terrain_costs = TerrainCosts::new(Default::default());

        let reachable = reachable_hexes(&map, &terrain_costs, [(0, 0)], &HashSet::new());

        // Water is never enterable, regardless of connectivity elsewhere.
        assert!(!reachable.contains(&(1, 3)));
        assert!(!reachable.contains(&(6, 0)));
    }

    #[test]
    fn enemy_occupied_hexes_block_the_flood() {
        let map = test_map();
        let terrain_costs = TerrainCosts::new(Default::default());
        // Wall off column x = 2 for the map's full height: nothing east of
        // it is reachable from a source west of it.
        let blocked: HashSet<(u32, u32)> = (0..=7).map(|y| (2, y)).collect();

        let reachable = reachable_hexes(&map, &terrain_costs, [(0, 0)], &blocked);

        assert!(reachable.contains(&(1, 1)));
        assert!(!reachable.contains(&(3, 1)));
        assert!(!reachable.contains(&(9, 7)));
    }

    #[test]
    fn a_blocked_source_hex_does_not_seed_the_flood() {
        let map = test_map();
        let terrain_costs = TerrainCosts::new(Default::default());
        let blocked: HashSet<(u32, u32)> = [(0, 0)].into_iter().collect();

        let reachable = reachable_hexes(&map, &terrain_costs, [(0, 0)], &blocked);

        assert!(reachable.is_empty());
    }

    #[test]
    fn multiple_sources_union_their_reachable_hexes() {
        let map = test_map();
        let terrain_costs = TerrainCosts::new(Default::default());
        // Isolate two pockets with a blocked column between them.
        let blocked: HashSet<(u32, u32)> = (0..=7).map(|y| (2, y)).collect();

        let reachable = reachable_hexes(&map, &terrain_costs, [(0, 0), (9, 7)], &blocked);

        // Both pockets are covered because each has its own source.
        assert!(reachable.contains(&(1, 1)));
        assert!(reachable.contains(&(9, 7)));
    }
}
