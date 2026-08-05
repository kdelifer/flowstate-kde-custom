//! Integration test for CLI-005/006: `TestClient::send_input` against a
//! real Server Edge (SRV-002 + LOOP-001) over loopback UDP.
//!
//! Proves the input actually round-trips through the wire: buffered by the
//! server's InputBuffer, selected over InputSeq ties, converted to
//! StepInput, and recorded as a non-fallback AppliedInput in the replay
//! artifact -- not just that `send_input` doesn't error locally.

use std::net::SocketAddr;
use std::time::Duration;

use flowstate_client::TestClient;
use flowstate_server::{ServerConfig, tick_loop};

const SERVER_ADDR: &str = "127.0.0.1:19293";

#[test]
fn test_cli_006_send_input_reaches_server() {
    let addr: SocketAddr = SERVER_ADDR.parse().unwrap();
    let config = ServerConfig {
        // Long enough that a burst of sends targeting a range of near-term
        // ticks is guaranteed to land on a tick the server hasn't stepped
        // past yet, without relying on precise wall-clock timing.
        match_duration_ticks: 30,
        connect_timeout_ms: 3000,
        ..Default::default()
    };

    let server_thread =
        std::thread::spawn(move || tick_loop::run(config, addr).expect("server run"));

    // Give the listener a moment to bind before clients connect.
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

    let player_id = client_a.welcome().player_id;
    let floor = client_a.tick_floor();

    // Send a burst targeting the floor and the next several ticks so at
    // least one lands before the server steps past it (CLI-006).
    for offset in 0..10 {
        client_a
            .send_input(floor + offset, [1.0, 0.0])
            .expect("send_input");
    }

    let artifact = server_thread.join().expect("server thread");
    assert_eq!(artifact.end_reason, "complete");

    // At least one AppliedInput for player_id was accepted from the wire
    // (not LastKnownIntent fallback) with the move_dir we sent.
    let accepted = artifact
        .inputs
        .iter()
        .any(|i| i.player_id == player_id && !i.is_fallback && i.move_dir == vec![1.0, 0.0]);
    assert!(
        accepted,
        "expected at least one non-fallback AppliedInput for player {player_id} with move_dir [1.0, 0.0]; got: {:?}",
        artifact.inputs
    );
}
