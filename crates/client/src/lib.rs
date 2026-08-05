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
use flowstate_wire::ServerWelcome;

/// A single test client instance: owns the ENet connection to a Server Edge
/// and tracks the locally observed match state (baseline, latest snapshot,
/// target tick floor).
///
/// `state`, `tick_floor`, and `input` tracking are populated incrementally
/// by CLI-003..CLI-007; today this only carries what CLI-002 (connect +
/// handshake) produces.
pub struct TestClient {
    host: ClientHost,
    welcome: ServerWelcome,
}

impl TestClient {
    /// Connect to a Server Edge at `addr` and complete the handshake's
    /// first half: send `ClientHello`, await `ServerWelcome`. (CLI-002)
    ///
    /// `JoinBaseline` reception (CLI-003) is left to a later call that
    /// services [`TestClient::host_mut`], since the server sends it
    /// immediately after `ServerWelcome` on the same Control channel.
    pub fn connect(addr: SocketAddr, timeout: Duration) -> Result<Self, ConnectError> {
        let (host, welcome) = connection::connect(addr, timeout)?;
        Ok(Self { host, welcome })
    }

    /// The `ServerWelcome` received during the handshake.
    pub fn welcome(&self) -> &ServerWelcome {
        &self.welcome
    }

    /// The live ENet host backing this connection, for callers that need to
    /// keep servicing it (CLI-003 onward).
    pub fn host_mut(&mut self) -> &mut ClientHost {
        &mut self.host
    }
}
