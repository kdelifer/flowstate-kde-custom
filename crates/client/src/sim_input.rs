//! Programmatic WASD input simulation for deterministic tests (no real
//! keyboard).
//!
//! Ref: CLI-008.

use flowstate_wire::Tick;

use crate::TestClient;
use crate::input::SendInputError;

/// Send the same `move_dir` for every tick in `start_tick..start_tick +
/// count`, one `InputCmdProto` per tick via [`TestClient::send_input`].
///
/// This is what makes the harness a *test* client rather than a playable
/// one: deterministic tests script a known input sequence instead of
/// reading real keyboard state, so the resulting movement can be checked
/// against the Simulation Core's own formula (CLI-009).
pub fn drive_move_dir(
    client: &mut TestClient,
    start_tick: Tick,
    count: u64,
    move_dir: [f64; 2],
) -> Result<(), SendInputError> {
    for offset in 0..count {
        client.send_input(start_tick + offset, move_dir)?;
    }
    Ok(())
}
