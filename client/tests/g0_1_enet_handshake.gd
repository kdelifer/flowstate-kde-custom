# FS-0008 Gate G0.1: raw ENet wire-compatibility spike.
#
# Proves Godot's low-level ENet stack (ENetConnection/ENetPacketPeer --
# NOT the high-level ENetMultiplayerPeer, which layers Godot's own
# SceneMultiplayer RPC protocol on top and would not match
# flowstate-server's custom protocol) can complete a raw ENet connect
# handshake against flowstate-server's rusty_enet host, and that an
# application-level packet round-trips over the real two-channel layout
# (0=Realtime, 1=Control) per ADR-0005.
#
# Connects two independent ENet clients (the server requires two sessions
# before it will send anything back), sends an empty-payload packet on the
# Control channel matching ClientHello's zero-byte protobuf encoding
# (ClientHello has no fields), and waits for a reply -- ServerWelcome is
# sent back once both sessions are accepted.
#
# Run: godot --headless --path client -s tests/g0_1_enet_handshake.gd
# Exit code 0 = pass, 1 = fail. Requires flowstate-server already running
# and listening on SERVER_PORT.

extends SceneTree

const SERVER_ADDR := "127.0.0.1"
const SERVER_PORT := 6060
const CHANNEL_COUNT := 2
const CHANNEL_CONTROL := 1
const DEADLINE_MS := 10000


class ClientPeer:
	var name: String
	var connection: ENetConnection
	var peer: ENetPacketPeer
	var connected := false
	var hello_sent := false
	var got_reply := false
	var failed := false
	var fail_reason := ""

	func _init(peer_name: String) -> void:
		name = peer_name
		connection = ENetConnection.new()

	func start(address: String, port: int, channels: int) -> void:
		var err := connection.create_host()
		if err != OK:
			failed = true
			fail_reason = "create_host failed: %s" % err
			return
		peer = connection.connect_to_host(address, port, channels)
		if peer == null:
			failed = true
			fail_reason = "connect_to_host returned null"

	func poll() -> void:
		if failed or got_reply:
			return
		# ENetConnection.service() returns [event_type, peer, <reserved>,
		# channel_id] -- verified empirically (index 2 is consistently 0 in
		# our traffic; index 3 tracks the real channel, confirmed against
		# known CHANNEL_CONTROL=1 vs CHANNEL_REALTIME=0 traffic from the
		# Rust server). This differs from a naive reading of the index
		# order and is easy to get backwards.
		var result: Array = connection.service(10)
		var event_type: int = result[0]
		var event_peer = result[1]
		var event_channel: int = result[3]

		match event_type:
			ENetConnection.EVENT_CONNECT:
				connected = true
				print("[%s] EVENT_CONNECT" % name)
			ENetConnection.EVENT_RECEIVE:
				var packet: PackedByteArray = event_peer.get_packet()
				print("[%s] EVENT_RECEIVE channel=%d bytes=%d" % [name, event_channel, packet.size()])
				if event_channel != CHANNEL_CONTROL:
					failed = true
					fail_reason = "reply arrived on channel %d, expected Control channel %d" % [event_channel, CHANNEL_CONTROL]
					return
				if packet.size() == 0:
					failed = true
					fail_reason = "reply on Control channel was empty (expected a non-empty ServerWelcome)"
					return
				got_reply = true
			ENetConnection.EVENT_DISCONNECT:
				failed = true
				fail_reason = "EVENT_DISCONNECT before reply"
			ENetConnection.EVENT_ERROR:
				failed = true
				fail_reason = "EVENT_ERROR from service()"

		if connected and not hello_sent and not failed:
			# ClientHello {} has no fields -> zero-byte protobuf encoding.
			var empty := PackedByteArray()
			var send_err := peer.send(CHANNEL_CONTROL, empty, ENetPacketPeer.FLAG_RELIABLE)
			if send_err != OK:
				failed = true
				fail_reason = "send failed: %s" % send_err
			else:
				hello_sent = true
				print("[%s] sent empty ClientHello on channel %d" % [name, CHANNEL_CONTROL])


func _initialize() -> void:
	var a := ClientPeer.new("client_a")
	var b := ClientPeer.new("client_b")
	a.start(SERVER_ADDR, SERVER_PORT, CHANNEL_COUNT)
	b.start(SERVER_ADDR, SERVER_PORT, CHANNEL_COUNT)

	if a.failed or b.failed:
		push_error("G0.1 FAIL: setup error a=%s b=%s" % [a.fail_reason, b.fail_reason])
		quit(1)
		return

	var deadline := Time.get_ticks_msec() + DEADLINE_MS
	while Time.get_ticks_msec() < deadline:
		a.poll()
		b.poll()

		if a.failed or b.failed:
			push_error("G0.1 FAIL: a_failed=%s (%s) b_failed=%s (%s)" % [a.failed, a.fail_reason, b.failed, b.fail_reason])
			quit(1)
			return

		if a.got_reply and b.got_reply:
			print("G0.1 PASS: both peers completed raw ENet handshake and received a reply")
			quit(0)
			return

	push_error("G0.1 FAIL: timed out. a_connected=%s a_reply=%s b_connected=%s b_reply=%s" % [a.connected, a.got_reply, b.connected, b.got_reply])
	quit(1)
