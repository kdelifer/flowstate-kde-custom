## Per-session strictly monotonic increasing InputSeq generator (DM-0026).
##
## Ref: FS-0008, mirrors crates/client/src/input.rs's InputSeqGen. Starts at
## 0; the first call to advance() returns 1 and never resets or wraps
## within a session.
class_name InputSeqGen
extends RefCounted

var _seq: int = 0


## Produce the next strictly increasing InputSeq value.
func advance() -> int:
	_seq += 1
	return _seq
