//! Server tick loop: connection acceptance, handshake, and paced ticking.
//!
//! Ref: LOOP-001..LOOP-006, SRV-004, SRV-009, SRV-023, SRV-024

use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use prost::Message;
use rusty_enet as enet;

use flowstate_wire::{ClientHello, InputCmdProto, ReplayArtifact};

use crate::net::{self, CHANNEL_CONTROL, CHANNEL_REALTIME, ServerHost};
use crate::session::SessionId;
use crate::{Server, ServerConfig};

/// Accept a newly-seen peer's `ClientHello` as a session and immediately
/// send it that session's own `ServerWelcome` + `JoinBaseline`, reflecting
/// the world's current state at the moment of accept -- not a shared,
/// batched match-start snapshot. Used both while waiting for the first
/// session and, continuously, for every later joiner (SRV-009).
fn accept_and_welcome(
    host: &mut ServerHost,
    server: &mut Server,
    peer_sessions: &mut HashMap<enet::PeerID, SessionId>,
    peer_id: enet::PeerID,
) {
    let (session_id, _player_id, _entity_id) = server.accept_session();
    peer_sessions.insert(peer_id, session_id);

    let welcome_bytes = server.welcome_for(session_id).encode_to_vec();
    let baseline_bytes = server.baseline_proto().encode_to_vec();

    if let Some(peer) = host.get_peer_mut(peer_id) {
        let _ = peer.send(
            CHANNEL_CONTROL,
            &enet::Packet::reliable(welcome_bytes.as_slice()),
        );
        let _ = peer.send(
            CHANNEL_CONTROL,
            &enet::Packet::reliable(baseline_bytes.as_slice()),
        );
    }
}

/// Run the server: accept sessions (one or many, joining at any time), then
/// tick until the match ends. A session disconnecting does not affect any
/// other session or end the match (SRV-004, SRV-009).
///
/// Blocking. Returns `Err` if the connect phase times out before *any*
/// session joins (T0.16, threshold adjusted from 2 sessions to 1 under the
/// N-session model); otherwise returns the finalized replay artifact.
pub fn run(config: ServerConfig, addr: SocketAddr) -> io::Result<ReplayArtifact> {
    let connect_timeout = Duration::from_millis(config.connect_timeout_ms);
    let tick_interval = Duration::from_secs_f64(1.0 / f64::from(config.tick_rate_hz));
    let max_sessions = config.max_sessions;

    let mut host = net::bind_host(addr, max_sessions)?;
    // Definitive readiness signal: unlike a pre-bind log line, this can only
    // print once the socket is actually bound, so callers (e.g. a
    // subprocess-spawning test) can safely start connecting on sight of it
    // instead of guessing a startup delay.
    println!("flowstate-server: listening on {addr}");

    let mut server = Server::new(config);
    let mut peer_sessions: HashMap<enet::PeerID, SessionId> = HashMap::new();

    // --- Wait for the FIRST session; the world begins ticking once at
    // least one has joined (does not wait for a second, unlike the old v0
    // model) ---
    let accept_deadline = Instant::now() + connect_timeout;
    while server.session_count() < 1 {
        if Instant::now() >= accept_deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "connect_timeout_ms exceeded before any session connected",
            ));
        }

        loop {
            match host.service() {
                Ok(Some(enet::Event::Receive {
                    peer,
                    channel_id,
                    packet,
                })) => {
                    let peer_id = peer.id();
                    if channel_id == CHANNEL_CONTROL
                        && !peer_sessions.contains_key(&peer_id)
                        && ClientHello::decode(packet.data()).is_ok()
                    {
                        accept_and_welcome(&mut host, &mut server, &mut peer_sessions, peer_id);
                    }
                }
                Ok(Some(_)) => {}
                Ok(None) => break,
                Err(e) => {
                    // A single peer's abrupt exit (e.g. Windows WSAECONNRESET
                    // surfaced from an ICMP port-unreachable bounce) must not
                    // take the whole accept phase down. Treat as a transient,
                    // ignorable read failure and retry on the next poll.
                    eprintln!(
                        "flowstate-server: transient service() error during connect phase, continuing: {e:?}"
                    );
                    break;
                }
            }
        }

        std::thread::sleep(Duration::from_millis(1));
    }

    // --- Paced tick loop (LOOP-001, LOOP-004, LOOP-005, LOOP-006). Also
    // continuously accepts new sessions (what the wait-for-first-session
    // loop above did once, now running for the server's whole lifetime)
    // and handles disconnects -- neither ends the match. ---
    let mut next_tick_at = Instant::now();
    let end_reason = loop {
        if let Some(reason) = server.should_end_match() {
            break reason;
        }

        loop {
            match host.service() {
                Ok(Some(enet::Event::Receive {
                    peer,
                    channel_id,
                    packet,
                })) => {
                    let peer_id = peer.id();
                    if channel_id == CHANNEL_CONTROL
                        && !peer_sessions.contains_key(&peer_id)
                        && ClientHello::decode(packet.data()).is_ok()
                    {
                        accept_and_welcome(&mut host, &mut server, &mut peer_sessions, peer_id);
                    } else if channel_id == CHANNEL_REALTIME
                        && let Some(&session_id) = peer_sessions.get(&peer_id)
                        && let Ok(input) = InputCmdProto::decode(packet.data())
                    {
                        let _ = server.receive_input(session_id, input);
                    }
                }
                Ok(Some(enet::Event::Disconnect { peer, .. })) => {
                    // Remove the peer->session mapping too, not just the
                    // server-side session bookkeeping -- otherwise a later
                    // reconnect that happens to reuse this ENet peer slot
                    // would incorrectly resolve to the stale, now-invalid
                    // session_id.
                    if let Some(session_id) = peer_sessions.remove(&peer.id()) {
                        server.disconnect_session(session_id);
                    }
                }
                Ok(Some(enet::Event::Connect { .. })) => {}
                Ok(None) => break,
                Err(e) => {
                    // A single peer's abrupt exit (e.g. Windows WSAECONNRESET
                    // surfaced from an ICMP port-unreachable bounce) must not
                    // take the whole match down for every other connected
                    // session. ENet's own peer-timeout logic will surface a
                    // proper Event::Disconnect for the actually-dead peer on
                    // a later service() call; treat this as a transient,
                    // ignorable read failure rather than a fatal server
                    // error.
                    eprintln!("flowstate-server: transient service() error, continuing: {e:?}");
                    break;
                }
            }
        }

        // Advance the simulation and broadcast (LOOP-005, LOOP-006, SRV-023).
        // Single serialization, single broadcast call -> byte-identical for
        // all currently-connected sessions by construction (T0.18).
        let (_snapshot, _floor, snapshot_bytes) = server.step();
        host.broadcast(
            CHANNEL_REALTIME,
            &enet::Packet::unreliable(snapshot_bytes.as_slice()),
        );
        // `Host::broadcast` only queues; without an explicit flush, a
        // broadcast queued on the match's final tick would never reach the
        // socket if `should_end_match()` breaks the loop on the next
        // iteration before another `service()` call happens.
        host.flush();

        // Wall-clock pacing (LOOP-001).
        next_tick_at += tick_interval;
        let now = Instant::now();
        if next_tick_at > now {
            std::thread::sleep(next_tick_at - now);
        } else {
            // Fell behind; resync instead of busy-looping catch-up ticks.
            next_tick_at = now;
        }
    };

    Ok(server.finalize(end_reason))
}
