# FS-0008 Gate G0.2: hand-written protobuf codec round-trips byte-identical
# to flowstate-wire's own prost-derived encoding.
#
# Loads tests/fixtures/g0_2_golden_bytes.json (generated from
# crates/client/examples/g0_2_golden_bytes.rs -- see that file's doc comment
# to regenerate) and, for every case: builds a message from the fixture's
# input field values, encodes it via WireCodec, and asserts the result is
# byte-identical to the Rust-encoded hex. Then decodes the same fixture
# bytes via WireCodec and asserts every field matches the input values
# exactly (doubles compared with no epsilon -- IEEE754 fixed64 round-trips
# losslessly, so exact equality is correct here).
#
# No live server required -- pure serialization test.
#
# Run: godot --headless --path client -s tests/g0_2_wire_codec_roundtrip.gd
# Exit code 0 = pass, 1 = fail.

extends SceneTree

var _failures: Array[String] = []


## Manual hex parse rather than String.hex_to_int(): that builtin range-checks
## against signed i64 and throws for values with bit 63 set (e.g. digest
## fixtures), instead of reinterpreting them as the correct two's-complement
## bit pattern. Left-shift/OR accumulation, like ByteReader.read_varint(),
## produces the right bit pattern even once the accumulated value becomes
## numerically negative.
func _hex(s: String) -> int:
	var digits := s.trim_prefix("0x")
	var result := 0
	for i in range(digits.length()):
		var nibble := "0123456789abcdef".find(digits[i].to_lower())
		result = (result << 4) | nibble
	return result


func _to_f64_array(arr: Array) -> PackedFloat64Array:
	var result := PackedFloat64Array()
	for v in arr:
		result.append(float(v))
	return result


func _fail(msg: String) -> void:
	_failures.append(msg)


func _check_eq(case_name: String, field: String, actual, expected) -> void:
	if actual != expected:
		_fail("%s.%s: got %s expected %s" % [case_name, field, str(actual), str(expected)])


func _build_entity(fields: Dictionary) -> EntitySnapshotProto:
	var msg := EntitySnapshotProto.new()
	msg.entity_id = _hex(fields["entity_id"])
	msg.position = _to_f64_array(fields["position"])
	msg.velocity = _to_f64_array(fields["velocity"])
	return msg


func _check_entity(case_name: String, prefix: String, actual: EntitySnapshotProto, fields: Dictionary) -> void:
	_check_eq(case_name, prefix + ".entity_id", actual.entity_id, _hex(fields["entity_id"]))
	_check_eq(case_name, prefix + ".position", actual.position, _to_f64_array(fields["position"]))
	_check_eq(case_name, prefix + ".velocity", actual.velocity, _to_f64_array(fields["velocity"]))


func _build_server_welcome(fields: Dictionary) -> ServerWelcome:
	var msg := ServerWelcome.new()
	msg.target_tick_floor = _hex(fields["target_tick_floor"])
	msg.tick_rate_hz = _hex(fields["tick_rate_hz"])
	msg.player_id = _hex(fields["player_id"])
	msg.controlled_entity_id = _hex(fields["controlled_entity_id"])
	return msg


func _check_server_welcome(case_name: String, actual: ServerWelcome, fields: Dictionary) -> void:
	_check_eq(case_name, "target_tick_floor", actual.target_tick_floor, _hex(fields["target_tick_floor"]))
	_check_eq(case_name, "tick_rate_hz", actual.tick_rate_hz, _hex(fields["tick_rate_hz"]))
	_check_eq(case_name, "player_id", actual.player_id, _hex(fields["player_id"]))
	_check_eq(case_name, "controlled_entity_id", actual.controlled_entity_id, _hex(fields["controlled_entity_id"]))


func _build_join_baseline(fields: Dictionary) -> JoinBaseline:
	var msg := JoinBaseline.new()
	msg.tick = _hex(fields["tick"])
	for e in fields["entities"]:
		msg.entities.append(_build_entity(e))
	msg.digest = _hex(fields["digest"])
	return msg


func _check_join_baseline(case_name: String, actual: JoinBaseline, fields: Dictionary) -> void:
	_check_eq(case_name, "tick", actual.tick, _hex(fields["tick"]))
	_check_eq(case_name, "digest", actual.digest, _hex(fields["digest"]))
	var expected_entities: Array = fields["entities"]
	if actual.entities.size() != expected_entities.size():
		_fail("%s.entities: got %d entities expected %d" % [case_name, actual.entities.size(), expected_entities.size()])
		return
	for i in range(expected_entities.size()):
		_check_entity(case_name, "entities[%d]" % i, actual.entities[i], expected_entities[i])


func _build_input_cmd(fields: Dictionary) -> InputCmdProto:
	var msg := InputCmdProto.new()
	msg.tick = _hex(fields["tick"])
	msg.input_seq = _hex(fields["input_seq"])
	msg.move_dir = _to_f64_array(fields["move_dir"])
	return msg


func _check_input_cmd(case_name: String, actual: InputCmdProto, fields: Dictionary) -> void:
	_check_eq(case_name, "tick", actual.tick, _hex(fields["tick"]))
	_check_eq(case_name, "input_seq", actual.input_seq, _hex(fields["input_seq"]))
	_check_eq(case_name, "move_dir", actual.move_dir, _to_f64_array(fields["move_dir"]))


func _build_snapshot(fields: Dictionary) -> SnapshotProto:
	var msg := SnapshotProto.new()
	msg.tick = _hex(fields["tick"])
	for e in fields["entities"]:
		msg.entities.append(_build_entity(e))
	msg.digest = _hex(fields["digest"])
	msg.target_tick_floor = _hex(fields["target_tick_floor"])
	return msg


func _check_snapshot(case_name: String, actual: SnapshotProto, fields: Dictionary) -> void:
	_check_eq(case_name, "tick", actual.tick, _hex(fields["tick"]))
	_check_eq(case_name, "digest", actual.digest, _hex(fields["digest"]))
	_check_eq(case_name, "target_tick_floor", actual.target_tick_floor, _hex(fields["target_tick_floor"]))
	var expected_entities: Array = fields["entities"]
	if actual.entities.size() != expected_entities.size():
		_fail("%s.entities: got %d entities expected %d" % [case_name, actual.entities.size(), expected_entities.size()])
		return
	for i in range(expected_entities.size()):
		_check_entity(case_name, "entities[%d]" % i, actual.entities[i], expected_entities[i])


func _run_case(case_name: String, case_data: Dictionary) -> void:
	var fields: Dictionary = case_data["fields"]
	var expected_hex: String = case_data["hex"]
	var decoded_bytes := expected_hex.hex_decode()
	var encoded: PackedByteArray

	if case_name == "client_hello":
		encoded = WireCodec.encode_client_hello(ClientHello.new())
		WireCodec.decode_client_hello(decoded_bytes)
	elif case_name.begins_with("server_welcome"):
		encoded = WireCodec.encode_server_welcome(_build_server_welcome(fields))
		_check_server_welcome(case_name, WireCodec.decode_server_welcome(decoded_bytes), fields)
	elif case_name.begins_with("join_baseline"):
		encoded = WireCodec.encode_join_baseline(_build_join_baseline(fields))
		_check_join_baseline(case_name, WireCodec.decode_join_baseline(decoded_bytes), fields)
	elif case_name.begins_with("input_cmd"):
		encoded = WireCodec.encode_input_cmd_proto(_build_input_cmd(fields))
		_check_input_cmd(case_name, WireCodec.decode_input_cmd_proto(decoded_bytes), fields)
	elif case_name.begins_with("snapshot"):
		encoded = WireCodec.encode_snapshot_proto(_build_snapshot(fields))
		_check_snapshot(case_name, WireCodec.decode_snapshot_proto(decoded_bytes), fields)
	elif case_name.begins_with("entity_snapshot"):
		encoded = WireCodec.encode_entity_snapshot_proto(_build_entity(fields))
		_check_entity(case_name, "", WireCodec.decode_entity_snapshot_proto(decoded_bytes), fields)
	else:
		_fail("%s: unrecognized fixture case name (no builder mapped)" % case_name)
		return

	var actual_hex := encoded.hex_encode()
	if actual_hex != expected_hex:
		_fail("%s: encode mismatch\n  got:      %s\n  expected: %s" % [case_name, actual_hex, expected_hex])


func _initialize() -> void:
	var path := "res://tests/fixtures/g0_2_golden_bytes.json"
	var file := FileAccess.open(path, FileAccess.READ)
	if file == null:
		push_error("G0.2 FAIL: could not open fixture %s (err=%s)" % [path, FileAccess.get_open_error()])
		quit(1)
		return
	var text := file.get_as_text()
	file.close()

	var parsed = JSON.parse_string(text)
	if parsed == null or not (parsed is Dictionary):
		push_error("G0.2 FAIL: fixture is not valid JSON")
		quit(1)
		return

	var cases: Dictionary = parsed
	for case_name in cases.keys():
		_run_case(case_name, cases[case_name])

	if _failures.is_empty():
		print("G0.2 PASS: all %d cases round-tripped byte-identical" % cases.size())
		quit(0)
	else:
		for f in _failures:
			push_error("G0.2 FAIL: %s" % f)
		quit(1)
