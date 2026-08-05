---
status: Draft
issue: 8
title: v0 Godot Player-Facing Game Client
---

# FS-0008: v0 Godot Player-Facing Game Client

> **Status:** Draft
> **Issue:** N/A — GitHub Issues are disabled on this fork (`kdelifer/flowstate-kde-custom`). This spec continues FS-0007's numbering sequence nominally; it is not linked to a tracked GitHub issue. If issue tracking is enabled or moved upstream, this frontmatter should be updated to a real issue number before the spec leaves Draft.
> **Owner:** @kenneth
> **Date:** 2026-08-05

## Problem

FS-0007 proved the v0 networking architecture end-to-end — deterministic simulation, wire protocol, replay verification, and a real ENet transport — but only through `crates/client`, a headless Rust test harness with no rendering and no real input. Per its own doc comment, it exists to validate the protocol, not to be played.

There is currently no way for a human to see or play Flowstate. The Game Client (DM-0015) — "the player runtime: rendering, input capture, UI" — has never been implemented. Per ADR-0005, it is intended to be a native Godot client using `ENetMultiplayerPeer` against the same Rust server. The repository already reserves `client/` for this (currently a placeholder), but no work has started there.

This spec defines the minimal Godot client needed to close that gap: connect to the existing `flowstate-server`, render two Characters, and let a human move one of them with WASD, observing the opponent's authoritative movement too — the same functional slice FS-0007 already proved server-side and via the test harness, now player-facing.

## Trace Map

| ID | Relationship | Notes |
|----|--------------|-------|
| DM-0015 | Implements | This spec is the first real implementation of the Game Client concept (rendering, input capture) |
| DM-0007 | Implements | Client renders Snapshot data received from the Server Edge; no local simulation |
| DM-0016 | Implements | Client applies JoinBaseline on connect for initial synchronized state |
| DM-0008 | Constrains | Client lifecycle is bound to the Session established at handshake |
| DM-0025 | Implements | Client tracks TargetTickFloor and clamps outgoing InputCmd.tick to it (ADR-0006) |
| DM-0026 | Implements | Client generates a strictly monotonic InputSeq per session |
| DM-0006 | Implements | Client sends InputCmd (wire: InputCmdProto) for WASD movement intent |
| DM-0003 | Implements | Client renders Character entities from Snapshot/Baseline entity data |
| DM-0019 | Constrains | Client MUST NOT assume or self-assign PlayerId; `controlled_entity_id` from ServerWelcome is authoritative |
| INV-0001 | Constrains | Client MUST NOT affect simulation determinism; it renders server-authoritative state only |
| INV-0002 | Constrains | Client-side rendering/interpolation MUST NOT feed back into the fixed-timestep simulation |
| INV-0003 | Constrains | Client input is intent only; the client MUST NOT author or assume authoritative state |
| KC-0001 | Constrains | Client MUST NOT gain direct access to the Simulation Core; wire-protocol boundary only, same as the test harness |
| ADR-0005 | Implements | v0 networking architecture: Godot client via `ENetMultiplayerPeer`, same channel/message semantics as `crates/client` |
| ADR-0006 | Implements | Input tick targeting (TargetTickFloor, InputSeq) — client-side half of the contract already implemented server-side |

## Domain Concepts

No new domain concepts are introduced. This spec is a second implementation of existing concepts (primarily DM-0015), not a new one.

| Concept | ID | Notes |
|---------|-----|-------|
| Game Client | DM-0015 | This spec's subject |
| Snapshot | DM-0007 | Rendered every tick, unmodified |
| Baseline | DM-0016 | Applied once at JoinBaseline |
| Session | DM-0008 | Established at handshake; scopes InputSeq |
| TargetTickFloor | DM-0025 | Tracked client-side per ADR-0006, same as `crates/client` |
| InputSeq | DM-0026 | Generated client-side, same as `crates/client` |
| InputCmd | DM-0006 | Sent as InputCmdProto on the Realtime channel |
| Character | DM-0003 | Rendered per controlled/remote entity |
| PlayerId / EntityId | DM-0019 / DM-0020 | Used only as received; never assumed |

## Interfaces

### Transport (low risk — protocol already defined)

Godot's `ENetMultiplayerPeer` connects to `flowstate-server`'s existing two-channel ENet host (Realtime = channel 0, Control = channel 1) and follows the exact message flow already implemented server-side and proven by `crates/client`: `ClientHello` → `ServerWelcome` + `JoinBaseline` → per-tick `InputCmdProto` / `SnapshotProto`. No server-side or wire-schema changes are required for this piece.

### Protocol serialization — OPEN ARCHITECTURAL QUESTION

`flowstate-wire`'s message types are Rust structs with inline `#[derive(prost::Message)]` (ADR-0005 explicitly deferred `.proto` files to "v0.2"). Godot/GDScript cannot consume this directly. Three options, in order of preference:

1. **Migrate to `.proto` files now, generate both sides from them.** `crates/wire` switches from inline derive to `prost-build` codegen from `.proto` sources; Godot uses an existing GDScript/GDExtension protobuf plugin to generate matching code from the *same* `.proto` files. Single canonical schema source, same principle T0.19 already enforces for the two Rust binaries, no Rust toolchain requirement on the Godot side. This also happens to be the `.proto` migration ADR-0005 already flagged as coming — this spec would be the natural trigger for it rather than a separate, later effort.
2. **GDExtension binding to `flowstate-wire`.** Compile the Rust wire crate into a GDExtension so GDScript calls real Rust encode/decode functions. Zero schema drift risk by construction, but adds a `gdext` build toolchain dependency and raises the bar for contributors touching only the Godot side.
3. **Hand-written GDScript encoding matching the wire format.** Fastest to start, but two independently-maintained implementations of the same schema with no shared source of truth — directly the kind of drift T0.19 (schema identity) exists to prevent on the Rust side. Not recommended.

**Recommendation:** Option 1. This is flagged as an open question rather than decided unilaterally here because it is a cross-cutting architectural choice with plausible alternatives — per `docs/delivery-path.md`'s Governance Classification Table, this may warrant an ADR (updating ADR-0005 or a new ADR) before implementation locks in, not just a spec-level decision. **Requesting maintainer decision before Gate G0.2 below is implemented.**

### New Godot-side components (conceptual, not final API)

* `NetworkClient` — owns the `ENetMultiplayerPeer` connection and protocol state machine (handshake, TargetTickFloor tracking, InputSeq generation), functionally mirroring what `crates/client::TestClient` already does in Rust.
* `MatchState` — holds the latest applied Baseline/Snapshot and the local `controlled_entity_id`.
* `CharacterView` — a scene node rendering one Character's position from `MatchState`; one instance per entity in the Snapshot, spawned/despawned as entities appear.
* `InputCapture` — reads WASD each frame, produces a `move_dir` intent, and calls `NetworkClient` to send it (client sends on its own cadence, clamped to TargetTickFloor — it does not need to send exactly once per server tick).

## Determinism Notes

No simulation impact. The client performs no simulation of its own:

* It renders exactly what `SnapshotProto`/`JoinBaseline` report — no client-side prediction or reconciliation (explicit FS-0007 Non-Goal, still true here).
* Any interpolation/smoothing between received Snapshots is a presentation-only concern (per DM-0015's own definition) and MUST NOT influence what is sent back to the server or be treated as authoritative by anything.
* The Simulation Core's 2D movement model (`position: [f64; 2]`) does not yet model verticality. This client renders that 2D state in a 3D scene (mapping sim x/y to world X/Z at a fixed height) as a presentation choice — it does not imply the Simulation Core has gained 3D/verticality support, and no such support is in scope here.

## Gate Plan

### Tier 0 (Must pass before merge)

* [ ] **G0.1 (spike, do first):** A minimal Godot scene using `ENetMultiplayerPeer` completes a raw ENet connect handshake against the real `flowstate-server` binary. This directly tests the unverified risk noted in `docs/licensing/third-party.md`'s `rusty_enet` entry — that a pure-Rust ENet transpile actually interoperates with Godot's C-libenet-backed peer. If this fails, the transport choice in ADR-0005/SRV-002 needs to be revisited before anything else here is built.
* [ ] **G0.2:** Chosen protocol serialization approach (see Interfaces) round-trips correctly: encoding/decoding each of `ClientHello`, `ServerWelcome`, `JoinBaseline`, `InputCmdProto`, `SnapshotProto` in Godot produces byte-identical output to `flowstate-wire`'s own encoding of the same logical message (golden-byte fixture generated from the Rust crate).
* [ ] **G0.3:** Two Godot client instances connect to `flowstate-server`, complete handshake, receive `JoinBaseline`, and each spawns a `CharacterView` per entity at the correct baseline position (parity with T0.1/T0.2).
* [ ] **G0.4:** WASD input in Godot produces `InputCmdProto` on the Realtime channel with `tick >= TargetTickFloor` and strictly monotonic `InputSeq` (parity with T0.3).
* [ ] **G0.5:** After N ticks of held WASD input, the Godot client's rendered position for the controlled Character matches the exact deterministic value already proven by `flowstate_sim`'s and `crates/client`'s own tests for the same input sequence (cross-check against the existing Rust-side known-good value, same exact-f64-equality bar as T0.4/CLI-009).
* [ ] **G0.6:** Godot client performs no local simulation: disconnecting the network stream freezes rendered state rather than continuing to move (proves it isn't predicting).
* [ ] Determinism/isolation gates already enforced server-side (T0.5, T0.19) continue to pass unchanged — this spec does not touch `crates/sim` or the wire schema's content, only (potentially) how the schema is authored (see Interfaces).

### Tier 1 (Tracked follow-up)

* [ ] Client-side interpolation between snapshots (presentation smoothing)
* [ ] Reconnect handling after transient disconnect
* [ ] Minimal connect UI (server address entry, connection status)
* [ ] Godot-side automated test integration into `just ci` (no such tooling exists yet in this repo; needs its own decision — e.g., Godot's headless GUT runner — out of scope for this spec to resolve)

### Tier 2 (Aspirational)

* [ ] Real gameplay per `docs/vision.md` (aiming, abilities, locomotion modes, verticality) — explicitly not this spec; the Simulation Core itself doesn't support any of this yet either

## Acceptance Criteria

* [ ] Two Godot client instances, run by a human, can each connect to a running `flowstate-server`, see two Characters rendered at the correct starting positions, and move their own Character with WASD while observing the other player's Character move on their screen too
* [ ] Gates G0.1–G0.6 above all pass
* [ ] `just ci` (Rust-side gates) continues to pass unmodified
* [ ] No changes to `crates/sim`'s public contract or `flowstate-wire`'s message *content* (only, possibly, how those wire messages are authored/generated — see Interfaces)

## Non-Goals

Explicitly out of scope for this spec (matching FS-0007's own Non-Goals plus client-specific exclusions):

* Combat, abilities, aiming/targeting, cast times — none of this exists in the Simulation Core yet
* Locomotion modes beyond grounded WASD (airborne, gliding, rail) — Simulation Core doesn't model these yet either
* Client-side prediction / reconciliation
* Matchmaking, lobbies, server browser
* Mobile/console input or controller support
* Art, UI polish, audio
* Cross-platform determinism (unaffected either way — the client doesn't touch the determinism boundary)
* Resolving the `.proto` migration itself if a different approach is chosen — that becomes its own follow-up scope depending on the maintainer decision requested above

## Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| `rusty_enet` (server) and Godot's `ENetMultiplayerPeer` (C libenet) are not wire-compatible | Medium | High — would force revisiting the SRV-002 transport choice | Gate G0.1 is a standalone spike, done first, before any other client investment |
| Protocol serialization approach undecided | High (certain, until resolved) | Medium — blocks G0.2 onward | Explicit maintainer decision requested in Interfaces section before implementation proceeds past G0.1 |
| No existing Godot test automation in this repo's CI | Medium | Low–Medium — Tier-0 gates above would be manually verified initially | Scoped to Tier 1 follow-up; does not block this spec's Tier 0 |
| Dual toolchain (Rust + Godot) raises contributor onboarding cost | Low | Low | Document setup once implementation lands; not a design blocker |

## Alternatives

### Alternative A: GDExtension binding to `flowstate-wire`

* Pros: zero schema drift by construction; single Rust source of truth
* Cons: adds `gdext`/GDExtension build toolchain; higher barrier for Godot-only contributors; couples Godot build to Rust ABI stability
* Not rejected — viable second choice if the `.proto` migration proves impractical

### Alternative B: `.proto` files + native codegen on both sides (recommended)

* Pros: single schema source without a cross-language ABI dependency; aligns with ADR-0005's already-planned v0.2 migration; standard tooling on both sides
* Cons: requires the migration itself as prerequisite work; two independent codegen outputs from one schema (small, well-understood risk class, not schema drift)

### Alternative C: Hand-written GDScript wire codec

* Pros: fastest to start
* Cons: no shared source of truth; direct schema-drift risk; rejected

## Changelog

| Date | Owner | Change |
|------|-------|--------|
| 2026-08-05 | @kenneth | Initial draft |
