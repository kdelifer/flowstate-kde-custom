//! FS-0008 Gate G0.2: golden-byte fixture generator.
//!
//! Prints one JSON document to stdout mapping each fixture case name to its
//! input field values and the exact hex-encoded bytes `flowstate-wire`'s own
//! `prost`-derived `encode_to_vec()` produces for it. The Godot-side gate
//! test (`client/tests/g0_2_wire_codec_roundtrip.gd`) loads this fixture and
//! checks its own hand-written `WireCodec` produces byte-identical output,
//! and that decoding the same bytes reproduces the same field values.
//!
//! Deliberately does not touch `crates/wire` (kept as-is per the deferred
//! `.proto` migration decision) -- this is the single source of truth for
//! both the Rust-encoded bytes and the field values used to reconstruct
//! each case in GDScript, so there is exactly one place a fixture case's
//! numbers are written down.
//!
//! Regenerate via:
//! `cargo run -p flowstate-client --example g0_2_golden_bytes > client/tests/fixtures/g0_2_golden_bytes.json`

use flowstate_wire::{
    ClientHello, EntitySnapshotProto, InputCmdProto, JoinBaseline, ServerWelcome, SnapshotProto,
};
use prost::Message;

/// All integer fields are emitted as 16-hex-digit strings of their exact u64
/// bit pattern (not JSON numbers) so values with bit 63 set -- e.g. `digest`,
/// an FNV1a64 hash -- survive round-tripping through a JSON parser without
/// float64 precision loss. Godot's `String.hex_to_int()` reconstructs the
/// same 64-bit two's-complement bit pattern on the read side.
fn hex_u64(v: u64) -> String {
    format!("0x{v:016x}")
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn json_string(s: &str) -> String {
    format!("\"{s}\"")
}

fn json_f64_array(values: &[f64]) -> String {
    let parts: Vec<String> = values.iter().map(|v| format!("{v}")).collect();
    format!("[{}]", parts.join(","))
}

fn entity_fields_json(e: &EntitySnapshotProto) -> String {
    format!(
        "{{\"entity_id\":{},\"position\":{},\"velocity\":{}}}",
        json_string(&hex_u64(e.entity_id)),
        json_f64_array(&e.position),
        json_f64_array(&e.velocity),
    )
}

fn entities_fields_json(entities: &[EntitySnapshotProto]) -> String {
    let parts: Vec<String> = entities.iter().map(entity_fields_json).collect();
    format!("[{}]", parts.join(","))
}

fn main() {
    let mut cases: Vec<(&str, String, Vec<u8>)> = Vec::new();

    // --- ClientHello: zero fields, zero-byte encoding. ---
    {
        let msg = ClientHello {};
        cases.push(("client_hello", "{}".to_string(), msg.encode_to_vec()));
    }

    // --- ServerWelcome: player_id=0 tests tag-3 omission on a zero scalar. ---
    {
        let msg = ServerWelcome {
            target_tick_floor: 1,
            tick_rate_hz: 60,
            player_id: 0,
            controlled_entity_id: 100,
        };
        let fields = format!(
            "{{\"target_tick_floor\":{},\"tick_rate_hz\":{},\"player_id\":{},\"controlled_entity_id\":{}}}",
            json_string(&hex_u64(msg.target_tick_floor)),
            json_string(&hex_u64(u64::from(msg.tick_rate_hz))),
            json_string(&hex_u64(u64::from(msg.player_id))),
            json_string(&hex_u64(msg.controlled_entity_id)),
        );
        cases.push(("server_welcome_p0", fields, msg.encode_to_vec()));
    }

    // --- ServerWelcome: all fields nonzero. ---
    {
        let msg = ServerWelcome {
            target_tick_floor: 1,
            tick_rate_hz: 60,
            player_id: 1,
            controlled_entity_id: 100,
        };
        let fields = format!(
            "{{\"target_tick_floor\":{},\"tick_rate_hz\":{},\"player_id\":{},\"controlled_entity_id\":{}}}",
            json_string(&hex_u64(msg.target_tick_floor)),
            json_string(&hex_u64(u64::from(msg.tick_rate_hz))),
            json_string(&hex_u64(u64::from(msg.player_id))),
            json_string(&hex_u64(msg.controlled_entity_id)),
        );
        cases.push(("server_welcome_p1", fields, msg.encode_to_vec()));
    }

    // --- JoinBaseline: tick=0 and entities=[] both test omission. ---
    {
        let msg = JoinBaseline {
            tick: 0,
            entities: vec![],
            digest: 0xdeadbeef,
        };
        let fields = format!(
            "{{\"tick\":{},\"entities\":{},\"digest\":{}}}",
            json_string(&hex_u64(msg.tick)),
            entities_fields_json(&msg.entities),
            json_string(&hex_u64(msg.digest)),
        );
        cases.push(("join_baseline_empty", fields, msg.encode_to_vec()));
    }

    // --- JoinBaseline: entity_id=0 (submessage-internal omission) plus a
    // negative-double position/velocity (exercises the IEEE754 sign bit),
    // and a zero-but-length-2 velocity that must still be encoded. ---
    {
        let msg = JoinBaseline {
            tick: 7,
            entities: vec![
                EntitySnapshotProto {
                    entity_id: 0,
                    position: vec![0.0, 0.0],
                    velocity: vec![0.0, 0.0],
                },
                EntitySnapshotProto {
                    entity_id: 1,
                    position: vec![-2.5, 3.5],
                    velocity: vec![1.0, -1.0],
                },
            ],
            digest: 0x1234,
        };
        let fields = format!(
            "{{\"tick\":{},\"entities\":{},\"digest\":{}}}",
            json_string(&hex_u64(msg.tick)),
            entities_fields_json(&msg.entities),
            json_string(&hex_u64(msg.digest)),
        );
        cases.push(("join_baseline_two_entities", fields, msg.encode_to_vec()));
    }

    // --- InputCmdProto: normal diagonal move. ---
    {
        let msg = InputCmdProto {
            tick: 5,
            input_seq: 1,
            move_dir: vec![0.707, 0.707],
        };
        let fields = format!(
            "{{\"tick\":{},\"input_seq\":{},\"move_dir\":{}}}",
            json_string(&hex_u64(msg.tick)),
            json_string(&hex_u64(msg.input_seq)),
            json_f64_array(&msg.move_dir),
        );
        cases.push(("input_cmd_normal", fields, msg.encode_to_vec()));
    }

    // --- InputCmdProto: move_dir=[0.0, 0.0] -- length 2, both zero, MUST
    // still encode 16 bytes of packed payload (empty-vs-zero-vector). ---
    {
        let msg = InputCmdProto {
            tick: 1,
            input_seq: 1,
            move_dir: vec![0.0, 0.0],
        };
        let fields = format!(
            "{{\"tick\":{},\"input_seq\":{},\"move_dir\":{}}}",
            json_string(&hex_u64(msg.tick)),
            json_string(&hex_u64(msg.input_seq)),
            json_f64_array(&msg.move_dir),
        );
        cases.push(("input_cmd_zero_move_dir", fields, msg.encode_to_vec()));
    }

    // --- InputCmdProto: move_dir=[] -- MUST omit field 3 entirely. ---
    {
        let msg = InputCmdProto {
            tick: 1,
            input_seq: 1,
            move_dir: vec![],
        };
        let fields = format!(
            "{{\"tick\":{},\"input_seq\":{},\"move_dir\":{}}}",
            json_string(&hex_u64(msg.tick)),
            json_string(&hex_u64(msg.input_seq)),
            json_f64_array(&msg.move_dir),
        );
        cases.push(("input_cmd_empty_move_dir", fields, msg.encode_to_vec()));
    }

    // --- SnapshotProto: digest with bit 63 set but not all bits set --
    // the varint sign-extension hazard regression case (see ByteWriter's
    // `_unsigned_shr` doc comment on the GDScript side). ---
    {
        let msg = SnapshotProto {
            tick: 1,
            entities: vec![],
            digest: 0x8000_0000_0000_0001,
            target_tick_floor: 2,
        };
        let fields = format!(
            "{{\"tick\":{},\"entities\":{},\"digest\":{},\"target_tick_floor\":{}}}",
            json_string(&hex_u64(msg.tick)),
            entities_fields_json(&msg.entities),
            json_string(&hex_u64(msg.digest)),
            json_string(&hex_u64(msg.target_tick_floor)),
        );
        cases.push(("snapshot_high_digest", fields, msg.encode_to_vec()));
    }

    // --- SnapshotProto: target_tick_floor=0 tests tag-4 (last field)
    // omission. ---
    {
        let msg = SnapshotProto {
            tick: 1,
            entities: vec![],
            digest: 0x1234,
            target_tick_floor: 0,
        };
        let fields = format!(
            "{{\"tick\":{},\"entities\":{},\"digest\":{},\"target_tick_floor\":{}}}",
            json_string(&hex_u64(msg.tick)),
            entities_fields_json(&msg.entities),
            json_string(&hex_u64(msg.digest)),
            json_string(&hex_u64(msg.target_tick_floor)),
        );
        cases.push((
            "snapshot_zero_target_tick_floor",
            fields,
            msg.encode_to_vec(),
        ));
    }

    // --- EntitySnapshotProto standalone: zero-but-length-2 velocity. ---
    {
        let msg = EntitySnapshotProto {
            entity_id: 7,
            position: vec![1.5, -2.5],
            velocity: vec![0.0, 0.0],
        };
        let fields = entity_fields_json(&msg);
        cases.push(("entity_snapshot_basic", fields, msg.encode_to_vec()));
    }

    let entries: Vec<String> = cases
        .into_iter()
        .map(|(name, fields, bytes)| {
            format!(
                "{}:{{\"fields\":{},\"hex\":{}}}",
                json_string(name),
                fields,
                json_string(&hex_bytes(&bytes)),
            )
        })
        .collect();

    println!("{{{}}}", entries.join(","));
}
