use crate::dummyisland::DummyIsland;
use clues::Axis;
use clues::{Bound, Clue};
use geometry::{Cell, DefaultMap, Map, Terrain};
use islandtrait::island::{Island, IslandInit};
use num_integer::Roots;
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use shared::{IslandId, Params, Seed};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};

const POOL_SIZE: usize = 256;
const POOL_ATTEMPTS_PER_CLUE: usize = 4;
const RESTARTS: usize = 32;
const RESIDUE: usize = 2;

// Custom struct type for clue generation.
// Keeps code a little cleaner.
struct ClueGenInfo<'a> {
    water: &'a [Cell],
    treasure: Cell,
    radius: i32,
    min_keep: usize,
}

#[derive(Debug)]
pub enum GenError {
    Error(String),
}

pub struct Chart {
    map: DefaultMap,
    treasure_cell: Cell,
    islands: Vec<Box<dyn Island>>,
}

impl Chart {
    pub fn map(&self) -> &DefaultMap {
        &self.map
    }

    pub fn generate(seed: Seed, params: &Params) -> Result<Self, GenError> {
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

        let treasure_cell = chosen_treasure.unwrap();

        let islands = Chart::create_islands(&mut rng, params.island_count, &current, treasure_cell, seed, params)?;

        Ok(Self {
            map: current,
            treasure_cell,
            islands,
        })
    }

    // This function selects the best clue from a clue pool generated a priori.
    // It does so by computing what is the ideal amount of culled cells based on how many candidates and steps are left;
    // scoring each clue by how much they get close to such target; then returning the first acceptable clue in the mentioned ranking.
    fn pick_clue(cgi: &ClueGenInfo<'_>, pool: &[Clue], candidates: &[Cell], steps_left: u8) -> Option<Clue> {
        let target = Chart::target_kept(candidates.len(), steps_left);

        let mut ranked: Vec<(usize, Clue)> = pool
            .iter()
            .filter_map(|clue| {
                // Amount of candidates after the clue gets applied.
                let kept = candidates.iter().filter(|cell| clue.holds(**cell)).count();
                // Whether the clue is admissible. If we are on the last step we need the clue to keep only one cell.
                // In any other case we want the clue to keep more than one candidate cell.
                let admissible = if steps_left == 1 { kept == 1 } else { kept > 1 };
                if admissible {
                    Some((kept.abs_diff(target), *clue))
                } else {
                    None
                }
            })
            .collect();

        // Sort the clues by how much they get close to the target.
        ranked.sort_by_key(|&(error, _)| error);

        // We look inside the ranking choosing the first clue that is not "overpowered".
        ranked
            .into_iter()
            .map(|(_, clue)| clue)
            .find(|clue| Chart::is_individually_weak(cgi, clue))
    }

    // This function return the amount of cells we would like the next generated clue to keep.
    fn target_kept(n: usize, steps_left: u8) -> usize {
        // If we have only one step left we must aim for keeping exactly 1 cell, the treasure one.
        if steps_left <= 1 {
            return 1;
        }

        // Otherwise we want to find the number ratio such that n / (ratio^(steps_left - 1)) is about RESIDUE.
        // In other words, we want to find the number such that, if we were to divide the number of cells n by it
        // (steps_left - 1) times, then we would get RESIDUE cells.
        // Such number is the (steps_left - 1)-root of (n / RESIDUE).
        let root = (n / RESIDUE).nth_root(u32::from(steps_left - 1)).max(1);
        (n / root).clamp(RESIDUE, n.saturating_sub(1))
    }

    // This function return whether a clue, taken by itself and itself only, is too powerful and culls too many cells at once.
    // This is not strictly needed and is more of a game design choice.
    // This can be totally bypassed by changing the min_keep field to 0.
    fn is_individually_weak(cgi: &ClueGenInfo<'_>, clue: &Clue) -> bool {
        cgi.water
            .iter()
            .filter(|cell| clue.holds(**cell))
            .take(cgi.min_keep)
            .count()
            >= cgi.min_keep
    }

    // This function generates a POOL_SIZE long pool of valid clues.
    // The clues in the pool are valid in the sense that they do not cull the treasure cell.
    // What this function does not consider is how good each clue in the pool, that task is left
    // for the `pick_clue` function.
    fn clue_pool(rng: &mut StdRng, cgi: &ClueGenInfo<'_>, candidates: &[Cell]) -> Vec<Clue> {
        // Decoys are the remaining non-treasure candidates. Created clues are such
        // that they include the treasure while excluding a chosen decoy, guaranteeing
        // that every generated clue eliminates at least one candidate.
        let decoys: Vec<Cell> = candidates
            .iter()
            .copied()
            .filter(|cell| *cell != cgi.treasure)
            .collect();

        let mut pool = Vec::new();
        if decoys.is_empty() {
            return pool;
        }

        for _ in 0..POOL_SIZE {
            let mut attempts = 0;
            while attempts < POOL_ATTEMPTS_PER_CLUE {
                attempts += 1;

                if let Some(&decoy) = decoys.get(rng.random_range(0..decoys.len()))
                    && let Some(clue) = Chart::sample_excluding(rng, cgi, decoy)
                {
                    pool.push(clue);
                    break;
                }
            }
        }
        pool
    }

    // This function is a helper function that generates a random cell lying inside the map.
    fn random_cell(rng: &mut StdRng, radius: i32) -> Cell {
        loop {
            let q = rng.random_range(-radius..=radius);
            let r = rng.random_range(-radius..=radius);
            if (q + r).abs() <= radius {
                return Cell { q, r };
            }
        }
    }

    fn axis_value(axis: Axis, cell: Cell) -> i32 {
        match axis {
            Axis::Q => cell.q,
            Axis::R => cell.r,
            Axis::S => cell.s(),
        }
    }

    // This function creates a valid clue. A valid clue keeps the treasure cell while excluding the decoy.
    fn sample_excluding(rng: &mut StdRng, cgi: &ClueGenInfo<'_>, decoy: Cell) -> Option<Clue> {
        let treasure = cgi.treasure;

        match rng.random_range(0..4) {
            // Within
            0 => {
                let centre = Chart::random_cell(rng, cgi.radius);
                let to_treasure = centre.distance(treasure);
                let to_decoy = centre.distance(decoy);

                if to_decoy <= to_treasure {
                    return None;
                }

                let room = to_decoy - 1 - to_treasure;
                Some(Clue::Within {
                    centre,
                    radius: to_treasure + rng.random_range(0..=room),
                })
            }
            // Beyond
            1 => {
                let centre = Chart::random_cell(rng, cgi.radius);
                let to_treasure = centre.distance(treasure);
                let to_decoy = centre.distance(decoy);

                if to_decoy >= to_treasure {
                    return None;
                }

                let room = to_treasure - 1 - to_decoy;
                Some(Clue::Beyond {
                    centre,
                    radius: to_treasure - 1 - rng.random_range(0..=room),
                })
            }
            // HalfPlane
            2 => {
                let axes = [Axis::Q, Axis::R, Axis::S];
                let chosen_axis = axes.get(rng.random_range(0..axes.len())).copied()?;
                let at_treasure = Chart::axis_value(chosen_axis, treasure);
                let at_decoy = Chart::axis_value(chosen_axis, decoy);

                let gap = (at_treasure - at_decoy).abs();

                // Treasure and decoy lie on the same coordinate with respect to the chosen axis.
                if gap == 0 {
                    return None;
                }

                let slack = rng.random_range(0..=(gap - 1));
                // Chose which half to keep based on the relation between treasure and decoy positions.
                let bound = if at_decoy < at_treasure {
                    Bound::AtLeast(at_treasure.saturating_sub(slack))
                } else {
                    Bound::AtMost(at_treasure.saturating_add(slack))
                };

                Some(Clue::HalfPlane {
                    axis: chosen_axis,
                    bound,
                })
            }

            // Nearer
            _ => {
                let mut a = Chart::random_cell(rng, cgi.radius);
                let mut b = Chart::random_cell(rng, cgi.radius);

                match treasure.distance(a).cmp(&treasure.distance(b)) {
                    // If the two random points are equally distant, nothing can be done.
                    Ordering::Equal => return None,
                    // We enforce a to be the nearest cell.
                    Ordering::Greater => std::mem::swap(&mut a, &mut b),
                    // What we wanted.
                    Ordering::Less => {}
                }

                // If the clue holds true for the decoy as well we discard it.
                if decoy.distance(a) < decoy.distance(b) {
                    return None;
                }

                Some(Clue::Nearer { a, b })
            }
        }
    }

    // This function tries and generate a set of clues.
    // The way it could return None is by not being able to pick a clue for a given step.
    fn try_build(rng: &mut StdRng, cgi: &ClueGenInfo<'_>, clue_count: u8) -> Option<Vec<Clue>> {
        let mut candidates = cgi.water.to_vec();
        let mut clues = Vec::new();

        for steps_left in (1..=clue_count).rev() {
            let pool = Chart::clue_pool(rng, cgi, &candidates);
            let clue = Chart::pick_clue(cgi, &pool, &candidates, steps_left)?;
            candidates.retain(|c| clue.holds(*c));
            clues.push(clue);
        }

        Some(clues)
    }

    // This function checks whether the generated clues make the game solvable.
    fn is_sufficient(cgi: &ClueGenInfo<'_>, clues: &[Clue]) -> bool {
        let survivors: Vec<&Cell> = cgi
            .water
            .iter()
            .filter(|cell| clues.iter().all(|clue| clue.holds(**cell)))
            .collect();

        // There must be only one survivor left
        if survivors.len() != 1 {
            return false;
        }
        // Such cell must be the treasure cell itself
        survivors.first() == Some(&&cgi.treasure)
    }

    fn create_clues(rng: &mut StdRng, clue_count: u8, map: &DefaultMap, treasure: Cell) -> Result<Vec<Clue>, GenError> {
        if clue_count == 0 {
            return Err(GenError::Error("Cannot create zero clues".to_string()));
        }

        let radius = i32::try_from(map.radius())
            .map_err(|_| GenError::Error("Archipelago radius does not fit a coordinate".to_string()))?;
        let water: Vec<Cell> = map.water_cells().collect();

        let cgi = ClueGenInfo {
            water: &water,
            treasure,
            radius,
            min_keep: water.len() / ((2 * clue_count) as usize),
        };

        for _ in 0..RESTARTS {
            if let Some(clues) = Chart::try_build(rng, &cgi, clue_count)
                && Chart::is_sufficient(&cgi, &clues)
            {
                return Ok(clues);
            }
        }

        Err(GenError::Error(
            "Could not generate a solvable set of clues".to_string(),
        ))
    }

    fn create_islands(
        rng: &mut StdRng,
        island_count: u8,
        map: &DefaultMap,
        treasure: Cell,
        seed: Seed,
        params: &Params,
    ) -> Result<Vec<Box<dyn Island>>, GenError> {
        let clues = Chart::create_clues(rng, params.island_count, map, treasure)?;

        let left_out_cells: Vec<Cell> = Chart::cells(map)
            .filter(|cell| clues.iter().all(|clue| clue.holds(*cell)))
            .collect();

        println!("{left_out_cells:?}");
        println!("{treasure:?}");

        // TODO: hardcode the island variants
        let mut islands: Vec<Box<dyn Island>> = Vec::new();

        for idx in 0..island_count {
            if let Some(clue) = clues.get(idx as usize) {
                match rng.random_range(0..2) {
                    // TODO: insert the correct island variants
                    0 | 1 => islands.push(Box::new(DummyIsland::new(IslandInit {
                        clue: *clue,
                        seed,
                        params: *params,
                    }))),
                    _ => return Err(GenError::Error("Invalid roll for island type".to_string())),
                }
            } else {
                return Err(GenError::Error(format!("Invalid island index {idx}")));
            }
        }

        Ok(islands)
    }

    fn legal_treasure_placements(map: &DefaultMap) -> impl Iterator<Item = Cell> {
        let mut illegal_treasure_placements: BTreeSet<Cell> = BTreeSet::new();
        let mut insert_into_illegal = |cell| {
            illegal_treasure_placements.insert(cell);
        };

        // No corners or corners neighbours
        Chart::for_each_cell_and_neighbour(map.corners().into_iter(), map, &mut insert_into_illegal);

        // No land or land neighbours
        let land_cells =
            Chart::cells(map).filter(|cell| matches!(map.terrain(*cell), Some(Terrain::Island(_) | Terrain::Cliff)));
        Chart::for_each_cell_and_neighbour(land_cells, map, &mut insert_into_illegal);

        // No rocks
        Chart::cells(map)
            .filter(|cell| matches!(map.terrain(*cell), Some(Terrain::Rock)))
            .for_each(insert_into_illegal);

        Chart::cells(map).filter(move |candidate| !illegal_treasure_placements.contains(candidate))
    }
    fn choose_treasure(rng: &mut StdRng, map: &DefaultMap) -> Option<Cell> {
        let legal_treasure_cells = Chart::legal_treasure_placements(map);
        Chart::pick_random_cell(rng, legal_treasure_cells)
    }

    fn water_bfs(starting_cell: Cell, map: &DefaultMap) -> HashSet<Cell> {
        let mut connected_water_cells: HashSet<Cell> = HashSet::new();

        if !map.contains(starting_cell) || !matches!(map.terrain(starting_cell), Some(Terrain::Clear)) {
            return connected_water_cells;
        }

        let mut queue: VecDeque<Cell> = VecDeque::new();
        queue.push_back(starting_cell);
        connected_water_cells.insert(starting_cell);

        while let Some(current) = queue.pop_front() {
            let next_cells: Vec<Cell> = current
                .neighbours()
                .into_iter()
                .filter(|neighbour| {
                    map.contains(*neighbour)
                        && !connected_water_cells.contains(neighbour)
                        && matches!(map.terrain(*neighbour), Some(Terrain::Clear))
                })
                .collect();

            for neighbour in next_cells {
                connected_water_cells.insert(neighbour);
                queue.push_back(neighbour);
            }
        }

        connected_water_cells
    }

    fn water_is_connected(starting_cell: Cell, map: &DefaultMap) -> bool {
        let connected_cells = Chart::water_bfs(starting_cell, map);
        Chart::cells(map)
            .filter(|cell| {
                map.terrain(*cell)
                    .is_some_and(|terrain| matches!(terrain, Terrain::Clear))
            })
            .all(|cell| connected_cells.contains(&cell))
    }

    fn check_connected_water(map: &DefaultMap) -> bool {
        let starting_cell = Chart::cells(map).find(|cell| {
            map.terrain(*cell)
                .is_some_and(|terrain| matches!(terrain, Terrain::Clear))
        });
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
