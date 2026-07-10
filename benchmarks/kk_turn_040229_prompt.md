# Solver A/B benchmark — KK turn spot (table_3_20260708_040229)

Use this prompt to run an alternate postflop solver with **identical inputs** as rjeans `solver_ext`, then compare **solve time** and **ranked decisions**.

**Hand:** HU turn, hero **KsKc** on **4d Qc Qd | 3c**, pot **6.16 BB**, villain **4.86 BB**, hero **BB** facing no bet.

**Pipeline:** `TurnRiverSolver` → `solver_ext::SolverSession::solve_flop_tree` → `_find_matching_sample` → `_solver_sample_to_decisions`.

---

## Task

1. Feed the solver the **OOP and IP range strings** below (exactly as supplied to CFR).
2. Use the same tree params (`stack`, `bet_sizes`, `max_iter`, etc.).
3. Report `solve_elapsed_ms`, matched-sample probs, and ranked decisions.
4. Compare to **Expected baseline**.

---

## Game state

| Field | Value |
|-------|-------|
| `hero_hand` | `KsKc` |
| `hero_pos` | `BB` (OOP) |
| `weip_flop` | `false` |
| `board_flop` | `4dQcQd` |
| `board_turn` | `3c` |
| `street_query` | `turn` |
| `pot_bb` | `6.16` |
| `call_cost_bb` | `0.0` |
| `effective_stack_bb` | `4.86` (min-stack; TUI baseline used `13.96` → stack bucket **12**) |
| `preflop_spot` | `bb_vs_c` |
| `action_path` | `P.c_P.k_F.4dQcQd` |
| `gto_spot` | `HU_CX` |
| `stack_bucket_bb` | **12** for TUI baseline timing/decisions below (`5` with current eff-stack fix) |

---

## OOP and IP ranges supplied to solver

Python passes `oop_range=None`, `ip_range=None` into `solve_flop_tree`. Rust then calls `extract_ranges_from_flop_file(flop/HU_CX/4dQcQd, stack=12, …)`. **No `.json.z` tree exists for HU_CX on this flop**, so Rust falls back to built-in defaults (`solver_ext/src/lib.rs`).

These are the **actual range strings** passed into `PostFlopGame` for the measured baseline:

### OOP range (BB — hero)

```
66+,A8s+,A5s-A4s,AJo+,K9s+,KQo,QTs+,JTs,96s+,85s+,75s+,65s,54s
```

### IP range (BTN — villain)

```
QQ-22,AQs-A2s,ATo+,K5s+,KJo+,Q8s+,J8s+,T7s+,96s+,86s+,75s+,64s+,53s+
```

**Format:** PokerStove / PPT shorthand (parsed by `postflop_solver` range parser). Hero **KsKc** is included in both ranges (OOP: `66+`; IP: `QQ-22`).

### Why not compiled GTO combo ranges?

`flop/compiled/HU_CX` has path-aware combo weights at `P.c_P.k_F.QcQd4c_P.k` (~9947 / ~14111 chars), but **they were not used** at runtime: `extract_ranges_at_path(P.c_P.k_F.4dQcQd)` fails (path lacks flop check suffix `_P.k` because pot `6.16` > 4.5 BB heuristic). Re-solving with those GTO combo ranges changes the answer (check ~89% vs ~49% with defaults).

---

## `solve_flop_tree` parameters

```json
{
  "flop_dir": "flop",
  "hero_hand": "KsKc",
  "spot": "HU_CX",
  "stack": 12,
  "flop": "4dQcQd",
  "weip_flop": false,
  "oop_range": "66+,A8s+,A5s-A4s,AJo+,K9s+,KQo,QTs+,JTs,96s+,85s+,75s+,65s,54s",
  "ip_range": "QQ-22,AQs-A2s,ATo+,K5s+,KJo+,Q8s+,J8s+,T7s+,96s+,86s+,75s+,64s+,53s+",
  "bet_sizes": "50%, 75%, 100%, a",
  "max_iter": 200,
  "target_frac": 0.05,
  "use_donk": false,
  "use_compression": true,
  "turn_card_limit": 2
}
```

| Param | Meaning |
|-------|---------|
| `max_iter` | 200 CFR iterations per turn-card solve |
| `target_frac` | Stop at exploitability ≤ `starting_pot × 0.05` |
| `bet_sizes` | `"50%, 75%, 100%, a"` + `"2.5x"` raise sizing in tree |
| `turn_card_limit` | 2 turn cards per invocation (rjeans training-speed default) |

---

## Rust pseudocode

```rust
fn solve_turn_spot(session: &SolverSession) -> SolveResult {
    let t0 = Instant::now();

    let oop_range = "66+,A8s+,A5s-A4s,AJo+,K9s+,KQo,QTs+,JTs,96s+,85s+,75s+,65s,54s";
    let ip_range  = "QQ-22,AQs-A2s,ATo+,K5s+,KJo+,Q8s+,J8s+,T7s+,96s+,86s+,75s+,64s+,53s+";

    let cfg = SolverTrainConfig {
        max_iter: 200,
        target_frac: 0.05,
        bet_sizes_str: "50%, 75%, 100%, a".into(),
        use_donk: false,
        use_compression: true,
    };

    // For each sampled turn card: solve_and_collect(oop, ip, flop, turn, starting_pot, stack, ...)
    let samples = session.solve_flop_tree(
        "KsKc", "HU_CX", 12, "4dQcQd", false,
        Some(200), Some(0.05), Some("50%, 75%, 100%, a"),
        Some(false), Some(true), Some(2),
        Some(oop_range), Some(ip_range),
    )?;

    // pick_sample(street="turn", pot=6.16, call=0.0, tolerance=5.0)
    // sample_to_decisions → rank by score
    SolveResult { elapsed_ms: t0.elapsed().as_millis(), samples }
}
```

### `TrainingSample` fields (per hero decision node)

| Field | Notes |
|-------|-------|
| `action_probs[3]` | `[fold, call/check, raise]` |
| `raise_probs[4]` | `[50% pot, 75%, 100%, all_in]` conditional on raise |
| `pot_bb`, `call_cost_bb` | Node geometry for sample matching |

### Decision scoring (Python post-process)

- Check/call score = `action_probs[1]`
- Raise score = `action_probs[2] * raise_probs[i]`
- Raise sizes = `50% / 75% / 100% × query_pot_bb` (6.16) + all-in bucket

---

## Expected baseline (rjeans `solver_ext`, release build)

Measured with **DEFAULT ranges above**, `stack=12`, `turn_card_limit=2`.

| Metric | Value |
|--------|-------|
| `solve_elapsed_s` | **0.6345** (`stack=12`) / **0.3742** (`stack=5`) |
| `total_samples` | 2320 |
| `turn_samples` | 16 |

### Matched sample (query: turn, pot=6.16, call=0.0)

| Field | Value |
|-------|-------|
| `action_probs` | `[0.0, 0.489746, 0.510254]` |
| `raise_probs` | `[0.009277, 0.009277, 0.009277, 0.972168]` |

### Ranked decisions (TUI showed CHECK)

| Rank | Action | `raise_to_bb` | Score | UI |
|------|--------|---------------|-------|-----|
| 1 | `call` | — | **0.489746** | CHECK |
| 2 | `raise` | 6.16 | 0.482483 | BET pot |
| 3 | `raise` | 3.08 | 0.009277 | BET 50% |
| 4 | `raise` | 4.62 | 0.009277 | BET 75% |

---

## Re-run baseline (Python)

```bash
maturin develop --release -m solver_ext
```

```python
import time, solver_ext
from rjeans_policy.state import Street
from rjeans_tui.solver_decide import _find_matching_sample, _solver_sample_to_decisions

OOP = "66+,A8s+,A5s-A4s,AJo+,K9s+,KQo,QTs+,JTs,96s+,85s+,75s+,65s,54s"
IP  = "QQ-22,AQs-A2s,ATo+,K5s+,KJo+,Q8s+,J8s+,T7s+,96s+,86s+,75s+,64s+,53s+"

s = solver_ext.SolverSession("flop")
t0 = time.monotonic()
samples = s.solve_flop_tree(
    hero_hand="KsKc", spot="HU_CX", stack=12, flop="4dQcQd", weip_flop=False,
    bet_sizes="50%, 75%, 100%, a", turn_card_limit=2, max_iter=200, target_frac=0.05,
    oop_range=OOP, ip_range=IP,
)
print(f"elapsed={time.monotonic()-t0:.4f}s")
m = _find_matching_sample(samples, Street.TURN, 6.16, 0.0)
for d in _solver_sample_to_decisions(list(m.action_probs), list(m.raise_probs), 6.16, None, None, 0.0):
    print(d.action_type, d.raise_to_bb, round(d.score, 4))
```

---

## Comparison checklist

- [ ] Same OOP / IP range strings (PPT shorthand above)
- [ ] Same `stack` bucket (12 for TUI baseline)
- [ ] Same `bet_sizes`, `max_iter=200`, `target_frac=0.05`
- [ ] Report `solve_elapsed_ms` vs **634 ms** (stack 12)
- [ ] Top action vs **CHECK @ 0.4897**
