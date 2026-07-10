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

**Production status (Jul 2026):** `rust_solver_py` achieves **solver_ext parity**
on the KK HU turn benchmark (check ≈0.61 vs 0.60, pot=2 BB, &lt;150 ms) with
deterministic training (seed + CFR+). Flop-entry infrastructure is in place for
future full-tree solves. Exploitability scale (P12.4) and multi-spot CI suite
(P12.5–P12.6) remain open before TUI `RUST_SOLVER=1` sign-off.

Companion task tracker: `TASKS.md`.

### Session checkpoint (Jul 10 2026 — production parity pass)

**Phase 12 (partial — KK parity gate passes):**
- P12.2 tree param parity: `solver_ext_action_abstraction`, 2 BB turn-entry pot
- P12.3 KK A/B: check **0.614** vs solver_ext **0.602** (±0.10), ~110 ms
- MCCFR: PublicChance card dealing; `TrainConfig::seed` for reproducibility
- `HeroSampleQuery` + flop-entry path for experiments (`solve_flop_entry_turn`)
- `scripts/run_quality_gates.sh` — Rust + Python smoke (P11.5 partial)
- **Still open:** P12.4 exploitability; P12.5–P12.8 multi-spot CI + TUI swap

**Phase 11 (scaffolded):**
- `rust_solver_py/` PyO3 crate: `SolverSession.solve_flop_tree`, `solve_turn_decision`, `TrainingSample`
- `src/solver/python_api.rs` + `src/lib.rs` library surface
- Docs: `rust_solver_py/README.md`
- **Still open:** P11.5 CI integration test; P11.6 `RUST_SOLVER=1` in rjeans TUI

### Session checkpoint (Jul 2026, earlier)

- Added KK turn A/B harness: `kk_turn_bench`, `benchmarks/run_kk_turn_compare.py`
- vs rjeans: **62 ms vs 711 ms**, but rust_solver top action ~uniform (0.33)
- Fixed tree-builder blockers: `is_terminal()` (river after betting settles),
  `street_closed()` (≤1 player can still bet)
- Expanded OOP/IP ranges via postflop-solver → `benchmarks/kk_turn_expanded_combos.txt`

---

## Long-term goal (unchanged)

Extend to **3 players**, discrete depth tiers {5, 8, 10, 12, 15, 18, 20, 25}
BB, fixed preflop ranges, EMD abstraction, 3-vector exploitability. Side pots
use **stack-cap** convention. Phases 1–9 below remain valid; **Phase 10–12
are the critical path** for production HU turn solving via Python.

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

### Phase 10 - Runtime decision quality (**priority 0**, blocks production)

Unblock trustworthy decisions before production TUI swap. Order matters.

- [x] **P10.1** Real SHOWDOWN eval in BR/EV terminal walker
- [x] **P10.2** Per-bucket reach normalization — trustworthy `max(eps)` plumbing
      (scale still wrong on turn trees; see P12.4)
- [x] **P10.3** Hero-exact strategy at query: `pin_hero`, `query_strategy()`
- [x] **P10.4** Convergence stop: `time_budget_ms` + `target_mbb` on `TrainConfig`
- [x] **P10.5** Flop-entry solve + turn-card sampling (infra; production uses turn-entry @ 2 BB pot matching `solver_ext`)
- [~] **P10.6** PPT hyphen range import (`range_parse.rs`); combo-file fallback
      remains when expansion is sparse
- [x] **P10.7** Quality gate v1: non-uniform + geometry + &lt;500 ms
- [~] **P10.8** KK quality gate v2: parity ±0.10 (**passes**); exploitability &lt;50 mbb/h **fails** (P12.4)

**Phase 10 gate v1 (passed Jul 2026):** KK spot — &lt;500 ms, non-uniform,
pot/call geometry match.

**Phase 10 gate v2 (required for production):** above + exploitability
trustworthy + rjeans parity (see Phase 12).

### Phase 11 - Python library (`rust_solver_py`)

- [x] **P11.1** Crate `rust_solver_py/` + `pyproject.toml` + `README.md`
- [x] **P11.2** `solve_turn_decision(...)` one-shot API
- [x] **P11.3** `TrainingSample` + `SolverSession.solve_flop_tree` compat layer
- [ ] **P11.4** Session-scoped config cache (ranges, stack bucket, tree params)
- [ ] **P11.5** uv integration test in CI: import, KK spot, assert gates
- [ ] **P11.6** rjeans TUI `RUST_SOLVER=1` swap-in (**experimental only** until Phase 12)

### Phase 12 - Production readiness: HU turn solving (**priority 0**, next session)

**Goal:** `RUST_SOLVER=1` safe for real TUI turn decisions on tested HU spots.

**Production definition (sign-off criteria):**

| # | Criterion | Target |
|---|-----------|--------|
| G1 | **Decision parity** | On benchmark suite (≥5 spots): same top action as rjeans ≥80%; key prob within ±0.10 (e.g. KK check ~0.49) |
| G2 | **Geometry** | `pot_bb`, `call_cost_bb`, board, stack bucket match query node (±0.5 BB) |
| G3 | **Speed** | `solve_elapsed_ms < 500` per spot (16-core desktop, 200 iters default) |
| G4 | **Exploitability** | `exploitability_max_mbb` finite and &lt;50 (stretch &lt;5) on benchmark spots |
| G5 | **Python UX** | `rust_solver_py` drop-in; `solver_decide.py` unchanged; docs + CI green |

**Not in scope for Phase 12:** 3p, EMD tier sweep, all stack tiers, river-only product.

**Root cause (why v1 is not production):** turn-entry solves with pot injected
at turn root skip flop betting history; rjeans `solver_ext` uses **flop-entry**
postflop trees. Strategy parity requires P12.1 + P12.2 first.

**Recommended order (next session — start here):**

1. **P12.1** Flop-entry solve — flop `4dQcQd`, `starting_pot=200` (2 BB),
   check-check line → turn card → hero decision. Wire `python_api` +
   `kk_turn_bench`. *Highest impact.*
2. **P12.2** Tree param parity — match `solver_ext`: bet `50/75/100%`, raise
   `2.5x`, `max_raises=3`, `all_in_threshold=1.5`; same range strings.
3. **P12.3** Re-run KK A/B (`run_kk_turn_compare.py`); target check ≈0.49 ±0.10.
4. **P12.4** Fix exploitability scale — debug `calc_br`/`calc_ev` on turn trees
   (all-in terminals, reach averaging); enable stop on `target_mbb`.
5. **P12.5** Benchmark suite — 5–10 HU turn spots from TUI hand history;
   JSON results + ranked-decision diff vs rjeans.
6. **P12.6** Parity gate in CI — assert G1–G4 on suite; document known deltas.
7. **P12.7** `RUST_SOLVER=1` in rjeans TUI (staging); default stays `solver_ext`.
8. **P12.8** Production sign-off — update README; close Phase 12 when G1–G5 pass.

**Effort estimate:** ~2–3 weeks focused (10–15 sessions).

- [~] **P12.1** Flop-entry solve + turn-card path (infra; default = turn-entry @ 2 BB)
- [x] **P12.2** Action/tree param parity with `solver_ext`
- [x] **P12.3** KK parity validation vs rjeans baseline (±0.10 check prob)
- [ ] **P12.4** Trustworthy exploitability on turn trees
- [ ] **P12.5** Multi-spot HU turn benchmark suite
- [ ] **P12.6** CI parity + exploitability gates
- [ ] **P12.7** rjeans `RUST_SOLVER=1` staging integration
- [ ] **P12.8** Production sign-off for HU turn solving

**Phase 12 gate (production):**

On `benchmarks/kk_turn_040229_prompt.md` + suite:

- G1–G5 all pass
- `RUST_SOLVER=1` recommended for staging; `solver_ext` remains default until
  suite coverage is satisfactory

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
| 12    | 1.5w  | 2.5w      | 3.5w        |
| **Total** | **~13.5w** | **~20w** | **~29.5w** |

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
7. **Uniform strategy at runtime (Jul 2026)**: **Mitigated** by P10.3
   (hero-pinned + `query_strategy`). Remaining gap is parity vs rjeans, not
   uniformity — see risks 8–9.
8. **Turn-entry vs flop-entry (Jul 2026)**: injecting pot at turn root gives
   correct geometry but wrong strategic context vs rjeans. **P12.1 flop-entry
   is the primary parity fix** before trusting turn decisions in production.
9. **Exploitability scale on turn trees**: BR reports ~76k mbb/h despite real
   hand eval — debug target for P12.4; blocks `target_mbb` stop criteria.
10. **Hand-indexer stub disables EMD pipeline**: the stub
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
