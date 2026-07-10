#!/usr/bin/env python3
"""Run KK turn A/B benchmark: rjeans solver_ext vs rust_solver kk_turn_bench."""

from __future__ import annotations

import json
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RJEANS = Path.home() / "rjeans"

OOP = "66+,A8s+,A5s-A4s,AJo+,K9s+,KQo,QTs+,JTs,96s+,85s+,75s+,65s,54s"
IP = "QQ-22,AQs-A2s,ATo+,K5s+,KJo+,Q8s+,J8s+,T7s+,96s+,75s+,64s+,53s+"

BASELINE = {
    "solve_elapsed_ms": 634.5,
    "total_samples": 2320,
    "top_action": "call",
    "top_score": 0.489746,
    "action_probs": [0.0, 0.489746, 0.510254],
    "raise_probs": [0.009277, 0.009277, 0.009277, 0.972168],
}


def run_rjeans() -> dict:
    script = f"""
import sys, time, json
from pathlib import Path
_ROOT = Path({str(RJEANS)!r})
sys.path[:0] = [str(_ROOT / "python"), str(_ROOT)]
import solver_ext
from rjeans_policy.state import Street
from rjeans_tui.solver_decide import _find_matching_sample, _solver_sample_to_decisions

OOP = {OOP!r}
IP = {IP!r}
s = solver_ext.SolverSession("flop")
t0 = time.monotonic()
samples = s.solve_flop_tree(
    hero_hand="KsKc", spot="HU_CX", stack=12, flop="4dQcQd", weip_flop=False,
    bet_sizes="50%, 75%, 100%, a", turn_card_limit=2, max_iter=200, target_frac=0.05,
    oop_range=OOP, ip_range=IP,
)
elapsed_ms = (time.monotonic() - t0) * 1000.0
turn_boards = sorted(set(x.board[3] for x in samples if len(x.board) >= 4))
m = _find_matching_sample(samples, Street.TURN, 6.16, 0.0)
decisions = []
if m is not None:
    for d in _solver_sample_to_decisions(list(m.action_probs), list(m.raise_probs), 6.16, None, None, 0.0)[:5]:
        decisions.append({{"action": d.action_type, "raise_to_bb": d.raise_to_bb, "score": round(d.score, 6)}})
    matched = {{
        "board": m.board,
        "pot_bb": m.pot_bb,
        "call_cost_bb": m.call_cost_bb,
        "action_probs": list(m.action_probs),
        "raise_probs": list(m.raise_probs),
    }}
else:
    matched = None
print(json.dumps({{
    "solver": "rjeans_solver_ext",
    "stack_bucket_bb": 12,
    "solve_elapsed_ms": round(elapsed_ms, 2),
    "total_samples": len(samples),
    "turn_boards_sampled": turn_boards,
    "matched_sample": matched,
    "decisions_ranked": decisions,
}}))
"""
    py = RJEANS / ".venv" / "bin" / "python"
    if not py.exists():
        py = Path(sys.executable)
    proc = subprocess.run(
        [str(py), "-c", script],
        cwd=str(RJEANS),
        capture_output=True,
        text=True,
        check=False,
    )
    if proc.returncode != 0:
        print(proc.stderr, file=sys.stderr)
        raise RuntimeError(f"rjeans benchmark failed (exit {proc.returncode})")
    return json.loads(proc.stdout.strip().splitlines()[-1])


def run_rust_solver() -> dict:
    bin_path = ROOT / "target" / "release" / "kk_turn_bench"
    if not bin_path.exists():
        subprocess.run(
            ["cargo", "build", "--release", "--bin", "kk_turn_bench"],
            cwd=str(ROOT),
            check=True,
        )
    proc = subprocess.run(
        [str(bin_path), "--json"],
        cwd=str(ROOT),
        capture_output=True,
        text=True,
        check=True,
    )
    # `--json` prints a pretty-printed object after human-readable output.
    text = proc.stdout
    start = text.find("{")
    if start < 0:
        raise RuntimeError("kk_turn_bench --json produced no JSON")
    return json.loads(text[start:])


def print_comparison(rjeans: dict, rust: dict) -> None:
    print("=" * 60)
    print("KK turn benchmark comparison (stack bucket 12)")
    print("=" * 60)
    for label, data in [("rjeans", rjeans), ("rust_solver", rust)]:
        print(f"\n--- {label} ---")
        print(f"solve_elapsed_ms: {data['solve_elapsed_ms']}")
        print(f"total_samples: {data['total_samples']}")
        print(f"turn_boards_sampled: {data.get('turn_boards_sampled')}")
        m = data.get("matched_sample")
        if m:
            print(f"matched pot/call: {m['pot_bb']:.4f} / {m['call_cost_bb']:.4f}")
            ap = m["action_probs"]
            print(f"action_probs: [{ap[0]:.6f}, {ap[1]:.6f}, {ap[2]:.6f}]")
        dec = data.get("decisions_ranked") or []
        for i, d in enumerate(dec[:5], 1):
            rt = d.get("raise_to_bb")
            if rt is not None:
                print(f"  {i}. {d['action']} raise_to={rt:.2f} score={d['score']:.6f}")
            else:
                print(f"  {i}. {d['action']} score={d['score']:.6f}")

    print("\n--- vs documented baseline ---")
    print(f"baseline time: {BASELINE['solve_elapsed_ms']} ms")
    print(f"baseline top: {BASELINE['top_action']} @ {BASELINE['top_score']:.6f}")
    rj_top = (rjeans.get("decisions_ranked") or [{}])[0]
    rs_top = (rust.get("decisions_ranked") or [{}])[0]
    print(
        f"rjeans fresh: {rj_top.get('action')} @ {rj_top.get('score', 0):.6f} "
        f"({rjeans['solve_elapsed_ms']:.1f} ms)"
    )
    print(
        f"rust_solver:  {rs_top.get('action')} @ {rs_top.get('score', 0):.6f} "
        f"({rust['solve_elapsed_ms']:.1f} ms)"
    )


def main() -> int:
    print("Running rjeans baseline...")
    rjeans = run_rjeans()
    print("Running rust_solver kk_turn_bench...")
    rust = run_rust_solver()
    print_comparison(rjeans, rust)

    out = ROOT / "benchmarks" / "kk_turn_040229_results.json"
    out.write_text(
        json.dumps({"rjeans": rjeans, "rust_solver": rust, "baseline": BASELINE}, indent=2)
        + "\n"
    )
    print(f"\nWrote {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
