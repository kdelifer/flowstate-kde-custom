//! ENet client connection and handshake.
//!
//! Ref: CLI-002 (connect + `ClientHello` / `ServerWelcome`), depends on
//! SRV-002 (server-side ENet host, already landed).

use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use prost::Message;
use rusty_enet as enet;

use flowstate_wire::ClientHello;
use flowstate_wire::ServerWelcome;
use flowstate_wire::channels::CHANNEL_CONTROL;

/// Number of ENet channels allocated per peer (Realtime + Control), matching
/// the server's channel layout (ADR-0005, mirrors `flowstate_server::net`).
pub const CHANNEL_LIMIT: usize = 2;

/// ENet host type used by the test client (UDP transport).
pub type ClientHost = enet::Host<UdpSocket>;

/// Errors that can occur while connecting to a Server Edge.
#[derive(Debug)]
pub enum ConnectError {
    /// The underlying ENet/UDP transport failed to initialize or service.
    Io(io::Error),
    /// No `ServerWelcome` arrived before the timeout elapsed.
    Timeout,
}

impl From<io::Error> for ConnectError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// Connect to a Server Edge at `addr` and complete the handshake's first
/// half: send `ClientHello` on the Control channel as soon as the ENet
/// connection is established, then wait up to `timeout` for `ServerWelcome`.
///
/// Returns the live ENet host (so the caller can keep servicing it -- the
/// server sends `JoinBaseline` immediately after `ServerWelcome`, per
/// `flowstate_server::tick_loop::run`; decoding it is CLI-003) along with
/// the decoded welcome.
pub fn connect(
    addr: SocketAddr,
    timeout: Duration,
) -> Result<(ClientHost, ServerWelcome), ConnectError> {
    let socket = UdpSocket::bind("127.0.0.1:0")?;
    let mut host = enet::Host::new(
        socket,
        enet::HostSettings {
            peer_limit: 1,
            channel_limit: CHANNEL_LIMIT,
            ..Default::default()
        },
    )
    .map_err(|e| io::Error::other(format!("failed to create ENet host: {e:?}")))?;

    host.connect(addr, CHANNEL_LIMIT, 0)
        .map_err(|e| io::Error::other(format!("failed to initiate ENet connect: {e:?}")))?;

    let deadline = Instant::now() + timeout;
    let mut hello_sent = false;

    while Instant::now() < deadline {
        while let Some(event) = host
            .service()
            .map_err(|e| io::Error::other(format!("ENet service error: {e:?}")))?
        {
            match event {
                enet::Event::Connect { peer, .. } => {
                    if !hello_sent {
                        let bytes = ClientHello {}.encode_to_vec();
                        peer.send(CHANNEL_CONTROL, &enet::Packet::reliable(bytes.as_slice()))
                            .map_err(|e| {
                                io::Error::other(format!("failed to send ClientHello: {e:?}"))
                            })?;
                        hello_sent = true;
                    }
                }
                enet::Event::Receive {
                    channel_id, packet, ..
                } if channel_id == CHANNEL_CONTROL => {
                    if let Ok(welcome) = ServerWelcome::decode(packet.data()) {
                        return Ok((host, welcome));
                    }
                }
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }

    Err(ConnectError::Timeout)
}
