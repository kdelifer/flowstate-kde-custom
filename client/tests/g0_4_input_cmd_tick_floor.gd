# FS-0008 Gate G0.4: WASD-equivalent input produces InputCmdProto on the
# Realtime channel with tick >= TargetTickFloor and strictly monotonic
# InputSeq.
#
# Requires flowstate-server already running on 127.0.0.1:6060. Connects a
# second, otherwise-idle client too -- the server requires exactly two
# sessions before it starts the match and sends JoinBaseline. Bypasses
# InputCapture/real keyboard entirely for the client under test -- calls
# NetworkClient.send_input() directly with scripted desired_tick values
# (some below the floor to exercise clamping, some above), observed via the
# input_sent signal. Mirrors crates/client/src/sim_input.rs's
# drive_move_dir / CLI-008.
#
# Run: godot --headless --path client -s tests/g0_4_input_cmd_tick_floor.gd
# Exit code 0 = pass, 1 = fail.

extends SceneTree

const SERVER_ADDR := "127.0.0.1"
const SERVER_PORT := 6060
const DEADLINE_MS := 10000
const EXPECTED_SEND_COUNT := 4

var client: NetworkClient
## Idle second session -- required only because the server needs two
## sessions to start the match; this test never sends input from it.
var client_idle: NetworkClient
var _ready_seen := false
var _sends_issued := false
var _sent: Array[InputCmdProto] = []
var _floor_at_send: Array[int] = []
var _deadline_msec: int
var _done := false


func _initialize() -> void:
	client = NetworkClient.new()
	client_idle = NetworkClient.new()
	root.add_child(client)
	root.add_child(client_idle)
	client.baseline_received.connect(_on_baseline_received)
	client.input_sent.connect(_on_input_sent)
	client.connect_to_server(SERVER_ADDR, SERVER_PORT)
	client_idle.connect_to_server(SERVER_ADDR, SERVER_PORT)
	_deadline_msec = Time.get_ticks_msec() + DEADLINE_MS


func _on_baseline_received(_baseline: JoinBaseline) -> void:
	_ready_seen = true


func _on_input_sent(cmd: InputCmdProto) -> void:
	_sent.append(cmd)
	_floor_at_send.append(client.tick_floor())


func _process(_delta: float) -> bool:
	if _done:
		return true

	if Time.get_ticks_msec() >= _deadline_msec:
		_done = true
		push_error("G0.4 FAIL: timed out (ready_seen=%s sends=%d)" % [_ready_seen, _sent.size()])
		quit(1)
		return true

	if _ready_seen and not _sends_issued:
		_sends_issued = true
		# Below-floor (0, 1) exercise the clamp-up; well-above-floor values
		# exercise the pass-through case.
		var current_floor := client.tick_floor()
		var desired_ticks: Array[int] = [0, 1, current_floor + 50, current_floor + 100]
		for t in desired_ticks:
			client.send_input(PackedFloat64Array([1.0, 0.0]), t)

	if _sends_issued and _sent.size() >= EXPECTED_SEND_COUNT:
		_done = true
		_finish()
		return true

	return false


func _finish() -> void:
	for i in range(_sent.size()):
		var cmd: InputCmdProto = _sent[i]
		var floor_then: int = _floor_at_send[i]
		if cmd.tick < floor_then:
			push_error("G0.4 FAIL: sent[%d].tick=%d < floor_at_send=%d" % [i, cmd.tick, floor_then])
			quit(1)
			return
		var expected_seq := i + 1
		if cmd.input_seq != expected_seq:
			push_error("G0.4 FAIL: sent[%d].input_seq=%d, expected %d (strictly monotonic from 1)" % [i, cmd.input_seq, expected_seq])
			quit(1)
			return

	print("G0.4 PASS: %d InputCmdProto sends all had tick >= floor and strictly monotonic input_seq from 1" % _sent.size())
	quit(0)
