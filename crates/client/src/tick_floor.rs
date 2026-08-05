//! Client-side `TargetTickFloor` tracking.
//!
//! Ref: CLI-004. Per ADR-0006: `floor = max(floor, received_floor)`,
//! updated from both `ServerWelcome.target_tick_floor` and every
//! `SnapshotProto.target_tick_floor`.
