# TASKS

Companion to `PLAN.md`. Each task has an id, estimate, phase, dependencies,
and a short spec. Tasks are grouped by phase, ordered by dependency within
each phase.

Estimates are in **minutes** (matching `bd create --estimate` semantics so
this can be re-imported into a tracker later).

## Legend

- **Estimate**: minutes
- **Deps**: task ids that must be closed first (within this file)
- **Phase dep**: phase-epic dependency (cross-phase blocker)
- **Priority**: 0 (highest) - 4 (lowest), matching `bd` convention

---

## Phase 0 - Hygiene

| ID | Title | Est (min) | Deps | Priority | Status |
|----|-------|-----------|------|----------|--------|
| P0.1 | Remove `extern crate cortex_m` and unused nightly features from `src/solver/main.rs`; verify `cargo build --release` | 60 | - | 0 | done |
| P0.2 | Resolve `src/solver/actions.rs`: move types from `action_abstraction.rs` into it (or delete the file) | 60 | - | 2 | done |
| P0.3 | Fix `kmeans::fit_growbatch` early-stop: remove trailing `break;`; add a test that exercises 5+ iterations on synthetic data | 120 | - | 0 | pending |
| P0.4 | Strip dead commented-out code: `gen_ochs`, `gen_emd(turn/river, ...)` in `gen_abstraction/main.rs`; non-sampling `cfr()` path in `cfr.rs` (or refactor into a clean `FullCFR` module for Phase 6) | 60 | - | 2 | pending |
| P0.5 | Smoke test: `train()` 10k iters asserts BR finite and `Infoset.regrets` mutates; verify `rust_poker 0.1.5` builds on current stable | 180 | P0.1, P0.3 | 0 | done |
| P0.5a | `scripts/setup.sh` for cmake + libclang-dev + build-essential | 30 | - | 0 | done |
| P0.5b | (Optional, alternative) Migrate solver from `rust_poker 0.1.5` to `rust_poker 0.1.14`. Pure Rust, but the public API was completely redesigned; call sites in 6 files need rewriting. **Recommendation:** not worth it; P0.5c is the better path. | 2400 | - | 3 | optional (declined) |
| P0.5c | Fix `rust_poker 0.1.5`'s `lazy_static!` thread-safety bug. Vendored fork at `vendor/rust_poker/`; both `lazy_static!` blocks (`CARDS` and `LOOKUP_TABLE`) replaced with `OnceLock`; 19 call sites updated to use `CARDS.get().expect("CARDS")[i]`; `init_cards()` exposed to pre-init the lookup tables from main. Wired up via `[patch.crates-io]`. | 240 | - | 0 | done |
| P0.5b | (Optional, alternative) Migrate solver from `rust_poker 0.1.5` to `rust_poker 0.1.14`. 0.1.14 is pure Rust (no cmake/libclang) but its public API was completely redesigned; call sites in 6 files need rewriting. **Effort:** ~3-5 days. **Recommendation:** not worth it; install `libclang-dev` instead. | 2400 | - | 3 | optional |

## Phase 1 - N-player state, no Preflop, stack-cap

| ID | Title | Est (min) | Deps | Priority |
|----|-------|-----------|------|----------|
| P1.1 | `GameState -> Vec<PlayerState>`; `MAX_PLAYERS = 3` in `constants.rs`; existing 2p tests still pass | 240 | phase-0 | 0 |
| P1.2 | Replace every `1 - current` with `next_active_player(current, &players)` skipping folded/acted | 120 | P1.1 | 0 |
| P1.3 | `apply_action` raise/call/fold: loop over active players; **cap wager at opponent's stack**; remove "TODO if more than two players" | 240 | P1.2 | 0 |
| P1.4 | `Options`: add `depth_tier_bb: u32` (replaces `stack`); add `preflop_ranges: [HandRange; 3]` stub; add `postflop_pot_override: Option<u32>`; add `rake: Option<(f64, u32)>` | 180 | P1.1 | 0 |
| P1.5 | `state.rs` reads `Options::max_raises` and `Options::all_in_threshold` (currently dead fields) | 60 | P1.4 | 0 |
| P1.6 | `TerminalNode::value` SHOWDOWN/ALLIN: N-player min-wager showdown (`sum_{j != i} min(my_wager, opp_wager)`); subtract rake | 180 | P1.3 | 0 |
| P1.7 | 2p regression tests: 4 hand scenarios, assert exact pot outcomes | 180 | P1.6 | 0 |

## Phase 2 - Wire abstraction pipeline

| ID | Title | Est (min) | Deps | Priority |
|----|-------|-----------|------|----------|
| P2.1 | Re-enable turn + river EMD gen in `gen_abstraction/main.rs` (replace hard-coded `round=1` with CLI flag); verify each round | 240 | phase-1 | 0 |
| P2.2 | Verify `fit_growbatch` (from P0.3) on real flop 1.29M-histogram dataset; document chosen `stop_threshold` and restarts | 120 | P0.3, P2.1 | 2 |
| P2.3 | Re-enable `EMD::Flop` and `EMD::Turn` construction in `solver/card_abstraction.rs::MCCFRTrainer::init`; add unit test for known hand -> known cluster | 180 | P2.1 | 0 |
| P2.4 | Bucket counts: Flop=200, Turn=150, River=50; document the tradeoff; make configurable via CLI | 60 | P2.3 | 2 |
| P2.5 | OCHS on flop entry: cluster opponent's preflop distribution (169 hands) into 8 buckets; turn/river use plain EMD on flop-entry bucket | 360 | P2.4 | 2 |
| P2.6 | `cargo bench` for infoset construction time across {1p, 2p, 3p} x {flop-only, full postflop}; save baseline | 120 | P2.3 | 3 |

## Phase 3 - Action abstraction expansion

| ID | Title | Est (min) | Deps | Priority |
|----|-------|-----------|------|----------|
| P3.1 | Per-street action size presets (Flop bet `[0.33,0.5,0.75,1.0]`, raise `[2.0,3.0]`; Turn bet `[0.5,0.75,1.0,1.5]`, raise `[2.0,2.5,3.0]`; River bet `[0.5,0.75,1.0,1.5,2.0]`, raise `[2.0,3.0]`; max 3 raises) | 120 | phase-1 | 2 |
| P3.2 | Verify `state.rs` reads `Options::max_raises` and `Options::all_in_threshold`; remove global constants as the source of truth | 60 | P1.5, P3.1 | 1 |
| P3.3 | `Options::max_action_sequences_per_street: u32` (default 200); in `tree_builder`, prune leaf actions when node would exceed the cap; verify tree-builder correctness | 240 | P3.2 | 2 |

| P4.prep | `convergence.json` schema (1.0) in `docs/convergence_schema.md`; `convergence::Sample` + `convergence::Recorder` in `cfr.rs`; 2 unit tests for the recorder | 180 | P1.6 | 0 | done |

## Phase 4 - Exploitability, BR, CFR+

| ID | Title | Est (min) | Deps | Priority |
|----|-------|-----------|------|----------|
| P4.1 | Replace `unsafe` raw-pointer discount thread with `crossbeam::channel::bounded(1)` snapshot pattern; remove UB | 240 | phase-1 | 0 |
| P4.2 | `br_2p` and `br_3p` modules: per-player BR; `br_2p` returns `(ev0, ev1)`, `br_3p` returns `(br0, br1, br2)`; tested against a known toy tree | 360 | P4.1 | 0 |
| P4.3 | `convergence.json` writer: per-1% emit `{iter, t_seconds, depth_tier_bb, n_players, ev, best_response, exploitability_mbb_per_hand, exploitability_max, memory_mb, n_threads}`; schema in `PLAN.md` | 180 | P4.2 | 0 |
| P4.4 | CFR+ for 2p: regret floor at 0, weighted by iteration count; verify 2-5x speedup on turn-river scenario | 180 | P4.1 | 2 |
| P4.5 | CLI flags `--target-exploitability-mbb`, `--max-iter`; stop when `max(eps_i) <= target` OR `iter >= max-iter`; emit final `convergence.json` with `stop_reason` | 120 | P4.3 | 2 |

## Phase 5 - Threading & memory

| ID | Title | Est (min) | Deps | Priority |
|----|-------|-----------|------|----------|
| P5.1 | 16-thread scaling benchmark: run trainer at 1/2/4/8/16 threads on a fixed scenario; report iter/sec, scaling efficiency; target >80% 1->16 | 240 | P4.4 | 2 |
| P5.2 | Sparse `Infoset` allocation: `Box<[i32]>` -> `Option<Box<[i32]>>` allocated on first regret write; verify 2p behaviour unchanged | 240 | P4.1 | 0 |
| P5.3 | `InfosetTable::total_bytes()`: walk the table, sum allocations, return byte count; log in `convergence.json` | 60 | P5.2, P4.3 | 2 |
| P5.4 | Configurable memory budget: `Options::memory_budget_mb: u32` (default 8 GB); refuse allocation beyond; emit warning + `stop_reason=oom` | 120 | P5.2 | 2 |

## Checkpoint: 2p postflop <= 5 mbb/h on 20bb turn-river

After Phase 5, the 2p postflop solver must hit `exploitability_max < 5
mbb/h` in <4 hours on the 20bb turn-river scenario. If 10x off, stop and
debug before Phase 6.

## Phase 6 - 3p blueprint + safe search

| ID | Title | Est (min) | Deps | Priority |
|----|-------|-----------|------|----------|
| P6.1 | 3p reach-propagation test: construct a tiny 3p tree, run `cfr()`/`FullCFR`, assert `sum(ev) == 0` and per-player reaches correct | 180 | phase-5, P4.2 | 0 |
| P6.2 | N-player MCCFR reach propagation: `cfr_reach_p * product(sample_reach_opp_j)` for `j != p`; verify 2p unchanged | 360 | P6.1 | 0 |
| P6.3 | `Subgame { hand, board, depth_l, blueprint_strategy }`; `solve_subgame` uses `FullCFR` (exhaustive, non-sampling) with target exploitability <1 mbb within subgame | 480 | P6.2 | 0 |
| P6.4 | LRU `HashMap<SubgameKey, LocalStrategy>` cache; key: `(hand_bucket, board, action_history_prefix, depth_l)`; default size 10000 | 240 | P6.3 | 2 |
| P6.5 | Runtime decision: lookup blueprint at subtree root, replace opponent strategies with blueprint, solve subtree, play first action; measure 200ms-2s target latency on 1 core | 480 | P6.4 | 0 |

## Phase 7 - Per-tier runner + validation

| ID | Title | Est (min) | Deps | Priority |
|----|-------|-----------|------|----------|
| P7.1 | `bin/solve_tier.rs`: CLI `--depth-bb 20 --out strategy_t20.bin`; drives abstraction gen (cached) + train; emits `convergence.json` + final strategy | 360 | phase-6, P4.5 | 0 |
| P7.2 | Cached abstraction regen: skip `*_emd.dat` regen if file exists and is newer than source; hash-based invalidation | 120 | P7.1, P2.1 | 2 |
| P7.3 | `bin/play_2p.rs` and `bin/play_3p.rs`: load serialized strategy, simulate hands against opponents; `--vs-rand` mode | 360 | P7.1 | 2 |
| P7.4 | `tests/integration/turn_river_2p.rs`: known ACPC-style scenario, assert `exploitability_max < 50 mbb/h` after 1M iters; the bar is loose, the goal is "not broken" | 240 | P7.1 | 0 |
| P7.5 | `tests/integration/asymmetric_3p_smoke.rs`: tiny 3p tree at depths [20, 5, 12], 10k iters, assert all `br_i` finite, `exploitability_max < 5000 mbb/h` | 240 | P7.4 | 0 |

## Phase 8 - Preflop range files

| ID | Title | Est (min) | Deps | Priority |
|----|-------|-----------|------|----------|
| P8.1 | `ranges/BTN_vs_SB_BB_3p.json`: 169-hand distribution per position for 3p; document source (GTO+ data, manual, or hand-history mining) | 240 | - | 3 |
| P8.2 | `ranges/BB_defend.json` + `load_ranges(path)` wired into `Options::preflop_ranges` | 180 | P8.1 | 3 |

---

## Cross-phase dependency summary

```
Phase 0 (no deps)
   |
   v
Phase 1
   |
   +--> Phase 2
   |       |
   |       v
   |     Phase 3 (also direct from Phase 1)
   |
   +--> Phase 3
   |       |
   |       v
   +--> Phase 4
           |
           v
         Phase 5
           |
           v
         Phase 6
           |
           v
         Phase 7
           
Phase 8 (independent; feeds runtime via Options::preflop_ranges)
```

## Total task count

| Phase | Tasks | Sum estimate (min) |
|-------|-------|--------------------|
| 0     | 8     | 1050 (plus optional 2400 if P0.5b chosen) |
| 1     | 7     | 1200               |
| 1-prep| 1     | 180                |
| 2     | 6     | 1080               |
| 3     | 3     | 420                |
| 4     | 5     | 1080               |
| 5     | 4     | 660                |
| 6     | 5     | 1740               |
| 7     | 5     | 1320               |
| 8     | 2     | 420                |
| **Total** | **46** | **~8880 min (~14.8 work-weeks at 10h/week)** |

## Why this file and not `bd`

`bd` (embedded Dolt) was unstable in this environment during setup
(lock contention, intermittent "no beads database found" errors after a
successful create). The task list is fully captured here, can be diffed
in `git`, and is the canonical source of truth. If a stable `bd` backend
is restored later, the table in this file can be imported row-by-row
(`bd create` with `--title`, `--parent`, `--estimate`, `--priority`,
`--labels`).
