#!/usr/bin/env python3
"""Run HU turn suite on rust_solver; diff vs rjeans baselines when available."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASELINES_PATH = ROOT / "benchmarks" / "hu_turn_suite_rjeans_baselines.json"
SUITE_PATH = ROOT / "benchmarks" / "hu_turn_suite.json"


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


def load_baselines() -> dict[str, dict]:
    if BASELINES_PATH.exists():
        doc = json.loads(BASELINES_PATH.read_text())
        return {b["spot_id"]: b for b in doc.get("baselines", [])}
    suite = json.loads(SUITE_PATH.read_text())
    out: dict[str, dict] = {}
    for spot in suite["spots"]:
        if spot.get("baseline_top_action") is not None:
            out[spot["id"]] = {
                "baseline_top_action": spot["baseline_top_action"],
                "baseline_check_prob": spot.get("baseline_check_prob"),
            }
    return out


def parity_ok(rust: dict, baseline: dict, *, check_tol: float = 0.15) -> tuple[bool, str]:
    rt = rust.get("top_action")
    rj = baseline.get("baseline_top_action")
    if rt is not None and rj is not None and rt == rj:
        return True, "top_action"
    rc = rust.get("check_prob")
    bc = baseline.get("baseline_check_prob")
    if rc is not None and bc is not None and abs(rc - bc) <= check_tol:
        return True, f"check±{check_tol}"
    return False, "mismatch"


def main() -> int:
    rust_doc = run_rust_suite()
    baselines = load_baselines()
    out = ROOT / "benchmarks" / "hu_turn_suite_results.json"

    comparisons = []
    parity_matches = 0
    parity_total = 0
    for spot in rust_doc.get("spots", []):
        sid = spot["spot_id"]
        base = baselines.get(sid)
        if not base:
            continue
        parity_total += 1
        ok, reason = parity_ok(spot, base)
        if ok:
            parity_matches += 1
        comparisons.append(
            {
                "spot_id": sid,
                "rust_top_action": spot.get("top_action"),
                "rjeans_top_action": base.get("baseline_top_action"),
                "rust_check_prob": spot.get("check_prob"),
                "rjeans_check_prob": base.get("baseline_check_prob"),
                "parity_ok": ok,
                "parity_reason": reason,
            }
        )

    payload = {
        "rust_solver": rust_doc,
        "comparisons": comparisons,
        "parity_matches": parity_matches,
        "parity_total": parity_total,
    }
    out.write_text(json.dumps(payload, indent=2) + "\n")
    print(f"Wrote {out}")
    print(f"parity: {parity_matches}/{parity_total}")
    for c in comparisons:
        mark = "OK" if c["parity_ok"] else "MISS"
        print(
            f"  [{mark}] {c['spot_id']}: rust={c['rust_top_action']} "
            f"rjeans={c['rjeans_top_action']} ({c['parity_reason']})"
        )
    for s in rust_doc.get("spots", []):
        eps = s.get("exploitability_max_mbb")
        eps_s = f"{eps:.1f}" if eps is not None else "n/a"
        print(
            f"  {s['spot_id']}: top={s.get('top_action')} "
            f"check={s.get('check_prob')} eps={eps_s} "
            f"ms={s.get('solve_elapsed_ms', 0):.0f}"
        )

    if parity_total >= 5 and parity_matches / parity_total < 0.6:
        print("FAIL: parity below 60% staging floor", file=sys.stderr)
        return 1
    if any(c["spot_id"] == "kk_kd_check" for c in comparisons):
        kk = next(c for c in comparisons if c["spot_id"] == "kk_kd_check")
        rc = kk.get("rust_check_prob") or 0.0
        if abs(rc - 0.6016) > 0.10:
            print(f"FAIL: kk_kd_check anchor check={rc:.3f}", file=sys.stderr)
            return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
