## Proto3 wire-type constants shared by ByteWriter/ByteReader/WireCodec.
##
## Ref: FS-0008 G0.2. Field-number/wire-type packing follows standard
## protobuf wire format: tag_byte = (field_number << 3) | wire_type.
class_name WireFormat
extends RefCounted

const WIRE_TYPE_VARINT := 0
const WIRE_TYPE_FIXED64 := 1
const WIRE_TYPE_LENGTH_DELIMITED := 2
const WIRE_TYPE_FIXED32 := 5
