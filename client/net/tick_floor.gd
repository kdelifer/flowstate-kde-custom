## Client-side tracking of the server-guided TargetTickFloor (DM-0025).
##
## Ref: FS-0008, mirrors crates/client/src/tick_floor.rs. Per ADR-0006, the
## floor is monotonic non-decreasing per session: observe() always takes
## max(previous, received), so a stale or reordered message can never move
## targeting backwards.
class_name TickFloor
extends RefCounted

var _floor: int


## Initialize from ServerWelcome.target_tick_floor.
func _init(initial: int) -> void:
	_floor = initial


func get_floor() -> int:
	return _floor


## Fold in a newly observed floor (from SnapshotProto.target_tick_floor).
func observe(received: int) -> void:
	_floor = max(_floor, received)
