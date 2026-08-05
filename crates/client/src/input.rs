//! `InputSeq` generation and `InputCmdProto` construction/send.
//!
//! Ref: CLI-005 (strictly monotonic `InputSeq` counter), CLI-006 (build and
//! send `InputCmdProto { tick, input_seq, move_dir }` on the Realtime
//! channel; `player_id` is intentionally omitted, per INV-0003 the server
//! binds identity from the session).

use std::io;

use prost::Message;
use rusty_enet as enet;

use flowstate_wire::channels::CHANNEL_REALTIME;
use flowstate_wire::{InputCmdProto, InputSeq, Tick};

use crate::connection::ClientHost;

/// Per-session strictly monotonic increasing `InputSeq` generator
/// (DM-0026). Starts at 1 and never resets or wraps within a session
/// (T0.3). (CLI-005)
#[derive(Debug, Default)]
pub struct InputSeqGen(InputSeq);

impl InputSeqGen {
    /// Create a fresh generator; the first call to [`Self::advance`] returns 1.
    pub fn new() -> Self {
        Self(0)
    }

    /// Produce the next strictly increasing `InputSeq` value.
    pub fn advance(&mut self) -> InputSeq {
        self.0 += 1;
        self.0
    }
}

/// Clamp `desired_tick` up to `floor`, per ADR-0006:
/// `InputCmd.tick >= TargetTickFloor`.
pub fn target_tick(floor: Tick, desired_tick: Tick) -> Tick {
    desired_tick.max(floor)
}

/// Errors that can occur while sending an `InputCmdProto`.
#[derive(Debug)]
pub enum SendInputError {
    /// The underlying ENet/UDP transport failed to send.
    Io(io::Error),
    /// The host has no connected peer to send to.
    NotConnected,
}

impl From<io::Error> for SendInputError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// Build and send an `InputCmdProto` on the Realtime channel (unreliable +
/// sequenced), targeting `desired_tick` clamped up to `floor`
/// ([`target_tick`]). (CLI-006)
///
/// `player_id` is intentionally omitted from the wire message -- the server
/// binds identity from the session (INV-0003).
pub fn send_input(
    host: &mut ClientHost,
    floor: Tick,
    desired_tick: Tick,
    input_seq: InputSeq,
    move_dir: [f64; 2],
) -> Result<(), SendInputError> {
    let cmd = InputCmdProto {
        tick: target_tick(floor, desired_tick),
        input_seq,
        move_dir: move_dir.to_vec(),
    };
    let bytes = cmd.encode_to_vec();

    let peer = host
        .connected_peers_mut()
        .next()
        .ok_or(SendInputError::NotConnected)?;

    peer.send(
        CHANNEL_REALTIME,
        &enet::Packet::unreliable(bytes.as_slice()),
    )
    .map_err(|e| io::Error::other(format!("failed to send InputCmdProto: {e:?}")))?;

    // `Peer::send` only queues the packet; without an explicit flush (or a
    // subsequent `service()` call) it never reaches the socket -- there is
    // no background thread servicing this host between `TestClient` calls.
    host.flush();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CLI-005: `InputSeq` is strictly monotonic increasing per session,
    /// starting at 1.
    #[test]
    fn test_cli_005_input_seq_is_strictly_monotonic() {
        let mut seq_gen = InputSeqGen::new();
        assert_eq!(seq_gen.advance(), 1);
        assert_eq!(seq_gen.advance(), 2);
        assert_eq!(seq_gen.advance(), 3);
    }

    /// CLI-006: sent tick is clamped up to the floor per ADR-0006, but left
    /// unchanged when the desired tick already meets or exceeds it.
    #[test]
    fn test_cli_006_target_tick_clamps_to_floor() {
        assert_eq!(target_tick(5, 3), 5); // below floor -> clamped up
        assert_eq!(target_tick(5, 5), 5); // at floor -> unchanged
        assert_eq!(target_tick(5, 10), 10); // above floor -> unchanged
    }
}
