//! Integration test for CLI-009: movement parity between the Game Client's
//! observed state and the Simulation Core's own formula (T0.4 parity).
//!
//! Two independent code paths -- flowstate_sim's own unit test
//! (`test_t0_04_wasd_deterministic_movement`) and this client-driven, real
//! wire round trip -- must converge on the same exact f64 result for the
//! same movement formula. That convergence is the point of this test, not
//! just "some movement happened."

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use flowstate_client::TestClient;
use flowstate_client::sim_input::drive_move_dir;
use flowstate_server::{INPUT_LEAD_TICKS, ServerConfig, tick_loop};
use flowstate_sim::MOVE_SPEED;

const SERVER_ADDR: &str = "127.0.0.1:19295";

#[test]
fn test_cli_009_movement_matches_simulation_core_formula() {
    const NUM_MOVE_TICKS: u64 = 10;
    // Margin between connect and the first targeted tick: without it, the
    // very first InputCmd (targeting `floor`, the earliest allowed tick)
    // races the server's own tick pacing -- thread startup/scheduling
    // jitter can let the server step past that tick before the packet
    // arrives, which drops it as "too late" (T0.11) and, since no valid
    // input has landed yet at that point, falls back to the *initial*
    // LastKnownIntent of [0, 0] instead of the scripted move_dir, silently
    // shorting the expected movement by one tick. Every tick from
    // `start_tick` onward is safely within max_future_ticks (120), so a
    // generous margin costs nothing but a few extra idle ticks.
    const START_MARGIN_TICKS: u64 = 10;

    // The earliest tick a client may target is INPUT_LEAD_TICKS (the
    // initial floor). The server processes ticks [0, match_duration_ticks),
    // so the last tick whose input actually gets applied is
    // match_duration_ticks - 1; the match must run at least one tick past
    // the last one we target.
    let start_tick = INPUT_LEAD_TICKS + START_MARGIN_TICKS;
    let checkpoint_tick = start_tick + NUM_MOVE_TICKS;

    let addr: SocketAddr = SERVER_ADDR.parse().unwrap();
    let config = ServerConfig {
        match_duration_ticks: checkpoint_tick,
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

    let controlled_entity_id = client_a.welcome().controlled_entity_id;
    let tick_rate_hz = client_a.welcome().tick_rate_hz;
    assert_eq!(client_a.tick_floor(), INPUT_LEAD_TICKS);

    // CLI-008: scripted WASD (move right) for NUM_MOVE_TICKS ticks, starting
    // once the margin has elapsed. Any zero-movement ticks before
    // `start_tick` (forced tick 0, plus the margin window) don't affect the
    // final position -- they each contribute exactly 0.
    drive_move_dir(&mut client_a, start_tick, NUM_MOVE_TICKS, [1.0, 0.0]).expect("drive_move_dir");

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut final_position: Option<[f64; 2]> = None;

    while Instant::now() < deadline && final_position.is_none() {
        if let Some(snapshot) = client_a.poll_snapshot().expect("poll_snapshot")
            && snapshot.tick == checkpoint_tick
        {
            final_position = snapshot
                .entities
                .iter()
                .find(|e| e.entity_id == controlled_entity_id)
                .map(|e| e.position);
        }
        std::thread::sleep(Duration::from_millis(1));
    }

    let final_position =
        final_position.expect("expected a snapshot at the match's final tick before deadline");

    // Same formula, same exact-equality bar as
    // flowstate_sim::tests::test_t0_04_wasd_deterministic_movement --
    // independently reproduced here via the real wire path (T0.4/CLI-009
    // parity).
    let dt = 1.0 / f64::from(tick_rate_hz);
    let expected_x = f64::from(NUM_MOVE_TICKS as u32) * MOVE_SPEED * dt;
    assert_eq!(
        final_position[0], expected_x,
        "Position X mismatch: got {}, expected {}",
        final_position[0], expected_x
    );
    assert_eq!(final_position[1], 0.0);

    let artifact = server_thread.join().expect("server thread");
    assert_eq!(artifact.end_reason, "complete");
    assert_eq!(artifact.checkpoint_tick, checkpoint_tick);
}
