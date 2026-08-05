//! Flowstate Server Edge binary.
//!
//! Ref: SRV-002, LOOP-001

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use flowstate_server::{ServerConfig, tick_loop};

fn main() {
    let addr: SocketAddr = "0.0.0.0:6060".parse().expect("valid default bind address");
    let config = ServerConfig::default();

    println!(
        "flowstate-server: listening on {addr} (seed={}, tick_rate_hz={})",
        config.seed, config.tick_rate_hz
    );

    let artifact = match tick_loop::run(config, addr) {
        Ok(artifact) => artifact,
        Err(e) => {
            eprintln!("flowstate-server: {e}");
            std::process::exit(1);
        }
    };

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let path = PathBuf::from("replays").join(format!("replay_{timestamp}.replay"));

    if let Err(e) = flowstate_replay::write_replay(&artifact, &path) {
        eprintln!("flowstate-server: failed to write replay artifact: {e}");
        std::process::exit(1);
    }

    println!(
        "flowstate-server: match complete (end_reason={}, checkpoint_tick={}), replay written to {}",
        artifact.end_reason,
        artifact.checkpoint_tick,
        path.display()
    );
}
