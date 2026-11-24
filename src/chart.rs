use geometry::{Cell, DefaultMap, Map, Terrain};
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use shared::{IslandId, Params, Seed};
use std::collections::{BTreeMap, BTreeSet, HashSet};

#[derive(Debug)]
pub enum GenError {
    Error(String),
}

pub struct Chart {
    map: DefaultMap,
    treasure_cell: Cell,
}

impl Chart {
    pub fn map(&self) -> &DefaultMap {
        &self.map
    }

    pub fn generate(seed: Seed, params: &Params) -> Result<Chart, GenError> {
        let mut rng = StdRng::seed_from_u64(seed);

        if params.archipelago_radius <= params.vision_range {
            return Err(GenError::Error(
                "Archipelago radius does not exceed vision range".to_string(),
            ));
        }

        let mut current = Chart::plain_water_map(params.archipelago_radius)?;
        current = Chart::generate_islands(&mut rng, &current, params.island_count)?;
        current = Chart::generate_cliffs(&mut rng, &current, params.cliff_count)?;
        current = Chart::generate_rocks(&mut rng, &current, params.rock_count)?;

        if !Chart::check_connected_water(&current) {
            return Err(GenError::Error("Water cells were not connected".to_string()));
        }

        let chosen_treasure = Chart::choose_treasure(&mut rng, &current);

        if chosen_treasure.is_none() {
            return Err(GenError::Error("No suitable treasure cell was found".to_string()));
        }

        Ok(Self {
            map: current,
            treasure_cell: chosen_treasure.unwrap(),
        })
    }

    fn choose_treasure(rng: &mut StdRng, map: &DefaultMap) -> Option<Cell> {
        Chart::pick_random_cell(
            rng,
            map.water_cells()
                .into_iter()
                .filter(|cell| matches!(map.terrain(*cell), Some(Terrain::Clear))),
        )
    }

    fn water_dfs_rec(current_cell: Cell, map: &DefaultMap, current_cc: &mut HashSet<Cell>) {
        if current_cc.insert(current_cell) {
            current_cell
                .neighbours()
                .into_iter()
                .filter(|neighbour| map.contains(*neighbour) && !current_cc.contains(neighbour))
                .collect::<Vec<Cell>>()
                .into_iter()
                .for_each(|neighbour| Chart::water_dfs_rec(neighbour, map, current_cc));
        }
    }

    fn water_dfs(starting_cell: Cell, map: &DefaultMap) -> HashSet<Cell> {
        let mut connected_water_cells: HashSet<Cell> = HashSet::new();
        Chart::water_dfs_rec(starting_cell, map, &mut connected_water_cells);
        connected_water_cells
    }

    fn water_is_connected(starting_cell: Cell, map: &DefaultMap) -> bool {
        let connected_cells = Chart::water_dfs(starting_cell, map);
        map.water_cells().all(|cell| connected_cells.contains(&cell))
    }

    fn check_connected_water(map: &DefaultMap) -> bool {
        let starting_cell = map.water_cells().next();
        if let Some(starting_cell) = starting_cell {
            Chart::water_is_connected(starting_cell, map)
        } else {
            false
        }
    }

    fn legal_rock_placements(map: &DefaultMap) -> impl Iterator<Item = Cell> {
        let mut illegal_rock_placements: BTreeSet<Cell> = BTreeSet::new();
        let mut insert_into_illegal = |cell| {
            illegal_rock_placements.insert(cell);
        };

        Chart::for_each_cell_and_neighbour(map.corners().into_iter(), map, &mut insert_into_illegal);
        Chart::cells(map)
            .filter(|cell| {
                let cell_terrain = map.terrain(*cell);
                matches!(cell_terrain, Some(Terrain::Island(_))) || matches!(cell_terrain, Some(Terrain::Cliff))
            })
            .for_each(|illegal_cell| {
                insert_into_illegal(illegal_cell);
            });

        Chart::cells(map).filter(move |candidate| !illegal_rock_placements.contains(candidate))
    }

    fn generate_rocks(rng: &mut StdRng, map: &DefaultMap, rock_count: u16) -> Result<DefaultMap, GenError> {
        let mut current_map = map.clone();

        for _ in 0..rock_count {
            let mut new_rocks: BTreeMap<Cell, Terrain> = BTreeMap::new();

            if let Some(rock_cell) = Chart::pick_random_cell(rng, Chart::legal_rock_placements(&current_map)) {
                new_rocks.insert(rock_cell, Terrain::Rock);
            } else {
                return Err(GenError::Error("Could not generate rocks".to_string()));
            }

            if let Ok(updated_map) = Chart::apply_modifications(&current_map, &new_rocks) {
                current_map = updated_map;
            } else {
                return Err(GenError::Error("Could not apply modifications".to_string()));
            }
        }

        Ok(current_map)
    }

    fn legal_cliff_placements(map: &DefaultMap) -> impl Iterator<Item = Cell> {
        let mut illegal_cliff_placements: BTreeSet<Cell> = BTreeSet::new();
        let mut insert_into_illegal = |cell| {
            illegal_cliff_placements.insert(cell);
        };

        Chart::for_each_cell_and_neighbour(map.corners().into_iter(), map, &mut insert_into_illegal);
        Chart::for_each_cell_and_neighbour(
            Chart::cells(map).filter(|cell| matches!(map.terrain(*cell), Some(Terrain::Island(_)))),
            map,
            &mut insert_into_illegal,
        );

        Chart::cells(map)
            .filter(|cell| matches!(map.terrain(*cell), Some(Terrain::Cliff)))
            .for_each(insert_into_illegal);
        Chart::cells(map).filter(move |candidate| !illegal_cliff_placements.contains(candidate))
    }

    fn generate_cliffs(rng: &mut StdRng, map: &DefaultMap, cliff_count: u16) -> Result<DefaultMap, GenError> {
        let mut current_map = map.clone();

        for _ in 0..cliff_count {
            let mut new_cliffs: BTreeMap<Cell, Terrain> = BTreeMap::new();

            if let Some(cliff_cell) = Chart::pick_random_cell(rng, Chart::legal_cliff_placements(&current_map)) {
                new_cliffs.insert(cliff_cell, Terrain::Cliff);
            } else {
                return Err(GenError::Error("Could not generate cliffs".to_string()));
            }

            if let Ok(updated_map) = Chart::apply_modifications(&current_map, &new_cliffs) {
                current_map = updated_map;
            } else {
                return Err(GenError::Error("Could not apply modifications".to_string()));
            }
        }

        Ok(current_map)
    }

    fn plain_water_map(radius: u32) -> Result<DefaultMap, GenError> {
        let mut map_cells: BTreeMap<Cell, Terrain> = BTreeMap::new();
        Cell { q: 0, r: 0 }.disc(radius).for_each(|cell| {
            map_cells.insert(cell, Terrain::Clear);
        });

        if let Ok(clear_map) = DefaultMap::try_new(radius, map_cells) {
            Ok(clear_map)
        } else {
            Err(GenError::Error("Could not generate plain water map".to_string()))
        }
    }

    fn pick_random_cell(rng: &mut StdRng, i: impl Iterator<Item = Cell>) -> Option<Cell> {
        let collection: Vec<Cell> = i.collect();
        if collection.is_empty() {
            None
        } else {
            let random_index = rng.random_range(0..collection.len());
            collection.get(random_index).copied()
        }
    }

    fn roll(rng: &mut StdRng, count: u32) -> bool {
        let n: f64 = 0.60;
        let decay: f64 = 0.75;

        let step = f64::from(count.saturating_sub(1));
        let prob = (n * decay.powf(step)).clamp(0.0, 1.0);
        rng.random_bool(prob)
    }

    fn neighbours_of_blob(map: &DefaultMap, blob: impl Iterator<Item = Cell>) -> impl Iterator<Item = Cell> {
        let blob_collection: BTreeSet<Cell> = blob.filter(|cell| map.contains(*cell)).collect();
        let mut blob_neighbours: BTreeSet<Cell> = BTreeSet::new();
        for cell in &blob_collection {
            cell.neighbours()
                .into_iter()
                .filter(|neighbour| !blob_collection.contains(neighbour) && map.contains(*neighbour))
                .for_each(|neighbour| {
                    blob_neighbours.insert(neighbour);
                });
        }

        blob_neighbours.into_iter()
    }

    fn island_expansion(map: &DefaultMap, i: impl Iterator<Item = Cell>) -> impl Iterator<Item = Cell> {
        let legal_expansion_collection: BTreeSet<Cell> = Chart::legal_island_placements(map).collect();
        Chart::neighbours_of_blob(map, i).filter(move |neighbour| legal_expansion_collection.contains(neighbour))
    }

    fn generate_islands(rng: &mut StdRng, map: &DefaultMap, island_count: u8) -> Result<DefaultMap, GenError> {
        let mut current_map = map.clone();

        for island_id in 0..island_count {
            let mut new_island: BTreeMap<Cell, Terrain> = BTreeMap::new();
            let mut count = 1;

            if let Some(island_starting_cell) =
                Chart::pick_random_cell(rng, Chart::legal_island_placements(&current_map))
            {
                new_island.insert(island_starting_cell, Terrain::Island(IslandId(island_id)));
            } else {
                return Err(GenError::Error("Could not generate islands".to_string()));
            }

            while Chart::roll(rng, count) {
                count += 1;
            }

            for _ in 0..count {
                let possible_expansions = Chart::island_expansion(&current_map, new_island.keys().copied());
                if let Some(random_expansion) = Chart::pick_random_cell(rng, possible_expansions) {
                    new_island.insert(random_expansion, Terrain::Island(IslandId(island_id)));
                }
            }

            if let Ok(updated_map) = Chart::apply_modifications(&current_map, &new_island) {
                current_map = updated_map;
            } else {
                return Err(GenError::Error("Could not generate island map".to_string()));
            }
        }

        Ok(current_map)
    }

    fn cells(map: &DefaultMap) -> impl Iterator<Item = Cell> {
        Cell { q: 0, r: 0 }.disc(map.radius())
    }

    fn apply_modifications(current: &DefaultMap, modifications: &BTreeMap<Cell, Terrain>) -> Result<DefaultMap, ()> {
        let mut new_terrains: Vec<(Cell, Terrain)> = Vec::new();
        for cell in Chart::cells(current) {
            match modifications.get(&cell) {
                Some(modified_terrain) => new_terrains.push((cell, *modified_terrain)),
                None => {
                    if let Some(original_terrain) = current.terrain(cell) {
                        new_terrains.push((cell, original_terrain));
                    } else {
                        return Err(());
                    }
                }
            }
        }

        if let Ok(new_map) = DefaultMap::try_new(current.radius(), new_terrains) {
            Ok(new_map)
        } else {
            Err(())
        }
    }

    fn for_each_cell_and_neighbour<F>(i: impl Iterator<Item = Cell>, map: &DefaultMap, mut f: F)
    where
        F: FnMut(Cell),
    {
        i.filter(|cell| map.contains(*cell)).for_each(|cell| {
            f(cell);
            cell.neighbours()
                .into_iter()
                .filter(|neighbour| map.contains(*neighbour))
                .for_each(&mut f);
        });
    }

    fn legal_island_placements(map: &DefaultMap) -> impl Iterator<Item = Cell> {
        let mut illegal_island_placements: BTreeSet<Cell> = BTreeSet::new();
        let mut insert_into_illegal = |cell| {
            illegal_island_placements.insert(cell);
        };

        Chart::for_each_cell_and_neighbour(map.corners().into_iter(), map, &mut insert_into_illegal);
        Chart::for_each_cell_and_neighbour(
            Chart::cells(map).filter(|cell| matches!(map.terrain(*cell), Some(Terrain::Island(_)))),
            map,
            &mut insert_into_illegal,
        );

        Chart::cells(map).filter(move |candidate| !illegal_island_placements.contains(candidate))
    }
}

#[cfg(test)]
mod tests {
    use super::Chart;
    use geometry::*;
    use shared::*;
    use std::collections::BTreeSet;
    const TEST_RADII: [u32; 8] = [1, 2, 3, 12, 40, 60, 80, 100];

    fn corner_zone(map: &DefaultMap) -> BTreeSet<Cell> {
        let mut corners_and_neighbours: BTreeSet<Cell> = BTreeSet::new();
        Chart::for_each_cell_and_neighbour(map.corners().into_iter(), map, |cell| {
            corners_and_neighbours.insert(cell);
        });
        corners_and_neighbours
    }

    fn all_cells_matching<F>(map: &DefaultMap, predicate: F) -> BTreeSet<Cell>
    where
        F: Fn(&Cell) -> bool,
    {
        Chart::cells(map).filter(predicate).collect()
    }

    fn get_random_seeds(n: u32) -> Vec<Seed> {
        (0..n).map(|_| rand::random()).collect()
    }

    #[test]
    fn plain_water_map_is_all_clear() {
        for radius in TEST_RADII {
            let map = Chart::plain_water_map(radius).expect("Failed to construct plain water map");
            assert_eq!(map.radius(), radius);
            let cells: Vec<Cell> = Chart::cells(&map).collect();
            assert!(cells.iter().all(|cell| {
                if let Some(terrain) = map.terrain(*cell) {
                    terrain == Terrain::Clear
                } else {
                    false
                }
            }));
        }
    }

    #[test]
    fn generation_depends_on_seed_only() {
        let seeds = get_random_seeds(10);
        for seed in seeds {
            let chart = Chart::generate(seed, &Params::default()).expect("Failed to generate map");
            let same_seed_chart = Chart::generate(seed, &Params::default()).expect("Failed to generate map");
            let different_seed_chart =
                Chart::generate(rand::random(), &Params::default()).expect("Failed to generate map");

            let map = chart.map();
            let same_seed_map = same_seed_chart.map();
            let different_seed_map = different_seed_chart.map();

            let terrains_map: Vec<_> = Chart::cells(map).map(|c| map.terrain(c)).collect();
            let terrains_same_seed_map: Vec<_> =
                Chart::cells(same_seed_map).map(|c| same_seed_map.terrain(c)).collect();
            let terrains_different_seed_map: Vec<_> = Chart::cells(different_seed_map)
                .map(|c| different_seed_map.terrain(c))
                .collect();

            assert_eq!(terrains_map, terrains_same_seed_map);
            assert_ne!(terrains_map, terrains_different_seed_map);
        }
    }

    #[test]
    fn cliff_count_cliff_cells_are_generated() {
        let seeds = get_random_seeds(10);
        for seed in seeds {
            let params = Params::default();
            let chart = Chart::generate(seed, &params).expect("Failed to generate map");
            let map = chart.map();

            let generated_cliffs_count = Chart::cells(map)
                .map(|cell| map.terrain(cell))
                .filter(|cell_terrain| matches!(*cell_terrain, Some(Terrain::Cliff)))
                .count();
            assert_eq!(generated_cliffs_count, params.cliff_count as usize);
        }
    }

    #[test]
    fn island_count_islands_are_generated() {
        let seeds = get_random_seeds(10);
        for seed in seeds {
            let params = Params::default();
            let chart = Chart::generate(seed, &params).expect("Failed to generate map");
            let map = chart.map();

            let mut generated_island_ids: BTreeSet<IslandId> = BTreeSet::new();
            Chart::cells(map)
                .map(|cell| map.terrain(cell))
                .for_each(|cell_terrain| {
                    if let Some(Terrain::Island(x)) = cell_terrain {
                        generated_island_ids.insert(x);
                    }
                });
            let generated_islands_count = generated_island_ids.len();

            assert_eq!(generated_islands_count, params.island_count as usize);
        }
    }

    // let corner_zone = corner_zone(&map);
    // let islands_cells = all_cells_matching(&map, |cell| matches!(map.terrain(*cell), Some(Terrain::Island(_))));
    // for cell in Chart::legal_cliff_placements(&map) {
    //     let cell_terrain = map.terrain(cell).expect("Legal cliff placement lies outside of map bounds");
    //     assert_eq!(cell_terrain, Terrain::Clear);
    //     assert!(!corner_zone.contains(&cell));
    //     assert!(!islands_cells.contains(&cell));
    //     assert!(!islands_cells.iter().any(|i| i.neighbours().contains(&cell)));
    // }
}
