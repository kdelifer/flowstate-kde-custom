//! Integration test for CLI-007: `TestClient::poll_snapshot` against a real
//! Server Edge (SRV-002 + LOOP-001) over loopback UDP.
//!
//! Proves snapshots actually arrive on the Realtime channel, decode, and
//! fold their `target_tick_floor` into local tracking -- not just that the
//! decode/keep-latest logic is correct in isolation (that's
//! `state::tests::test_cli_007_is_newer_keeps_highest_tick`).

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use flowstate_client::TestClient;
use flowstate_server::{ServerConfig, tick_loop};

const SERVER_ADDR: &str = "127.0.0.1:19294";

#[test]
fn test_cli_007_poll_snapshot_receives_and_updates_floor() {
    let addr: SocketAddr = SERVER_ADDR.parse().unwrap();
    let config = ServerConfig {
        match_duration_ticks: 10,
        connect_timeout_ms: 3000,
        ..Default::default()
    };

    let server_thread =
        std::thread::spawn(move || tick_loop::run(config, addr).expect("server run"));

    std::thread::sleep(Duration::from_millis(50));

    let client_a = std::thread::spawn(move || TestClient::connect(addr, Duration::from_secs(5)));
    let client_b = std::thread::spawn(move || TestClient::connect(addr, Duration::from_secs(5)));

    let mut client_a = client_a
        .join()
        .expect("client a thread")
        .expect("client a connect");
    let _client_b = client_b
        .join()
        .expect("client b thread")
        .expect("client b connect");

    let initial_floor = client_a.tick_floor();
    let controlled_entity_id = client_a.welcome().controlled_entity_id;

    // Poll until at least one snapshot has arrived (LOOP-001 paces ticks at
    // 30Hz, so this should resolve within a couple of ticks on loopback).
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut seen_tick = None;
    while Instant::now() < deadline && seen_tick.is_none() {
        if let Some(snapshot) = client_a.poll_snapshot().expect("poll_snapshot") {
            seen_tick = Some(snapshot.tick);
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let seen_tick = seen_tick.expect("expected at least one SnapshotProto before deadline");

    // T0.5a/ADR-0006: target_tick_floor == snapshot.tick + input_lead_ticks
    // (input_lead_ticks=1 by default), and the client's locally tracked
    // floor must have advanced to reflect it (CLI-004 fold, exercised via
    // CLI-007).
    assert_eq!(client_a.tick_floor(), seen_tick + 1);
    assert!(client_a.tick_floor() > initial_floor);

    // The snapshot carries the entity this client controls.
    assert!(
        client_a
            .snapshot()
            .unwrap()
            .entities
            .iter()
            .any(|e| e.entity_id == controlled_entity_id)
    );

    let artifact = server_thread.join().expect("server thread");
    assert_eq!(artifact.end_reason, "complete");
}
