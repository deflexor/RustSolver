#!/usr/bin/env python3
"""Collect rjeans solver_ext baselines for hu_turn_suite.json spots.

KK spots use ``solve_flop_tree`` (turn_card_limit=2, TUI path).
Other spots use ``solve_single`` (turn-entry, matches rust_solver turn solve).
"""

from __future__ import annotations

import json
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RJEANS = Path.home() / "rjeans"
SUITE_PATH = ROOT / "benchmarks" / "hu_turn_suite.json"
OUT_PATH = ROOT / "benchmarks" / "hu_turn_suite_rjeans_baselines.json"


def _setup_rjeans() -> None:
    sys.path[:0] = [str(RJEANS / "python"), str(RJEANS)]


def _decisions(sample, pot_bb: float, call_bb: float):
    from rjeans_tui.solver_decide import _solver_sample_to_decisions

    return _solver_sample_to_decisions(
        list(sample.action_probs),
        list(sample.raise_probs),
        pot_bb,
        None,
        None,
        call_bb,
    )[:5]


def _match_sample(samples, pot_bb: float, call_bb: float, turn_card: str):
    from rjeans_policy.state import Street
    from rjeans_tui.solver_decide import _find_matching_sample

    turn = [x for x in samples if x.street == "turn"]
    on_card = [
        x
        for x in turn
        if len(x.board) >= 4
        and x.board[3] == turn_card
        and abs(x.pot_bb - pot_bb) <= 0.5
        and abs(x.call_cost_bb - call_bb) <= 0.01
    ]
    if on_card:
        return on_card[0]

    matched = _find_matching_sample(samples, Street.TURN, pot_bb, call_bb)
    if matched is not None:
        return matched
    if not turn:
        return None
    return min(turn, key=lambda x: abs(x.pot_bb - pot_bb) + abs(x.call_cost_bb - call_bb))


def baseline_for_spot(session, spot: dict) -> dict:
    import solver_ext

    pot_bb = spot["query_pot_bb"]
    call_bb = spot["query_call_bb"]
    t0 = time.monotonic()

    if spot["id"].startswith("kk_"):
        samples = session.solve_flop_tree(
            hero_hand=spot["hero_hand"],
            spot="HU_CX",
            stack=spot["stack_bb"],
            flop=spot["flop"],
            weip_flop=False,
            bet_sizes="50%, 75%, 100%, a",
            turn_card_limit=2,
            max_iter=1000,
            oop_range=spot["oop_range"],
            ip_range=spot["ip_range"],
        )
        path = "solve_flop_tree"
    else:
        samples = session.solve_single(
            hero_hand=spot["hero_hand"],
            oop_range=spot["oop_range"],
            ip_range=spot["ip_range"],
            flop=spot["flop"],
            turn_card=spot["turn_card"],
            starting_pot=2,
            effective_stack=spot["stack_bb"],
            weip_flop=False,
        )
        path = "solve_single"

    elapsed_ms = (time.monotonic() - t0) * 1000.0
    matched = _match_sample(samples, pot_bb, call_bb, spot["turn_card"])
    top_action = None
    check_prob = None
    decisions = []
    if matched is not None:
        check_prob = matched.action_probs[1]
        for d in _decisions(matched, pot_bb, call_bb):
            decisions.append(
                {
                    "action": d.action_type,
                    "raise_to_bb": d.raise_to_bb,
                    "score": round(d.score, 6),
                }
            )
        if decisions:
            top_action = decisions[0]["action"]

    return {
        "spot_id": spot["id"],
        "solver_path": path,
        "solve_elapsed_ms": round(elapsed_ms, 1),
        "baseline_top_action": top_action,
        "baseline_check_prob": check_prob,
        "matched_sample": None
        if matched is None
        else {
            "board": list(matched.board),
            "pot_bb": matched.pot_bb,
            "call_cost_bb": matched.call_cost_bb,
            "action_probs": list(matched.action_probs),
        },
        "decisions_ranked": decisions,
    }


def main() -> int:
    if not RJEANS.is_dir():
        print(f"rjeans not found at {RJEANS}", file=sys.stderr)
        return 1

    _setup_rjeans()
    import solver_ext

    suite = json.loads(SUITE_PATH.read_text())
    session = solver_ext.SolverSession("flop")
    baselines = [baseline_for_spot(session, spot) for spot in suite["spots"]]

    OUT_PATH.write_text(json.dumps({"baselines": baselines}, indent=2) + "\n")
    print(f"Wrote {OUT_PATH}")
    for b in baselines:
        cp = b["baseline_check_prob"]
        cp_s = f"{cp:.4f}" if cp is not None else "n/a"
        print(f"  {b['spot_id']}: top={b['baseline_top_action']} check={cp_s} ({b['solver_path']})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
