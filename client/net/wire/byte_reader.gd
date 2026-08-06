## Low-level protobuf byte decoding: varints, tags, fixed64 doubles.
##
## Ref: FS-0008 G0.2. Counterpart to `ByteWriter`; `WireCodec` is the only
## caller. Permissive of unknown fields via `skip_field`, per proto3
## forward-compat, even though v0 messages are fixed-shape.
class_name ByteReader
extends RefCounted

var _bytes: PackedByteArray
var _pos: int = 0


func _init(bytes: PackedByteArray) -> void:
	_bytes = bytes


## LEB128 unsigned varint decode. Left-shift/OR accumulation is safe even
## when the accumulated value becomes numerically negative in GDScript's
## signed 64-bit `int` -- only the write-side right-shift needs the unsigned
## emulation (see `ByteWriter._unsigned_shr`).
func read_varint() -> int:
	var result := 0
	var shift := 0
	while true:
		var b := _bytes[_pos]
		_pos += 1
		result = result | ((b & 0x7F) << shift)
		if (b & 0x80) == 0:
			break
		shift += 7
	return result


## Returns `[field_number, wire_type]`.
func read_tag() -> Array:
	var tag := read_varint()
	return [tag >> 3, tag & 0x07]


func read_double() -> float:
	var v := _bytes.decode_double(_pos)
	_pos += 8
	return v


func read_bytes(length: int) -> PackedByteArray:
	var result := _bytes.slice(_pos, _pos + length)
	_pos += length
	return result


## Discard a field whose tag was already consumed, per proto3 forward-compat
## unknown-field handling.
func skip_field(wire_type: int) -> void:
	match wire_type:
		WireFormat.WIRE_TYPE_VARINT:
			read_varint()
		WireFormat.WIRE_TYPE_FIXED64:
			_pos += 8
		WireFormat.WIRE_TYPE_LENGTH_DELIMITED:
			var length := read_varint()
			_pos += length
		WireFormat.WIRE_TYPE_FIXED32:
			_pos += 4
		_:
			push_error("ByteReader: unknown wire_type %d, cannot skip" % wire_type)


func has_more() -> bool:
	return _pos < _bytes.size()
