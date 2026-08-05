//! `InputSeq` generation and `InputCmdProto` construction/send.
//!
//! Ref: CLI-005 (strictly monotonic `InputSeq` counter), CLI-006 (build and
//! send `InputCmdProto { tick, input_seq, move_dir }` on the Realtime
//! channel; `player_id` is intentionally omitted, per INV-0003 the server
//! binds identity from the session).
