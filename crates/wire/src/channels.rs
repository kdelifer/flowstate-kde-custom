//! Logical channel constants shared by Server Edge and Game Client.
//!
//! Ref: ADR-0005 (v0 Networking Architecture), WIRE-008
//!
//! - Realtime Channel: unreliable + sequenced (Snapshots, InputCmds)
//! - Control Channel: reliable + ordered (handshake, match lifecycle)

/// Unreliable + sequenced. Carries Snapshots and InputCmds.
pub const CHANNEL_REALTIME: u8 = 0;

/// Reliable + ordered. Carries ClientHello, ServerWelcome, JoinBaseline.
pub const CHANNEL_CONTROL: u8 = 1;
