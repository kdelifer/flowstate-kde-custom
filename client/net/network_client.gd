## Owns the ENet connection to a Server Edge and drives the v0 protocol:
## ClientHello -> ServerWelcome -> JoinBaseline -> per-frame InputCmdProto
## send / SnapshotProto receive.
##
## Ref: FS-0008 G0.3/G0.4/G0.5. Mirrors crates/client/src/{connection,state,
## tick_floor,input}.rs's behavior. Uses the low-level ENetConnection/
## ENetPacketPeer API per G0.1's corrected finding -- ENetMultiplayerPeer
## layers Godot's own SceneMultiplayer RPC framing on top of raw ENet,
## which flowstate-server does not speak. Only ever calls WireCodec's
## named encode_*/decode_* functions; never touches a PackedByteArray's
## contents directly.
class_name NetworkClient
extends Node

signal welcome_received(welcome: ServerWelcome)
signal baseline_received(baseline: JoinBaseline)
signal snapshot_received(snapshot: SnapshotProto)
## Emitted after every send_input() call, for test observability (G0.4).
signal input_sent(cmd: InputCmdProto)
signal disconnected()

enum State { DISCONNECTED, CONNECTING, AWAITING_BASELINE, READY }

## Realtime + Control, matching the server's channel layout (ADR-0005).
const CHANNEL_COUNT := 2

var _connection: ENetConnection
var _peer: ENetPacketPeer
var _state: State = State.DISCONNECTED
var _hello_sent: bool = false
var _tick_floor: TickFloor
var _input_seq := InputSeqGen.new()
var _welcome: ServerWelcome


## Connect to a Server Edge at address:port and begin the handshake.
## ClientHello is sent once the ENet connection completes (observed in
## _process via EVENT_CONNECT), not synchronously here.
func connect_to_server(address: String, port: int) -> void:
	_connection = ENetConnection.new()
	var err := _connection.create_host()
	if err != OK:
		push_error("NetworkClient: create_host failed: %d" % err)
		return
	_peer = _connection.connect_to_host(address, port, CHANNEL_COUNT)
	if _peer == null:
		push_error("NetworkClient: connect_to_host returned null")
		return
	_state = State.CONNECTING
	_hello_sent = false


func is_ready() -> bool:
	return _state == State.READY


func welcome() -> ServerWelcome:
	return _welcome


func tick_floor() -> int:
	return _tick_floor.get_floor() if _tick_floor != null else 0


## Clean ENet disconnect (completes ENet's own disconnect handshake), as
## opposed to an abrupt kill. Deliberate choice for gate tests and the demo
## scene -- the server's tick_loop.rs now tolerates abrupt disconnects too
## (FS-0008 server robustness fix), but a clean disconnect remains the
## recommended path.
func disconnect_clean() -> void:
	if _peer != null:
		_peer.peer_disconnect()


## Build and send an InputCmdProto on the Realtime channel (unreliable +
## sequenced -- flags=0, matching ADR-0005 and the server's
## enet::Packet::unreliable), targeting desired_tick clamped up to the
## locally tracked TargetTickFloor. player_id is intentionally omitted --
## the server binds identity from the session (INV-0003).
func send_input(move_dir: PackedFloat64Array, desired_tick: int) -> void:
	if _state != State.READY:
		return
	var cmd := InputCmdProto.new()
	cmd.tick = max(_tick_floor.get_floor(), desired_tick)
	cmd.input_seq = _input_seq.advance()
	cmd.move_dir = move_dir
	var bytes := WireCodec.encode_input_cmd_proto(cmd)
	_peer.send(Channels.CHANNEL_REALTIME, bytes, 0)
	input_sent.emit(cmd)


func _process(_delta: float) -> void:
	if _connection == null or _state == State.DISCONNECTED:
		return

	# Within one frame's drain, keep only the highest-tick SnapshotProto and
	# emit once -- mirrors state.rs's poll_snapshot/is_newer under
	# ADR-0005's unreliable+sequenced semantics (a single drain may contain
	# multiple or reordered snapshots).
	var latest_snapshot: SnapshotProto = null
	var latest_snapshot_floor: int = 0

	while true:
		var result: Array = _connection.service(0)
		var event_type: int = result[0]
		if event_type == ENetConnection.EVENT_NONE:
			break

		if event_type == ENetConnection.EVENT_CONNECT:
			if not _hello_sent:
				var hello_bytes := WireCodec.encode_client_hello(ClientHello.new())
				_peer.send(Channels.CHANNEL_CONTROL, hello_bytes, ENetPacketPeer.FLAG_RELIABLE)
				_hello_sent = true
		elif event_type == ENetConnection.EVENT_RECEIVE:
			# service() returns [event_type, peer, <reserved, always 0>,
			# channel_id] -- channel is at index 3, not 2. See G0.1's spike
			# for the empirical verification of this quirk.
			var event_peer = result[1]
			var channel: int = result[3]
			var packet: PackedByteArray = event_peer.get_packet()

			if channel == Channels.CHANNEL_CONTROL:
				_handle_control_packet(packet)
			elif channel == Channels.CHANNEL_REALTIME and _state == State.READY:
				var snapshot := WireCodec.decode_snapshot_proto(packet)
				if latest_snapshot == null or snapshot.tick > latest_snapshot.tick:
					latest_snapshot = snapshot
					latest_snapshot_floor = snapshot.target_tick_floor
		elif event_type == ENetConnection.EVENT_DISCONNECT:
			_state = State.DISCONNECTED
			disconnected.emit()
		elif event_type == ENetConnection.EVENT_ERROR:
			push_warning("NetworkClient: EVENT_ERROR from connection.service()")

	if latest_snapshot != null:
		_tick_floor.observe(latest_snapshot_floor)
		snapshot_received.emit(latest_snapshot)


func _handle_control_packet(packet: PackedByteArray) -> void:
	if _state == State.CONNECTING:
		_welcome = WireCodec.decode_server_welcome(packet)
		_tick_floor = TickFloor.new(_welcome.target_tick_floor)
		_state = State.AWAITING_BASELINE
		welcome_received.emit(_welcome)
	elif _state == State.AWAITING_BASELINE:
		var baseline := WireCodec.decode_join_baseline(packet)
		_state = State.READY
		baseline_received.emit(baseline)
