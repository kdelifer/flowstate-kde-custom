#!/usr/bin/env python
"""
Schema identity gate for FlowState (T0.19, CI-003).

Ref: ADR-0005 (v0 Networking Architecture): Game Client and Server Edge
must be built from the same schema -- both binaries must depend on the
exact same flowstate-wire package (same source, same resolved version),
not independently-defined or forked copies of the wire types.

Uses `cargo metadata` to inspect the resolved dependency graph (not just
declared Cargo.toml entries) so this catches the case a hand-written
Cargo.toml scan would miss: two different resolved versions/sources of a
crate with the same name.

Usage:
    python scripts/check_wire_shared.py

Exit codes:
    0 = success (flowstate-server and flowstate-client share one resolved
        flowstate-wire package)
    1 = gate failure (missing dependency, or resolved to different
        packages)
"""
from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

WIRE_CRATE_NAME = "flowstate-wire"
CHECKED_CRATE_NAMES = ["flowstate-server", "flowstate-client"]


def load_metadata() -> dict:
    result = subprocess.run(
        ["cargo", "metadata", "--format-version", "1"],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        print("ERROR: `cargo metadata` failed:", file=sys.stderr)
        print(result.stderr, file=sys.stderr)
        sys.exit(1)
    return json.loads(result.stdout)


def find_package_id(packages: list[dict], name: str) -> str | None:
    # A workspace member's package id path always starts with the
    # workspace root -- prefer that match if there happen to be multiple
    # same-named packages from different sources (there shouldn't be, but
    # this is a schema-identity gate, so be precise rather than lucky).
    candidates = [p for p in packages if p["name"] == name]
    for p in candidates:
        if p["id"].startswith("path+file://") and str(ROOT).replace("\\", "/") in p[
            "id"
        ].replace("\\", "/"):
            return p["id"]
    return candidates[0]["id"] if candidates else None


def resolved_normal_deps(nodes_by_id: dict[str, dict], pkg_id: str) -> dict[str, str]:
    """Map dep name -> resolved pkg id, for normal (non-dev, non-build) deps."""
    node = nodes_by_id.get(pkg_id)
    if node is None:
        return {}
    result: dict[str, str] = {}
    for dep in node.get("deps", []):
        kinds = [k.get("kind") for k in dep.get("dep_kinds", [])]
        if any(k is None for k in kinds):
            result[dep["name"]] = dep["pkg"]
    return result


def main() -> None:
    data = load_metadata()
    packages = data["packages"]
    nodes_by_id = {n["id"]: n for n in data["resolve"]["nodes"]}

    errors: list[str] = []

    wire_pkg_id = find_package_id(packages, WIRE_CRATE_NAME)
    if wire_pkg_id is None:
        print(
            f"ERROR: workspace does not contain a '{WIRE_CRATE_NAME}' package",
            file=sys.stderr,
        )
        sys.exit(1)

    resolved_wire_ids: dict[str, str] = {}

    for crate_name in CHECKED_CRATE_NAMES:
        pkg_id = find_package_id(packages, crate_name)
        if pkg_id is None:
            errors.append(f"workspace does not contain a '{crate_name}' package")
            continue

        deps = resolved_normal_deps(nodes_by_id, pkg_id)
        # cargo metadata reports dep names with underscores (the Rust
        # identifier form), regardless of the hyphenated crate name.
        dep_key = WIRE_CRATE_NAME.replace("-", "_")
        resolved = deps.get(dep_key)
        if resolved is None:
            errors.append(
                f"'{crate_name}' does not have '{WIRE_CRATE_NAME}' as a "
                "normal (non-dev) dependency"
            )
            continue

        resolved_wire_ids[crate_name] = resolved

    if not errors and len(set(resolved_wire_ids.values())) > 1:
        detail = ", ".join(
            f"{name}={pkg}" for name, pkg in sorted(resolved_wire_ids.items())
        )
        errors.append(
            f"'{WIRE_CRATE_NAME}' resolves to different packages across "
            f"binaries (schema identity broken): {detail}"
        )

    if errors:
        print("ERRORS:", file=sys.stderr)
        for e in errors:
            print(f"  - {e}", file=sys.stderr)
        print()
        print(f"Schema identity check failed: {len(errors)} error(s)")
        sys.exit(1)

    print(
        f"Schema identity check passed (T0.19): "
        f"{', '.join(CHECKED_CRATE_NAMES)} share one resolved "
        f"'{WIRE_CRATE_NAME}' package"
    )
    sys.exit(0)


if __name__ == "__main__":
    main()
