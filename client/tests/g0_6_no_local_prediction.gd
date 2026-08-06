# FS-0008 Gate G0.6: the Godot client performs no local simulation --
# disconnecting the network stream freezes rendered state rather than
# continuing to move.
#
# Uses a real CharacterView Node (instantiated under root, updated only via
# apply_state() from decoded snapshots) rather than a data-only stand-in,
# specifically so that if a future regression added per-frame local
# prediction into CharacterView, this test's normal frame ticking would
# organically exercise and catch it -- checking only data structures never
# touched by the engine's frame loop would make this gate a tautology.
#
# Drives a few ticks of movement, then performs a CLEAN ENet disconnect
# (peer.peer_disconnect(), not an abrupt kill) -- deliberate choice to
# isolate what this gate actually verifies (no client-side prediction) from
# the separate, already-fixed server robustness path (see tick_loop.rs's
# fix for abrupt disconnects). Requires flowstate-server already running on
# 127.0.0.1:6060.
#
# Run: godot --headless --path client -s tests/g0_6_no_local_prediction.gd
# Exit code 0 = pass, 1 = fail.

extends SceneTree

const SERVER_ADDR := "127.0.0.1"
const SERVER_PORT := 6060
const DEADLINE_MS := 15000

const INPUT_LEAD_TICKS := 1
const START_MARGIN_TICKS := 10
const NUM_MOVE_TICKS := 5
## Real engine frames to let elapse after disconnect before asserting the
## rendered position never moved.
const POST_DISCONNECT_FRAMES := 60

var client_a: NetworkClient
var client_idle: NetworkClient
var view: CharacterView
var _controlled_id := -1
var _ready_seen := false
var _driven := false
var _start_tick: int
var _last_applied_tick := -1
var _disconnect_requested := false
var _frozen_position: Vector3
var _post_disconnect_frame_count := 0
var _deadline_msec: int
var _done := false


func _initialize() -> void:
	_start_tick = INPUT_LEAD_TICKS + START_MARGIN_TICKS

	client_a = NetworkClient.new()
	client_idle = NetworkClient.new()
	root.add_child(client_a)
	root.add_child(client_idle)
	client_a.baseline_received.connect(_on_baseline_received)
	client_a.snapshot_received.connect(_on_snapshot_received)
	client_a.connect_to_server(SERVER_ADDR, SERVER_PORT)
	client_idle.connect_to_server(SERVER_ADDR, SERVER_PORT)
	_deadline_msec = Time.get_ticks_msec() + DEADLINE_MS


func _on_baseline_received(baseline: JoinBaseline) -> void:
	_ready_seen = true
	_controlled_id = client_a.welcome().controlled_entity_id

	var scene: PackedScene = load("res://view/character_view.tscn")
	view = scene.instantiate()
	root.add_child(view)
	view.entity_id = _controlled_id
	view.set_controlled(true)
	for e in baseline.entities:
		var entity: EntitySnapshotProto = e
		if entity.entity_id == _controlled_id:
			view.apply_state(entity.position, entity.velocity)


func _on_snapshot_received(snapshot: SnapshotProto) -> void:
	# Once disconnect has been requested, deliberately ignore anything still
	# in flight -- from this point nothing should update the render, which
	# is exactly the behavior under test.
	if _disconnect_requested:
		return
	for e in snapshot.entities:
		var entity: EntitySnapshotProto = e
		if entity.entity_id == _controlled_id:
			view.apply_state(entity.position, entity.velocity)
			_last_applied_tick = snapshot.tick


func _process(_delta: float) -> bool:
	if _done:
		return true

	if Time.get_ticks_msec() >= _deadline_msec:
		_done = true
		push_error("G0.6 FAIL: timed out (ready_seen=%s driven=%s last_applied_tick=%d)" % [_ready_seen, _driven, _last_applied_tick])
		quit(1)
		return true

	if _ready_seen and not _driven:
		_driven = true
		for offset in range(NUM_MOVE_TICKS):
			client_a.send_input(PackedFloat64Array([1.0, 0.0]), _start_tick + offset)

	if _driven and not _disconnect_requested and _last_applied_tick >= _start_tick + NUM_MOVE_TICKS:
		_disconnect_requested = true
		_frozen_position = view.global_position
		client_a.disconnect_clean()

	if _disconnect_requested:
		_post_disconnect_frame_count += 1
		if _post_disconnect_frame_count >= POST_DISCONNECT_FRAMES:
			_done = true
			_finish()
			return true

	return false


func _finish() -> void:
	if view.global_position != _frozen_position:
		push_error("G0.6 FAIL: position moved after disconnect (was %s, now %s) -- client-side prediction detected" % [_frozen_position, view.global_position])
		quit(1)
		return

	print("G0.6 PASS: position stayed frozen at %s for %d frames after a clean disconnect" % [_frozen_position, POST_DISCONNECT_FRAMES])
	quit(0)
