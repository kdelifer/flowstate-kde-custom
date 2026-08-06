## Renders one Character's position, driven exclusively by network-received
## state via apply_state().
##
## Ref: FS-0008 G0.6. MUST NOT contain any _process/_physics_process that
## advances global_position locally -- that absence is exactly what G0.6
## verifies (no client-side prediction). Position changes only through
## apply_state() calls driven by decoded SnapshotProto/JoinBaseline data.
class_name CharacterView
extends Node3D

## Sim state is 2D; rendered in a 3D scene at a fixed height as a
## presentation choice only (FS-0008 Determinism Notes) -- does not imply
## the Simulation Core has gained verticality.
const FIXED_HEIGHT := 1.0

var entity_id: int = -1
var is_controlled: bool = false


## position/velocity are [x, y] per EntitySnapshotProto. velocity is
## accepted but unused for now -- rendering uses only the authoritative
## position, matching "no prediction/interpolation" (Tier 1 follow-up).
func apply_state(position: PackedFloat64Array, _velocity: PackedFloat64Array) -> void:
	global_position = Vector3(position[0], FIXED_HEIGHT, position[1])


func set_controlled(value: bool) -> void:
	is_controlled = value
	var mesh_instance: MeshInstance3D = $MeshInstance3D
	var material := StandardMaterial3D.new()
	material.albedo_color = Color(0.2, 0.6, 1.0) if is_controlled else Color(0.8, 0.2, 0.2)
	mesh_instance.material_override = material
