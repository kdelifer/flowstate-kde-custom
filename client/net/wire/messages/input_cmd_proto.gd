## Wire message: client input command targeting a specific tick. Mirrors
## `flowstate_wire::InputCmdProto`.
##
## Note: `player_id` is NOT included -- the server binds identity from the
## session (INV-0003).
class_name InputCmdProto
extends RefCounted

## Target tick for this input. MUST be >= TargetTickFloor.
var tick: int = 0
## Per-session sequence number for deterministic selection.
var input_seq: int = 0
## [x, y], magnitude <= 1.0.
var move_dir: PackedFloat64Array = PackedFloat64Array()
