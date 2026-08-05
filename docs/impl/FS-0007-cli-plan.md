# FS-0007: Game Client (v0 Test Harness) — Implementation Plan for CLI-001–CLI-009

> **Spec:** [FS-0007-v0-multiplayer-slice.md](../specs/FS-0007-v0-multiplayer-slice.md)
> **Parent Plan:** [FS-0007-plan.md](FS-0007-plan.md) §1.7
> **Gates:** [FS-0007-gates.md](FS-0007-gates.md)
> **Issue:** [#7](https://github.com/project-flowstate/flowstate/issues/7)
> **Date:** 2026-08-04

---

## 0. Why this doc exists

CLI-001–CLI-009 sit at the end of the critical path in the parent plan:

```
SIM → WIRE → SRV → LOOP → CLI → CI
```

A test client is only useful against a server that actually accepts network
connections and runs ticks unattended. Before writing the CLI task plan, this
doc audits the current state of SRV-002 and LOOP-001 (the two tasks CLI
depends on transitively) to confirm what exists, what's missing, and what
must land first.

---

## 1. Gap Analysis: SRV-002 and LOOP-001 Current State

### 1.1 What exists today

`crates/server/src/lib.rs` implements `Server` as an **in-process state
machine only**:

| Method | Behavior |
|---|---|
| `Server::new(config)` | Constructs world, sessions, buffers — no I/O |
| `accept_session()` | Synchronous call, simulates a connection; no network involved |
| `receive_input(session_id, InputCmdProto)` | Takes an already-decoded message directly; no socket read |
| `step()` | Advances one tick on demand; caller drives pacing (tests call it in a `for` loop) |
| `finalize(end_reason)` | Produces `ReplayArtifact` |

All 13 unit tests in that file (`test_t0_01_...` through
`test_t0_16_connection_timeout`) exercise this API in-process, with no
sockets, no serialization over a wire, and no wall clock.

### 1.2 What's missing

Confirmed by repo search:

- **No `enet` (or any) dependency** in `crates/server/Cargo.toml` — only
  `flowstate-sim`, `flowstate-wire`, `flowstate-replay`, `prost`.
- **No `crates/server/src/main.rs`** — the crate has no binary entry point,
  only a library (`lib.rs`).
- **No `handshake.rs`, `tick_loop.rs`, `replay_writer.rs`, `config.rs`** —
  the module files named in the parent plan's file inventory (§3.2) don't
  exist yet; everything currently lives in `lib.rs`.
- **No `crates/client/` directory** — CLI-001 ("create minimal Rust test
  client binary or test module") has no home yet.
- **CLI parsing (SRV-006), env var fallbacks (SRV-007), and CLI-invoked
  ENet host (SRV-002)** are all unimplemented — there is no way to actually
  run this server as a standalone process today.

So concretely:

- **SRV-002** (ENet host init, two channels) — **not started**. SRV-001 is
  only partially done: the crate and its Cargo.toml exist, but the `enet-rs`
  dependency called for in that task has not been added.
- **LOOP-001** (tick loop pacing at `tick_rate_hz`, wall-clock for
  production) — **not started**. `step()` exists and is correct in
  isolation, but nothing calls it on a wall-clock cadence; that's
  effectively LOOP-002 (manual-step mode) already available implicitly via
  direct `step()` calls in tests, with LOOP-001's production pacing loop
  absent.

### 1.3 Why this blocks CLI-001–009 specifically

Per ADR-0005, v0 networking is **real ENet**, not an in-process stub — the
Godot client uses `ENetMultiplayerPeer` and the Rust server uses the `enet`
crate, with channel 0 = Realtime (unreliable+sequenced) and channel 1 =
Control (reliable+ordered). CLI-002 ("Implement ENet client connection and
handshake") requires an ENet host on the other end that is:

1. Actually bound to a socket and listening (SRV-002)
2. Actually running a loop that reads packets, buffers input, and steps the
   world on a cadence without a test harness manually calling `step()`
   (LOOP-001, plus the receive/broadcast wiring in LOOP-004–LOOP-006)

Without both, CLI-001–009 can only be written against the in-process
`Server` API directly (bypassing the wire entirely), which does not
exercise T0.1–T0.3, T0.18, or T0.19 as written — those gates specifically
require two independent processes communicating over real sockets.

### 1.4 Recommendation

Implement SRV-002 and a minimal LOOP-001/004/006 slice **before** CLI-002
onward. Suggested minimal server-side prerequisite scope (not full SRV-*/LOOP-*
completion — just enough for a client to have something to connect to):

| Task | Scope for CLI unblock |
|---|---|
| SRV-001 (finish) | Add `enet` crate dependency to `crates/server/Cargo.toml` |
| SRV-002 | ENet host bound to a port, 2 channels (Realtime=0, Control=1) |
| SRV-004 | Accept loop with `connect_timeout_ms`, up to 2 peers |
| SRV-009 | Handshake: receive `ClientHello`, send `ServerWelcome` + `JoinBaseline` over Control channel |
| SRV-023 | Serialize+broadcast `SnapshotProto` over Realtime channel |
| LOOP-001 | Wall-clock-paced loop calling `Server::step()` at `tick_rate_hz` |
| LOOP-004/005/006 | Per-tick receive → buffer → `step()` → broadcast cycle |
| `main.rs` | Bootstraps the above so the server is runnable as `cargo run -p flowstate-server` |

Everything else in SRV-* / LOOP-* (rate limiting refinements, disconnect
handling, replay writing, CLI arg parsing) can land in parallel or after,
since CLI-001–009 as scoped (T0.1–T0.4 style checks) don't require it. This
plan assumes that minimal slice lands first; CLI-001–009 below is written
against the resulting server binary.

---

## 2. CLI-001–009 Task Plan

### 2.1 Crate scaffold

New crate: `crates/client/` (currently does not exist).

```
crates/client/
├── Cargo.toml          # flowstate-wire, flowstate-sim (types only), enet dep
├── src/
│   ├── lib.rs           # Module exports; TestClient struct
│   ├── connection.rs     # ENet connect + handshake (CLI-002)
│   ├── state.rs          # Baseline/snapshot state tracking (CLI-003, CLI-007)
│   ├── tick_floor.rs      # TargetTickFloor tracking (CLI-004)
│   ├── input.rs           # InputSeq generation + InputCmdProto send (CLI-005, CLI-006)
│   └── sim_input.rs        # Programmatic WASD simulation (CLI-008)
└── tests/
    └── integration/
        └── movement_test.rs  # CLI-009 assertions, drives a real server subprocess or in-proc instance
```

Add `"crates/*"` already covers this via the workspace glob in the root
`Cargo.toml` — no workspace file changes needed beyond creating the crate.

### 2.2 Task breakdown

| Task ID | Description | Depends on | Notes |
|---|---|---|---|
| CLI-001 | Create `crates/client/` with `Cargo.toml`; depend on `flowstate-wire` for shared message types (T0.19 requires this) | WIRE-001 | Decide: standalone test binary vs. library exercised from `tests/integration/`. Recommend **library + integration tests** — matches how `crates/server` is structured (logic in `lib.rs`, `#[cfg(test)]` and `tests/` for gates) |
| CLI-002 | ENet client connection: connect to `127.0.0.1:<port>`, send `ClientHello` on Control channel, await `ServerWelcome` | CLI-001, WIRE-003, WIRE-004, **SRV-002** | Blocked until SRV-002 lands (§1.4). Use whatever `enet` crate the server settles on for API consistency |
| CLI-003 | Receive `JoinBaseline` on Control channel; decode via `prost`; convert to local `Baseline` via the existing `TryFrom<JoinBaseline> for flowstate_sim::Baseline` impl in `flowstate-wire` | CLI-002, WIRE-005 | Conversion trait already exists (`crates/wire/src/lib.rs:371`) — reuse it, don't reimplement |
| CLI-004 | Track `TargetTickFloor` locally: `floor = max(floor, received_floor)` on both `ServerWelcome.target_tick_floor` and every `SnapshotProto.target_tick_floor` | CLI-003 | Matches ADR-0006 semantics; mirrors server's `last_emitted_floor` bookkeeping conceptually but client-side |
| CLI-005 | `InputSeq` generator: `u64` counter starting at 1, strictly incrementing per send | CLI-001 | Trivial; must never reset within a session (T0.3 assertion) |
| CLI-006 | Build and send `InputCmdProto { tick: max(current_floor, desired_tick), input_seq, move_dir }` on Realtime channel | CLI-004, CLI-005, WIRE-006 | `player_id` is intentionally omitted from the wire message (server binds identity from session, INV-0003) |
| CLI-007 | Receive `SnapshotProto` on Realtime channel; decode; update local entity state; call CLI-004's floor update | CLI-003, WIRE-007 | Client should discard snapshots for ticks older than the last one applied (unreliable+sequenced semantics per ADR-0005) |
| CLI-008 | Programmatic WASD input simulation: given a scripted `move_dir` sequence, call CLI-006 once per local frame/tick for use in deterministic tests (no real keyboard) | CLI-006 | This is what makes the harness a *test* client rather than a playable one |
| CLI-009 | Test assertions: after N ticks of scripted `move_dir=[1,0]`, assert `position.x == MOVE_SPEED * dt * N` exactly (f64 equality, no epsilon), matching T0.4's method on the server/sim side | CLI-007, SIM-015 | Cross-check: this must agree with `crates/sim/src/tests/movement_test.rs`'s own assertion for the same inputs — same formula, two independent code paths converging is the point of the test |

### 2.3 Two-client harness shape (for T0.1–T0.3, T0.18)

CLI-009's real test target isn't a single client — it's **two** `TestClient`
instances connecting to one server, per FS-0007's acceptance criteria
(AC-0001.1/.2). Suggested integration test shape:

```rust
// crates/client/tests/integration/two_client_movement.rs
// (or crates/server/tests/integration/*.rs, driving TestClient from there —
//  either location works since T0.19 only requires both binaries depend on
//  flowstate-wire, not that the test itself live in a specific crate)

let server = spawn_test_server(seed=0, manual_step_or_paced);
let client_a = TestClient::connect(server.addr())?;
let client_b = TestClient::connect(server.addr())?;
// assert both received ServerWelcome + JoinBaseline (T0.1, T0.2)
client_a.send_move([1.0, 0.0]);
// advance N ticks
// assert client_a's tracked entity position matches expected formula (T0.4/T0.9 parity)
```

Whether the test spawns the server as a **subprocess** (`cargo run -p
flowstate-server -- --test-mode ...`, real sockets) or **in the same test
process** (calling into `Server` + a manual-step loop directly) is an open
call — subprocess is closer to production reality and required for a true
T0.19 schema-identity check across binaries; in-process is faster to
iterate on. Recommend starting in-process for CLI-002–007 development
speed, then adding at least one subprocess-based test before calling
T0.1–T0.3 done, since that's the only way to prove the ENet wire path
(not just the shared Rust types) actually round-trips.

---

## 3. Suggested Order

1. **Land the SRV-002/LOOP-001 minimal slice** from §1.4 — without this,
   CLI-002 has nothing to connect to.
2. CLI-001 (crate scaffold) can be done in parallel with step 1 — it has no
   runtime dependency on the server, only on `flowstate-wire` (already
   complete).
3. CLI-002 → CLI-007 in dependency order, developed in-process against a
   locally spawned `Server` + manual `step()` loop for fast iteration.
4. CLI-008, CLI-009 once the read/write path is solid.
5. Add the subprocess-based two-client test (§2.3) to validate the real
   ENet path before marking T0.1–T0.3 and T0.18 done.
6. Only then proceed to CI-001–005, which assumes a working client to
   exercise `just ci`'s integration test targets.

---

## 4. Open Questions

- **enet crate choice:** ADR-0005 says "Rust server via `enet` crate" but
  doesn't pin a specific published crate name/version. This needs to be
  resolved as part of SRV-002 (§1.4) before CLI-002 can pick a matching
  client-side dependency — recommend deciding this once, in one place
  (probably `crates/server/Cargo.toml`'s SRV-002 PR), and having
  `crates/client` follow suit.
- **Test client location:** library-in-`crates/client`-with-integration-tests
  vs. a literal separate test binary, per CLI-001's "binary or test module"
  wording. §2.1 recommends the library approach for parity with how
  `crates/server` is already structured, but this is worth confirming
  before scaffolding.
