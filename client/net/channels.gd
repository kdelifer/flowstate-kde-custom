## Logical channel constants shared with the Server Edge.
##
## Ref: ADR-0005, mirrors `crates/wire/src/channels.rs`. Kept separate from
## `WireCodec` since channel routing is a transport-layer concern, not a
## wire-encoding concern -- it should survive a future `.proto` migration
## unchanged even though `codec.gd`'s internals get replaced.
class_name Channels
extends RefCounted

## Unreliable + sequenced. Carries Snapshots and InputCmds.
const CHANNEL_REALTIME := 0
## Reliable + ordered. Carries ClientHello, ServerWelcome, JoinBaseline.
const CHANNEL_CONTROL := 1
