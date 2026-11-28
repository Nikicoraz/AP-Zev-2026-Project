//! A hater island.
//!
//! This island hates two random ships chosen during initialization for no apparent reason.
//! To balance this out, the island also picks a ship that it will favor.
//!
//! ### Contributors
//! @Nikicoraz

use clues::Clue;
use islandtrait::island::{Island, IslandInit, IslandResponse, MoorResponse, RepairHullResponse};
use rand::prelude::SliceRandom;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use shared::{Params, Round, ShipId};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};

const HATED_SHIPS: usize = 2;
const BERTH_NUMBER: usize = 3;

struct IslandParams {
    pub clue_wait: u32,
    pub repair_delay: u32,
    // pub ask_delay: u32,
    pub requests_for_full_clue: u32,
    pub hull_points_on_repair: u32,
}

impl IslandParams {
    fn default() -> IslandParams {
        IslandParams {
            clue_wait: 2,
            repair_delay: 4,
            // ask_delay: 5,
            requests_for_full_clue: 5,
            hull_points_on_repair: 3,
        }
    }

    fn hated() -> IslandParams {
        let mut init = Self::default();
        init.repair_delay *= 2;
        // init.ask_delay *= 2;
        init.clue_wait *= 2;
        init.requests_for_full_clue *= 2;
        init.hull_points_on_repair = 2;

        init
    }

    fn loved() -> IslandParams {
        IslandParams {
            clue_wait: 0,
            repair_delay: 0,
            // ask_delay: 0,
            requests_for_full_clue: 0,
            hull_points_on_repair: 3,
        }
    }
}

struct Waiting {
    pub clue: u32,
    pub repair: u32,
    // pub ask: u32,
}

pub struct HaterIsland {
    current_round: Round,
    clue: Clue,
    params: Params,
    // todo(@Nikicoraz): Maybe change to the TypedIndex thing that the ship trait has
    history: BTreeMap<ShipId, Vec<Round>>, // This is to match the signature of the ask_history function
    currently_moored: Vec<ShipId>,
    clues_given: BTreeMap<ShipId, u32>,
    wait_queues: BTreeMap<ShipId, Waiting>, // u32 to match the remaining_wait function signature
    hated: HashSet<ShipId>,
    loved: HashSet<ShipId>,
    rng: RefCell<StdRng>,
}

impl HaterIsland {
    fn log_ship_mooring(&mut self, ship_id: ShipId) {
        match self.history.get_mut(&ship_id) {
            Some(registry) => {
                registry.push(self.current_round);
            }
            None => {
                self.history.insert(ship_id, vec![self.current_round]);
            }
        }
    }

    fn get_island_params(&self, ship_id: ShipId) -> IslandParams {
        if self.hated.contains(&ship_id) {
            IslandParams::hated()
        } else if self.loved.contains(&ship_id) {
            IslandParams::loved()
        } else {
            IslandParams::default()
        }
    }

    fn get_ship_wait_queue(&mut self, ship_id: ShipId) -> &mut Waiting {
        self.wait_queues.get_mut(&ship_id).expect("ShipId does not exist")
    }
}

impl Island for HaterIsland {
    fn new(init: IslandInit) -> Self
    where
        Self: Sized,
    {
        let mut rng = StdRng::seed_from_u64(init.seed);

        let mut hated = HashSet::new();
        let mut loved = HashSet::new();

        // Copied by the `Copy` trait
        let mut samples: [ShipId; 6] = ShipId::ALL;

        // Without shuffling the order of the elements is not random
        samples.shuffle(&mut rng);

        let mut i = 0;
        while hated.len() < HATED_SHIPS {
            hated.insert(samples[i]);
            i += 1;
        }

        loved.insert(samples[i]);

        let wait_queues = ShipId::ALL.iter().fold(BTreeMap::new(), |mut acc, ship_id| {
            acc.insert(
                *ship_id,
                Waiting {
                    clue: 0,
                    repair: 0,
                    // ask: 0,
                },
            );
            acc
        });

        HaterIsland {
            current_round: 1,
            clue: init.clue,
            params: init.params,
            history: BTreeMap::new(),
            currently_moored: Vec::new(),
            clues_given: BTreeMap::new(),
            rng: RefCell::new(rng),
            hated,
            loved,
            wait_queues,
        }
    }

    /// See [`Island::moor`] for general behavior.
    ///
    /// # Panics
    ///
    /// Panics if a ship tries to moor when all berths all occupied. The number of berths is defined by [`BERTH_NUMBER`].
    fn moor(&mut self, ship: ShipId) {
        if self.currently_moored.len() < BERTH_NUMBER {
            self.currently_moored.push(ship);
            self.log_ship_mooring(ship);
        } else {
            panic!("All berths are occupied in hater island!");
        }
    }

    fn can_moor(&self, ship: ShipId) -> MoorResponse {
        if self.hated.contains(&ship) && (self.rng.borrow_mut().next_u64() % 100 < 25) {
            return MoorResponse::Refused;
        }

        if self.currently_moored.len() < BERTH_NUMBER {
            MoorResponse::Accepted
        } else {
            MoorResponse::NoBerth
        }
    }

    fn is_moored(&self, ship: ShipId) -> bool {
        self.currently_moored.contains(&ship)
    }

    fn leave_berth(&mut self, ship: ShipId) {
        self.currently_moored.retain(|&x| x != ship);
    }

    fn ask_clue(&mut self, ship: ShipId) -> IslandResponse {
        let i_params = self.get_island_params(ship);

        if self.get_ship_wait_queue(ship).clue > self.current_round {
            return IslandResponse::Wait(self.get_ship_wait_queue(ship).clue);
        }

        if self.current_round.is_multiple_of(7) && self.hated.contains(&ship) {
            return IslandResponse::Refused;
        }

        let given_clues = *self.clues_given.get(&ship).unwrap_or(&0);

        let mut deviation = 0;
        let mut ret = Err(String::new());
        while ret.is_err() {
            ret = self.clue.weaken(
                self.params.archipelago_radius,
                // If a generated clue has an error we add some deviation to try to correct it
                (i_params.requests_for_full_clue - given_clues).saturating_sub(deviation),
            );
            self.clues_given.insert(ship, given_clues + 1);

            deviation += 1;
        }

        IslandResponse::Respond(ret.unwrap())
    }

    fn remaining_wait(&self, ship: ShipId) -> Option<u32> {
        self.wait_queues.get(&ship).map(|x| self.current_round - x.clue)
    }

    fn has_more_to_offer(&self, ship: ShipId) -> bool {
        (i64::from(self.get_island_params(ship).requests_for_full_clue)
            - i64::from(*self.clues_given.get(&ship).unwrap_or(&0)))
            <= 0
    }

    fn round_advance(&mut self, round: Round) {
        self.current_round = round;
    }

    // It's not possible to use underscore bindings for clippy::pedantic, but is it issue worthy?
    fn repair_hull(&mut self, ship: ShipId) -> Option<RepairHullResponse> {
        if self.hated.contains(&ship) && self.rng.borrow_mut().next_u64() % 100 < 50 {
            return Some(RepairHullResponse::Refused);
        }

        let i_params = self.get_island_params(ship);
        if self.get_ship_wait_queue(ship).repair <= self.current_round {
            self.get_ship_wait_queue(ship).repair = self.current_round + i_params.repair_delay;
            Some(RepairHullResponse::Accepted(i_params.hull_points_on_repair))
        } else {
            Some(RepairHullResponse::Wait(
                self.current_round - self.get_ship_wait_queue(ship).repair,
            ))
        }
    }

    fn ask_history(&self, ship: ShipId) -> Option<BTreeMap<ShipId, Vec<Round>>> {
        if self.hated.contains(&ship) && self.rng.borrow_mut().next_u64() % 100 < 50 {
            return None;
        }

        Some(self.history.clone())
    }
}

#[cfg(test)]
mod tests {
    // todo(@Nikicoraz): add tests
    // Test ideas:
    // [ ] Determinism maintainment test
    // [x] Mooring test
    // [ ] history test
    // [ ] clue weakening and full clue test
    // [ ] repairing test

    use super::*;
    use geometry::Cell;

    fn dummy_island_init() -> IslandInit {
        let clue = Clue::Within {
            centre: Cell { q: 1, r: 3 },
            radius: 2,
        };

        IslandInit {
            clue,
            // The seed is the same, so for all tests the hated ships will always be the same
            seed: 42,
            params: Params::default(),
        }
    }

    #[test]
    fn mooring_test() {
        let mut island = HaterIsland::new(dummy_island_init());

        // These are the hated ships
        assert!(island.hated.contains(&ShipId(4)));
        assert!(island.hated.contains(&ShipId(0)));

        // This ship has no trouble mooring
        assert!(!island.is_moored(ShipId(2)));
        assert_eq!(island.can_moor(ShipId(2)), MoorResponse::Accepted);
        island.moor(ShipId(2));
        assert!(island.is_moored(ShipId(2)));

        // This is a hated ship
        assert!(!island.is_moored(ShipId(4)));
        assert_eq!(island.can_moor(ShipId(4)), MoorResponse::Accepted);
        assert_eq!(island.can_moor(ShipId(4)), MoorResponse::Accepted);
        assert_eq!(island.can_moor(ShipId(4)), MoorResponse::Accepted);
        assert_eq!(island.can_moor(ShipId(4)), MoorResponse::Refused);
        assert_eq!(island.can_moor(ShipId(4)), MoorResponse::Accepted);
        island.moor(ShipId(4));
        assert!(island.is_moored(ShipId(4)));

        // Add another ship to fill the berth
        assert!(!island.is_moored(ShipId(1)));
        assert_eq!(island.can_moor(ShipId(1)), MoorResponse::Accepted);
        island.moor(ShipId(1));
        assert!(island.is_moored(ShipId(1)));

        // There is no more space
        assert!(!island.is_moored(ShipId(3)));
        assert_eq!(island.can_moor(ShipId(3)), MoorResponse::NoBerth);

        island.leave_berth(ShipId(4));
        assert!(!island.is_moored(ShipId(4)));
        assert_eq!(island.can_moor(ShipId(3)), MoorResponse::Accepted);
        island.moor(ShipId(3));
        assert!(island.is_moored(ShipId(3)));
    }

    #[test]
    fn test() {
        let mut island = HaterIsland::new(dummy_island_init());

        assert!(island.hated.contains(&ShipId(4)));
        assert!(island.hated.contains(&ShipId(0)));

        assert!(!island.is_moored(ShipId(2)));
        assert_eq!(island.can_moor(ShipId(2)), MoorResponse::Accepted);
        island.moor(ShipId(2));
        assert!(island.is_moored(ShipId(2)));

        // Will fail since 0 is hated
        assert_eq!(island.can_moor(ShipId(4)), MoorResponse::Accepted);
        assert_eq!(island.can_moor(ShipId(4)), MoorResponse::Accepted);
        assert_eq!(island.can_moor(ShipId(4)), MoorResponse::Accepted);
        assert_eq!(island.can_moor(ShipId(4)), MoorResponse::Refused);
    }
}
