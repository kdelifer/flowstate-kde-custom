## Wire message: initial baseline state sent to client after welcome.
## Mirrors `flowstate_wire::JoinBaseline`.
class_name JoinBaseline
extends RefCounted

var tick: int = 0
## Ordered by entity_id ascending per INV-0007.
var entities: Array[EntitySnapshotProto] = []
var digest: int = 0
