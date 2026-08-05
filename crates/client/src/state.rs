//! Locally tracked match state: `JoinBaseline` reception and per-tick
//! `SnapshotProto` updates.
//!
//! Ref: CLI-003 (baseline reception, via the existing
//! `TryFrom<JoinBaseline> for flowstate_sim::Baseline` conversion in
//! `flowstate-wire`), CLI-007 (snapshot reception and state update).

use std::io;
use std::time::{Duration, Instant};

use prost::Message;
use rusty_enet as enet;

use flowstate_sim::{Baseline, Snapshot};
use flowstate_wire::channels::{CHANNEL_CONTROL, CHANNEL_REALTIME};
use flowstate_wire::{JoinBaseline, SnapshotProto, Tick};

use crate::connection::ClientHost;

/// Errors that can occur while awaiting `JoinBaseline`.
#[derive(Debug)]
pub enum BaselineError {
    /// The underlying ENet/UDP transport failed while servicing the host.
    Io(io::Error),
    /// No `JoinBaseline` arrived before the timeout elapsed.
    Timeout,
    /// `JoinBaseline` decoded but failed to convert to a simulation-plane
    /// `Baseline` (malformed `EntitySnapshotProto` payload).
    Invalid(&'static str),
}

impl From<io::Error> for BaselineError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// Service `host` until `JoinBaseline` arrives on the Control channel --
/// sent immediately after `ServerWelcome`, per
/// `flowstate_server::tick_loop::run` -- then decode it and convert to the
/// simulation-plane `Baseline` via the shared `TryFrom<JoinBaseline>`
/// conversion already defined in `flowstate-wire` (reused here rather than
/// reimplemented, per the CLI-003 task note). (CLI-003)
pub fn recv_baseline(host: &mut ClientHost, timeout: Duration) -> Result<Baseline, BaselineError> {
    let deadline = Instant::now() + timeout;

    while Instant::now() < deadline {
        while let Some(event) = host
            .service()
            .map_err(|e| io::Error::other(format!("ENet service error: {e:?}")))?
        {
            if let enet::Event::Receive {
                channel_id, packet, ..
            } = event
                && channel_id == CHANNEL_CONTROL
                && let Ok(join_baseline) = JoinBaseline::decode(packet.data())
            {
                return join_baseline.try_into().map_err(BaselineError::Invalid);
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }

    Err(BaselineError::Timeout)
}

/// Errors that can occur while polling for `SnapshotProto` messages.
#[derive(Debug)]
pub enum SnapshotError {
    /// The underlying ENet/UDP transport failed while servicing the host.
    Io(io::Error),
}

impl From<io::Error> for SnapshotError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// Non-blocking: drain all events currently available on `host`, decode any
/// `SnapshotProto` received on the Realtime channel, and return the most
/// recent one (highest `tick`) along with its `target_tick_floor`, if any
/// arrived. Malformed entity payloads are skipped rather than failing the
/// whole poll.
///
/// Per ADR-0005 (unreliable + sequenced), a single drain may contain
/// multiple or reordered snapshots; only the latest tick matters to a
/// client tracking authoritative state, so older ones are discarded.
/// (CLI-007)
pub fn poll_snapshot(host: &mut ClientHost) -> Result<Option<(Snapshot, Tick)>, SnapshotError> {
    let mut latest: Option<(Snapshot, Tick)> = None;

    while let Some(event) = host
        .service()
        .map_err(|e| io::Error::other(format!("ENet service error: {e:?}")))?
    {
        if let enet::Event::Receive {
            channel_id, packet, ..
        } = event
            && channel_id == CHANNEL_REALTIME
            && let Ok(proto) = SnapshotProto::decode(packet.data())
        {
            let floor = proto.target_tick_floor;
            if let Ok(snapshot) = Snapshot::try_from(proto)
                && is_newer(latest.as_ref().map(|(s, _)| s.tick), snapshot.tick)
            {
                latest = Some((snapshot, floor));
            }
        }
    }

    Ok(latest)
}

/// Whether `candidate_tick` should replace the currently tracked snapshot
/// tick. Split out as a pure function so the "keep only the latest tick"
/// rule is unit-testable without a live connection.
fn is_newer(existing: Option<Tick>, candidate_tick: Tick) -> bool {
    existing.is_none_or(|t| candidate_tick > t)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CLI-007: only a strictly higher tick replaces the currently tracked
    /// one -- ties and reordered/stale ticks are discarded.
    #[test]
    fn test_cli_007_is_newer_keeps_highest_tick() {
        assert!(is_newer(None, 5));
        assert!(is_newer(Some(5), 6));
        assert!(!is_newer(Some(5), 5));
        assert!(!is_newer(Some(5), 3));
    }
}
