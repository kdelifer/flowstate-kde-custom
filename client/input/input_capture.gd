## Reads WASD each frame and emits a movement intent.
##
## Ref: FS-0008. Kept separate from NetworkClient so gate tests can script
## input directly via NetworkClient.send_input() without touching real
## keyboard state -- mirrors crates/client/src/sim_input.rs's
## drive_move_dir bypassing real input in the Rust test harness.
class_name InputCapture
extends Node

signal move_dir_changed(move_dir: Vector2)


func _process(_delta: float) -> void:
	var move_dir := Input.get_vector("move_left", "move_right", "move_forward", "move_backward")
	move_dir_changed.emit(move_dir)
