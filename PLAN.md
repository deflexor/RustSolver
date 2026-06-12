# RustSolver — 3p, asymmetric stacks, fixed preflop range

## Goal

Extend `RustSolver` from "2-player postflop research skeleton" to a usable
postflop-only MCCFR solver for **3 players**, with **discrete depth tiers**
{5, 8, 10, 12, 15, 18, 20, 25} BB and **fixed preflop ranges** loaded from
file. Card abstraction pipeline is fully wired. Exploitability reported as a
3-vector `eps = (eps_0, eps_1, eps_2)` plus `max(eps_i)`. Side pots use the
**stack-cap** convention (no true side-pot bookkeeping).

Companion task tracker: `TASKS.md`. The bd (beads) tracker was abandoned
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

## Phases

### Phase 0 - Hygiene (2-3 days)

Remove things that block further work, no design changes.

- [ ] Remove `extern crate cortex_m` from `src/solver/main.rs`
- [ ] Remove unused nightly feature gates: `generators`, `generator_trait`,
      `box_into_pin`, `box_syntax`, `feature(test)`
- [ ] Either implement `src/solver/actions.rs` or delete the file
- [ ] Fix `kmeans::fit_growbatch` early-stop (remove the `break`; let
      `min_change > stop_threshold` actually run)
- [ ] Strip dead/commented-out code: `gen_ochs`, `gen_emd(turn, ...)`,
      `gen_emd(river, ...)` calls in `gen_abstraction/main.rs`; the
      non-sampling `cfr()` path in `cfr.rs` (or refactor into a clean
      `FullCFR` module for Phase 6 safe-search)
- [ ] Add a smoke test: `train()` for 10k iters asserts BR values are
      finite and the regret table mutates
- [ ] Verify `rust_poker 0.1.5` still builds; if not, evaluate replacement
      (`poker_eval_rs`, `poker`)

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
| **Total** | **~9w** | **~13w** | **~19w** |

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
