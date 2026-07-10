# RustSolver — postflop MCCFR solver + Python runtime

## Goal (2026-Q3 revision — **current north star**)

Ship a **fast, low-exploitability** postflop solver as a **Python library**
(`rust_solver_py` via PyO3 + maturin) that replaces rjeans `solver_ext` in
the TUI/policy stack. **We do not need parity with rjeans** (PostFlopGame
CFR); we need **winning, hard-to-exploit decisions under a time budget**.

| Priority | Target |
|----------|--------|
| 1 | **Decision quality** — converged strategy at query time, not uniform fallbacks |
| 2 | **Speed** — beat rjeans wall-clock on real spots (KK turn: **~11×** already) |
| 3 | **Python UX** — `uv` venv + `maturin develop`; drop-in for `solver_decide` |
| 4 | 3p / tier sweep / EMD abstraction — **after** 2p HU runtime is trustworthy |

**Not ready for production Python yet** (Jul 2026): benchmark shows ~uniform
action probs even at 20k MCCFR iters; exploitability stop is not trustworthy
(SHOWDOWN placeholder in BR walker). See Phase 10 gate below.

Companion task tracker: `TASKS.md`.

### Session checkpoint (Jul 2026)

- Added KK turn A/B harness: `kk_turn_bench`, `benchmarks/run_kk_turn_compare.py`
- vs rjeans: **62 ms vs 711 ms**, but rust_solver top action ~uniform (0.33)
- Fixed tree-builder blockers: `is_terminal()` (river after betting settles),
  `street_closed()` (≤1 player can still bet)
- Expanded OOP/IP ranges via postflop-solver → `benchmarks/kk_turn_expanded_combos.txt`

---

## Long-term goal (unchanged)

Extend to **3 players**, discrete depth tiers {5, 8, 10, 12, 15, 18, 20, 25}
BB, fixed preflop ranges, EMD abstraction, 3-vector exploitability. Side pots
use **stack-cap** convention. Phases 1–9 below remain valid; **Phase 10–11
are now the critical path** for the Python product.

## Performance target (2026-Q2 revision)

The 15-25BB use case is the dominant target: postflop-only heads-up NLHE
at depths where 50-80% of terminal nodes are SHOWDOWN. The converged
strategy must be available in **<1 minute per depth tier** on the 16-core
desktop, with exploitability `max(eps_i) <= 5 mbb/h`. This drives a new
focus on **per-iteration speed** in addition to the original
exploitability convergence goal.

Phase 9 covers speed work; **Phase 10** covers decision quality required
before any Python ship.

The bd (beads) tracker was abandoned
mid-setup because the embedded Dolt backend in this environment was
unstable (lock contention, intermittent "no beads database found" errors).
The work is fully captured in `TASKS.md` and this plan.

## Hardware / environment

- 1x 16-core desktop
- No GPU
- Rust 2021. Verify `rust_poker 0.1.5` still resolves; if not, swap to
  `poker_eval_rs` or a minimal in-house evaluator (see Phase 0).

## Architectural decisions (locked)

| #   | Decision |
|-----|----------|
| D1  | Players: 2 or 3 (compile/runtime selectable; MAX_PLAYERS = 3) |
| D2  | Streets: postflop only (no Preflop enum) |
| D3  | Preflop ranges: fixed, loaded from `ranges/*.json` |
| D4  | Stack depth: discrete tiers {5, 8, 10, 12, 15, 18, 20, 25} BB; one solver per tier |
| D5  | Side pots: stack-cap approximation (wager clipped to opponent stack) |
| D6  | Card abstraction: EMD flop + turn + river; OCHS on flop entry; ISOMORPHIC preflop |
| D7  | Action abstraction: 4 bet sizes + 2 raise sizes per round, max 3 raises per street |
| D8  | Algorithm: external-sampling MCCFR + linear CFR discount; CFR+ for 2p |
| D9  | 3p: blueprint MCCFR + safe-search at runtime |
| D10 | Exploitability: per-player BR, vector output, report `max(eps_i)` |
| D11 | Rake: configurable (fraction + cap), default 0 |
| D12 | Unsafe discount thread replaced with `crossbeam::channel` round-trip |
| D13 | Sparse infoset allocation (only allocate boxes on first write) |
| D14 | State persistence: `bincode` format for strategies + `convergence.json` for telemetry |
| D15 | **Python runtime**: PyO3 crate `rust_solver_py`; build with maturin; consume via `uv` venv |
| D16 | **Quality gate**: no Python release until benchmark spots pass exploitability + non-uniform strategy checks |
| D17 | **Hero-exact query**: return strategy for the queried combo, not only abstract-bucket average |

## Phases

### Phase 0 - Hygiene (2-3 days)

Remove things that block further work, no design changes.

- [x] Remove `extern crate cortex_m` from `src/solver/main.rs`
- [x] Remove unused nightly feature gates: `generators`, `generator_trait`,
      `box_into_pin`, `box_syntax`, `feature(test)`
- [x] Either implement `src/solver/actions.rs` or delete the file
- [x] Fix `kmeans::fit_growbatch` early-stop (remove the `break`; let
      `min_change > stop_threshold` actually run)
- [x] Strip dead/commented-out code: `gen_ochs`, `gen_emd(turn, ...)`,
      `gen_emd(river, ...)` calls in `gen_abstraction/main.rs`; the
      non-sampling `cfr()` path in `cfr.rs` (or refactor into a clean
      `FullCFR` module for Phase 6 safe-search)
- [x] Add a smoke test: `train()` for 10k iters asserts BR values are
      finite and the regret table mutates
- [x] Verify `rust_poker 0.1.5` still builds; if not, evaluate replacement
      (`poker_eval_rs`, `poker`).
      **Status (post-C-removal):** The C `hand_indexer` library has been
      **removed** from the vendored fork. `build.rs` is now a no-op;
      `Cargo.toml` no longer has `bindgen`/`cmake` build-deps; the C
      source under `hand_indexer/` has been deleted; and
      `src/hand_indexer.rs` is a pure-Rust stub. The full solver
      builds in ~4s without cmake or libclang. The
      `scripts/setup.sh` script is preserved (in case it's needed
      for a future deck) but is no longer required.
      The full hand-indexing algorithm is being reimplemented in
      `crates/poker_canon/` (in progress, paused).

### Phase 1 - N-player state, no Preflop, stack-cap (1-1.5 weeks)

- [ ] `MAX_PLAYERS = 3` (constants.rs)
- [ ] `PlayerState { stack, wager, has_folded, has_acted_this_street }`
- [ ] `GameState { num_players, players: Vec<PlayerState> }` (no fixed array)
- [ ] Replace every `1 - current` with `next_active_player(current, &players)`
      skipping folded/acted
- [ ] `apply_action` raise/call/fold: loop over active players; **cap wager
      at opponent's stack**
- [ ] `BettingRound` stays `Flop | Turn | River` (no Preflop)
- [ ] `Options::depth_tier_bb: u32` replaces `stack: u32`
- [ ] `Options::preflop_ranges: [HandRange; 3]` loaded from `ranges/*.json`
- [ ] `Options::postflop_pot_override: Option<u32>` (default 1.5 BB)
- [ ] `Options::rake: Option<(f64, u32)>` (default None)
- [ ] `Options::max_raises: u8` and `Options::all_in_threshold: f64` are
      actually read by `state.rs` (currently dead fields)
- [ ] `TerminalNode::value` SHOWDOWN/ALLIN uses min-wager convention:
      player i wins `sum_{j != i} min(my_wager, opp_wager)`; subtract rake
- [ ] Existing 2p test cases must still pass

### Phase 2 - Wire abstraction pipeline (1-1.5 weeks)

- [ ] Re-enable turn + river EMD generation in `gen_abstraction/main.rs`
      (currently hard-coded `round=1`); parameterize via CLI flag
- [ ] `kmeans::fit_growbatch` (fixed in Phase 0) re-tested on flop dataset
- [ ] Re-enable `EMD::Flop` and `EMD::Turn` construction in
      `solver/card_abstraction.rs::MCCFRTrainer::init` (currently
      commented out)
- [ ] Add unit test: tiny synthetic dataset -> known hand maps to known
      cluster
- [ ] Bucket counts: Flop=200, Turn=150, River=50
- [ ] OCHS on flop entry: cluster opponent's preflop distribution into 8
      buckets; turn/river use plain EMD on flop-entry bucket
- [ ] Add a small `cargo bench` for infoset construction time

### Phase 3 - Action abstraction expansion (0.5-1 week)

- [ ] Defaults per street:
  - Flop: bet `[0.33, 0.5, 0.75, 1.0]`, raise `[2.0, 3.0]`, max 3 raises
  - Turn: bet `[0.5, 0.75, 1.0, 1.5]`, raise `[2.0, 2.5, 3.0]`, max 3 raises
  - River: bet `[0.5, 0.75, 1.0, 1.5, 2.0]`, raise `[2.0, 3.0]`, max 3 raises
- [ ] Wire `Options::max_raises` and `Options::all_in_threshold` to
      `state.rs` (overlaps with P1.5; verify when both land)
- [ ] Add `Options::max_action_sequences_per_street: u32` (default 200) to
      cap tree width in 3p

### Phase 4 - Exploitability, BR, CFR+ (1.5-2 weeks)

- [ ] Replace `unsafe` raw-pointer discount thread with
      `crossbeam::channel::bounded` snapshot pattern (no UB)
- [ ] Add `br_2p` and `br_3p` modules: per-player best response
- [ ] `convergence.json` writer with the schema in
      `docs/convergence_schema.md`
- [ ] Add CFR+ for the 2p path: regret floor at 0, weighted by iteration
      count
- [ ] Add `--target-exploitability-mbb` and `--max-iter` flags; stop when
      either is met
- [ ] In 3p, sum of `ev[i]` should be 0 (constant-sum convention)

### Phase 5 - Threading & memory (1-1.5 weeks)

- [ ] Benchmark at 1/2/4/8/16 threads; report scaling efficiency
- [ ] Switch `Infoset` to sparse allocation: only allocate `regrets` and
      `strategy_sum` boxes on first write
- [ ] Add `InfosetTable::total_bytes()` and log in `convergence.json`
- [ ] Configurable memory budget (default 8 GB); refuse to allocate beyond

### Checkpoint: 2p postflop <= 5 mbb/h

After Phase 5, we should be able to:
- Train a 2p turn-river solver for depth 20 BB
- Hit `exploitability_max < 5 mbb/h` in <4 hours
- Run the 4 most-important tiers {5, 10, 20, 25} in series

If the checkpoint fails by >10x, stop and debug before Phase 6.

## Checkpoint: 15-25BB <1 minute per depth tier (Phase 9, post-P9.5)

After Phase 9.5, we should be able to:
- Train a 2p 15BB postflop solver to <50 mbb/h in <10 seconds
- Train a 2p 25BB postflop solver to <5 mbb/h in <30 seconds
- Train a 3p 25BB postflop solver to <10 mbb/h in <3 minutes
- Run all 8 tiers {5, 8, 10, 12, 15, 18, 20, 25} in <10 minutes total

If the checkpoint fails by >10x, the per-iteration profile is the
debug target: regret-update vs. strategy-sample vs. leaf-eval.

### Phase 6 - 3p blueprint + safe search (2.5-3.5 weeks)

- [ ] 3p reach-propagation test (write before implementation)
- [ ] External-sampling MCCFR generalized to N players: track
      `cfr_reach_p * product(sample_reach_opp_j)` for `j != p`
- [ ] `Subgame { hand, board, depth_l, blueprint_strategy }` struct
- [ ] `solve_subgame(subgame) -> LocalStrategy`: exhaustive (non-sampling)
      CFR with full traversal, exploitability target <1 mbb within subgame
- [ ] LRU `HashMap<SubgameKey, LocalStrategy>` cache
- [ ] Runtime decision: blueprint at subtree root, replace opponents with
      blueprint, solve subtree, play first action
- [ ] 3-vector exploitability from per-player BR (already in Phase 4)

### Phase 7 - Per-tier runner + validation (1-1.5 weeks)

- [ ] `bin/solve_tier.rs`: `--depth-bb 20 --out strategy_t20.bin`
- [ ] Cache `*_emd.dat`; skip regeneration if newer than source
- [ ] `bin/play_2p.rs` and `bin/play_3p.rs`: load strategy, play out hands
- [ ] `tests/integration/turn_river_2p.rs`: known ACPC-style scenario;
      assert `exploitability_max < 50 mbb/h` after 1M iters
- [ ] `tests/integration/asymmetric_3p_smoke.rs`: tiny 3p tree to 10k
      iters; assert all `br_i` finite, `exploitability_max < 5000 mbb/h`
- [ ] `bincode` serialization for strategies

### Phase 8 - Preflop range files (0.5 week)

- [ ] `ranges/BTN_vs_SB_BB_3p.json`: 169-hand distribution per position
- [ ] `ranges/BB_defend.json`: defending distribution when BB acts first
- [ ] Loader: `Options::preflop_ranges = load_ranges(path)`

### Phase 9 - Speed & precision for 15-25BB (1-2 weeks, **priority 0**)

Drives the 15-25BB use case. Each task is small and independent; do in
this order for compounding gains.

- [ ] **P9.1** Real hand evaluation in terminal walker (Phase 5 work,
      2-3 days). Wire `rust_poker::hand_evaluator::Evaluator` into
      `abstract_br_terminal` / `abstract_ev_terminal` so SHOWDOWN leaves
      return the actual hand-vs-hand winner. Thread `Hand` (both players'
      hole cards + 5 board cards) into the terminal walker. ALLIN leaves
      keep the precomputed `tn.value` path (correct by construction). Add
      2-3 unit tests using known hand ranks (AA vs 22 = AA wins, AKs vs
      72o = AKs wins, etc.).
- [ ] **P9.2** Per-bucket reach (1 day). Add
      `ICardAbstraction::bucket_count(player) -> usize`. The stub's
      `hand_indexer_s::size()` returns 12888 for flop, 54912 for turn, so
      the count is `size[1]` and `size[2]` respectively. Change
      `initial_reach()` from `vec![vec![1.0]; n_players]` to
      `vec![vec![1.0 / bucket_count(p)]; n_players]`. `calc_br`/`calc_ev`
      already loop `for b in 0..res[p].len()` so the work is plumbing.
      This brings absolute exploitability values down by 12888x (from
      ~55000 mbb/h to ~5 mbb/h for a 1M-iter run).
- [ ] **P9.3** `rayon` parallel MCCFR walker (1 hour). The current
      trainer is single-threaded over leaves. `rayon::par_iter()` over the
      leaf batch gives near-linear scaling on the 16-core box. `rayon` is
      already a transitive dep; only `use rayon::prelude::*` is needed.
      ~16x speedup.
- [ ] **P9.4** Pre-built abstract game tree cache (0.5 day). Build the
      full game tree once at trainer init; serialize to `game_tree.bin`;
      MCCFR iterates over a `Vec<NodeId>` rather than reconstructing. The
      trainer already builds the tree once, so this is mostly a refactor
      for cache locality. ~2-3x speedup.
- [ ] **P9.5** External-sampling MCCFR (1 day). Replace the alternating
      player-update pattern with Lanctot et al. 2009's external-sampling:
      one player is the "regret-updating" player, opponents' actions are
      sampled from their strategy, all players updated in a single walk.
      4-6x algorithmic speedup.
- [ ] **P9.6** Action abstraction presets (0.5 day). Per-street
      `Options::action_sizes: HashMap<Round, Vec<f32>>`. Flop: `[0.33,
      0.5, 0.75, 1.0, all-in]`. Turn/River: `[0.5, 0.75, 1.0, 1.5, 2.0,
      all-in]`. Cuts leaves 2-3x with no measurable exploitability loss.
- [ ] **P9.7** 2p 15BB / 25BB benchmark suite (0.5 day). Reproducible
      timing: 1M iters, single-thread baseline; with P9.3+P9.4 applied,
      target <30 seconds per depth tier to <50 mbb/h. Print
      `convergence.jsonl` summary table. This is the "speed checkpoint".

**Combined effect (cumulative):**

| Steps applied | 2p 25BB to ≤5 mbb/h | 3p 25BB to ≤10 mbb/h |
|---------------|----------------------|-----------------------|
| Today (post-Phase 5) | ~10-30 min | ~2-8 hours |
| + P9.3 rayon (16x) | ~30 sec – 2 min | ~5-30 min |
| + P9.4 tree cache (3x) | ~10-30 sec | ~2-10 min |
| + P9.5 external-sample (5x) | **~3-10 sec** | **~30 sec – 3 min** |
| + P9.6 action ab (2x) | ~1-5 sec | ~15-90 sec |

### Phase 10 - Runtime decision quality (**priority 0**, blocks Python)

Unblock trustworthy decisions before `rust_solver_py`. Order matters.

- [ ] **P10.1** Real SHOWDOWN eval in BR/EV terminal walker (unblocks P9.1;
      same work — wire `rust_poker::hand_evaluator::Evaluator`)
- [ ] **P10.2** Per-bucket reach normalization (P9.2) — trustworthy `max(eps)`
- [ ] **P10.3** **Hero-exact strategy at query**: given `hero_hand` + board,
      return that combo's strategy (bypass uniform fallback for unvisited
      abstract buckets); optional "exact combo" training mode for query spot
- [ ] **P10.4** Convergence stop: train until `max(eps) ≤ target_mbb` or
      `time_budget_ms`; expose in API (not fixed 200 iters)
- [ ] **P10.5** Flop-entry solve + turn-card sampling (match TUI geometry:
      pot/call at decision node); extend `benchmark/kk_turn` harness
- [ ] **P10.6** PPT range parsing or postflop-solver range import (hyphen
      syntax `QQ-22`, `A5s-A4s`); drop ad-hoc combo file when done
- [ ] **P10.7** **Quality gate**: `benchmarks/kk_turn_*` asserts non-uniform
      hero strategy, `exploitability_max < 50 mbb/h` (then tighten to 5)

**Phase 10 gate (must pass before Phase 11):**

On `benchmarks/kk_turn_040229_prompt.md` spot (stack 12, explicit OOP/IP
ranges, turn cards `Kd`/`8s`):

- `solve_elapsed_ms < 500` (speed — already met)
- Top action score ≠ uniform (1/3); strategy visibly converged
- `exploitability_max_mbb` finite and `< 50` (then `< 5` stretch)

### Phase 11 - Python library (`rust_solver_py`) (**priority 0 after Phase 10**)

- [ ] **P11.1** Crate `rust_solver_py/` with PyO3 + `pyproject.toml`
      (maturin build-backend); `uv venv` + `maturin develop --release` docs
- [ ] **P11.2** Minimal API first (not full `solver_ext` clone):
      `solve_turn_decision(hero_hand, board, pot_bb, call_cost_bb,
      eff_stack_bb, oop_range, ip_range, ...) -> Decision`
- [ ] **P11.3** `TrainingSample` + `solve_flop_tree` compatibility layer for
      `rjeans_tui/solver_decide.py` (if TUI still needs sample traversal)
- [ ] **P11.4** `SolverSession` holder (reuse tree/range config across calls)
- [ ] **P11.5** Integration test: import from uv venv, run KK spot, compare
      to Phase 10 gate
- [ ] **P11.6** Wire into rjeans TUI behind feature flag (`RUST_SOLVER=1`)

**Explicitly deferred until Phase 10 passes:** shipping PyO3 bindings that
wrap the current trainer — they would expose fast but exploitable play.

## Total effort

| Phase | Best  | Realistic | Pessimistic |
|-------|-------|-----------|-------------|
| 0     | 2d    | 3d        | 1w          |
| 1     | 1w    | 1.5w      | 2.5w        |
| 2     | 1w    | 1.5w      | 2.5w        |
| 3     | 0.5w  | 1w        | 1.5w        |
| 4     | 1.5w  | 2w        | 3w          |
| 5     | 1w    | 1.5w      | 2w          |
| 6     | 2.5w  | 3.5w      | 5w          |
| 7     | 1w    | 1.5w      | 2.5w        |
| 8     | 0.5w  | 0.5w      | 1w          |
| 9     | 1w    | 1.5w      | 2.5w        |
| 10    | 1w    | 2w        | 3w          |
| 11    | 0.5w  | 1w        | 1.5w        |
| **Total** | **~12w** | **~17.5w** | **~26w** |

## Risks

1. **Unsafe discount thread / UB**: silent corruption. Phase 4 fix is
   non-negotiable.
2. **Memory blowup in 3p**: 1.5-30 GB if sparse allocation is skipped.
   Phase 5 must precede Phase 6.
3. **Broken non-sampling `cfr()` path**: safe-search needs exhaustive CFR.
   Either fix or rewrite as `FullCFR` module in Phase 0.
4. **3p constant-sum convention**: subtle. Math preamble doc + reach test
   before Phase 6.1.
5. **Disk format**: choose `bincode` once (Phase 4), not later.
6. **15-25BB exploitability precision**: the SHOWDOWN-leaf placeholder
   used in the BR/EV walker (post-Phase 4 commit `950c27f`) is structurally
   correct (zero-sum, comparable magnitude to EV) but uses "player 0 always
   wins" as a stand-in for real hand evaluation. Absolute values are
   10000x off the target. **P9.1 (real hand eval) is the unblocking step**
   for any quantitative convergence claim.
7. **Uniform strategy at runtime (Jul 2026)**: KK turn benchmark stays at
   ~33% / 33% / 33% through 20k MCCFR iters — external sampling + ISOMORPHIC
   buckets + sparse unvisited clusters. **Phase 10.3 (hero-exact) is the
   primary fix**; may also need more iters or less abstraction on query path.
8. **Hand-indexer stub disables EMD pipeline**: the stub
   `hand_indexer_s::size()` returns hard-coded values, and `get_index`
   returns a `DefaultHasher` hash. Tools that enumerate 0..N hand indices
   (`gen_ehs`, `gen_abstraction`) panic or return garbage. The trainer
   works because it draws hands from `hand_ranges` (valid preflop combos)
   and never enumerates. **Resuming P0.5d (`crates/poker_canon/`) or
   shipping a real Waugh-style indexer is the unblocking step for EMD
   flop/turn/river (Phase 2).**

## `convergence.json` schema

```json
{
  "iter": 1000000,
  "t_seconds": 145.2,
  "depth_tier_bb": 20,
  "n_players": 2,
  "ev": [0.012, -0.012],
  "best_response": [0.045, 0.040],
  "exploitability_mbb_per_hand": [4.5, 4.0],
  "exploitability_max_mbb_per_hand": 4.5,
  "memory_mb": 1820,
  "n_threads": 16
}
```

In 3p constant-sum, sum of `ev[i] == 0`. `exploitability_max` is the
per-player worst case.

## Why not `bd` (beads)?

The repo's `bd` instance uses an embedded Dolt backend. During plan
setup, that backend became unstable in this environment (lock contention,
intermittent "no beads database found" errors after a successful
create). To avoid a tool that randomly breaks the workflow, this plan
uses `TASKS.md` as the source of truth and `git` for history. If `bd` is
later migrated to a stable remote-Dolt setup, the same task list can be
imported from `TASKS.md`.
