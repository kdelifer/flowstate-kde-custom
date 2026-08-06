## Hand-written protobuf codec matching `crates/wire`'s prost-derived wire
## format byte-for-byte.
##
## Ref: FS-0008 G0.2. This is the ONLY file that knows proto3 field numbers,
## packed-vs-not rules, and default-value omission for flowstate's wire
## messages -- callers (NetworkClient etc.) only ever call `encode_*`/
## `decode_*` here and never touch a PackedByteArray directly. When the
## `.proto` migration (deferred for now, see FS-0008's Interfaces section)
## eventually lands, this file's bodies get replaced by generated-codec
## calls; its function signatures are the stable contract the rest of the
## client depends on.
##
## Field/tag layout is taken directly from `crates/wire/src/lib.rs` -- do
## not re-derive tag numbers, read them from that file if this ever needs
## updating.
class_name WireCodec
extends RefCounted


# ============================================================================
# ClientHello -- zero fields, zero-byte encoding.
# ============================================================================

static func encode_client_hello(_msg: ClientHello) -> PackedByteArray:
	return PackedByteArray()


static func decode_client_hello(_bytes: PackedByteArray) -> ClientHello:
	return ClientHello.new()


# ============================================================================
# ServerWelcome
# ============================================================================

static func encode_server_welcome(msg: ServerWelcome) -> PackedByteArray:
	var w := ByteWriter.new()
	if msg.target_tick_floor != 0:
		w.write_tag(1, WireFormat.WIRE_TYPE_VARINT)
		w.write_varint(msg.target_tick_floor)
	if msg.tick_rate_hz != 0:
		w.write_tag(2, WireFormat.WIRE_TYPE_VARINT)
		w.write_varint(msg.tick_rate_hz)
	if msg.player_id != 0:
		w.write_tag(3, WireFormat.WIRE_TYPE_VARINT)
		w.write_varint(msg.player_id)
	if msg.controlled_entity_id != 0:
		w.write_tag(4, WireFormat.WIRE_TYPE_VARINT)
		w.write_varint(msg.controlled_entity_id)
	return w.to_bytes()


static func decode_server_welcome(bytes: PackedByteArray) -> ServerWelcome:
	var msg := ServerWelcome.new()
	var r := ByteReader.new(bytes)
	while r.has_more():
		var tag := r.read_tag()
		var field_number: int = tag[0]
		var wire_type: int = tag[1]
		match field_number:
			1:
				msg.target_tick_floor = r.read_varint()
			2:
				msg.tick_rate_hz = r.read_varint()
			3:
				msg.player_id = r.read_varint()
			4:
				msg.controlled_entity_id = r.read_varint()
			_:
				r.skip_field(wire_type)
	return msg


# ============================================================================
# EntitySnapshotProto
# ============================================================================

static func encode_entity_snapshot_proto(msg: EntitySnapshotProto) -> PackedByteArray:
	var w := ByteWriter.new()
	if msg.entity_id != 0:
		w.write_tag(1, WireFormat.WIRE_TYPE_VARINT)
		w.write_varint(msg.entity_id)
	_write_packed_doubles(w, 2, msg.position)
	_write_packed_doubles(w, 3, msg.velocity)
	return w.to_bytes()


static func decode_entity_snapshot_proto(bytes: PackedByteArray) -> EntitySnapshotProto:
	var msg := EntitySnapshotProto.new()
	var r := ByteReader.new(bytes)
	while r.has_more():
		var tag := r.read_tag()
		var field_number: int = tag[0]
		var wire_type: int = tag[1]
		match field_number:
			1:
				msg.entity_id = r.read_varint()
			2:
				msg.position = _read_packed_doubles(r)
			3:
				msg.velocity = _read_packed_doubles(r)
			_:
				r.skip_field(wire_type)
	return msg


# ============================================================================
# JoinBaseline
# ============================================================================

static func encode_join_baseline(msg: JoinBaseline) -> PackedByteArray:
	var w := ByteWriter.new()
	if msg.tick != 0:
		w.write_tag(1, WireFormat.WIRE_TYPE_VARINT)
		w.write_varint(msg.tick)
	for entity in msg.entities:
		var entity_bytes := encode_entity_snapshot_proto(entity)
		w.write_tag(2, WireFormat.WIRE_TYPE_LENGTH_DELIMITED)
		w.write_varint(entity_bytes.size())
		w.write_bytes(entity_bytes)
	if msg.digest != 0:
		w.write_tag(3, WireFormat.WIRE_TYPE_VARINT)
		w.write_varint(msg.digest)
	return w.to_bytes()


static func decode_join_baseline(bytes: PackedByteArray) -> JoinBaseline:
	var msg := JoinBaseline.new()
	var r := ByteReader.new(bytes)
	while r.has_more():
		var tag := r.read_tag()
		var field_number: int = tag[0]
		var wire_type: int = tag[1]
		match field_number:
			1:
				msg.tick = r.read_varint()
			2:
				var length := r.read_varint()
				msg.entities.append(decode_entity_snapshot_proto(r.read_bytes(length)))
			3:
				msg.digest = r.read_varint()
			_:
				r.skip_field(wire_type)
	return msg


# ============================================================================
# InputCmdProto
# ============================================================================

static func encode_input_cmd_proto(msg: InputCmdProto) -> PackedByteArray:
	var w := ByteWriter.new()
	if msg.tick != 0:
		w.write_tag(1, WireFormat.WIRE_TYPE_VARINT)
		w.write_varint(msg.tick)
	if msg.input_seq != 0:
		w.write_tag(2, WireFormat.WIRE_TYPE_VARINT)
		w.write_varint(msg.input_seq)
	_write_packed_doubles(w, 3, msg.move_dir)
	return w.to_bytes()


static func decode_input_cmd_proto(bytes: PackedByteArray) -> InputCmdProto:
	var msg := InputCmdProto.new()
	var r := ByteReader.new(bytes)
	while r.has_more():
		var tag := r.read_tag()
		var field_number: int = tag[0]
		var wire_type: int = tag[1]
		match field_number:
			1:
				msg.tick = r.read_varint()
			2:
				msg.input_seq = r.read_varint()
			3:
				msg.move_dir = _read_packed_doubles(r)
			_:
				r.skip_field(wire_type)
	return msg


# ============================================================================
# SnapshotProto
# ============================================================================

static func encode_snapshot_proto(msg: SnapshotProto) -> PackedByteArray:
	var w := ByteWriter.new()
	if msg.tick != 0:
		w.write_tag(1, WireFormat.WIRE_TYPE_VARINT)
		w.write_varint(msg.tick)
	for entity in msg.entities:
		var entity_bytes := encode_entity_snapshot_proto(entity)
		w.write_tag(2, WireFormat.WIRE_TYPE_LENGTH_DELIMITED)
		w.write_varint(entity_bytes.size())
		w.write_bytes(entity_bytes)
	if msg.digest != 0:
		w.write_tag(3, WireFormat.WIRE_TYPE_VARINT)
		w.write_varint(msg.digest)
	if msg.target_tick_floor != 0:
		w.write_tag(4, WireFormat.WIRE_TYPE_VARINT)
		w.write_varint(msg.target_tick_floor)
	return w.to_bytes()


static func decode_snapshot_proto(bytes: PackedByteArray) -> SnapshotProto:
	var msg := SnapshotProto.new()
	var r := ByteReader.new(bytes)
	while r.has_more():
		var tag := r.read_tag()
		var field_number: int = tag[0]
		var wire_type: int = tag[1]
		match field_number:
			1:
				msg.tick = r.read_varint()
			2:
				var length := r.read_varint()
				msg.entities.append(decode_entity_snapshot_proto(r.read_bytes(length)))
			3:
				msg.digest = r.read_varint()
			4:
				msg.target_tick_floor = r.read_varint()
			_:
				r.skip_field(wire_type)
	return msg


# ============================================================================
# Shared helpers: packed repeated double (move_dir/position/velocity)
# ============================================================================

## proto3/prost default-value omission for `repeated` fields is keyed on
## whether the collection is EMPTY (len == 0) -- never on whether individual
## elements equal their scalar default. `[0.0, 0.0]` is length 2 and MUST
## still be encoded; only `[]` is omitted.
static func _write_packed_doubles(w: ByteWriter, field_number: int, values: PackedFloat64Array) -> void:
	if values.size() == 0:
		return
	w.write_tag(field_number, WireFormat.WIRE_TYPE_LENGTH_DELIMITED)
	w.write_varint(values.size() * 8)
	for v in values:
		w.write_double(v)


static func _read_packed_doubles(r: ByteReader) -> PackedFloat64Array:
	var length := r.read_varint()
	var result := PackedFloat64Array()
	var count := length / 8
	for _i in range(count):
		result.append(r.read_double())
	return result
