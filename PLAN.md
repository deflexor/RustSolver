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

**Production status (Jul 10 2026):** **Staging-ready** for HU **turn-entry**
spots (pot=2 BB, call=0, stack bucket 12). `rust_solver_py` matches
**solver_ext** on KK (check **0.614** vs **0.602**, ~35 ms/spot). Quality
gates pass via `scripts/run_quality_gates.sh` + `.gitlab-ci.yml`. Exploitability
scale fixed (P12.4); 5-spot suite + CI wired (P12.5–P12.6). `RUST_SOLVER=1`
staging swap in rjeans (P12.7). **P12.8 production sign-off** still open:
full G1 suite parity vs rjeans, strict G4 &lt;50 mbb on wide ranges.

Companion task tracker: `TASKS.md`.

### Session checkpoint (Jul 10 2026 — Phase 12 staging pass, commit `55db788`)

**Phase 12 (P12.4–P12.7 done; P12.8 open):**
- P12.4 exploitability scale: chip→BB, hand-weighted reach, combo subsampling;
  `target_mbb` stop without requiring convergence log; small-tree G4 &lt;50 unit test
- P12.5 `benchmarks/hu_turn_suite.json` (5 spots) + `hu_turn_suite_bench`
- P12.6 `.gitlab-ci.yml` + expanded `scripts/run_quality_gates.sh`
- P12.3 KK parity: check **0.614** vs solver_ext **0.602** (±0.10)
- P12.7 `RUST_SOLVER=1` in rjeans `solver_decide.py` (staging; rollback = unset env)
- **Still open (P12.8):** full G1 on suite vs rjeans; G4 &lt;50 on wide ranges;
  README/PLAN gate closure

**Phase 11:**
- P11.5 CI quality gates done (`run_quality_gates.sh` + GitLab CI)
- P11.6 staging swap done in rjeans (separate repo; commit/validate there)
- P11.4 config cache still open

### Session checkpoint (Jul 10 2026 — KK parity pass, earlier)

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

**Status: mostly done** (HU 2p path production-ready; `preflop_ranges` JSON
loader deferred to Phase 8).

- [x] `MAX_PLAYERS = 3` (constants.rs)
- [x] `PlayerState { stack, wager, has_folded, has_acted_this_street }`
- [x] `GameState { players: Vec<PlayerState> }` (no fixed array; `num_players()`)
- [x] Replace every `1 - current` with `next_active_player(current, &players)`
      skipping folded/acted
- [x] `apply_action` raise/call/fold: loop over active players; **cap wager
      at opponent's stack**
- [x] `BettingRound` stays `Flop | Turn | River` (no Preflop)
- [x] `Options::depth_tier_bb: u32` replaces `stack: u32`
- [ ] `Options::preflop_ranges: [HandRange; 3]` loaded from `ranges/*.json`
      (field exists; loader is Phase 8)
- [x] `Options::postflop_pot_override: Option<u32>` (default 1.5 BB)
- [x] `Options::rake: Option<(f64, u32)>` (default None; applied in tree builder)
- [x] `Options::max_raises: u8` and `Options::all_in_threshold: f64` are
      actually read by `state.rs`
- [x] `TerminalNode::value` SHOWDOWN/ALLIN uses min-wager convention:
      player i wins `sum_{j != i} min(my_wager, opp_wager)`; subtract rake
      (`player_wagers` on `TerminalNode`)
- [x] Existing 2p test cases must still pass

### Phase 2 - Wire abstraction pipeline (1-1.5 weeks)

**Status: deferred** (hand-indexer stub blocks EMD flop/turn/river; see risk #10).

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

**Status: deferred** (`max_raises` / `all_in_threshold` wired in Phase 1; per-street
presets not yet productized).

- [ ] Defaults per street:
  - Flop: bet `[0.33, 0.5, 0.75, 1.0]`, raise `[2.0, 3.0]`, max 3 raises
  - Turn: bet `[0.5, 0.75, 1.0, 1.5]`, raise `[2.0, 2.5, 3.0]`, max 3 raises
  - River: bet `[0.5, 0.75, 1.0, 1.5, 2.0]`, raise `[2.0, 3.0]`, max 3 raises
- [ ] Wire `Options::max_raises` and `Options::all_in_threshold` to
      `state.rs` (overlaps with P1.5; verify when both land)
- [ ] Add `Options::max_action_sequences_per_street: u32` (default 200) to
      cap tree width in 3p

### Phase 4 - Exploitability, BR, CFR+ (1.5-2 weeks)

**Status: done for 2p HU** (BR/EV in `cfr.rs`; 3p constant-sum not verified).

- [x] Replace `unsafe` raw-pointer discount thread with `crossbeam::scope`
      + atomic regrets (no UB)
- [x] Per-player best response: `calc_br()` / `calc_ev()` walkers in `cfr.rs`
      (not separate `br_2p`/`br_3p` modules)
- [x] `convergence.json` writer with the schema in
      `docs/convergence_schema.md`
- [x] Add CFR+ for the 2p path: regret floor at 0 (weighted strategy_sum
      approximate; see P9.8)
- [x] Add `--target-mbb` and `--max-iter` flags; stop when either is met
- [ ] In 3p, sum of `ev[i]` should be 0 (constant-sum convention)

### Phase 5 - Threading & memory (1-1.5 weeks)

**Status: partial** (sparse infosets + `total_bytes` done; scaling bench + OOM
budget open).

- [ ] Benchmark at 1/2/4/8/16 threads; report scaling efficiency
- [x] Switch `Infoset` to sparse allocation: only allocate `regrets` and
      `strategy_sum` boxes on first write
- [x] Add `InfosetTable::total_bytes()` and log in `convergence.json`
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

**Status: deferred** (after HU turn production sign-off).

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

**Status: deferred.**

- [ ] `bin/solve_tier.rs`: `--depth-bb 20 --out strategy_t20.bin`
- [ ] Cache `*_emd.dat`; skip regeneration if newer than source
- [ ] `bin/play_2p.rs` and `bin/play_3p.rs`: load strategy, play out hands
- [ ] `tests/integration/turn_river_2p.rs`: known ACPC-style scenario;
      assert `exploitability_max < 50 mbb/h` after 1M iters
- [ ] `tests/integration/asymmetric_3p_smoke.rs`: tiny 3p tree to 10k
      iters; assert all `br_i` finite, `exploitability_max < 5000 mbb/h`
- [ ] `bincode` serialization for strategies

### Phase 8 - Preflop range files (0.5 week)

**Status: deferred.**

- [ ] `ranges/BTN_vs_SB_BB_3p.json`: 169-hand distribution per position
- [ ] `ranges/BB_defend.json`: defending distribution when BB acts first
- [ ] Loader: `Options::preflop_ranges = load_ranges(path)`

### Phase 9 - Speed & precision for 15-25BB (1-2 weeks, **priority 0**)

**Status: partial** — P9.1–P9.3 and P9.5 done (largely superseded by Phase 10
for eval/reach quality); P9.4/P9.6 and full tier sweep still open.

Drives the 15-25BB use case. Each task is small and independent; do in
this order for compounding gains.

- [x] **P9.1** Real hand evaluation in terminal walker — done via **P10.1**
      (`terminal_payoffs_from_combos`, `evaluate()`)
- [x] **P9.2** Per-bucket reach — done via **P10.2** / **P12.4**
      (hand-weighted reach + chip→BB scale)
- [x] **P9.3** `rayon` parallel MCCFR walker (`rayon::ThreadPoolBuilder` in
      `cfr.rs`)
- [ ] **P9.4** Pre-built abstract game tree cache (0.5 day). Build the
      full game tree once at trainer init; serialize to `game_tree.bin`;
      MCCFR iterates over a `Vec<NodeId>` rather than reconstructing. The
      trainer already builds the tree once, so this is mostly a refactor
      for cache locality. ~2-3x speedup.
- [x] **P9.5** External-sampling MCCFR (`external_sampling_mccfr` in `cfr.rs`)
- [ ] **P9.6** Action abstraction presets (0.5 day). Per-street
      `Options::action_sizes: HashMap<Round, Vec<f32>>`. Flop: `[0.33,
      0.5, 0.75, 1.0, all-in]`. Turn/River: `[0.5, 0.75, 1.0, 1.5, 2.0,
      all-in]`. Cuts leaves 2-3x with no measurable exploitability loss.
- [~] **P9.7** 2p benchmark suite — KK + HU turn suite timing gates pass;
      full 15–25 BB tier sweep and strict exploitability checkpoint open

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
      (scale fixed P12.4: chip→BB, hand-weighted reach)
- [x] **P10.3** Hero-exact strategy at query: `pin_hero`, `query_strategy()`
- [x] **P10.4** Convergence stop: `time_budget_ms` + `target_mbb` on `TrainConfig`
- [x] **P10.5** Flop-entry solve + turn-card sampling (infra; production uses turn-entry @ 2 BB pot matching `solver_ext`)
- [~] **P10.6** PPT hyphen range import (`range_parse.rs`); combo-file fallback
      remains when expansion is sparse
- [x] **P10.7** Quality gate v1: non-uniform + geometry + &lt;500 ms
- [~] **P10.8** KK quality gate v2: parity ±0.10 (**passes**); G4 &lt;50 on wide
      ranges **open** (small-tree unit test passes; sampled BR ~300–440 mbb)

**Phase 10 gate v1 (passed Jul 2026):** KK spot — &lt;500 ms, non-uniform,
pot/call geometry match.

**Phase 10 gate v2 (partial):** parity passes; strict exploitability on wide
ranges deferred to P12.8 (see Phase 12 G4).

### Phase 11 - Python library (`rust_solver_py`)

- [x] **P11.1** Crate `rust_solver_py/` + `pyproject.toml` + `README.md`
- [x] **P11.2** `solve_turn_decision(...)` one-shot API
- [x] **P11.3** `TrainingSample` + `SolverSession.solve_flop_tree` compat layer
- [ ] **P11.4** Session-scoped config cache (ranges, stack bucket, tree params)
- [x] **P11.5** uv integration test in CI: import, KK spot, assert gates
      (`scripts/run_quality_gates.sh` + `.gitlab-ci.yml`)
- [x] **P11.6** rjeans TUI `RUST_SOLVER=1` swap-in (staging; code in rjeans repo)

### Phase 12 - Production readiness: HU turn solving (**priority 0**)

**Goal:** P12.8 sign-off — `RUST_SOLVER=1` safe for real TUI turn decisions on
tested HU spots. **Current:** staging-ready; not full production.

**Production definition (sign-off criteria):**

| # | Criterion | Target | Staging (Jul 10) |
|---|-----------|--------|------------------|
| G1 | **Decision parity** | Suite ≥5 spots: top action vs rjeans ≥80%; key probs ±0.10 | KK yes; 3/5 spots lack rjeans baselines |
| G2 | **Geometry** | `pot_bb`, `call_cost_bb`, board, stack ±0.5 BB | Pass |
| G3 | **Speed** | `solve_elapsed_ms < 500` per spot | Pass (~35 ms) |
| G4 | **Exploitability** | `exploitability_max_mbb` &lt;50 on benchmark spots | Scale fixed; small-tree &lt;50; wide ~300–440 (sampled BR) |
| G5 | **Python UX** | `rust_solver_py` drop-in; CI green | Pass |

**Not in scope for Phase 12:** 3p, EMD tier sweep, all stack tiers, river-only product.

**Root cause (why v1 is not production):** turn-entry solves with pot injected
at turn root skip flop betting history; rjeans `solver_ext` uses **flop-entry**
postflop trees. Strategy parity requires P12.1 + P12.2 first.

**Recommended order (P12.8 — next session):**

1. **P12.8** Production sign-off — close G1–G5 gaps below; update README + PLAN.
2. **Suite A/B** — rjeans baselines for all 5 spots in `hu_turn_suite.json`;
   extend `run_hu_turn_suite.py` to diff vs `solver_ext` (like `run_kk_turn_compare.py`).
3. **G4 decision** — more iters + `target_mbb=50` stop, or document sampled-BR
   bound and keep small-tree unit test for strict &lt;50.
4. **P12.1** (optional) — flop-entry parity for raised-pot lines if turn-entry
   insufficient.
5. **rjeans** — commit/validate `RUST_SOLVER=1` staging in rjeans repo.

**Effort estimate:** P12.8 ~1–2 sessions.

- [~] **P12.1** Flop-entry solve + turn-card path (infra; default = turn-entry @ 2 BB)
- [x] **P12.2** Action/tree param parity with `solver_ext`
- [x] **P12.3** KK parity validation vs rjeans baseline (±0.10 check prob)
- [x] **P12.4** Trustworthy exploitability on turn trees (scale fixed; sampled BR on wide ranges)
- [x] **P12.5** Multi-spot HU turn benchmark suite
- [x] **P12.6** CI parity + exploitability gates
- [x] **P12.7** rjeans `RUST_SOLVER=1` staging integration
- [ ] **P12.8** Production sign-off for HU turn solving

**Phase 12 gate (production = P12.8):**

On `benchmarks/hu_turn_suite.json` + KK prompt:

- G1–G5 all pass (see table above for current gaps)
- `RUST_SOLVER=1` recommended for **staging** on tested turn-entry spots;
  `solver_ext` remains default until P12.8 closed

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

1. **Unsafe discount thread / UB**: **Fixed** (Phase 4; `crossbeam::scope` +
   atomic regrets). Do not reintroduce raw-pointer discount threads.
2. **Memory blowup in 3p**: 1.5-30 GB if sparse allocation is skipped.
   Phase 5 must precede Phase 6.
3. **Broken non-sampling `cfr()` path**: safe-search needs exhaustive CFR.
   Either fix or rewrite as `FullCFR` module in Phase 0.
4. **3p constant-sum convention**: subtle. Math preamble doc + reach test
   before Phase 6.1.
5. **Disk format**: choose `bincode` once (Phase 4), not later.
6. **15-25BB exploitability precision**: **Mitigated** by P9.1/P10.1 (real
   hand eval) and P9.2/P10.2/P12.4 (reach + chip→BB scale). Remaining gap:
   strict &lt;50 mbb/h on wide HU ranges at modest iter budgets (see risk #9).
7. **Uniform strategy at runtime (Jul 2026)**: **Mitigated** by P10.3
   (hero-pinned + `query_strategy`). Remaining gap is parity vs rjeans, not
   uniformity — see risks 8–9.
8. **Turn-entry vs flop-entry (Jul 2026)**: turn-entry @ 2 BB achieves KK parity
   with `solver_ext`; flop-entry (P12.1) may still matter for raised-pot lines.
9. **Exploitability on wide ranges (Jul 2026)**: scale bug fixed (P12.4; was ~76k
   mbb/h). Sampled BR on wide HU ranges reports ~300–440 mbb at 1000 iters;
   strict &lt;50 mbb/h requires more iters, exact BR, or P12.8 sign-off policy.
   Small-tree unit test `turn_tree_exploitability_under_budget` passes G4.
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
