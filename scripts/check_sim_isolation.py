#!/usr/bin/env python
"""
Simulation Core isolation gate for FlowState (T0.5, CI-001/CI-002).

Ref: INV-0004 (Simulation Core Isolation), KC-0001 (Kill: Boundary
Violation), ADR-0003 (Fixed Timestep).

Enforces, for crates/sim only:
  1. `#![deny(unsafe_code)]` is present at the crate root.
  2. Every [dependencies] entry in crates/sim/Cargo.toml appears in the
     allowlist at scripts/allowed_sim_deps.txt (CI-001). [dev-dependencies]
     are not constrained -- they never ship in the production Simulation
     Core boundary.
  3. No forbidden-API source pattern (scripts/forbidden_sim_patterns.txt)
     appears anywhere under crates/sim/src/ (CI-002). Coarse substring scan,
     not semantic -- a false positive should be fixed by rewording, not
     suppressing the check.
  4. `World::advance` takes an explicit `tick: Tick` parameter (ADR-0003).

Usage:
    python scripts/check_sim_isolation.py

Exit codes:
    0 = success
    1 = isolation violation found
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SIM_DIR = ROOT / "crates" / "sim"
SIM_CARGO_TOML = SIM_DIR / "Cargo.toml"
SIM_SRC_DIR = SIM_DIR / "src"
SIM_LIB_RS = SIM_SRC_DIR / "lib.rs"
ALLOWLIST_PATH = ROOT / "scripts" / "allowed_sim_deps.txt"
PATTERNS_PATH = ROOT / "scripts" / "forbidden_sim_patterns.txt"

DENY_UNSAFE_RE = re.compile(r"^\s*#!\[deny\(unsafe_code\)\]\s*$", re.MULTILINE)
ADVANCE_SIG_RE = re.compile(r"fn\s+advance\s*\(\s*&mut\s+self\s*,\s*tick\s*:\s*Tick")

# Matches a [section-name] header, including [dependencies.foo] and
# [target.'cfg(...)'.dependencies] variants.
SECTION_HEADER_RE = re.compile(r"^\s*\[([^\]]+)\]\s*$")
# A plain `key = ...` or `key.subkey = ...` line inside a table.
DEP_KEY_RE = re.compile(r"^\s*([A-Za-z0-9_.\-]+)\s*=")


def load_lines(path: Path) -> list[str]:
    """Read non-blank, non-comment lines from a fixture file."""
    if not path.exists():
        return []
    lines = []
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        lines.append(line)
    return lines


def parse_dependency_names(cargo_toml_text: str) -> list[str]:
    """Extract dependency crate names from every `[dependencies*]` table.

    Deliberately not a full TOML parser -- crates/sim's manifest is small
    and hand-written, and this only needs to recognize `name = "..."` and
    `name = { ... }` entries within [dependencies] / [dependencies.foo]
    style tables (excluding [dev-dependencies] and [build-dependencies]).
    """
    names: list[str] = []
    in_deps_table = False

    for raw_line in cargo_toml_text.splitlines():
        header_match = SECTION_HEADER_RE.match(raw_line)
        if header_match:
            section = header_match.group(1).strip()
            in_deps_table = section == "dependencies" or section.startswith(
                "dependencies."
            )
            continue

        if not in_deps_table:
            continue

        key_match = DEP_KEY_RE.match(raw_line)
        if key_match:
            names.append(key_match.group(1))

    return names


def check_deny_unsafe(errors: list[str]) -> None:
    if not SIM_LIB_RS.exists():
        errors.append(f"Missing {SIM_LIB_RS.relative_to(ROOT).as_posix()}")
        return
    text = SIM_LIB_RS.read_text(encoding="utf-8")
    if not DENY_UNSAFE_RE.search(text):
        errors.append(
            f"{SIM_LIB_RS.relative_to(ROOT).as_posix()} is missing "
            "`#![deny(unsafe_code)]` (INV-0004)"
        )


def check_dependency_allowlist(errors: list[str]) -> None:
    if not SIM_CARGO_TOML.exists():
        errors.append(f"Missing {SIM_CARGO_TOML.relative_to(ROOT).as_posix()}")
        return

    allowed = set(load_lines(ALLOWLIST_PATH))
    declared = parse_dependency_names(SIM_CARGO_TOML.read_text(encoding="utf-8"))

    for name in declared:
        if name not in allowed:
            errors.append(
                f"crates/sim depends on '{name}', which is not in "
                f"{ALLOWLIST_PATH.relative_to(ROOT).as_posix()} (INV-0004: "
                "Simulation Core Isolation). Either remove the dependency "
                "or add it to the allowlist with justification."
            )


def check_forbidden_patterns(errors: list[str]) -> None:
    patterns = load_lines(PATTERNS_PATH)
    if not patterns:
        errors.append(
            f"{PATTERNS_PATH.relative_to(ROOT).as_posix()} has no patterns "
            "to check against -- refusing to pass an isolation gate that "
            "would silently check nothing"
        )
        return

    if not SIM_SRC_DIR.exists():
        errors.append(f"Missing {SIM_SRC_DIR.relative_to(ROOT).as_posix()}")
        return

    for rs_file in sorted(SIM_SRC_DIR.rglob("*.rs")):
        for lineno, line in enumerate(
            rs_file.read_text(encoding="utf-8").splitlines(), start=1
        ):
            for pattern in patterns:
                if pattern in line:
                    rel = rs_file.relative_to(ROOT).as_posix()
                    errors.append(
                        f"{rel}:{lineno}: forbidden pattern '{pattern}' "
                        "(INV-0004: Simulation Core Isolation)"
                    )


def check_advance_signature(errors: list[str]) -> None:
    if not SIM_LIB_RS.exists():
        return  # already reported by check_deny_unsafe
    text = SIM_LIB_RS.read_text(encoding="utf-8")
    if not ADVANCE_SIG_RE.search(text):
        errors.append(
            f"{SIM_LIB_RS.relative_to(ROOT).as_posix()}: World::advance must "
            "take an explicit `tick: Tick` parameter (ADR-0003: Fixed "
            "Timestep Simulation)"
        )


def main() -> None:
    errors: list[str] = []

    check_deny_unsafe(errors)
    check_dependency_allowlist(errors)
    check_forbidden_patterns(errors)
    check_advance_signature(errors)

    if errors:
        print("ERRORS:", file=sys.stderr)
        for e in errors:
            print(f"  - {e}", file=sys.stderr)
        print()
        print(f"Simulation Core isolation check failed: {len(errors)} error(s)")
        sys.exit(1)

    print("Simulation Core isolation check passed (T0.5)")
    sys.exit(0)


if __name__ == "__main__":
    main()
