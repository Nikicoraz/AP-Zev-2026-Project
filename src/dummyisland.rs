use clues::Clue;
use islandtrait::island::{Island, IslandInit, IslandResponse, MoorResponse};
use shared::{Round, ShipId};

pub struct DummyIsland {
    clue: Clue
}

impl Island for DummyIsland {
    fn new(init: IslandInit) -> Self
    where
        Self: Sized
    {
        Self {
            clue: init.clue
        }
    }

    fn moor(&mut self, ship: ShipId) {
        todo!()
    }

    fn can_moor(&self, ship: ShipId) -> MoorResponse {
        todo!()
    }

    fn is_moored(&self, ship: ShipId) -> bool {
        todo!()
    }

    fn leave_berth(&mut self, ship: ShipId) {
        todo!()
    }

    fn ask_clue(&mut self, ship: ShipId) -> IslandResponse {
        todo!()
    }

    fn remaining_wait(&self, ship: ShipId) -> Option<u32> {
        todo!()
    }

    fn has_more_to_offer(&self, ship: ShipId) -> bool {
        todo!()
    }

    fn round_advance(&mut self, round: Round) {
        todo!()
    }
}
