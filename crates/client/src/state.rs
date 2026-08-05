//! Locally tracked match state: `JoinBaseline` reception and per-tick
//! `SnapshotProto` updates.
//!
//! Ref: CLI-003 (baseline reception, via the existing
//! `TryFrom<JoinBaseline> for flowstate_sim::Baseline` conversion in
//! `flowstate-wire`), CLI-007 (snapshot reception and state update).
