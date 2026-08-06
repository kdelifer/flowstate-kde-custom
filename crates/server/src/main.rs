//! Flowstate Server Edge binary.
//!
//! Ref: SRV-002, SRV-006 (CLI parsing), SRV-007 (env var fallbacks), LOOP-001

use std::env;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use flowstate_server::{ServerConfig, tick_loop};

const DEFAULT_BIND: &str = "0.0.0.0:6060";
const DEFAULT_REPLAY_DIR: &str = "replays";

const USAGE: &str = "\
Usage: flowstate-server [OPTIONS]

Options:
  --bind <ADDR:PORT>       Address to listen on
                           [env: FLOWSTATE_BIND] [default: 0.0.0.0:6060]
  --seed <N>               Deterministic simulation seed (u64)
                           [env: FLOWSTATE_SEED] [default: 0]
  --replay-dir <PATH>      Directory to write replay artifacts
                           [env: FLOWSTATE_REPLAY_DIR] [default: replays]
  --test-mode              Tag the replay artifact as test mode
                           [env: FLOWSTATE_TEST_MODE=1]
  --test-player-ids <IDS>  Comma-separated PlayerIds assigned in connection
                           order instead of the default 0,1,2,... (e.g.
                           17,99); implies --test-mode
                           [env: FLOWSTATE_TEST_PLAYER_IDS]
  -h, --help               Print this help and exit

CLI flags take precedence over the matching environment variable, which
takes precedence over the default.
";

#[derive(Debug)]
enum Action {
    Help,
    Run(SocketAddr, PathBuf, ServerConfig),
}

/// Parse CLI args, falling back to environment variables (SRV-006,
/// SRV-007), then built-in defaults. Returns `Err` with a human-readable
/// message on invalid input -- `main` prints it alongside usage and exits
/// non-zero rather than panicking.
fn parse_args(args: impl Iterator<Item = String>) -> Result<Action, String> {
    let mut bind_raw: Option<String> = None;
    let mut seed_raw: Option<String> = None;
    let mut replay_dir_raw: Option<String> = None;
    let mut test_mode = false;
    let mut test_player_ids_raw: Option<String> = None;

    let mut args = args;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Action::Help),
            "--bind" => bind_raw = Some(next_value(&mut args, "--bind")?),
            "--seed" => seed_raw = Some(next_value(&mut args, "--seed")?),
            "--replay-dir" => replay_dir_raw = Some(next_value(&mut args, "--replay-dir")?),
            "--test-mode" => test_mode = true,
            "--test-player-ids" => {
                test_player_ids_raw = Some(next_value(&mut args, "--test-player-ids")?);
            }
            other => return Err(format!("unrecognized argument: {other}")),
        }
    }

    let bind_str = bind_raw
        .or_else(|| env::var("FLOWSTATE_BIND").ok())
        .unwrap_or_else(|| DEFAULT_BIND.to_string());
    let bind = bind_str
        .parse::<SocketAddr>()
        .map_err(|e| format!("invalid --bind value {bind_str:?}: {e}"))?;

    let seed = match seed_raw.or_else(|| env::var("FLOWSTATE_SEED").ok()) {
        Some(s) => s
            .parse::<u64>()
            .map_err(|e| format!("invalid --seed value {s:?}: {e}"))?,
        None => 0,
    };

    let replay_dir = replay_dir_raw
        .or_else(|| env::var("FLOWSTATE_REPLAY_DIR").ok())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_REPLAY_DIR));

    if !test_mode && env_flag_set("FLOWSTATE_TEST_MODE") {
        test_mode = true;
    }

    let test_player_ids =
        match test_player_ids_raw.or_else(|| env::var("FLOWSTATE_TEST_PLAYER_IDS").ok()) {
            Some(s) => {
                test_mode = true; // --test-player-ids implies test mode
                Some(parse_player_ids(&s)?)
            }
            None => None,
        };

    let config = ServerConfig {
        seed,
        test_mode,
        test_player_ids,
        ..Default::default()
    };

    Ok(Action::Run(bind, replay_dir, config))
}

fn next_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn env_flag_set(key: &str) -> bool {
    matches!(
        env::var(key).ok().as_deref(),
        Some("1" | "true" | "TRUE" | "yes")
    )
}

/// Comma-separated PlayerIds, e.g. "17,99" -> `[17, 99]`.
fn parse_player_ids(s: &str) -> Result<Vec<u8>, String> {
    s.split(',')
        .map(|part| {
            part.trim()
                .parse::<u8>()
                .map_err(|e| format!("invalid --test-player-ids entry {part:?}: {e}"))
        })
        .collect()
}

fn main() -> ExitCode {
    let (addr, replay_dir, config) = match parse_args(env::args().skip(1)) {
        Ok(Action::Help) => {
            print!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Ok(Action::Run(addr, replay_dir, config)) => (addr, replay_dir, config),
        Err(e) => {
            eprintln!("flowstate-server: {e}\n");
            eprint!("{USAGE}");
            return ExitCode::FAILURE;
        }
    };

    println!(
        "flowstate-server: starting on {addr} (seed={}, tick_rate_hz={})",
        config.seed, config.tick_rate_hz
    );

    let artifact = match tick_loop::run(config, addr) {
        Ok(artifact) => artifact,
        Err(e) => {
            eprintln!("flowstate-server: {e}");
            return ExitCode::FAILURE;
        }
    };

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let path = replay_dir.join(format!("replay_{timestamp}.replay"));

    if let Err(e) = flowstate_replay::write_replay(&artifact, &path) {
        eprintln!("flowstate-server: failed to write replay artifact: {e}");
        return ExitCode::FAILURE;
    }

    println!(
        "flowstate-server: match complete (end_reason={}, checkpoint_tick={}), replay written to {}",
        artifact.end_reason,
        artifact.checkpoint_tick,
        path.display()
    );

    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(items: &[&str]) -> impl Iterator<Item = String> {
        items
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
            .into_iter()
    }

    #[test]
    fn test_defaults_when_nothing_provided() {
        let Ok(Action::Run(addr, replay_dir, config)) = parse_args(args(&[])) else {
            panic!("expected Action::Run");
        };
        assert_eq!(addr, DEFAULT_BIND.parse::<SocketAddr>().unwrap());
        assert_eq!(replay_dir, PathBuf::from(DEFAULT_REPLAY_DIR));
        assert_eq!(config.seed, 0);
        assert!(!config.test_mode);
        assert_eq!(config.test_player_ids, None);
    }

    #[test]
    fn test_cli_flags_set_config() {
        let Ok(Action::Run(addr, replay_dir, config)) = parse_args(args(&[
            "--bind",
            "127.0.0.1:7777",
            "--seed",
            "42",
            "--replay-dir",
            "/tmp/replays",
            "--test-player-ids",
            "17,99",
        ])) else {
            panic!("expected Action::Run");
        };
        assert_eq!(addr, "127.0.0.1:7777".parse::<SocketAddr>().unwrap());
        assert_eq!(replay_dir, PathBuf::from("/tmp/replays"));
        assert_eq!(config.seed, 42);
        assert!(config.test_mode, "--test-player-ids must imply test_mode");
        assert_eq!(config.test_player_ids, Some(vec![17, 99]));
    }

    #[test]
    fn test_help_flag_short_circuits() {
        assert!(matches!(parse_args(args(&["--help"])), Ok(Action::Help)));
        assert!(matches!(parse_args(args(&["-h"])), Ok(Action::Help)));
    }

    #[test]
    fn test_invalid_seed_is_an_error_not_a_panic() {
        let err = parse_args(args(&["--seed", "not-a-number"])).unwrap_err();
        assert!(err.contains("--seed"), "error should name the flag: {err}");
    }

    #[test]
    fn test_invalid_bind_is_an_error_not_a_panic() {
        let err = parse_args(args(&["--bind", "not-an-address"])).unwrap_err();
        assert!(err.contains("--bind"), "error should name the flag: {err}");
    }

    #[test]
    fn test_missing_flag_value_is_an_error() {
        let err = parse_args(args(&["--seed"])).unwrap_err();
        assert!(err.contains("--seed"), "error should name the flag: {err}");
    }

    #[test]
    fn test_unrecognized_flag_is_an_error() {
        let err = parse_args(args(&["--nonsense"])).unwrap_err();
        assert!(err.contains("--nonsense"));
    }

    #[test]
    fn test_bare_test_mode_flag_without_player_ids() {
        let Ok(Action::Run(_, _, config)) = parse_args(args(&["--test-mode"])) else {
            panic!("expected Action::Run");
        };
        assert!(config.test_mode);
        assert_eq!(config.test_player_ids, None);
    }
}
