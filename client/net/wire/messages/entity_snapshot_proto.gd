## Wire message: entity snapshot embedded in JoinBaseline/SnapshotProto.
## Mirrors `flowstate_wire::EntitySnapshotProto`.
class_name EntitySnapshotProto
extends RefCounted

var entity_id: int = 0
## [x, y]
var position: PackedFloat64Array = PackedFloat64Array()
## [vx, vy]
var velocity: PackedFloat64Array = PackedFloat64Array()
