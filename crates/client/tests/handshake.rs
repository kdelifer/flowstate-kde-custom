//! Integration test for CLI-002: `TestClient::connect` against a real
//! Server Edge (SRV-002 + LOOP-001) over loopback UDP.
//!
//! Mirrors `crates/server/tests/smoke_net.rs`, but exercises the client's
//! own connect path (`flowstate_client::connection`) instead of a hand-rolled
//! ENet client, proving the two independently-authored halves of the wire
//! path actually agree (T0.1, T0.19).

use std::net::SocketAddr;
use std::time::Duration;

use flowstate_client::TestClient;
use flowstate_server::{ServerConfig, tick_loop};

const SERVER_ADDR: &str = "127.0.0.1:19292";

#[test]
fn test_cli_002_connect_and_handshake() {
    let addr: SocketAddr = SERVER_ADDR.parse().unwrap();
    let config = ServerConfig {
        match_duration_ticks: 2,
        connect_timeout_ms: 3000,
        ..Default::default()
    };

    let server_thread =
        std::thread::spawn(move || tick_loop::run(config, addr).expect("server run"));

    // Give the listener a moment to bind before clients connect.
    std::thread::sleep(Duration::from_millis(50));

    let client_a = std::thread::spawn(move || TestClient::connect(addr, Duration::from_secs(5)));
    let client_b = std::thread::spawn(move || TestClient::connect(addr, Duration::from_secs(5)));

    let result_a = client_a
        .join()
        .expect("client a thread")
        .expect("client a connect");
    let result_b = client_b
        .join()
        .expect("client b thread")
        .expect("client b connect");
    let artifact = server_thread.join().expect("server thread");

    // T0.1: both clients connected, completed the handshake, and received
    // distinct player_ids with a valid controlled entity.
    assert_ne!(result_a.welcome().player_id, result_b.welcome().player_id);
    assert!(result_a.welcome().controlled_entity_id > 0);
    assert!(result_b.welcome().controlled_entity_id > 0);
    assert_eq!(result_a.welcome().tick_rate_hz, 60);
    assert_eq!(result_a.welcome().target_tick_floor, 1);

    // Match still ran to completion independent of the client's own pacing.
    assert_eq!(artifact.end_reason, "complete");
}
