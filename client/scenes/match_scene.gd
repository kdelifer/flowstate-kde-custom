## Playable demo composition root: connects to a local flowstate-server,
## spawns a CharacterView per entity from JoinBaseline/SnapshotProto, and
## wires WASD input to the controlled Character.
##
## Ref: FS-0008 Acceptance Criteria -- two instances of this scene, run by
## a human, connect and see each other move. No connect UI (Tier 1/out of
## scope) -- auto-connects to a hardcoded local address on _ready().
extends Node3D

const SERVER_ADDR := "127.0.0.1"
const SERVER_PORT := 6060
const CHARACTER_VIEW_SCENE := preload("res://view/character_view.tscn")

@onready var _network_client: NetworkClient = $NetworkClient
@onready var _input_capture: InputCapture = $InputCapture
@onready var _entities_root: Node3D = $Entities
@onready var _camera: Camera3D = $Camera3D
@onready var _light: DirectionalLight3D = $DirectionalLight3D

var _match_state := MatchState.new()
## entity_id (int) -> CharacterView
var _views: Dictionary = {}


func _ready() -> void:
	_camera.position = Vector3(0.0, 8.0, 8.0)
	_camera.look_at(Vector3(0.0, CharacterView.FIXED_HEIGHT, 0.0), Vector3.UP)
	_light.rotation_degrees = Vector3(-45.0, -30.0, 0.0)

	_network_client.baseline_received.connect(_on_baseline_received)
	_network_client.snapshot_received.connect(_on_snapshot_received)
	_input_capture.move_dir_changed.connect(_on_move_dir_changed)
	_network_client.connect_to_server(SERVER_ADDR, SERVER_PORT)


func _on_baseline_received(baseline: JoinBaseline) -> void:
	_match_state.controlled_entity_id = _network_client.welcome().controlled_entity_id
	_match_state.apply_baseline(baseline)
	for entity_id in _match_state.entities.keys():
		_spawn_or_update_view(entity_id, _match_state.entities[entity_id])


func _on_snapshot_received(snapshot: SnapshotProto) -> void:
	if not _match_state.apply_snapshot(snapshot):
		return
	for entity_id in _match_state.entities.keys():
		_spawn_or_update_view(entity_id, _match_state.entities[entity_id])


func _spawn_or_update_view(entity_id: int, entity: EntitySnapshotProto) -> void:
	var view: CharacterView = _views.get(entity_id)
	if view == null:
		view = CHARACTER_VIEW_SCENE.instantiate()
		view.entity_id = entity_id
		view.set_controlled(entity_id == _match_state.controlled_entity_id)
		_entities_root.add_child(view)
		_views[entity_id] = view
	view.apply_state(entity.position, entity.velocity)


## Client sends on its own cadence (every frame InputCapture reports a
## value), clamped server-side to TargetTickFloor -- it does not need to
## send exactly once per server tick (FS-0008 Interfaces).
func _on_move_dir_changed(move_dir: Vector2) -> void:
	if not _network_client.is_ready():
		return
	_network_client.send_input(PackedFloat64Array([move_dir.x, move_dir.y]), _network_client.tick_floor())
