//! Client-side `TargetTickFloor` tracking.
//!
//! Ref: CLI-004. Per ADR-0006: `floor = max(floor, received_floor)`,
//! updated from both `ServerWelcome.target_tick_floor` and every
//! `SnapshotProto.target_tick_floor`.

use flowstate_wire::Tick;

/// Client-side tracking of the server-guided `TargetTickFloor` (DM-0025).
///
/// Per ADR-0006, the floor is monotonic non-decreasing per session: clients
/// MUST take `max(previous, received)` on every update, so a stale or
/// reordered message can never move targeting backwards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TickFloor(Tick);

impl TickFloor {
    /// Initialize from `ServerWelcome.target_tick_floor`.
    pub fn from_welcome(target_tick_floor: Tick) -> Self {
        Self(target_tick_floor)
    }

    /// The current locally tracked floor value.
    pub fn get(&self) -> Tick {
        self.0
    }

    /// Fold in a newly observed floor (from `SnapshotProto.target_tick_floor`
    /// or any other server-provided value), taking `max(current, received)`.
    pub fn observe(&mut self, received: Tick) {
        self.0 = self.0.max(received);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CLI-004: floor is monotonic non-decreasing under out-of-order or
    /// stale observations.
    #[test]
    fn test_cli_004_floor_is_monotonic_non_decreasing() {
        let mut floor = TickFloor::from_welcome(1);
        assert_eq!(floor.get(), 1);

        floor.observe(5);
        assert_eq!(floor.get(), 5);

        // A stale/reordered observation must not move the floor backwards.
        floor.observe(3);
        assert_eq!(floor.get(), 5);

        floor.observe(5);
        assert_eq!(floor.get(), 5);

        floor.observe(10);
        assert_eq!(floor.get(), 10);
    }
}
