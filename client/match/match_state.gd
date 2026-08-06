## Locally tracked match state: latest Baseline/Snapshot entity positions and
## the controlled entity.
##
## Ref: FS-0008 G0.3/G0.5. The client performs no simulation of its own --
## renders exactly what JoinBaseline/SnapshotProto report (FS-0008
## Determinism Notes).
class_name MatchState
extends RefCounted

var controlled_entity_id: int = -1
var latest_tick: int = -1
## entity_id (int) -> EntitySnapshotProto
var entities: Dictionary = {}


func apply_baseline(baseline: JoinBaseline) -> void:
	latest_tick = baseline.tick
	entities.clear()
	for e in baseline.entities:
		entities[e.entity_id] = e


## No-op on a stale/duplicate tick -- defense in depth; NetworkClient
## already keeps only the highest-tick snapshot per drain (mirrors the
## layered guard in crates/client/src/state.rs). Returns whether the
## snapshot was applied.
func apply_snapshot(snapshot: SnapshotProto) -> bool:
	if snapshot.tick <= latest_tick:
		return false
	latest_tick = snapshot.tick
	entities.clear()
	for e in snapshot.entities:
		entities[e.entity_id] = e
	return true
