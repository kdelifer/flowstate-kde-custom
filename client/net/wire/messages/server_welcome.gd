## Wire message: server welcome response with session info and tick
## guidance. Mirrors `flowstate_wire::ServerWelcome`.
class_name ServerWelcome
extends RefCounted

## Initial TargetTickFloor for client input targeting.
var target_tick_floor: int = 0
## Server tick rate in Hz.
var tick_rate_hz: int = 0
## Assigned PlayerId for this session.
var player_id: int = 0
## EntityId of the Character this client controls.
var controlled_entity_id: int = 0
