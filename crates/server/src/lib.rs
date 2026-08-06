//! Flowstate Server Edge
//!
//! The Server Edge mediates communication between Game Clients and the
//! Simulation Core. It owns:
//! - Session management (DM-0008)
//! - Input validation and buffering
//! - TargetTickFloor computation (ADR-0006)
//! - AppliedInput → StepInput conversion
//! - Replay recording
//!
//! # Architecture (INV-0003, INV-0004)
//!
//! The Server Edge performs all I/O on behalf of the Game Server Instance.
//! The Simulation Core is invoked only with StepInput and produces Snapshots.
//!
//! # References
//!
//! - INV-0003: Authoritative Simulation
//! - INV-0004: Simulation Core Isolation
//! - INV-0005: Tick-Indexed I/O Contract
//! - ADR-0005: v0 Networking Architecture
//! - ADR-0006: Input Tick Targeting
//! - DM-0011: Server Edge

#![deny(unsafe_code)]

pub mod input_buffer;
pub mod net;
pub mod session;
pub mod tick_loop;
pub mod validation;

use std::collections::HashMap;

use flowstate_replay::{AppliedInput, BuildFingerprintData, ReplayConfig, ReplayRecorder};
use flowstate_sim::{PlayerId, Snapshot, StepInput, Tick, World};
use flowstate_wire::{InputCmdProto, JoinBaseline, ReplayArtifact, ServerWelcome, SnapshotProto};
use input_buffer::InputBuffer;
use session::{Session, SessionId};
use validation::{ValidationConfig, ValidationResult, validate_input};

// ============================================================================
// v0 Parameters (from docs/networking/v0-parameters.md)
// ============================================================================

/// v0 tick rate in Hz.
pub const TICK_RATE_HZ: u32 = 60;

/// Maximum ticks ahead a client can target.
pub const MAX_FUTURE_TICKS: u64 = 120;

/// TargetTickFloor lead.
pub const INPUT_LEAD_TICKS: u64 = 1;

/// Input rate limit per second.
pub const INPUT_RATE_LIMIT_PER_SEC: u32 = 120;

/// Match duration in ticks.
pub const MATCH_DURATION_TICKS: u64 = 3600;

/// Connection timeout in milliseconds.
pub const CONNECT_TIMEOUT_MS: u64 = 30000;

/// Default cap on concurrent sessions (transport-level ENet peer limit).
/// Generous for testing with any number of clients; well under `PlayerId`'s
/// `u8` range.
pub const DEFAULT_MAX_SESSIONS: usize = 16;

// ============================================================================
// Match End Reason
// ============================================================================

/// Reason for match termination.
///
/// Disconnects no longer end the match (a session dropping does not affect
/// other connected sessions), so `Complete` (duration reached) is currently
/// the only way a match ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndReason {
    Complete,
}

impl EndReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Complete => "complete",
        }
    }
}

// ============================================================================
// Server State
// ============================================================================

/// Server configuration.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub seed: u64,
    pub tick_rate_hz: u32,
    pub max_future_ticks: u64,
    pub input_lead_ticks: u64,
    pub input_rate_limit_per_sec: u32,
    pub match_duration_ticks: u64,
    pub connect_timeout_ms: u64,
    /// Cap on concurrent sessions (drives the ENet transport's peer limit).
    pub max_sessions: usize,
    pub test_mode: bool,
    /// Pre-assigned PlayerIds, consumed in connection order. Exhausting the
    /// list (more accepts than configured IDs) panics -- test-only usage
    /// where the caller controls how many accepts happen.
    pub test_player_ids: Option<Vec<PlayerId>>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            seed: 0,
            tick_rate_hz: TICK_RATE_HZ,
            max_future_ticks: MAX_FUTURE_TICKS,
            input_lead_ticks: INPUT_LEAD_TICKS,
            input_rate_limit_per_sec: INPUT_RATE_LIMIT_PER_SEC,
            match_duration_ticks: MATCH_DURATION_TICKS,
            connect_timeout_ms: CONNECT_TIMEOUT_MS,
            max_sessions: DEFAULT_MAX_SESSIONS,
            test_mode: false,
            test_player_ids: None,
        }
    }
}

/// Server state for running a match.
pub struct Server {
    config: ServerConfig,
    world: World,
    sessions: HashMap<SessionId, Session>,
    next_session_id: SessionId,
    /// Monotonically increasing, never reused across the server's lifetime
    /// -- reusing `sessions.len()`-based assignment would collide once
    /// sessions can disconnect and new ones join later. In test_mode, used
    /// as an ordinal index into `test_player_ids` instead of an ID value.
    next_player_id: PlayerId,
    /// PlayerId → SessionId mapping
    player_sessions: HashMap<PlayerId, SessionId>,
    /// SessionId → PlayerId mapping (for convenience)
    session_players: HashMap<SessionId, PlayerId>,
    /// Input buffer per (player_id, tick)
    input_buffer: InputBuffer,
    /// Last known intent per player
    last_known_intent: HashMap<PlayerId, [f64; 2]>,
    /// Last emitted target tick floor per session
    last_emitted_floor: HashMap<SessionId, Tick>,
    /// Replay recorder
    replay_recorder: ReplayRecorder,
    /// Entity spawn order (player_ids in order)
    entity_spawn_order: Vec<PlayerId>,
    /// Player → Entity mapping
    player_entity_mapping: HashMap<PlayerId, flowstate_sim::EntityId>,
    /// Whether the replay's initial baseline has been recorded yet (happens
    /// lazily on the first `step()` call).
    baseline_recorded: bool,
    /// Build fingerprint
    build_fingerprint: Option<BuildFingerprintData>,
}

impl Server {
    /// Create a new server with the given configuration.
    pub fn new(config: ServerConfig) -> Self {
        let validation_config = ValidationConfig {
            max_future_ticks: config.max_future_ticks,
            input_rate_limit_per_sec: config.input_rate_limit_per_sec,
            tick_rate_hz: config.tick_rate_hz,
        };

        let replay_config = ReplayConfig {
            seed: config.seed,
            tick_rate_hz: config.tick_rate_hz,
            rng_algorithm: "none".to_string(),
            test_mode: config.test_mode,
            test_player_ids: config.test_player_ids.clone().unwrap_or_default(),
        };

        Self {
            world: World::new(config.seed, config.tick_rate_hz),
            sessions: HashMap::new(),
            next_session_id: 1,
            next_player_id: 0,
            player_sessions: HashMap::new(),
            session_players: HashMap::new(),
            input_buffer: InputBuffer::new(validation_config),
            last_known_intent: HashMap::new(),
            last_emitted_floor: HashMap::new(),
            replay_recorder: ReplayRecorder::new(replay_config),
            entity_spawn_order: Vec::new(),
            player_entity_mapping: HashMap::new(),
            baseline_recorded: false,
            build_fingerprint: None,
            config,
        }
    }

    /// Set the build fingerprint.
    pub fn set_build_fingerprint(&mut self, fingerprint: BuildFingerprintData) {
        self.build_fingerprint = Some(fingerprint.clone());
        self.replay_recorder.set_build_fingerprint(fingerprint);
    }

    /// Get current tick.
    pub fn current_tick(&self) -> Tick {
        self.world.tick()
    }

    /// Get number of connected sessions.
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// Accept a new session (client connected). Callable at any time --
    /// before or after other sessions exist, before or after the world has
    /// started ticking. Each accepted session must be individually welcomed
    /// via [`Server::welcome_for`] (no more single-shot `start_match`
    /// barrier).
    /// Returns (session_id, assigned_player_id, controlled_entity_id).
    ///
    /// # Panics
    /// If `config.max_sessions` is exceeded. In practice this should not
    /// happen: the ENet transport's peer limit (driven by the same config)
    /// already rejects connections beyond that cap before a ClientHello
    /// could ever reach this call.
    pub fn accept_session(&mut self) -> (SessionId, PlayerId, flowstate_sim::EntityId) {
        assert!(
            self.sessions.len() < self.config.max_sessions,
            "max_sessions ({}) exceeded",
            self.config.max_sessions
        );

        let session_id = self.next_session_id;
        self.next_session_id += 1;

        // Assign player ID: monotonic ordinal, never reused (see
        // next_player_id's doc comment). In test_mode this ordinal indexes
        // into the pre-configured ID list instead of being used directly.
        let ordinal = self.next_player_id;
        self.next_player_id += 1;
        let player_id = if let Some(test_ids) = &self.config.test_player_ids {
            *test_ids.get(ordinal as usize).unwrap_or_else(|| {
                panic!(
                    "test_player_ids exhausted: accept #{ordinal} requested but only {} configured",
                    test_ids.len()
                )
            })
        } else {
            ordinal
        };

        // Spawn character
        let entity_id = self.world.spawn_character(player_id);

        // Create session
        let session = Session::new(session_id, player_id, entity_id);
        self.sessions.insert(session_id, session);
        self.player_sessions.insert(player_id, session_id);
        self.session_players.insert(session_id, player_id);

        // Record spawn order
        self.entity_spawn_order.push(player_id);
        self.player_entity_mapping.insert(player_id, entity_id);
        self.replay_recorder.record_spawn(player_id, entity_id);

        // Initialize last known intent
        self.last_known_intent.insert(player_id, [0.0, 0.0]);

        (session_id, player_id, entity_id)
    }

    /// Compute and record the `ServerWelcome` for a session immediately
    /// after [`Server::accept_session`]. Reflects the world's *current*
    /// tick/state at the moment of the call, not a fixed match-start
    /// snapshot -- a session accepted after the world has already advanced
    /// gets a floor computed from wherever the world currently is.
    ///
    /// # Panics
    /// If `session_id` was not returned by a prior `accept_session` call.
    pub fn welcome_for(&mut self, session_id: SessionId) -> ServerWelcome {
        let session = self
            .sessions
            .get(&session_id)
            .expect("welcome_for: unknown session_id");
        let target_tick_floor = self.world.tick() + self.config.input_lead_ticks;
        self.last_emitted_floor
            .insert(session_id, target_tick_floor);

        ServerWelcome {
            target_tick_floor,
            tick_rate_hz: self.config.tick_rate_hz,
            player_id: u32::from(session.player_id),
            controlled_entity_id: session.controlled_entity_id,
        }
    }

    /// Check if match should end. Duration-based only -- a session
    /// disconnecting no longer ends the match for everyone else.
    pub fn should_end_match(&self) -> Option<EndReason> {
        if self.world.tick() >= self.config.match_duration_ticks {
            Some(EndReason::Complete)
        } else {
            None
        }
    }

    /// Handle session disconnect. Does NOT end the match. The departed
    /// player's Character freezes in place: their last known intent resets
    /// to zero, so subsequent ticks stop applying their last real input
    /// (which would otherwise have them coast forever) instead of
    /// despawning them.
    pub fn disconnect_session(&mut self, session_id: SessionId) {
        if let Some(session) = self.sessions.remove(&session_id) {
            self.player_sessions.remove(&session.player_id);
            self.session_players.remove(&session_id);
            self.last_emitted_floor.remove(&session_id);
            self.last_known_intent.insert(session.player_id, [0.0, 0.0]);
        }
    }

    /// Receive and buffer an input from a client.
    /// Returns validation result.
    pub fn receive_input(
        &mut self,
        session_id: SessionId,
        input: InputCmdProto,
    ) -> ValidationResult {
        // Every session in `session_players` has already been synchronously
        // welcomed (accept + welcome_for happen before any input could
        // possibly arrive over the network), so this lookup already covers
        // "not accepted yet" -- no separate pre-welcome state to track.
        let Some(&player_id) = self.session_players.get(&session_id) else {
            return ValidationResult::DroppedUnknownSession;
        };

        // Get last emitted floor for this session
        let floor = self
            .last_emitted_floor
            .get(&session_id)
            .copied()
            .unwrap_or(0);

        // Validate input
        validate_input(
            &input,
            self.world.tick(),
            floor,
            &mut self.input_buffer,
            player_id,
        )
    }

    /// Process a single tick.
    /// Returns (snapshot, target_tick_floor, serialized_snapshot_bytes).
    ///
    /// The serialized bytes are identical for all sessions (T0.18).
    pub fn step(&mut self) -> (Snapshot, Tick, Vec<u8>) {
        // Record the replay's initial baseline lazily, on the first step
        // ever taken -- captures whatever sessions have joined by the
        // moment ticking begins, mirroring the old start_match()-recorded
        // timing without requiring a fixed number of sessions upfront.
        if !self.baseline_recorded {
            self.baseline_recorded = true;
            self.replay_recorder.record_baseline(self.world.baseline());
        }

        let current_tick = self.world.tick();

        // Produce AppliedInput per player
        let mut applied_inputs: Vec<AppliedInput> = Vec::new();

        for &player_id in self.entity_spawn_order.iter() {
            let (move_dir, is_fallback) = self
                .input_buffer
                .take_input(player_id, current_tick)
                .map(|cmd| {
                    // Validate and normalize move_dir
                    let move_dir = if cmd.move_dir.len() == 2 {
                        [cmd.move_dir[0], cmd.move_dir[1]]
                    } else {
                        [0.0, 0.0]
                    };
                    (move_dir, false)
                })
                .unwrap_or_else(|| {
                    // LastKnownIntent fallback
                    let lki = self
                        .last_known_intent
                        .get(&player_id)
                        .copied()
                        .unwrap_or([0.0, 0.0]);
                    (lki, true)
                });

            // Update last known intent
            self.last_known_intent.insert(player_id, move_dir);

            applied_inputs.push(AppliedInput {
                tick: current_tick,
                player_id,
                move_dir,
                is_fallback,
            });
        }

        // Record for replay
        for input in &applied_inputs {
            self.replay_recorder.record_input(input.clone());
        }

        // Convert to StepInput (sorted by player_id)
        let mut step_inputs: Vec<StepInput> = applied_inputs
            .iter()
            .map(AppliedInput::to_step_input)
            .collect();
        step_inputs.sort_by_key(|i| i.player_id);

        // Advance world
        let snapshot = self.world.advance(current_tick, &step_inputs);

        // Compute new target tick floor (post-step tick + lead)
        let target_tick_floor = self.world.tick() + self.config.input_lead_ticks;

        // Update floor for all sessions
        for session_id in self.sessions.keys() {
            self.last_emitted_floor
                .insert(*session_id, target_tick_floor);
        }

        // Evict old buffered inputs
        self.input_buffer.evict_before(self.world.tick());

        // Serialize snapshot (identical for all sessions - T0.18)
        let snapshot_proto = SnapshotProto {
            tick: snapshot.tick,
            entities: snapshot
                .entities
                .iter()
                .map(|e| flowstate_wire::EntitySnapshotProto {
                    entity_id: e.entity_id,
                    position: e.position.to_vec(),
                    velocity: e.velocity.to_vec(),
                })
                .collect(),
            digest: snapshot.digest,
            target_tick_floor,
        };
        let snapshot_bytes = prost::Message::encode_to_vec(&snapshot_proto);

        (snapshot, target_tick_floor, snapshot_bytes)
    }

    /// Finalize the match and produce a replay artifact.
    pub fn finalize(self, end_reason: EndReason) -> ReplayArtifact {
        let final_digest = self.world.state_digest();
        let checkpoint_tick = self.world.tick();

        self.replay_recorder
            .finalize(final_digest, checkpoint_tick, end_reason.as_str())
    }

    /// Get the baseline for JoinBaseline message.
    pub fn baseline_proto(&self) -> JoinBaseline {
        let baseline = self.world.baseline();
        baseline.into()
    }

    /// Get all connected session IDs.
    pub fn session_ids(&self) -> Vec<SessionId> {
        self.sessions.keys().copied().collect()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// T0.1: Two clients connect, complete handshake.
    #[test]
    fn test_t0_01_two_client_handshake() {
        let mut server = Server::new(ServerConfig::default());

        // Accept first session
        let (session1, player1, entity1) = server.accept_session();
        assert_eq!(player1, 0);
        assert!(entity1 > 0);
        assert_eq!(server.session_count(), 1);
        let welcome1 = server.welcome_for(session1);
        assert_eq!(welcome1.target_tick_floor, INPUT_LEAD_TICKS);
        assert_eq!(welcome1.tick_rate_hz, TICK_RATE_HZ);
        assert_eq!(welcome1.player_id, 0);
        assert_eq!(welcome1.controlled_entity_id, entity1);

        // Accept second session
        let (session2, player2, entity2) = server.accept_session();
        assert_eq!(player2, 1);
        assert!(entity2 > 0);
        assert_ne!(entity1, entity2);
        assert_eq!(server.session_count(), 2);
        let welcome2 = server.welcome_for(session2);
        assert_eq!(welcome2.target_tick_floor, INPUT_LEAD_TICKS);
        assert_eq!(welcome2.player_id, 1);
        assert_eq!(welcome2.controlled_entity_id, entity2);
    }

    /// T0.2: JoinBaseline delivers initial Baseline.
    #[test]
    fn test_t0_02_join_baseline() {
        let mut server = Server::new(ServerConfig::default());
        server.accept_session();
        server.accept_session();

        let baseline = server.baseline_proto();

        // Baseline should have 2 entities at tick 0
        assert_eq!(baseline.tick, 0);
        assert_eq!(baseline.entities.len(), 2);
        assert!(baseline.digest != 0);
    }

    /// T0.5a: Tick/floor relationship assertion.
    #[test]
    fn test_t0_05a_tick_floor_relationship() {
        let mut server = Server::new(ServerConfig::default());
        server.accept_session();
        server.accept_session();

        // Step once
        let (snapshot, floor, _) = server.step();

        // After advance(0, inputs), snapshot.tick should be 1
        assert_eq!(snapshot.tick, 1);
        // Floor should be post-step tick + lead = 1 + 1 = 2
        assert_eq!(floor, 1 + INPUT_LEAD_TICKS);

        // Step again
        let (snapshot2, floor2, _) = server.step();
        assert_eq!(snapshot2.tick, 2);
        assert_eq!(floor2, 2 + INPUT_LEAD_TICKS);
    }

    /// T0.14: Disconnect handling -- does NOT end the match, and freezes
    /// the departed player's Character (last known intent resets to zero)
    /// rather than letting it coast on stale intent.
    #[test]
    fn test_t0_14_disconnect_handling() {
        let mut server = Server::new(ServerConfig::default());
        let (session1, _player1, entity1) = server.accept_session();
        server.accept_session();
        server.welcome_for(session1);

        server.receive_input(
            session1,
            InputCmdProto {
                tick: INPUT_LEAD_TICKS,
                input_seq: 1,
                move_dir: vec![1.0, 0.0],
            },
        );
        // Tick 0 is always a forced no-op (ADR-0006 startup behavior: the
        // initial floor is input_lead_ticks=1, so no client can target tick
        // 0) -- the buffered input above targets tick 1, so it only takes
        // effect on the *second* step.
        server.step();
        let (snapshot_before, _, _) = server.step();
        let pos_before = snapshot_before
            .entities
            .iter()
            .find(|e| e.entity_id == entity1)
            .unwrap()
            .position;
        assert_ne!(pos_before, [0.0, 0.0], "sanity: player1 should have moved");

        server.disconnect_session(session1);
        assert_eq!(server.session_count(), 1);
        assert_eq!(
            server.should_end_match(),
            None,
            "disconnect must not end the match"
        );

        let (snapshot_after, _, _) = server.step();
        let pos_after = snapshot_after
            .entities
            .iter()
            .find(|e| e.entity_id == entity1)
            .unwrap()
            .position;
        assert_eq!(
            pos_after, pos_before,
            "disconnected player's Character must freeze in place"
        );
    }

    /// PlayerIds must never be reused, even after a disconnect frees up a
    /// "slot" -- otherwise a new joiner could collide with a still-connected
    /// session's PlayerId.
    #[test]
    fn test_player_id_not_reused_after_disconnect() {
        let mut server = Server::new(ServerConfig::default());
        let (session0, player0, _) = server.accept_session();
        let (_, player1, _) = server.accept_session();
        assert_eq!((player0, player1), (0, 1));

        server.disconnect_session(session0);
        let (_, player2, _) = server.accept_session();
        assert_eq!(player2, 2);
    }

    /// A single session is sufficient to start ticking and receive
    /// snapshots -- no second session is required.
    #[test]
    fn test_single_session_can_start_and_step() {
        let mut server = Server::new(ServerConfig::default());
        let (session1, _, entity1) = server.accept_session();
        let welcome = server.welcome_for(session1);
        assert_eq!(welcome.target_tick_floor, INPUT_LEAD_TICKS);

        let (snapshot, _, _) = server.step();
        assert_eq!(snapshot.entities.len(), 1);
        assert_eq!(snapshot.entities[0].entity_id, entity1);
    }

    /// Sessions beyond the old v0 cap of 2 are supported.
    #[test]
    fn test_accept_session_supports_more_than_two() {
        let mut server = Server::new(ServerConfig::default());
        let (_, player0, _) = server.accept_session();
        let (_, player1, _) = server.accept_session();
        let (_, player2, _) = server.accept_session();
        assert_eq!([player0, player1, player2], [0, 1, 2]);
        assert_eq!(server.session_count(), 3);
    }

    /// T0.15: Match termination.
    #[test]
    fn test_t0_15_match_termination() {
        let config = ServerConfig {
            match_duration_ticks: 10, // Short match for testing
            ..Default::default()
        };
        let mut server = Server::new(config);
        server.accept_session();
        server.accept_session();

        // Run until match should end
        for _ in 0..10 {
            assert!(server.should_end_match().is_none());
            server.step();
        }

        assert_eq!(server.should_end_match(), Some(EndReason::Complete));
    }

    /// T0.17: PlayerId non-assumption (test mode).
    #[test]
    fn test_t0_17_playerid_test_mode() {
        let config = ServerConfig {
            test_mode: true,
            test_player_ids: Some(vec![17, 99]),
            match_duration_ticks: 10,
            ..Default::default()
        };
        let mut server = Server::new(config);

        let (_, player1, _) = server.accept_session();
        let (_, player2, _) = server.accept_session();

        assert_eq!(player1, 17);
        assert_eq!(player2, 99);

        // Run a few ticks
        for _ in 0..5 {
            server.step();
        }

        // Finalize and check artifact
        let artifact = server.finalize(EndReason::Complete);
        assert!(artifact.test_mode);
        assert_eq!(artifact.test_player_ids, vec![17, 99]);
        assert_eq!(artifact.entity_spawn_order, vec![17, 99]);
    }

    /// T0.18: Floor coherency - byte-identical broadcasts.
    #[test]
    fn test_t0_18_floor_coherency_broadcast() {
        let mut server = Server::new(ServerConfig::default());
        server.accept_session();
        server.accept_session();

        // Step and get serialized snapshot
        let (_, floor1, bytes1) = server.step();

        // The bytes would be sent to all sessions identically
        // Decode to verify floor is consistent
        let decoded: SnapshotProto = prost::Message::decode(bytes1.as_slice()).unwrap();
        assert_eq!(decoded.target_tick_floor, floor1);

        // Step again
        let (_, floor2, bytes2) = server.step();
        let decoded2: SnapshotProto = prost::Message::decode(bytes2.as_slice()).unwrap();
        assert_eq!(decoded2.target_tick_floor, floor2);
        assert!(floor2 > floor1, "Floor should be monotonic increasing");
    }

    /// T0.12: LastKnownIntent determinism - empty inputs use LKI.
    #[test]
    fn test_t0_12_lki_fallback() {
        let config = ServerConfig {
            match_duration_ticks: 10,
            ..Default::default()
        };
        let mut server = Server::new(config);
        server.accept_session();
        server.accept_session();

        // Step without any inputs - should use LKI (zero)
        let (snapshot1, _, _) = server.step();

        // All entities should be at origin (no movement with zero LKI)
        for entity in &snapshot1.entities {
            assert_eq!(entity.position, [0.0, 0.0]);
        }

        // Now finalize and verify artifact has fallback inputs
        let artifact = server.finalize(EndReason::Complete);

        // All inputs should be fallback since we didn't send any
        assert!(artifact.inputs.iter().all(|i| i.is_fallback));
    }

    /// Test replay artifact generation.
    #[test]
    fn test_replay_artifact_generation() {
        let config = ServerConfig {
            match_duration_ticks: 5,
            ..Default::default()
        };
        let mut server = Server::new(config);
        server.accept_session();
        server.accept_session();

        // Run the match
        while server.should_end_match().is_none() {
            server.step();
        }

        let artifact = server.finalize(EndReason::Complete);

        assert_eq!(artifact.replay_format_version, 1);
        assert!(artifact.initial_baseline.is_some());
        assert_eq!(artifact.tick_rate_hz, 60);
        assert_eq!(artifact.checkpoint_tick, 5);
        assert_eq!(artifact.end_reason, "complete");
        // 5 ticks * 2 players = 10 inputs
        assert_eq!(artifact.inputs.len(), 10);
    }

    /// T0.13a: Floor enforcement and recovery.
    ///
    /// Simulates a scenario where inputs are submitted below floor (as if
    /// snapshot packets were lost). Verifies these are dropped, then
    /// "recovery" occurs when inputs target future ticks again.
    #[test]
    fn test_t0_13a_floor_enforcement_recovery() {
        let config = ServerConfig {
            match_duration_ticks: 20,
            ..Default::default()
        };
        let mut server = Server::new(config);
        let (session1, _, _) = server.accept_session();
        server.accept_session();
        let welcome1 = server.welcome_for(session1);

        // Get initial floor (verified for sanity)
        let initial_floor = welcome1.target_tick_floor;
        assert_eq!(initial_floor, INPUT_LEAD_TICKS);

        // Step a few times to advance the floor
        for _ in 0..5 {
            server.step();
        }

        // Floor should now be higher
        let current_tick = 5;
        let current_floor = current_tick + INPUT_LEAD_TICKS;

        // Try to submit an input targeting OLD tick (below floor) - should be dropped
        let stale_input = InputCmdProto {
            tick: 2, // Way below current floor
            input_seq: 1,
            move_dir: vec![1.0, 0.0],
        };
        let result = server.receive_input(session1, stale_input);
        assert!(
            matches!(result, ValidationResult::DroppedBelowFloor { .. }),
            "Input below floor should be dropped: {:?}",
            result
        );

        // Now submit a valid input targeting current floor - should be accepted
        let valid_input = InputCmdProto {
            tick: current_floor,
            input_seq: 2,
            move_dir: vec![1.0, 0.0],
        };
        let result = server.receive_input(session1, valid_input);
        assert!(
            result.is_accepted(),
            "Input at floor should be accepted: {:?}",
            result
        );
    }

    /// T0.16: Connection timeout.
    ///
    /// Server should detect when connection phase exceeds timeout.
    /// Note: In v0, actual timeout is external (e.g., orchestrator checks).
    /// This test verifies the timeout constant exists and server exposes
    /// connection state for external timeout enforcement (now gating "at
    /// least 1 session" instead of "exactly 2").
    #[test]
    fn test_t0_16_connection_timeout() {
        // Verify timeout constant is set per v0-parameters
        assert_eq!(CONNECT_TIMEOUT_MS, 30000);

        // Create server and verify session tracking
        let mut server = Server::new(ServerConfig::default());
        assert_eq!(server.session_count(), 0);

        // Add one session - the external timeout check
        // (session_count() >= 1) would now be satisfied.
        server.accept_session();
        assert_eq!(server.session_count(), 1);

        // The timeout itself would be enforced externally by checking:
        // - start_time (when server was created)
        // - current_time - start_time > CONNECT_TIMEOUT_MS
        // - server.session_count() == 0
        // If that condition is true, orchestrator would exit with non-zero.
        // The server exposes enough state for this check.
    }
}
