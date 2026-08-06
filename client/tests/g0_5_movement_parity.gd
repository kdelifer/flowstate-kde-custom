# FS-0008 Gate G0.5: after N ticks of held movement, the Godot client's
# rendered position for the controlled Character matches the exact
# deterministic value already proven server-side.
#
# Direct GDScript translation of
# crates/client/tests/movement_parity.rs's
# test_cli_009_movement_matches_simulation_core_formula: drives a scripted
# move_dir=[1,0] for NUM_MOVE_TICKS ticks starting after a margin past the
# initial floor (avoids racing the very first eligible tick against server
# pacing/scheduling jitter), polls until the checkpoint tick's snapshot,
# and asserts the controlled entity's position with exact f64 equality --
# same bar as the Rust-side test_t0_04_wasd_deterministic_movement /
# CLI-009 parity.
#
# Requires flowstate-server already running on 127.0.0.1:6060 with default
# ServerConfig (MOVE_SPEED=5.0, tick_rate_hz from ServerWelcome).
#
# Run: godot --headless --path client -s tests/g0_5_movement_parity.gd
# Exit code 0 = pass, 1 = fail.

extends SceneTree

const SERVER_ADDR := "127.0.0.1"
const SERVER_PORT := 6060
const DEADLINE_MS := 15000

const INPUT_LEAD_TICKS := 1
const START_MARGIN_TICKS := 10
const NUM_MOVE_TICKS := 10
## Mirrors flowstate_sim::MOVE_SPEED (crates/sim/src/lib.rs).
const MOVE_SPEED := 5.0

var client_a: NetworkClient
var client_idle: NetworkClient
var _ready_seen := false
var _driven := false
var _start_tick: int
var _checkpoint_tick: int
var _final_position := PackedFloat64Array()
var _deadline_msec: int
var _done := false


func _initialize() -> void:
	_start_tick = INPUT_LEAD_TICKS + START_MARGIN_TICKS
	_checkpoint_tick = _start_tick + NUM_MOVE_TICKS

	client_a = NetworkClient.new()
	client_idle = NetworkClient.new()
	root.add_child(client_a)
	root.add_child(client_idle)
	client_a.baseline_received.connect(_on_baseline_received)
	client_a.snapshot_received.connect(_on_snapshot_received)
	client_a.connect_to_server(SERVER_ADDR, SERVER_PORT)
	client_idle.connect_to_server(SERVER_ADDR, SERVER_PORT)
	_deadline_msec = Time.get_ticks_msec() + DEADLINE_MS


func _on_baseline_received(_baseline: JoinBaseline) -> void:
	_ready_seen = true


func _on_snapshot_received(snapshot: SnapshotProto) -> void:
	if snapshot.tick != _checkpoint_tick or not _final_position.is_empty():
		return
	var controlled_id := client_a.welcome().controlled_entity_id
	for e in snapshot.entities:
		var entity: EntitySnapshotProto = e
		if entity.entity_id == controlled_id:
			_final_position = entity.position


func _process(_delta: float) -> bool:
	if _done:
		return true

	if Time.get_ticks_msec() >= _deadline_msec:
		_done = true
		push_error("G0.5 FAIL: timed out before checkpoint tick %d (ready_seen=%s driven=%s)" % [_checkpoint_tick, _ready_seen, _driven])
		quit(1)
		return true

	if _ready_seen and not _driven:
		_driven = true
		for offset in range(NUM_MOVE_TICKS):
			client_a.send_input(PackedFloat64Array([1.0, 0.0]), _start_tick + offset)

	if not _final_position.is_empty():
		_done = true
		_finish()
		return true

	return false


func _finish() -> void:
	var tick_rate_hz := client_a.welcome().tick_rate_hz
	var dt := 1.0 / float(tick_rate_hz)
	var expected_x := float(NUM_MOVE_TICKS) * MOVE_SPEED * dt

	if _final_position[0] != expected_x:
		push_error("G0.5 FAIL: position.x=%.17f expected %.17f" % [_final_position[0], expected_x])
		quit(1)
		return
	if _final_position[1] != 0.0:
		push_error("G0.5 FAIL: position.y=%.17f expected 0.0" % _final_position[1])
		quit(1)
		return

	print("G0.5 PASS: position after %d driven ticks matches sim formula exactly: [%s, %s]" % [NUM_MOVE_TICKS, _final_position[0], _final_position[1]])
	quit(0)
