//! Subprocess integration test: two `TestClient`s connecting to the real
//! `flowstate-server` binary running as an independent OS process -- not a
//! thread inside this test binary, unlike every other integration test so
//! far (`smoke_net.rs`, and everything under `crates/client/tests/`).
//!
//! Per docs/impl/FS-0007-cli-plan.md §2.3/§3: thread-based tests share this
//! test binary's process, memory space, and socket stack instance with the
//! "server" they drive -- they never actually exercise `main.rs`'s
//! bootstrap or a truly separate process boundary. This test proves the
//! wire path still round-trips when the server is the real compiled
//! artifact, launched exactly as an operator would run it.

use std::io::{BufRead, BufReader};
use std::net::SocketAddr;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use flowstate_client::TestClient;

/// Matches the hardcoded bind address in `crates/server/src/main.rs`.
/// main.rs does not yet support configuring this via CLI/env (SRV-006/007
/// are unimplemented) -- tracked as a follow-up, not a limitation of this
/// test.
const SERVER_ADDR: &str = "127.0.0.1:6060";

/// Kills (and reaps) the wrapped child process on drop, so the subprocess
/// is cleaned up even if an assertion panics partway through the test.
struct KillOnDrop(Child);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Block until the server subprocess logs that it has actually bound its
/// socket (`crates/server/src/tick_loop.rs`, printed immediately after
/// `net::bind_host` succeeds) before returning.
///
/// Connecting before the socket is bound risks the OS delivering an ICMP
/// Port Unreachable back to the client's own socket (surfaces as
/// `WSAECONNRESET` on Windows, per the flush bug fixed in the CLI-008/009
/// commit) instead of ENet's connect handshake simply retrying -- a fixed
/// startup sleep would just make that race rare instead of impossible.
fn wait_for_listening(child: &mut Child) {
    let stdout = child.stdout.take().expect("piped stdout");
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            let is_ready = line.contains("listening on");
            let _ = tx.send(line);
            if is_ready {
                break;
            }
        }
    });

    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(remaining) {
            Ok(line) if line.contains("listening on") => return,
            Ok(_) => continue,
            Err(_) => panic!("server subprocess did not report listening within 10s"),
        }
    }
}

#[test]
fn test_two_clients_connect_to_server_subprocess() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_flowstate-server"))
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn flowstate-server subprocess");

    wait_for_listening(&mut child);
    // From here on, ensure the subprocess is killed no matter how the test
    // exits (success, assertion panic, or early return) -- we deliberately
    // don't wait for the default match_duration_ticks=1800 (60s at 30Hz) to
    // elapse naturally.
    let _guard = KillOnDrop(child);

    let addr: SocketAddr = SERVER_ADDR.parse().unwrap();
    let client_a = std::thread::spawn(move || TestClient::connect(addr, Duration::from_secs(10)));
    let client_b = std::thread::spawn(move || TestClient::connect(addr, Duration::from_secs(10)));

    let mut client_a = client_a
        .join()
        .expect("client a thread")
        .expect("client a connect");
    let mut client_b = client_b
        .join()
        .expect("client b thread")
        .expect("client b connect");

    // T0.1: both clients connected, completed the handshake, and received
    // distinct player_ids with a valid controlled entity -- against the
    // real subprocess, not an in-process stand-in.
    assert_ne!(client_a.welcome().player_id, client_b.welcome().player_id);
    assert!(client_a.welcome().controlled_entity_id > 0);
    assert!(client_b.welcome().controlled_entity_id > 0);
    assert_eq!(client_a.welcome().tick_rate_hz, 30);
    assert_eq!(client_a.tick_floor(), 1);

    // T0.2: each client's JoinBaseline reflects world state at the moment
    // of its own accept, not a shared match-start snapshot -- under the
    // N-session model, whichever client connects first sees only itself
    // (thread connect order is not deterministic). Each baseline must at
    // minimum contain that client's own controlled entity; convergence to
    // both entities is verified below once live snapshots arrive.
    let baseline_a = client_a
        .recv_baseline(Duration::from_secs(5))
        .expect("baseline a")
        .clone();
    let baseline_b = client_b
        .recv_baseline(Duration::from_secs(5))
        .expect("baseline b")
        .clone();
    assert_eq!(baseline_a.tick, 0);
    assert_eq!(baseline_b.tick, 0);
    assert!(
        baseline_a
            .entities
            .iter()
            .any(|e| e.entity_id == client_a.welcome().controlled_entity_id)
    );
    assert!(
        baseline_b
            .entities
            .iter()
            .any(|e| e.entity_id == client_b.welcome().controlled_entity_id)
    );

    // T0.18-ish: live SnapshotProto broadcasts actually cross the process
    // boundary. This is the one thing no in-process thread-based test can
    // exercise -- everything above this line already has in-process
    // coverage in crates/client/tests/handshake.rs and poll_snapshot.rs.
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut seen_tick = None;
    while Instant::now() < deadline && seen_tick.is_none() {
        if let Some(snapshot) = client_a.poll_snapshot().expect("poll_snapshot") {
            seen_tick = Some(snapshot.tick);
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let seen_tick =
        seen_tick.expect("expected at least one live SnapshotProto from the subprocess");
    assert!(seen_tick >= 1);
    assert_eq!(client_a.tick_floor(), seen_tick + 1);
    // By the time a live snapshot arrives, both sessions have been folded
    // into the broadcast state, regardless of which client's own baseline
    // arrived first with only 1 entity.
    assert_eq!(
        client_a.snapshot().expect("snapshot").entities.len(),
        2,
        "both sessions should be reflected in live snapshots"
    );

    // `_guard` drops here: kills the subprocess rather than waiting out the
    // full default match duration.
}
