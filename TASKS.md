# TASKS

Companion to `PLAN.md`. Each task has an id, estimate, phase, dependencies,
and a short spec. Tasks are grouped by phase, ordered by dependency within
each phase.

**Current north star (2026-Q3):** production-ready **HU turn** decisions via
`rust_solver_py`, replacing rjeans `solver_ext` in the TUI under `RUST_SOLVER=1`.
**Phase 12 is the critical path.** Phases 1–9 remain deferred (3p, EMD, tiers).

### Next session — start here

1. **P12.1** Flop-entry solve (check-check flop → turn → hero node) in
   `python_api.rs` + `kk_turn_bench`
2. **P12.2** Tree param parity with `solver_ext` (bet sizes, raises, flop pot)
3. **P12.3** KK A/B vs rjeans — target check prob ≈0.49 ±0.10
4. See Phase 12 table below for full production roadmap

**Quick commands:**

```bash
OUT_DIR=target/release/deps cargo run --release --bin kk_turn_bench
OUT_DIR=target/release/deps cargo test --release --bin kk_turn_bench kk_turn_quality_gate -- --ignored
cd rust_solver_py && maturin develop --release   # from activated .venv
```

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
| P0.3 | Fix `kmeans::fit_growbatch` early-stop: remove trailing `break;`; add a test that exercises 5+ iterations on synthetic data | 120 | - | 0 | done |
| P0.4 | Strip dead commented-out code: `gen_ochs`, `gen_emd(turn/river, ...)` in `gen_abstraction/main.rs`; non-sampling `cfr()` path in `cfr.rs` (or refactor into a clean `FullCFR` module for Phase 6) | 60 | - | 2 | done |
| P0.5 | Smoke test: `train()` 10k iters asserts BR finite and `Infoset.regrets` mutates; verify `rust_poker 0.1.5` builds on current stable | 180 | P0.1, P0.3 | 0 | done |
| P0.5a | `scripts/setup.sh` for cmake + libclang-dev + build-essential (no longer required; build is now C-free) | 30 | - | 0 | done (obsolete) |
| P0.5b | **Superseded** by the C-removal work. The C `hand_indexer` library has been **stubbed out** in the vendored rust_poker; the solver builds in ~4s without cmake/libclang. See `vendor/rust_poker/README.md`. | 2400 | - | 3 | declined |
| P0.5c | Fix `rust_poker 0.1.5`'s `lazy_static!` thread-safety bug. Vendored fork at `vendor/rust_poker/`; both `lazy_static!` blocks (`CARDS` and `LOOKUP_TABLE`) replaced with `OnceLock`; 19 call sites updated to use `CARDS.get().expect("CARDS")[i]`; `init_cards()` exposed to pre-init the lookup tables from main. Wired up via `[patch.crates-io]`. | 240 | - | 0 | done |
| P0.5d | Port the C `hand_indexer` algorithm to pure Rust as a publishable `poker_canon` crate at `crates/poker_canon/`. **Status: paused** (the C removal made this non-blocking). In-progress code at `crates/poker_canon/src/{deck,hand_indexer}.rs` has type errors that need fixing before it'll compile. | 2400 | - | 3 | paused |

## Phase 0.5 - Benchmark harness (Jul 2026 session)

| ID | Title | Est (min) | Deps | Priority | Status |
|----|-------|-----------|------|----------|--------|
| P0.6 | KK turn A/B harness: `src/solver/benchmark/kk_turn.rs`, `kk_turn_bench` binary, `benchmarks/kk_turn_040229_prompt.md`, `run_kk_turn_compare.py`, results JSON | 360 | P0.5c | 0 | done |
| P0.6a | Turn-card sampling seed matches rjeans (`Kd`, `8s` for KsKc + 4dQcQd) | 60 | P0.6 | 0 | done |
| P0.6b | Expanded OOP/IP combos via postflop-solver → `benchmarks/kk_turn_expanded_combos.txt` | 60 | P0.6 | 0 | done |
| P0.6c | `MCCFRTrainer::collect_hero_samples` + ranked-decision export | 180 | P0.6 | 0 | done |
| P0.6d | Tree-builder fixes: `is_terminal()` river only after `bets_settled`; `street_closed()` when ≤1 player can bet | 120 | P0.6 | 0 | done |

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

| ID | Title | Est (min) | Deps | Priority | Status |
|----|-------|-----------|------|----------|--------|
| P4.1 | Atomic `Infoset` (Box<[AtomicI32]> regrets / strategy_sum); replace `unsafe` raw-pointer discount thread with `crossbeam::channel::bounded(1)` snapshot pattern; remove UB | 240 | phase-1 | 0 | done |
| P4.2 | `calc_br()` and `calc_ev()` per-player walkers; `abstract_br_infoset`, `abstract_ev_infoset`, `abstract_br_terminal`, `abstract_ev_terminal`; placeholder for SHOWDOWN/ALLIN leaves | 360 | P4.1 | 0 | done (placeholder) |
| P4.3 | `convergence.jsonl` writer: per-N iters emit `{iter, t_seconds, depth_tier_bb, n_players, ev, best_response, exploitability_mbb_per_hand, exploitability_max, memory_mb, n_threads, schema_version, stop_reason}`; 12-field v1.0 schema | 180 | P4.2 | 0 | done |
| P4.4 | CFR+ for 2p: regret floor at 0; weighted strategy_sum by iter_t is approximate (multiplier=1) — full weighted version is P9.8 | 180 | P4.1 | 2 | done (partial) |
| P4.5 | CLI flags `--max-iter`, `--target-mbb`, `--convergence-interval`, `--convergence-path`, `--cfr-plus` / `--no-cfr-plus`; `parse_cli()` in main; hand-rolled (no clap) | 120 | P4.3 | 2 | done |

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

## Phase 9 - Speed & precision for 15-25BB (priority 0)

Drives the 15-25BB use case: <1 minute per depth tier on the 16-core
desktop, exploitability `max(eps_i) <= 5 mbb/h`. Each task is small and
independent; do in this order for compounding gains.

| ID | Title | Est (min) | Deps | Priority |
|----|-------|-----------|------|----------|
| P9.1 | Real hand evaluation in terminal walker: wire `rust_poker::hand_evaluator::Evaluator` into `abstract_br_terminal` / `abstract_ev_terminal`; thread `Hand` (both players' hole cards + 5 board cards) into the walker; ALLIN leaves keep precomputed `tn.value`; 2-3 unit tests with known hand ranks | 1440 | phase-1, P4.2 | 0 |
| P9.2 | Per-bucket reach: add `ICardAbstraction::bucket_count(player) -> usize`; change `initial_reach()` from `vec![vec![1.0]; n_players]` to `vec![vec![1.0 / bucket_count(p)]; n_players]`; verify `calc_br`/`calc_ev` divide by reach correctly | 240 | P9.1 | 0 |
| P9.3 | `rayon` parallel MCCFR walker: `use rayon::prelude::*`; `par_iter()` over leaf batch; verify 16-core scaling >80% | 60 | P9.1 | 0 | done |
| P9.4 | Pre-built abstract game tree cache: build full tree once at trainer init; serialize `game_tree.bin`; MCCFR iterates pre-built `Vec<NodeId>` | 240 | P9.1 | 0 |
| P9.5 | External-sampling MCCFR (Lanctot et al. 2009): one player is the regret-updating player, opponents' actions sampled from their strategy, all players updated in a single walk | 480 | P9.3 | 0 | done |
| P9.6 | Action abstraction presets: `Options::action_sizes: HashMap<Round, Vec<f32>>`; Flop `[0.33, 0.5, 0.75, 1.0, all-in]`; Turn/River `[0.5, 0.75, 1.0, 1.5, 2.0, all-in]` | 240 | phase-1 | 1 |
| P9.7 | 2p benchmark suite incl. KK turn spot (`kk_turn_bench`); timing + strategy quality gates | 240 | P9.5 | 0 | partial (timing only; quality gate open) |
| P9.8 | CFR+ weighted strategy_sum by iter_t (proper version); thread the iteration counter through `mccfr`; verify strategy_sum weights are correct (current P4.4 is multiplier=1) | 180 | P4.4, P9.3 | 1 |
| P9.9 | Unit test for CFR+ regret-floor-at-zero behavior (verifies `floor_regrets_at_zero()` actually clamps to 0) | 60 | P4.4 | 1 |
| P9.10 | Bench harness: `cargo bench --bench mccfr` with criterion; per-tier throughput (iters/sec) and exploitability curve | 240 | P9.7 | 2 |

## Checkpoint: 15-25BB <1 minute per depth tier (Phase 9.5)

After Phase 9.5, the 2p postflop solver must hit:
- 15BB: `exploitability_max < 50 mbb/h` in <10 seconds
- 25BB: `exploitability_max < 5 mbb/h` in <30 seconds
- 3p 25BB: `exploitability_max < 10 mbb/h` in <3 minutes

If 10x off, the per-iteration profile (regret-update vs strategy-sample
vs leaf-eval) is the debug target.

---

## Phase 10 - Runtime decision quality

| ID | Title | Est (min) | Deps | Priority | Status |
|----|-------|-----------|------|----------|--------|
| P10.1 | Real hand eval in BR/EV terminal walker; AA vs 22, AKs vs 72o unit tests | 1440 | P4.2 | 0 | done |
| P10.2 | Per-bucket reach normalization; trustworthy `exploitability_max` plumbing | 240 | P10.1 | 0 | done (scale: see P12.4) |
| P10.3 | Hero-exact strategy: `pin_hero`, `query_strategy()` at extract | 480 | P10.1 | 0 | done |
| P10.4 | Convergence stop: `time_budget_ms` + `target_mbb` on `TrainConfig` | 180 | P10.2 | 0 | done |
| P10.5 | Flop-entry solve + turn-card sampling; match TUI action paths | 360 | P0.6 | 0 | partial (infra; prod uses turn-entry @ 2 BB) |
| P10.6 | PPT hyphen range import; replace combo-file fallback when sparse | 240 | P0.6b | 1 | partial |
| P10.7 | KK quality gate v1: non-uniform + geometry + &lt;500 ms | 120 | P10.3 | 0 | done |
| P10.8 | KK quality gate v2: parity ±0.10 + exploitability &lt;50 mbb/h | 120 | P12.4 | 0 | partial (parity passes) |

## Phase 11 - Python library `rust_solver_py`

| ID | Title | Est (min) | Deps | Priority | Status |
|----|-------|-----------|------|----------|--------|
| P11.1 | PyO3 crate + `pyproject.toml` + `rust_solver_py/README.md` | 240 | P10.7 | 0 | done |
| P11.2 | `solve_turn_decision(...)` one-shot API | 360 | P11.1, P10.4 | 0 | done |
| P11.3 | `TrainingSample` + `SolverSession.solve_flop_tree` compat | 480 | P11.2 | 1 | done |
| P11.4 | Session-scoped config cache (ranges, stack, tree params) | 120 | P11.2 | 2 | open |
| P11.5 | uv integration test: import, KK spot, assert gates | 120 | P11.2 | 0 | partial (`scripts/run_quality_gates.sh`) |
| P11.6 | rjeans TUI `RUST_SOLVER=1` swap-in (staging only until P12) | 240 | P11.3, P12.6 | 2 | open |

## Phase 12 - Production readiness: HU turn solving (**priority 0**)

Production = G1–G5 in `PLAN.md` Phase 12 (parity, geometry, speed, exploitability, Python UX).

| ID | Title | Est (min) | Deps | Priority |
|----|-------|-----------|------|----------|
| P12.1 | Flop-entry solve: flop pot 2 BB, check-check → turn → query node; wire `python_api` + `kk_turn` | 2400 | P10.5 | 0 | partial |
| P12.2 | Tree param parity with `solver_ext` (bet 50/75/100%, raise 2.5x, max_raises, all-in threshold) | 480 | P12.1 | 0 | done |
| P12.3 | KK A/B vs rjeans: top action + check prob within ±0.10 of baseline | 360 | P12.1, P12.2 | 0 | done |
| P12.4 | Fix exploitability scale on turn trees; enable `target_mbb` stop | 1440 | P10.2 | 0 | done |
| P12.5 | Multi-spot HU turn benchmark suite (≥5 TUI spots) + JSON diff | 720 | P12.3 | 0 | done |
| P12.6 | CI gates: parity (G1), exploitability (G4) on suite | 360 | P12.5, P12.4 | 0 | done |
| P12.7 | `RUST_SOLVER=1` in rjeans TUI (staging); document rollback | 240 | P12.6, P11.3 | 1 | done |
| P12.8 | Production sign-off: README + PLAN gate closed for HU turn | 120 | P12.6, P12.7 | 0 |

---

## Cross-phase dependency summary

```
Phase 0 (no deps) ──> P0.6 benchmark harness (done)
   |
   v
Phase 10 (v1 done) ──> Phase 11 (scaffolded) ──> Phase 12 (production) ── GATE
   ^
   | (P10.5 flop-entry ──> P12.1)
   |
Phase 1
   |
   +--> Phase 2 ... Phase 9 (3p, EMD, tier sweep — deferred)
```

Legacy path (3p product):

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
   +--> Phase 4 (atomic Infoset, BR/EV walkers, convergence, CFR+)
           |
           v
         Phase 5 (memory, threading, OOM guards)
           |
           v
         Phase 6 (3p blueprint + safe search)
           |
           v
         Phase 7 (per-tier runner, validation)
           |
           v
         Phase 9 (speed & precision for 15-25BB)

Phase 8 (independent; feeds runtime via Options::preflop_ranges)
```

## Total task count

| Phase | Tasks | Sum estimate (min) |
|-------|-------|--------------------|
| 0     | 9     | 3450 (incl. paused P0.5d at 2400) |
| 1     | 7     | 1200               |
| 1-prep| 1     | 180                |
| 2     | 6     | 1080               |
| 3     | 3     | 420                |
| 4     | 5     | 1080 (done)        |
| 5     | 4     | 660                |
| 6     | 5     | 1740               |
| 7     | 5     | 1320               |
| 8     | 2     | 420                |
| 9     | 10    | 3420               |
| 0.5   | 4     | 780                |
| 10    | 8     | 3180               |
| 11    | 6     | 1560               |
| 12    | 8     | 6120               |
| **Total** | **82** | **~26220 min (~43.7 work-weeks at 10h/week)** |

Note: **Phase 12 is priority 0** for production HU turn solving. Phases 1–9
deferred. P10.7 v1 done; v2 (exploitability) moves to P10.8 / P12.4.

## Performance budget (2026-Q3 revision)

**Production target (Phase 12):** HU turn spots in **&lt;500 ms** with
**rjeans parity** (top action match ≥80% on suite; key probs ±0.10) and
**exploitability_max &lt;50 mbb/h**.

**Current (Jul 2026):** ~110 ms KK spot, check **0.614** vs solver_ext **0.602** (pot=2 BB,
call=0); exploitability scale still broken.

**15-25BB tier target (unchanged):** Phase 9 speed work after Phase 12
production gate for HU turn.

## Why this file and not `bd`

`bd` (embedded Dolt) was unstable in this environment during setup
(lock contention, intermittent "no beads database found" errors after a
successful create). The task list is fully captured here, can be diffed
in `git`, and is the canonical source of truth. If a stable `bd` backend
is restored later, the table in this file can be imported row-by-row
(`bd create` with `--title`, `--parent`, `--estimate`, `--priority`,
`--labels`).
