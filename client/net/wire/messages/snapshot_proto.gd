## Wire message: server snapshot broadcast. Mirrors
## `flowstate_wire::SnapshotProto`.
class_name SnapshotProto
extends RefCounted

## Post-step tick.
var tick: int = 0
## Ordered by entity_id ascending per INV-0007.
var entities: Array[EntitySnapshotProto] = []
var digest: int = 0
## TargetTickFloor for client input targeting.
var target_tick_floor: int = 0
