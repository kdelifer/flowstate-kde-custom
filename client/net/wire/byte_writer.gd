## Low-level protobuf byte encoding: varints, tags, fixed64 doubles.
##
## Ref: FS-0008 G0.2. Knows nothing about message shapes -- only the
## proto3 wire-format primitives. `WireCodec` is the only caller.
class_name ByteWriter
extends RefCounted

var _bytes := PackedByteArray()


## GDScript's `int` is 64-bit signed with no unsigned right-shift; a plain
## `>>` sign-extends negative values. Varint encoding of a u64 whose bit 63
## is set (e.g. `digest`, an FNV1a64 hash with ~50% chance of that bit being
## set) would silently produce wrong output under a naive `value >>= 7`.
## This clears exactly the `bits` high bits an arithmetic shift would have
## wrongly filled with 1s, giving the correct logical-shift bit pattern.
static func _unsigned_shr(value: int, bits: int) -> int:
	return (value >> bits) & (0x7FFFFFFFFFFFFFFF >> (bits - 1))


## LEB128 unsigned varint, matching prost's encoding for uint32/uint64 tags
## and values.
func write_varint(value: int) -> void:
	while true:
		var byte := value & 0x7F
		value = ByteWriter._unsigned_shr(value, 7)
		if value != 0:
			_bytes.append(byte | 0x80)
		else:
			_bytes.append(byte)
			break


## tag_byte = (field_number << 3) | wire_type, per proto3 wire format.
func write_tag(field_number: int, wire_type: int) -> void:
	write_varint((field_number << 3) | wire_type)


## Wire type 1 (fixed64), little-endian IEEE754 -- matches prost's `double`
## encoding exactly.
func write_double(value: float) -> void:
	var tmp := PackedByteArray()
	tmp.resize(8)
	tmp.encode_double(0, value)
	_bytes.append_array(tmp)


func write_bytes(bytes: PackedByteArray) -> void:
	_bytes.append_array(bytes)


func to_bytes() -> PackedByteArray:
	return _bytes
