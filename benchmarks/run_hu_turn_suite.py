#!/usr/bin/env python3
"""Run HU turn suite on rust_solver; write JSON results for diff vs solver_ext."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def run_rust_suite() -> dict:
    bin_path = ROOT / "target" / "release" / "hu_turn_suite_bench"
    if not bin_path.exists():
        subprocess.run(
            ["cargo", "build", "--release", "--bin", "hu_turn_suite_bench"],
            cwd=str(ROOT),
            check=True,
        )
    env = {**dict(__import__("os").environ)}
    env.setdefault("OUT_DIR", str(ROOT / "target" / "release" / "deps"))
    proc = subprocess.run(
        [str(bin_path)],
        cwd=str(ROOT),
        capture_output=True,
        text=True,
        check=True,
        env=env,
    )
    return json.loads(proc.stdout)


def main() -> int:
    rust = run_rust_suite()
    out = ROOT / "benchmarks" / "hu_turn_suite_results.json"
    payload = {"rust_solver": rust}
    out.write_text(json.dumps(payload, indent=2) + "\n")
    print(f"Wrote {out}")
    print(f"spots: {len(rust.get('spots', []))}")
    print(f"parity: {rust.get('parity_matches')}/{rust.get('parity_total')}")
    for s in rust.get("spots", []):
        eps = s.get("exploitability_max_mbb")
        eps_s = f"{eps:.1f}" if eps is not None else "n/a"
        print(
            f"  {s['spot_id']}: top={s.get('top_action')} "
            f"check={s.get('check_prob')} eps={eps_s} "
            f"ms={s.get('solve_elapsed_ms', 0):.0f}"
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
