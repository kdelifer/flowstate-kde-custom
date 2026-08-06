# FS-0008 Gate G0.3: two Godot NetworkClient instances connect to a live
# flowstate-server, complete handshake, receive JoinBaseline, and each
# spawns a CharacterView per entity at the correct baseline position.
#
# Uses the SceneTree._process(delta)->bool override so real NetworkClient/
# CharacterView Nodes, added under `root`, get genuine per-frame _process
# scheduling from the engine's own main loop -- the same code path the
# playable demo scene uses, not a stand-in.
#
# Requires flowstate-server already running and listening on SERVER_PORT
# (no in-script server spawn, same convention as G0.1).
#
# Run: godot --headless --path client -s tests/g0_3_baseline_spawn.gd
# Exit code 0 = pass, 1 = fail.

extends SceneTree

const SERVER_ADDR := "127.0.0.1"
const SERVER_PORT := 6060
const DEADLINE_MS := 10000

## Exact spawn position, verified against flowstate_sim::Character::new
## (crates/sim/src/lib.rs), which spawns every character at position [0.0,
## 0.0]. FIXED_HEIGHT matches CharacterView.FIXED_HEIGHT.
const EXPECTED_SPAWN_POSITION := Vector3(0.0, 1.0, 0.0)

var client_a: NetworkClient
var client_b: NetworkClient
var _baseline_a: JoinBaseline
var _baseline_b: JoinBaseline
var _deadline_msec: int
var _done := false


func _initialize() -> void:
	client_a = NetworkClient.new()
	client_b = NetworkClient.new()
	root.add_child(client_a)
	root.add_child(client_b)
	client_a.baseline_received.connect(_on_baseline_a)
	client_b.baseline_received.connect(_on_baseline_b)
	client_a.connect_to_server(SERVER_ADDR, SERVER_PORT)
	client_b.connect_to_server(SERVER_ADDR, SERVER_PORT)
	_deadline_msec = Time.get_ticks_msec() + DEADLINE_MS


func _on_baseline_a(b: JoinBaseline) -> void:
	_baseline_a = b


func _on_baseline_b(b: JoinBaseline) -> void:
	_baseline_b = b


func _process(_delta: float) -> bool:
	if _done:
		return true

	if _baseline_a != null and _baseline_b != null:
		_done = true
		_finish()
		return true

	if Time.get_ticks_msec() >= _deadline_msec:
		_done = true
		push_error("G0.3 FAIL: timed out waiting for both baselines (a=%s b=%s)" % [_baseline_a != null, _baseline_b != null])
		quit(1)
		return true

	return false


func _finish() -> void:
	if _baseline_a.entities.size() != 2:
		push_error("G0.3 FAIL: client_a baseline has %d entities, expected 2" % _baseline_a.entities.size())
		quit(1)
		return
	if _baseline_b.entities.size() != 2:
		push_error("G0.3 FAIL: client_b baseline has %d entities, expected 2" % _baseline_b.entities.size())
		quit(1)
		return

	if _baseline_a.tick != _baseline_b.tick or _baseline_a.digest != _baseline_b.digest:
		push_error("G0.3 FAIL: baselines diverged: a(tick=%d digest=%d) b(tick=%d digest=%d)" % [_baseline_a.tick, _baseline_a.digest, _baseline_b.tick, _baseline_b.digest])
		quit(1)
		return

	for i in range(2):
		var ea: EntitySnapshotProto = _baseline_a.entities[i]
		var eb: EntitySnapshotProto = _baseline_b.entities[i]
		if ea.entity_id != eb.entity_id or ea.position != eb.position or ea.velocity != eb.velocity:
			push_error("G0.3 FAIL: entity %d diverged between client_a and client_b" % i)
			quit(1)
			return

	var scene: PackedScene = load("res://view/character_view.tscn")
	for e in _baseline_a.entities:
		var entity: EntitySnapshotProto = e
		var view: CharacterView = scene.instantiate()
		root.add_child(view)
		view.entity_id = entity.entity_id
		view.apply_state(entity.position, entity.velocity)
		if view.global_position != EXPECTED_SPAWN_POSITION:
			push_error("G0.3 FAIL: entity %d spawned at %s, expected %s" % [entity.entity_id, view.global_position, EXPECTED_SPAWN_POSITION])
			quit(1)
			return

	print("G0.3 PASS: both clients received identical 2-entity baseline; CharacterViews spawned at (0,0)")
	quit(0)
