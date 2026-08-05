//! Integration test for CI-004: replay verification against an artifact
//! produced by a real (in-process) match -- not just the recorder-built
//! synthetic artifacts flowstate_replay's own unit tests cover
//! (T0.8-T0.12a in crates/replay/src/lib.rs).
//!
//! Ref: INV-0006 (Replay Verifiability), T0.9, ADR-0007 (StateDigest).

use flowstate_replay::{VerifyOptions, read_replay, verify_replay, write_replay};
use flowstate_server::{EndReason, Server, ServerConfig};
use flowstate_wire::InputCmdProto;

#[test]
fn test_ci_004_replay_verification_against_real_match_artifact() {
    const NUM_MOVE_TICKS: u64 = 19;

    let config = ServerConfig {
        // Tick 0 is always a forced no-op (the initial floor is
        // input_lead_ticks=1), so NUM_MOVE_TICKS ticks of real input need
        // one extra tick to land.
        match_duration_ticks: NUM_MOVE_TICKS + 1,
        ..Default::default()
    };
    let mut server = Server::new(config);
    let (session_a, player_a, _) = server.accept_session();
    let (session_b, player_b, _) = server.accept_session();
    server.start_match();

    // Pre-buffer input for every tick the match will actually process
    // (1..=NUM_MOVE_TICKS) before stepping at all -- submitted this early,
    // every target clears TargetTickFloor trivially, since the floor only
    // advances once step() runs. This produces a mix of real, non-fallback
    // AppliedInputs (ticks 1..=19) and one LastKnownIntent fallback (the
    // forced-zero tick 0), which is a more representative artifact than an
    // all-fallback or all-synthetic one.
    for tick in 1..=NUM_MOVE_TICKS {
        let result_a = server.receive_input(
            session_a,
            InputCmdProto {
                tick,
                input_seq: tick,
                move_dir: vec![1.0, 0.0],
            },
        );
        assert!(
            result_a.is_accepted(),
            "input a at tick {tick}: {result_a:?}"
        );
        let result_b = server.receive_input(
            session_b,
            InputCmdProto {
                tick,
                input_seq: tick,
                move_dir: vec![0.0, 1.0],
            },
        );
        assert!(
            result_b.is_accepted(),
            "input b at tick {tick}: {result_b:?}"
        );
    }

    while server.should_end_match().is_none() {
        server.step();
    }

    let artifact = server.finalize(EndReason::Complete);
    assert_eq!(artifact.checkpoint_tick, NUM_MOVE_TICKS + 1);

    // Sanity: the pre-buffered inputs actually landed as real (non-fallback)
    // AppliedInputs, not silently dropped -- otherwise this test would only
    // be exercising the all-LKI-fallback path.
    let real_inputs = artifact.inputs.iter().filter(|i| !i.is_fallback).count();
    assert_eq!(
        real_inputs,
        (NUM_MOVE_TICKS * 2) as usize,
        "expected {} non-fallback inputs (2 players x {} ticks), got {}: {:?}",
        NUM_MOVE_TICKS * 2,
        NUM_MOVE_TICKS,
        real_inputs,
        artifact.inputs
    );

    let options = VerifyOptions {
        strict_build_check: false, // build fingerprint is dev-mode/"unknown" in tests
        current_build: None,
    };
    let result = verify_replay(&artifact, &options);
    assert!(
        result.is_ok(),
        "replay verification failed against a real match artifact: {result:?}"
    );

    // Round-trip through the actual on-disk artifact format too (T0.8's
    // encode/decode path, not just the in-memory struct).
    let path = std::env::temp_dir().join(format!(
        "flowstate_ci004_{}_{}_{}.replay",
        std::process::id(),
        player_a,
        player_b
    ));
    let _ = std::fs::remove_file(&path);
    write_replay(&artifact, &path).expect("write_replay");
    let reloaded = read_replay(&path).expect("read_replay");
    let _ = std::fs::remove_file(&path);

    assert_eq!(
        reloaded, artifact,
        "decoded artifact must match the original"
    );
    let reloaded_result = verify_replay(&reloaded, &options);
    assert!(
        reloaded_result.is_ok(),
        "replay verification failed against the round-tripped artifact: {reloaded_result:?}"
    );
}
