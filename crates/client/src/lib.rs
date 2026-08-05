//! Flowstate Game Client (v0 Test Harness)
//!
//! A minimal ENet client that connects to the Server Edge, completes the
//! handshake, and tracks locally observed match state. v0 scope is a *test*
//! client: it drives scripted movement rather than real input, so that
//! FS-0007's Tier-0 gates (T0.1-T0.4, T0.18) can be exercised against the
//! real wire path instead of the in-process `Server` API alone.
//!
//! # Architecture
//!
//! Depends on `flowstate-wire` for shared Protobuf message types (T0.19:
//! Schema Identity CI Gate requires both client and server to depend on the
//! same wire crate) and `flowstate-sim` for shared type aliases used when
//! interpreting decoded state.
//!
//! # References
//!
//! - [`docs/impl/FS-0007-cli-plan.md`](../../../docs/impl/FS-0007-cli-plan.md): CLI-001..CLI-009 task plan
//! - ADR-0005: v0 Networking Architecture
//! - ADR-0006: Input Tick Targeting
//! - DM-0019: PlayerId

#![deny(unsafe_code)]

pub mod connection;
pub mod input;
pub mod sim_input;
pub mod state;
pub mod tick_floor;

use std::net::SocketAddr;
use std::time::Duration;

use connection::{ClientHost, ConnectError};
use flowstate_sim::Baseline;
use flowstate_wire::{ServerWelcome, Tick};
use input::{InputSeqGen, SendInputError};
use state::BaselineError;
use tick_floor::TickFloor;

/// A single test client instance: owns the ENet connection to a Server Edge
/// and tracks the locally observed match state (baseline, latest snapshot,
/// target tick floor).
///
/// Snapshot tracking (CLI-007) is not wired up yet; today this carries what
/// CLI-002 (connect + handshake), CLI-003 (baseline reception), CLI-004
/// (tick floor tracking), and CLI-005/006 (InputSeq generation + input
/// send) produce.
pub struct TestClient {
    host: ClientHost,
    welcome: ServerWelcome,
    tick_floor: TickFloor,
    baseline: Option<Baseline>,
    input_seq: InputSeqGen,
}

impl TestClient {
    /// Connect to a Server Edge at `addr` and complete the handshake's
    /// first half: send `ClientHello`, await `ServerWelcome`. (CLI-002)
    ///
    /// Also initializes [`TestClient::tick_floor`] from
    /// `ServerWelcome.target_tick_floor` (CLI-004) and a fresh
    /// [`InputSeqGen`] (CLI-005). `JoinBaseline` reception (CLI-003) is a
    /// separate call -- see [`TestClient::recv_baseline`] -- since the
    /// server sends it immediately after `ServerWelcome` on the same
    /// Control channel.
    pub fn connect(addr: SocketAddr, timeout: Duration) -> Result<Self, ConnectError> {
        let (host, welcome) = connection::connect(addr, timeout)?;
        let tick_floor = TickFloor::from_welcome(welcome.target_tick_floor);
        Ok(Self {
            host,
            welcome,
            tick_floor,
            baseline: None,
            input_seq: InputSeqGen::new(),
        })
    }

    /// The `ServerWelcome` received during the handshake.
    pub fn welcome(&self) -> &ServerWelcome {
        &self.welcome
    }

    /// The live ENet host backing this connection, for callers that need to
    /// keep servicing it directly (e.g. CLI-007 snapshot reception).
    pub fn host_mut(&mut self) -> &mut ClientHost {
        &mut self.host
    }

    /// Service the connection until `JoinBaseline` arrives, decode it, and
    /// store the resulting `Baseline`. (CLI-003)
    pub fn recv_baseline(&mut self, timeout: Duration) -> Result<&Baseline, BaselineError> {
        let baseline = state::recv_baseline(&mut self.host, timeout)?;
        Ok(self.baseline.insert(baseline))
    }

    /// The `Baseline` received via [`TestClient::recv_baseline`], if any.
    pub fn baseline(&self) -> Option<&Baseline> {
        self.baseline.as_ref()
    }

    /// The current locally tracked `TargetTickFloor` (DM-0025). (CLI-004)
    pub fn tick_floor(&self) -> Tick {
        self.tick_floor.get()
    }

    /// Fold a newly observed floor value into local tracking, taking
    /// `max(current, received)` per ADR-0006. Called with
    /// `SnapshotProto.target_tick_floor` once snapshot reception (CLI-007)
    /// is wired up.
    pub fn observe_tick_floor(&mut self, received: Tick) {
        self.tick_floor.observe(received);
    }

    /// Build and send an `InputCmdProto` on the Realtime channel, targeting
    /// `desired_tick` clamped up to the locally tracked
    /// [`TestClient::tick_floor`] per ADR-0006, consuming the next
    /// `InputSeq`. (CLI-005, CLI-006)
    pub fn send_input(
        &mut self,
        desired_tick: Tick,
        move_dir: [f64; 2],
    ) -> Result<(), SendInputError> {
        let seq = self.input_seq.advance();
        input::send_input(
            &mut self.host,
            self.tick_floor.get(),
            desired_tick,
            seq,
            move_dir,
        )
    }
}
