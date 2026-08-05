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

use flowstate_sim::Baseline;
use flowstate_wire::JoinBaseline;
use flowstate_wire::channels::CHANNEL_CONTROL;

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
