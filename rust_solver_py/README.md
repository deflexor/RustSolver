# rust_solver_py

Python bindings (PyO3 + maturin) for the RustSolver MCCFR postflop engine.
Designed as a **drop-in replacement** for `solver_ext` on HU turn/river decision
queries in the rjeans TUI.

## Requirements

- Rust stable (2021 edition)
- Python ≥ 3.10
- [uv](https://github.com/astral-sh/uv) or `python -m venv`
- [maturin](https://www.maturin.rs/) ≥ 1.0

Build the core solver once so the hand evaluator tables exist:

```bash
cd /path/to/RustSolver
cargo build --release
```

The evaluator reads `offset_table.dat` from `OUT_DIR` (set automatically when
you build with Cargo) or from `target/release/deps/`.

## Install (editable, recommended)

```bash
cd /path/to/RustSolver
uv venv
source .venv/bin/activate   # Windows: .venv\Scripts\activate
pip install maturin

cd rust_solver_py
maturin develop --release
```

Verify:

```bash
OUT_DIR=../target/release/deps python -c "import rust_solver_py; print(rust_solver_py.SolverSession)"
```

## Install (wheel)

```bash
cd rust_solver_py
maturin build --release
pip install target/wheels/rust_solver_py-*.whl
```

## API

| Symbol | Role |
|--------|------|
| `SolverSession(flop_dir)` | Session holder (`flop_dir` ignored in v1) |
| `SolverSession.solve_flop_tree(...)` | Same signature as `solver_ext`; returns `list[TrainingSample]` |
| `solve_turn_decision(...)` | One-shot API for a single turn card |
| `TrainingSample` | Hero decision node: `action_probs`, `raise_probs`, `pot_bb`, `call_cost_bb`, … |

### `solve_flop_tree` parameters

Matches rjeans `solver_ext`:

```python
samples = session.solve_flop_tree(
    hero_hand="KsKc",
    spot="HU_CX",           # ignored in v1 when ranges are passed
    stack=12,               # stack bucket (BB)
    flop="4dQcQd",
    weip_flop=False,        # False → hero OOP
    max_iter=200,
    target_frac=0.05,       # accepted, not used yet
    bet_sizes="50%, 75%, 100%, a",  # accepted, not used yet
    turn_card_limit=2,
    oop_range="66+,A8s+,…",
    ip_range="QQ-22,AQs-A2s,…",
)
```

PPT hyphen ranges (`QQ-22`, `A5s-A4s`) are expanded before parsing. If expansion
yields too few combos, built-in expanded combo lists from
`benchmarks/kk_turn_expanded_combos.txt` are used as fallback.

### `TrainingSample` fields (TUI-relevant)

| Field | Meaning |
|-------|---------|
| `action_probs[3]` | `[fold, call/check, raise]` |
| `raise_probs[4]` | Conditional raise sizing (50% / 75% / 100% / all-in) |
| `pot_bb`, `call_cost_bb` | Node geometry for `_find_matching_sample` |
| `board`, `street` | Match query street and board |
| `hero_hole`, `hero_pos`, `weip_flop` | Metadata |

## rjeans TUI integration

Pipeline (unchanged):

```
SolverSession.solve_flop_tree → _find_matching_sample → _solver_sample_to_decisions
```

Swap behind an env flag:

```python
import os

if os.environ.get("RUST_SOLVER"):
    import rust_solver_py as solver_ext
else:
    import solver_ext
```

Run with the new solver:

```bash
export RUST_SOLVER=1
export OUT_DIR=/path/to/RustSolver/target/release/deps
# start TUI as usual
```

## Example — KK turn spot

Same spot as `benchmarks/kk_turn_040229_prompt.md`:

```python
import os
import time

os.environ.setdefault("OUT_DIR", "../target/release/deps")

import rust_solver_py
from rjeans_policy.state import Street
from rjeans_tui.solver_decide import _find_matching_sample, _solver_sample_to_decisions

OOP = "66+,A8s+,A5s-A4s,AJo+,K9s+,KQo,QTs+,JTs,96s+,85s+,75s+,65s,54s"
IP  = "QQ-22,AQs-A2s,ATo+,K5s+,KJo+,Q8s+,J8s+,T7s+,96s+,86s+,75s+,64s+,53s+"

session = rust_solver_py.SolverSession("flop")
t0 = time.monotonic()
samples = session.solve_flop_tree(
    hero_hand="KsKc",
    spot="HU_CX",
    stack=12,
    flop="4dQcQd",
    weip_flop=False,
    bet_sizes="50%, 75%, 100%, a",
    turn_card_limit=2,
    max_iter=200,
    oop_range=OOP,
    ip_range=IP,
)
print(f"solve elapsed: {(time.monotonic() - t0) * 1000:.0f} ms, samples={len(samples)}")

matched = _find_matching_sample(samples, Street.TURN, 6.16, 0.0)
if matched:
    for d in _solver_sample_to_decisions(
        list(matched.action_probs),
        list(matched.raise_probs),
        6.16, None, None, 0.0,
    )[:5]:
        print(d.action_type, d.raise_to_bb, round(d.score, 4))
```

One-shot variant:

```python
d = rust_solver_py.solve_turn_decision(
    "KsKc", "4dQcQd", "Kd",
    stack_bb=12, pot_bb=6.16, call_cost_bb=0.0,
    oop_range=OOP, ip_range=IP,
)
print(d.pot_bb, d.action_probs)
```

## Benchmark parity

Compare against the Rust harness:

```bash
OUT_DIR=target/release/deps cargo run --release --bin kk_turn_bench
```

Phase 10 quality gate (geometry, non-uniform strategy, &lt;500 ms):

```bash
OUT_DIR=target/release/deps cargo test --release --bin kk_turn_bench kk_turn_quality_gate -- --ignored
```

## Current limitations (v0.1)

- **Turn-entry** solves per sampled turn card (not full flop-tree traversal like postflop-solver).
- `spot`, `target_frac`, `bet_sizes`, `use_donk`, `use_compression` are accepted for API compat but not fully wired.
- No `list_spots` / GTO `.json.z` range extraction (pass `oop_range` / `ip_range` explicitly).
- Exploitability reporting is not exposed; BR scale on turn trees is still being calibrated.
- Strategy quality is non-uniform and fast but may differ from rjeans `solver_ext` CFR baselines.

See `PLAN.md` Phase 11 and `TASKS.md` P11.x for the roadmap.
