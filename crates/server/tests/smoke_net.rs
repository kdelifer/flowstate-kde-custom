//! Integration smoke test for SRV-002 (ENet host) + LOOP-001 (paced tick
//! loop) over a real UDP socket pair. Proves the wire path works end-to-end
//! -- not just that it compiles -- before Game Client work (CLI-001..009)
//! builds on top of it.

use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use prost::Message;
use rusty_enet as enet;

use flowstate_server::{ServerConfig, tick_loop};
use flowstate_wire::channels::{CHANNEL_CONTROL, CHANNEL_REALTIME};
use flowstate_wire::{ClientHello, JoinBaseline, ServerWelcome, SnapshotProto};

const SERVER_ADDR: &str = "127.0.0.1:19191";

struct ClientResult {
    welcome: ServerWelcome,
    baseline: JoinBaseline,
    snapshots_seen: usize,
    last_tick: u64,
    last_snapshot_entity_count: usize,
}

fn run_client(server_addr: SocketAddr) -> ClientResult {
    let socket = UdpSocket::bind("127.0.0.1:0").expect("bind client socket");
    let mut host = enet::Host::new(
        socket,
        enet::HostSettings {
            peer_limit: 1,
            channel_limit: 2,
            ..Default::default()
        },
    )
    .expect("create client host");

    host.connect(server_addr, 2, 0).expect("connect to server");

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut hello_sent = false;
    let mut welcome: Option<ServerWelcome> = None;
    let mut baseline: Option<JoinBaseline> = None;
    let mut snapshots_seen = 0usize;
    let mut last_tick = 0u64;
    let mut last_snapshot_entity_count = 0usize;

    while Instant::now() < deadline {
        while let Some(event) = host.service().expect("client service") {
            match event {
                enet::Event::Connect { peer, .. } => {
                    if !hello_sent {
                        let bytes = ClientHello {}.encode_to_vec();
                        peer.send(CHANNEL_CONTROL, &enet::Packet::reliable(bytes.as_slice()))
                            .expect("send hello");
                        hello_sent = true;
                    }
                }
                enet::Event::Receive {
                    channel_id, packet, ..
                } => {
                    if channel_id == CHANNEL_CONTROL {
                        if welcome.is_none() {
                            welcome = ServerWelcome::decode(packet.data()).ok();
                        } else if baseline.is_none() {
                            baseline = JoinBaseline::decode(packet.data()).ok();
                        }
                    } else if channel_id == CHANNEL_REALTIME
                        && let Ok(snapshot) = SnapshotProto::decode(packet.data())
                    {
                        snapshots_seen += 1;
                        last_tick = snapshot.tick;
                        last_snapshot_entity_count = snapshot.entities.len();
                    }
                }
                enet::Event::Disconnect { .. } => {}
            }
        }

        if snapshots_seen >= 3 {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }

    ClientResult {
        welcome: welcome.expect("did not receive ServerWelcome"),
        baseline: baseline.expect("did not receive JoinBaseline"),
        snapshots_seen,
        last_tick,
        last_snapshot_entity_count,
    }
}

/// Proves SRV-002 (ENet host, 2 channels) + LOOP-001 (paced tick loop) work
/// end-to-end over real UDP sockets: two independent clients connect,
/// handshake, and receive live snapshot broadcasts.
#[test]
fn test_two_client_handshake_over_real_enet() {
    let addr: SocketAddr = SERVER_ADDR.parse().unwrap();
    let config = ServerConfig {
        match_duration_ticks: 5,
        connect_timeout_ms: 3000,
        ..Default::default()
    };

    let server_thread =
        std::thread::spawn(move || tick_loop::run(config, addr).expect("server run"));

    // Give the listener a moment to bind before clients connect.
    std::thread::sleep(Duration::from_millis(50));

    let client_a = std::thread::spawn(move || run_client(addr));
    let client_b = std::thread::spawn(move || run_client(addr));

    let result_a = client_a.join().expect("client a thread");
    let result_b = client_b.join().expect("client b thread");
    let artifact = server_thread.join().expect("server thread");

    // T0.1: both clients received distinct player_ids and a valid entity id.
    assert_ne!(result_a.welcome.player_id, result_b.welcome.player_id);
    assert!(result_a.welcome.controlled_entity_id > 0);
    assert!(result_b.welcome.controlled_entity_id > 0);
    assert_eq!(result_a.welcome.tick_rate_hz, 60);

    // T0.2: each client's baseline reflects world state at the moment of
    // *its own* accept, not a shared match-start snapshot -- under the
    // N-session model, whichever client connects first sees only itself
    // (connect order between two independently-spawned threads is not
    // deterministic, so either client may be "first"). Each baseline must
    // at minimum contain that client's own controlled entity.
    assert_eq!(result_a.baseline.tick, 0);
    assert_eq!(result_b.baseline.tick, 0);
    assert!(
        result_a
            .baseline
            .entities
            .iter()
            .any(|e| e.entity_id == result_a.welcome.controlled_entity_id)
    );
    assert!(
        result_b
            .baseline
            .entities
            .iter()
            .any(|e| e.entity_id == result_b.welcome.controlled_entity_id)
    );

    // T0.18: both clients observed live snapshot broadcasts advancing, and
    // by the time each stopped watching, both sessions had been folded into
    // the broadcast state (2 entities), regardless of which client's own
    // baseline arrived first with only 1.
    assert!(result_a.snapshots_seen >= 3);
    assert!(result_b.snapshots_seen >= 3);
    assert!(result_a.last_tick >= 1);
    assert_eq!(result_a.last_snapshot_entity_count, 2);
    assert_eq!(result_b.last_snapshot_entity_count, 2);

    // LOOP-001/SRV-023: match ran to completion and was recorded.
    assert_eq!(artifact.end_reason, "complete");
    assert_eq!(artifact.checkpoint_tick, 5);
}
