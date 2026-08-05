//! ENet transport binding for the Server Edge.
//!
//! Ref: SRV-002, ADR-0005 (v0 Networking Architecture)
//!
//! Two ENet channels are allocated per peer, matching the semantic Realtime/
//! Control split defined in ADR-0005:
//! - Channel 0 (Realtime): unreliable + sequenced — Snapshots, InputCmds
//! - Channel 1 (Control): reliable + ordered — handshake, lifecycle

use std::io;
use std::net::{SocketAddr, UdpSocket};

use rusty_enet as enet;

pub use flowstate_wire::channels::{CHANNEL_CONTROL, CHANNEL_REALTIME};

/// Number of ENet channels allocated per peer (Realtime + Control).
pub const CHANNEL_LIMIT: usize = 2;

/// ENet host type used by the server (UDP transport).
pub type ServerHost = enet::Host<UdpSocket>;

/// Bind a UDP socket and create an ENet host listening for up to
/// `peer_limit` peers, with the two channels defined by ADR-0005.
pub fn bind_host(addr: SocketAddr, peer_limit: usize) -> io::Result<ServerHost> {
    let socket = UdpSocket::bind(addr)?;
    enet::Host::new(
        socket,
        enet::HostSettings {
            peer_limit,
            channel_limit: CHANNEL_LIMIT,
            ..Default::default()
        },
    )
    .map_err(|e| io::Error::other(format!("failed to create ENet host: {e:?}")))
}
