use conformance::*;
use geometry::*;
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use shared::*;
use std::collections::HashMap;
use std::collections::HashSet;

enum GenError {
    Error(String),
}

struct Chart {
    map: DefaultMap,
}

impl Chart {
    // fn generate(seed: Seed, params: &Params) -> Result<Chart, GenError> {
    //     let mut rng = StdRng::seed_from_u64(seed);
    //
    //     if params.archipelago_radius <= params.vision_range {
    //         return Err(GenError::Error("Archipelago radius does not exceed vision range".to_string()));
    //     }
    //     let current = Chart::plain_water_map(params.archipelago_radius);
    //     Chart::generate_islands(&mut rng, &current, params.island_count)
    // }
    fn plain_water_map(radius: u32) -> Result<DefaultMap, ()> {
        let mut map_cells: HashMap<Cell, Terrain> = HashMap::new();
        Cell { q: 0, r: 0 }.disc(radius).for_each(|cell| {
            map_cells.insert(cell, Terrain::Clear);
        });

        if let Ok(clear_map) = DefaultMap::try_new(radius, map_cells.into_iter()) {
            Ok(clear_map)
        } else {
            Err(())
        }
    }

    fn pick_random_cell(rng: &mut StdRng, i: impl Iterator<Item = Cell>) -> Option<Cell> {
        let collection: Vec<Cell> = i.collect();
        if collection.is_empty() {
            None
        } else {
            let random_index = rng.random_range(0..collection.len());
            collection.get(random_index).cloned()
        }
    }

    fn roll(rng: &mut StdRng, count: u32) -> bool {
        let n: f64 = 0.60;
        let decay: f64 = 0.75;

        let step = count.saturating_sub(1);
        let prob = (n * decay.powi(step as i32)).clamp(0.0, 1.0);
        rng.random_bool(prob)
    }

    fn neighbours_of_blob(map: &DefaultMap, blob: impl Iterator<Item = Cell>) -> impl Iterator<Item = Cell> {
        let blob_collection: HashSet<Cell> = blob.filter(|cell| map.contains(*cell)).collect();
        let mut blob_neighbours: HashSet<Cell> = HashSet::new();
        blob_collection.iter().for_each(|cell| {
            cell.neighbours()
                .into_iter()
                .filter(|neighbour| !blob_collection.contains(neighbour) && map.contains(*neighbour))
                .for_each(|neighbour| {
                    blob_neighbours.insert(neighbour);
                });
        });

        blob_neighbours.into_iter()
    }

    fn island_expansion(map: &DefaultMap, i: impl Iterator<Item = Cell>) -> impl Iterator<Item = Cell> {
        let legal_expansion_collection: HashSet<Cell> = Chart::legal_island_placements(map).collect();
        Chart::neighbours_of_blob(map, i).filter(move |neighbour| legal_expansion_collection.contains(neighbour))
    }

    fn generate_islands(rng: &mut StdRng, map: &DefaultMap, island_count: u8) -> Result<DefaultMap, ()> {
        let mut current_map = map.clone();

        for island_id in 0..island_count {
            let mut new_island: HashMap<Cell, Terrain> = HashMap::new();
            let mut count = 1;

            if let Some(island_starting_cell) =
                Chart::pick_random_cell(rng, Chart::legal_island_placements(&current_map))
            {
                new_island.insert(island_starting_cell, Terrain::Island(IslandId(island_id)));
            } else {
                return Err(());
            }

            while Chart::roll(rng, count) {
                count += 1
            }

            for _ in 0..count {
                let possible_expansions = Chart::island_expansion(&current_map, new_island.keys().cloned());
                if let Some(random_expansion) = Chart::pick_random_cell(rng, possible_expansions) {
                    new_island.insert(random_expansion, Terrain::Island(IslandId(island_id)));
                }
            }

            current_map = Chart::apply_modifications(&current_map, new_island)?;
        }

        Ok(current_map)
    }

    fn cells(map: &DefaultMap) -> impl Iterator<Item = Cell> {
        Cell { q: 0, r: 0 }.disc(map.radius())
    }

    fn apply_modifications(current: &DefaultMap, modifications: HashMap<Cell, Terrain>) -> Result<DefaultMap, ()> {
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

        if let Ok(new_map) = DefaultMap::try_new(current.radius(), new_terrains.into_iter()) {
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
                .for_each(|neighbour| f(neighbour));
        })
    }

    fn legal_island_placements(map: &DefaultMap) -> impl Iterator<Item = Cell> {
        let mut illegal_island_placements: HashSet<Cell> = HashSet::new();
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
