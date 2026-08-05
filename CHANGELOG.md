# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Versioning Policy

- **`0.0.0`** (current): Foundation phase, no releases
- **`0.x.y`**: Pre-1.0 development; breaking changes permitted between minors
- **`1.0.0+`**: Stable API; strict SemVer (MAJOR.MINOR.PATCH)

Versions are updated manually when cutting releases (milestone-based, not per-PR).
Changes accumulate in `[Unreleased]` until a version is tagged.

## [Unreleased]

### Added

**Governance & Constitution:**
- Constitution with authority map, product thesis, and derivation contract (`docs/constitution.md`)
- Constitution annexes: invariants (`INV-0001`, `INV-0002`), domain model (`DM-0001..0004`), acceptance/kill criteria, tag taxonomy, ID system, scope/non-goals
- Constitution ID system with automated validation (`just ids`) and generation (`just ids-gen`)
- ADR template for future architecture decisions (`docs/adr/0000-adr-template.md`)
- Golden Delivery Path protocol with governance classification and trace blocks (`docs/delivery-path.md`)

**Infrastructure & Automation:**
- Justfile with `ci`, `fmt`, `lint`, `test`, `ids`, `spec-lint`, `pr-trace` targets
- GitHub Actions CI workflow with Rust caching, Python setup, and PR trace validation
- Python validation scripts: Constitution IDs, spec linting, PR trace parsing
- Dependabot configuration for Rust and GitHub Actions

**GitHub Governance:**
- Issue templates: feature requests, governance changes, bug reports, agent tasks
- PR template with required trace block
- Contributing guide, Code of Conduct, Security policy, CODEOWNERS

**Documentation:**
- Game vision and design intent (`docs/vision.md`)
- Human handbook and operating model (`docs/handbook.md`)
- Repository routing table (`docs/repo-map.md`)
- Agent operating rules (`AGENTS.md`)
- Licensing policy and third-party intake process (`docs/licensing/third-party.md`)
- Spec template and structure (`docs/specs/`)

**Code:**
- Rust toolchain pinned to 1.92.0 with edition 2024
- Top-level `client/` and `protocol/` directories reserved (currently placeholders) for the eventual Godot presentation client and engine-agnostic schemas, distinct from `crates/client` below

**Simulation Core (`crates/sim`):**
- Deterministic, fixed-timestep `World` with explicit-tick `advance()` (ADR-0003); no I/O, networking, wall-clock, or ambient RNG (INV-0004)
- v0 WASD movement model (`MOVE_SPEED = 5.0` units/sec)
- FNV-1a 64-bit `state_digest()` with f64 canonicalization (`-0.0` → `+0.0`, NaN → quiet NaN) per ADR-0007
- PlayerId non-assumption: no reliance on contiguous/zero-based IDs (verified with non-contiguous test IDs)

**Wire Protocol (`crates/wire`):**
- Inline Protobuf (prost) message types shared by client and server: `ClientHello`, `ServerWelcome`, `JoinBaseline`, `InputCmdProto`, `SnapshotProto`, `ReplayArtifact`, and supporting types
- Conversions between wire types and Simulation Core types (`Baseline`, `Snapshot`, `EntitySnapshot`)
- Realtime (unreliable+sequenced) / Control (reliable+ordered) channel constants per ADR-0005

**Replay System (`crates/replay`):**
- `ReplayRecorder` for match-time `AppliedInput` and baseline capture
- `ReplayVerifier` (`verify_replay`): build fingerprint check, AppliedInput stream integrity, initialization anchor (baseline digest), full tick replay, final digest match (INV-0006)
- Build fingerprint acquisition (binary SHA-256, target triple, profile, git commit)
- On-disk artifact read/write (`write_replay`/`read_replay`)

**Server Edge (`crates/server`):**
- Real ENet transport: two-channel host (Realtime/Control), wall-clock-paced tick loop, runnable as `cargo run -p flowstate-server`
- Session management, PlayerId assignment (including `--test-mode`/`--test-player-ids` override for non-contiguous-ID testing)
- Input validation: NaN/Inf drop, magnitude clamp, tick window, TargetTickFloor enforcement, rate limiting, InputSeq tie-breaking (DM-0026)
- LastKnownIntent fallback for missing input (DM-0023)
- Match lifecycle: connection timeout, disconnect handling, match completion, ReplayArtifact persistence

**Game Client Test Harness (`crates/client`):**
- Minimal ENet client (not the player-facing game client — see "What's Missing" in `README.md`) proving the wire path against a real server: connect/handshake, `JoinBaseline` reception, `TargetTickFloor` tracking (ADR-0006), `InputSeq` generation and `InputCmdProto` send, `SnapshotProto` polling, scripted WASD movement driver
- Movement observed over the real network round trip verified to match the Simulation Core's own formula (exact f64 equality, no epsilon)
- Subprocess-based integration test connecting against the actual compiled `flowstate-server` binary (not just an in-process stand-in), proving the wire path survives a real process boundary

### Changed

- (none yet)

### Deprecated

- (none yet)

### Removed

- (none yet)

### Fixed

- Server Edge: `Host::broadcast()` only queues a packet; without an explicit flush, the final tick's `SnapshotProto` of a match could be silently dropped if the tick loop ended before another `service()` call happened to flush it
- Game Client Test Harness: the same class of bug on the send side — `Peer::send()` only queues; `TestClient::send_input` now flushes explicitly since nothing else services the connection between calls

### Security

- (none yet)
